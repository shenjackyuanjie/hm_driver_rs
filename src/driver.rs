use crate::agent::{
    AgentProfile, AgentResolver, AgentSource, CompatibilityStatus, HarmonyTransport,
    materialize_agent,
};
use crate::hdc::{CommandOutput, HdcConfig, HdcRunner};
use crate::rpc::{ApiDialect, RpcClient};
use crate::selector::{Element, Selector};
use crate::types::{validate_ability, *};
use crate::ui::{UiNode, parse_layout};
use crate::xpath::XPathElement;
use crate::{DriverError, Result};
use regex::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

static OPERATION_ID: AtomicU64 = AtomicU64::new(1);

/// Driver 的运行时配置。
#[derive(Clone, Debug)]
pub struct DriverConfig {
    pub rpc_timeout: Duration,
    pub max_rpc_frame_size: usize,
    pub kill_daemon_on_close: bool,
    pub cleaner_batch_size: usize,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            rpc_timeout: Duration::from_secs(20),
            max_rpc_frame_size: 8 * 1024 * 1024,
            kill_daemon_on_close: false,
            cleaner_batch_size: 20,
        }
    }
}

/// 创建异步 Driver 的 Builder。
#[derive(Clone, Debug, Default)]
pub struct HmDriverBuilder {
    selector: DeviceSelector,
    hdc: HdcConfig,
    agent_source: AgentSource,
    config: DriverConfig,
}

impl HmDriverBuilder {
    pub fn device(mut self, selector: DeviceSelector) -> Self {
        self.selector = selector;
        self
    }

    pub fn hdc_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.hdc.path = Some(path.into());
        self
    }

    pub fn hdc_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.hdc.server = Some((host.into(), port));
        self
    }

    pub fn hdc_config(mut self, config: HdcConfig) -> Self {
        self.hdc = config;
        self
    }

    pub fn agent_source(mut self, source: AgentSource) -> Self {
        self.agent_source = source;
        self
    }

    pub fn driver_config(mut self, config: DriverConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn connect(self) -> Result<HmDriver> {
        let discovery = HdcRunner::new(self.hdc)?;
        let descriptor = discovery.select(&self.selector).await?;
        let hdc = discovery.with_serial(descriptor.serial.clone());
        let probe = probe_device(&hdc).await?;
        let profile = AgentResolver::new()?.resolve(&probe.architecture, &probe.uitest_version)?;
        if profile.compatibility == CompatibilityStatus::OfficialReferenceOnly {
            tracing::warn!(
                target: "hm_driver_rs::compatibility",
                agent_version = %profile.version,
                "所选 Agent 分支仅有官方参考验证，尚未完成本地真机验证"
            );
        }
        let agent_path = materialize_agent(&self.agent_source, &profile).await?;
        ensure_agent(&hdc, &profile, &agent_path).await?;
        let session =
            establish_session(&hdc, &profile.transport, &self.config, probe.api_level).await?;
        Ok(HmDriver {
            inner: Arc::new(HmDriverInner {
                hdc,
                source: self.agent_source,
                profile,
                config: self.config,
                state: Mutex::new(SessionState {
                    rpc: Some(session.rpc),
                    dialect: Some(session.dialect),
                    driver_reference: Some(session.driver_reference),
                    local_port: Some(session.local_port),
                    generation: 1,
                    closed: false,
                    api_level: probe.api_level,
                }),
                cleaner: StdMutex::new(Vec::new()),
                generation: AtomicU64::new(1),
            }),
        })
    }
}

/// 一个设备上的异步 HarmonyOS Driver。
#[derive(Clone)]
pub struct HmDriver {
    pub(crate) inner: Arc<HmDriverInner>,
}

impl std::fmt::Debug for HmDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HmDriver")
            .field("agent", &self.inner.profile.version)
            .field("transport", &self.inner.profile.transport)
            .finish_non_exhaustive()
    }
}

pub(crate) struct HmDriverInner {
    hdc: HdcRunner,
    source: AgentSource,
    profile: AgentProfile,
    config: DriverConfig,
    state: Mutex<SessionState>,
    cleaner: StdMutex<Vec<QueuedReference>>,
    generation: AtomicU64,
}

struct SessionState {
    rpc: Option<RpcClient>,
    dialect: Option<ApiDialect>,
    driver_reference: Option<String>,
    local_port: Option<u16>,
    generation: u64,
    closed: bool,
    api_level: Option<u32>,
}

struct QueuedReference {
    value: String,
    generation: u64,
}

struct EstablishedSession {
    rpc: RpcClient,
    dialect: ApiDialect,
    driver_reference: String,
    local_port: u16,
}

struct DeviceProbe {
    architecture: String,
    uitest_version: String,
    api_level: Option<u32>,
}

impl HmDriver {
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    /// 使用当前 HDC 配置发现设备，不建立 Agent 会话。
    pub async fn discover_devices(config: HdcConfig) -> Result<Vec<DeviceDescriptor>> {
        HdcRunner::new(config)?.discover().await
    }

    pub fn agent_profile(&self) -> &AgentProfile {
        &self.inner.profile
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    pub(crate) async fn dialect(&self) -> Result<ApiDialect> {
        self.inner
            .state
            .lock()
            .await
            .dialect
            .ok_or(DriverError::SessionInvalid)
    }

    pub async fn call_hypium_api(
        &self,
        api: &str,
        this: Option<&str>,
        args: Value,
    ) -> Result<Value> {
        self.call_api_raw(api, this, args).await
    }

    pub(crate) async fn call_api_raw(
        &self,
        api: &str,
        this: Option<&str>,
        args: Value,
    ) -> Result<Value> {
        self.flush_cleaner(false).await?;
        self.call_direct(api, this, args).await
    }

    async fn call_direct(&self, api: &str, this: Option<&str>, args: Value) -> Result<Value> {
        let rpc = {
            let state = self.inner.state.lock().await;
            if state.closed {
                return Err(DriverError::DriverClosed);
            }
            state.rpc.clone().ok_or(DriverError::SessionInvalid)?
        };
        rpc.call(api, this, args).await
    }

    pub async fn recover(&self) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        if state.closed {
            return Err(DriverError::DriverClosed);
        }
        if let Some(rpc) = state.rpc.take() {
            rpc.invalidate();
        }
        if let Some(port) = state.local_port.take() {
            let _ = self.inner.hdc.remove_forward(port).await;
        }
        self.inner.cleaner.lock().expect("清理队列锁中毒").clear();
        let path = materialize_agent(&self.inner.source, &self.inner.profile).await?;
        ensure_agent(&self.inner.hdc, &self.inner.profile, &path).await?;
        let session = establish_session(
            &self.inner.hdc,
            &self.inner.profile.transport,
            &self.inner.config,
            state.api_level,
        )
        .await?;
        state.rpc = Some(session.rpc);
        state.dialect = Some(session.dialect);
        state.driver_reference = Some(session.driver_reference);
        state.local_port = Some(session.local_port);
        state.generation = state.generation.saturating_add(1);
        self.inner
            .generation
            .store(state.generation, Ordering::Release);
        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        self.flush_cleaner(true).await?;
        let mut state = self.inner.state.lock().await;
        if state.closed {
            return Ok(());
        }
        if let Some(rpc) = state.rpc.take() {
            rpc.invalidate();
        }
        if let Some(port) = state.local_port.take() {
            let _ = self.inner.hdc.remove_forward(port).await;
        }
        if self.inner.config.kill_daemon_on_close {
            stop_singleness_daemon(&self.inner.hdc).await?;
        }
        state.closed = true;
        Ok(())
    }

    pub async fn display_size(&self) -> Result<DisplaySize> {
        let value = self.driver_call("getDisplaySize", json!([])).await?;
        let width = value
            .get("x")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let height = value
            .get("y")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        match (width, height) {
            (Some(width), Some(height)) => Ok(DisplaySize { width, height }),
            _ => Err(DriverError::Protocol("显示尺寸响应无效".into())),
        }
    }

    pub async fn display_rotation(&self) -> Result<DisplayRotation> {
        let value = self.driver_call("getDisplayRotation", json!([])).await?;
        DisplayRotation::try_from(
            value
                .as_u64()
                .ok_or_else(|| DriverError::Protocol("显示旋转响应无效".into()))?,
        )
    }

    pub async fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        self.driver_call("setDisplayRotation", json!([rotation as u8]))
            .await
            .map(|_| ())
    }

    pub async fn device_info(&self) -> Result<DeviceInfo> {
        let product_name = self
            .parameter("const.product.name")
            .await
            .unwrap_or_default();
        let model = self
            .parameter("const.product.model")
            .await
            .unwrap_or_default();
        let brand = self
            .parameter("const.product.brand")
            .await
            .unwrap_or_default();
        let system_version = self
            .parameter("const.product.software.version")
            .await
            .unwrap_or_default();
        let cpu_abi = self
            .parameter("const.product.cpu.abilist")
            .await
            .unwrap_or_default();
        let api_version = self
            .parameter("const.ohos.apiversion")
            .await
            .ok()
            .and_then(|value| value.parse().ok());
        Ok(DeviceInfo {
            product_name,
            model,
            brand,
            api_version,
            system_version,
            cpu_abi,
            display_size: self.display_size().await?,
            display_rotation: self.display_rotation().await?,
        })
    }

    pub async fn screen_on(&self) -> Result<()> {
        self.inner.hdc.shell("power-shell wakeup").await.map(|_| ())
    }

    pub async fn screen_off(&self) -> Result<()> {
        self.press_key(18).await
    }

    pub async fn unlock(&self) -> Result<()> {
        self.screen_on().await?;
        let size = self.display_size().await?;
        self.swipe(
            Point::new((size.width / 2) as i32, (size.height * 4 / 5) as i32),
            Point::new((size.width / 2) as i32, (size.height / 5) as i32),
            6000,
        )
        .await
    }

    pub async fn press_key(&self, key_code: u32) -> Result<()> {
        if key_code > 3200 {
            return Err(DriverError::InvalidCoordinate("按键码超过 3200".into()));
        }
        self.inner
            .hdc
            .shell(format!("uitest uiInput keyEvent {key_code}"))
            .await
            .map(|_| ())
    }

    pub async fn click(&self, point: Point) -> Result<()> {
        self.coordinate_call("click", json!([point.x, point.y]))
            .await
    }

    pub async fn click_position(&self, position: Position) -> Result<()> {
        self.click(self.absolute_position(position).await?).await
    }

    pub async fn double_click(&self, point: Point) -> Result<()> {
        self.coordinate_call("doubleClick", json!([point.x, point.y]))
            .await
    }

    pub async fn long_click(&self, point: Point) -> Result<()> {
        self.coordinate_call("longClick", json!([point.x, point.y]))
            .await
    }

    pub async fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        if !(200..=40_000).contains(&speed) {
            return Err(DriverError::InvalidCoordinate(
                "滑动速度必须位于 200 到 40000".into(),
            ));
        }
        self.coordinate_call("swipe", json!([from.x, from.y, to.x, to.y, speed]))
            .await
    }

    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.coordinate_call("inputText", json!([{"x": 1, "y": 1}, text]))
            .await
    }

    pub async fn install_app(&self, package: impl AsRef<Path>) -> Result<()> {
        self.inner.hdc.install(package.as_ref()).await.map(|_| ())
    }

    pub async fn uninstall_app(&self, bundle: &AppIdentifier) -> Result<()> {
        self.inner.hdc.uninstall(bundle.as_str()).await.map(|_| ())
    }

    pub async fn start_app(&self, bundle: &AppIdentifier, ability: Option<&str>) -> Result<()> {
        let ability = match ability {
            Some(value) => {
                validate_ability(value)?;
                value.to_owned()
            }
            None => self
                .main_ability(bundle)
                .await?
                .ok_or_else(|| DriverError::Protocol("应用没有 main ability".into()))?,
        };
        self.inner
            .hdc
            .shell(format!("aa start -a {ability} -b {}", bundle.as_str()))
            .await
            .map(|_| ())
    }

    pub async fn stop_app(&self, bundle: &AppIdentifier) -> Result<()> {
        self.inner
            .hdc
            .shell(format!("aa force-stop {}", bundle.as_str()))
            .await
            .map(|_| ())
    }

    pub async fn clear_app(&self, bundle: &AppIdentifier) -> Result<()> {
        self.inner
            .hdc
            .shell(format!("bm clean -n {} -c", bundle.as_str()))
            .await?;
        self.inner
            .hdc
            .shell(format!("bm clean -n {} -d", bundle.as_str()))
            .await
            .map(|_| ())
    }

    pub async fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        let output = self
            .inner
            .hdc
            .shell(format!("bm dump -n {}", bundle.as_str()))
            .await?;
        let Some(start) = output.stdout.find('{') else {
            return Ok(None);
        };
        let Some(end) = output.stdout.rfind('}') else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(&output.stdout[start..=end])?;
        Ok(find_string_key(&value, "mainAbility"))
    }

    pub async fn current_app(&self) -> Result<Option<(AppIdentifier, String)>> {
        let output = self.inner.hdc.shell("aa dump -l").await?;
        let bundle_re = Regex::new(r"bundle name \[([A-Za-z0-9_.]+)\]")
            .map_err(|error| DriverError::Protocol(error.to_string()))?;
        let ability_re = Regex::new(r"main name \[([A-Za-z0-9_.]+)\]")
            .map_err(|error| DriverError::Protocol(error.to_string()))?;
        for block in output.stdout.split("Mission ID #") {
            if !block.contains("state #FOREGROUND") {
                continue;
            }
            let bundle = bundle_re.captures(block).and_then(|capture| capture.get(1));
            let ability = ability_re
                .captures(block)
                .and_then(|capture| capture.get(1));
            if let (Some(bundle), Some(ability)) = (bundle, ability) {
                return Ok(Some((
                    AppIdentifier::new(bundle.as_str())?,
                    ability.as_str().to_owned(),
                )));
            }
        }
        Ok(None)
    }

    pub async fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        self.inner
            .hdc
            .send_file(local.as_ref(), remote)
            .await
            .map(|_| ())
    }

    pub async fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        self.inner
            .hdc
            .receive_file(remote, local.as_ref())
            .await
            .map(|_| ())
    }

    /// 显式执行设备端 shell。字符串不会交给主机 shell。
    pub async fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        self.inner.hdc.shell(command).await
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let directory = tempdir()?;
        let local = directory.path().join("screen.bin");
        let remote = format!("/data/local/tmp/hm_driver_{}.png", next_operation_id());
        let first = self
            .inner
            .hdc
            .shell(format!("snapshot_display -f {remote}"))
            .await;
        if first.is_err() {
            self.inner
                .hdc
                .shell(format!("uitest screenCap -p {remote}"))
                .await?;
        }
        let result = async {
            self.inner.hdc.receive_file(&remote, &local).await?;
            tokio::fs::read(&local).await.map_err(DriverError::Io)
        }
        .await;
        let _ = self.inner.hdc.shell(format!("rm -f {remote}")).await;
        result
    }

    pub async fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.screenshot().await?).await?;
        Ok(())
    }

    pub async fn ui_tree(&self) -> Result<UiNode> {
        let directory = tempdir()?;
        let local = directory.path().join("layout.json");
        let remote = format!("/data/local/tmp/hm_driver_{}.json", next_operation_id());
        self.inner
            .hdc
            .shell(format!("uitest dumpLayout -p {remote}"))
            .await?;
        let result = async {
            self.inner.hdc.receive_file(&remote, &local).await?;
            let bytes = tokio::fs::read(&local).await?;
            parse_layout(serde_json::from_slice(&bytes)?)
        }
        .await;
        let _ = self.inner.hdc.shell(format!("rm -f {remote}")).await;
        result
    }

    pub async fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let index = selector.selected_index();
        let references = self.find_remote_references(selector).await?;
        Ok(references.get(index).cloned().map(|reference| {
            Element::new(
                self.clone(),
                selector.clone(),
                index,
                reference,
                self.generation(),
            )
        }))
    }

    pub async fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        let generation = self.generation();
        Ok(self
            .find_remote_references(selector)
            .await?
            .into_iter()
            .enumerate()
            .map(|(index, reference)| {
                Element::new(self.clone(), selector.clone(), index, reference, generation)
            })
            .collect())
    }

    pub async fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(element) = self.find(selector).await? {
                return Ok(element);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DriverError::ElementNotFound);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn xpath(&self, expression: &str) -> Result<XPathElement> {
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)?
            .into_iter()
            .next()
            .ok_or(DriverError::XPathNotFound)
    }

    pub async fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)
    }

    pub async fn xpath_exists(&self, expression: &str) -> Result<bool> {
        Ok(!self.xpath_all(expression).await?.is_empty())
    }

    pub(crate) async fn find_remote_references(&self, selector: &Selector) -> Result<Vec<String>> {
        let selector_reference = selector.build_remote(self).await?;
        let dialect = self.dialect().await?;
        let driver_reference = {
            let state = self.inner.state.lock().await;
            state
                .driver_reference
                .clone()
                .ok_or(DriverError::SessionInvalid)?
        };
        let result = self
            .call_api_raw(
                &format!("{}.findComponents", dialect.driver()),
                Some(&driver_reference),
                json!([selector_reference]),
            )
            .await?;
        self.queue_remote_reference(selector_reference, self.generation());
        match result {
            Value::Null => Ok(Vec::new()),
            Value::String(reference) => Ok(vec![reference]),
            Value::Array(values) => values
                .into_iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        DriverError::Protocol("findComponents 返回了非引用值".into())
                    })
                })
                .collect(),
            _ => Err(DriverError::Protocol("findComponents 响应类型无效".into())),
        }
    }

    pub(crate) fn queue_remote_reference(&self, value: String, generation: u64) {
        if !value.ends_with("#seed") {
            self.inner
                .cleaner
                .lock()
                .expect("清理队列锁中毒")
                .push(QueuedReference { value, generation });
        }
    }

    async fn flush_cleaner(&self, force: bool) -> Result<()> {
        let generation = self.inner.state.lock().await.generation;
        let references = {
            let mut queue = self.inner.cleaner.lock().expect("清理队列锁中毒");
            if !force && queue.len() < self.inner.config.cleaner_batch_size {
                return Ok(());
            }
            let mut current = Vec::new();
            queue.retain(|item| {
                if item.generation == generation {
                    current.push(item.value.clone());
                }
                false
            });
            current
        };
        if references.is_empty() {
            return Ok(());
        }
        match self
            .call_direct("BackendObjectsCleaner.clean", None, json!([references]))
            .await
        {
            Ok(_) => Ok(()),
            Err(DriverError::SessionInvalid | DriverError::RpcTimeout { .. }) => Ok(()),
            Err(error) if force => {
                tracing::debug!(target: "hm_driver_rs::rpc", error = %error, "释放远端引用失败");
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    async fn coordinate_call(&self, method: &str, args: Value) -> Result<()> {
        self.driver_call(method, args).await.map(|_| ())
    }

    async fn driver_call(&self, method: &str, args: Value) -> Result<Value> {
        let (dialect, reference) = {
            let state = self.inner.state.lock().await;
            (
                state.dialect.ok_or(DriverError::SessionInvalid)?,
                state
                    .driver_reference
                    .clone()
                    .ok_or(DriverError::SessionInvalid)?,
            )
        };
        self.call_api_raw(
            &format!("{}.{}", dialect.driver(), method),
            Some(&reference),
            args,
        )
        .await
    }

    async fn absolute_position(&self, position: Position) -> Result<Point> {
        match position {
            Position::Absolute(point) => Ok(point),
            Position::Normalized(point) => {
                let size = self.display_size().await?;
                Ok(Point::new(
                    (point.x * f64::from(size.width)).round() as i32,
                    (point.y * f64::from(size.height)).round() as i32,
                ))
            }
        }
    }

    async fn parameter(&self, name: &str) -> Result<String> {
        let output = self.inner.hdc.shell(format!("param get {name}")).await?;
        Ok(output
            .stdout
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned())
    }

    #[cfg(test)]
    pub(crate) fn with_test_rpc(rpc: RpcClient, dialect: ApiDialect) -> Self {
        let hdc = HdcRunner::new(
            HdcConfig::default()
                .with_path(std::env::current_exe().expect("测试程序路径"))
                .with_server("127.0.0.1", 8710),
        )
        .expect("测试 HDC runner");
        Self {
            inner: Arc::new(HmDriverInner {
                hdc,
                source: AgentSource::Embedded,
                profile: AgentProfile {
                    path: String::new(),
                    file_name: String::new(),
                    size: 0,
                    sha256: String::new(),
                    architecture: "arm64".into(),
                    version: "test".into(),
                    transport: HarmonyTransport::Tcp { remote_port: 8012 },
                    condition: String::new(),
                    compatibility: CompatibilityStatus::OfficialReferenceOnly,
                },
                config: DriverConfig {
                    cleaner_batch_size: usize::MAX,
                    ..DriverConfig::default()
                },
                state: Mutex::new(SessionState {
                    rpc: Some(rpc),
                    dialect: Some(dialect),
                    driver_reference: Some(format!("{}#0", dialect.driver())),
                    local_port: None,
                    generation: 1,
                    closed: false,
                    api_level: Some(9),
                }),
                cleaner: StdMutex::new(Vec::new()),
                generation: AtomicU64::new(1),
            }),
        }
    }
}

async fn probe_device(hdc: &HdcRunner) -> Result<DeviceProbe> {
    let file = hdc.shell("file /system/bin/uitest").await?;
    let architecture = if file.stdout.to_ascii_lowercase().contains("x86-64")
        || file.stdout.to_ascii_lowercase().contains("x86_64")
    {
        "x86_64".to_owned()
    } else if file.stdout.to_ascii_lowercase().contains("aarch64")
        || file.stdout.to_ascii_lowercase().contains("arm64")
        || file.stdout.to_ascii_lowercase().contains("arm")
    {
        "arm64".to_owned()
    } else {
        return Err(DriverError::UnsupportedArchitecture(
            "uitest ELF 架构无法识别".into(),
        ));
    };
    let version_output = hdc.shell("/system/bin/uitest --version").await?;
    let uitest_version = extract_four_part_version(&version_output.stdout)?;
    let api_level = match hdc.shell("param get const.ohos.apiversion").await {
        Ok(output) => output.stdout.trim().parse().ok(),
        Err(_) => None,
    };
    Ok(DeviceProbe {
        architecture,
        uitest_version,
        api_level,
    })
}

fn extract_four_part_version(output: &str) -> Result<String> {
    let regex = Regex::new(r"\d+\.\d+\.\d+\.\d+").map_err(|_| DriverError::InvalidUitestVersion)?;
    let versions: Vec<_> = regex
        .find_iter(output)
        .filter(|matched| {
            let before = output[..matched.start()].chars().next_back();
            let after = output[matched.end()..].chars().next();
            before.is_none_or(|ch| !ch.is_ascii_digit() && ch != '.')
                && after.is_none_or(|ch| !ch.is_ascii_digit() && ch != '.')
        })
        .map(|matched| matched.as_str())
        .collect();
    if versions.len() == 1 {
        versions[0]
            .parse::<crate::agent::UitestVersion>()
            .map(|_| versions[0].to_owned())
    } else {
        Err(DriverError::InvalidUitestVersion)
    }
}

async fn ensure_agent(hdc: &HdcRunner, profile: &AgentProfile, local: &Path) -> Result<()> {
    if remote_agent_matches(hdc, profile).await {
        if daemon_running(hdc).await.unwrap_or(false) {
            return Ok(());
        }
    } else {
        stop_singleness_daemon(hdc).await?;
        let temporary = format!("/data/local/tmp/.hm_driver_{}.so", next_operation_id());
        hdc.send_file(local, &temporary).await?;
        let pushed_hash = hdc
            .shell(format!("sha256sum {temporary}"))
            .await?
            .stdout
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if pushed_hash != profile.sha256 {
            let _ = hdc.shell(format!("rm -f {temporary}")).await;
            return Err(DriverError::AgentVerification(
                "设备端临时 Agent SHA-256 不匹配".into(),
            ));
        }
        hdc.shell(format!(
            "chmod 700 {temporary} && mv {temporary} /data/local/tmp/agent.so && chmod 700 /data/local/tmp/agent.so"
        ))
        .await?;
        hdc.shell(format!(
            "echo {} > /data/local/tmp/.hm_driver_agent.sha256",
            profile.sha256
        ))
        .await?;
    }
    hdc.shell_timeout("uitest start-daemon singleness", hdc.agent_timeout())
        .await?;
    let deadline = tokio::time::Instant::now() + hdc.agent_timeout();
    loop {
        if daemon_running(hdc).await.unwrap_or(false) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(DriverError::AgentStartup(
                "等待 singleness daemon 超时".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn remote_agent_matches(hdc: &HdcRunner, profile: &AgentProfile) -> bool {
    if let Ok(output) = hdc.shell("sha256sum /data/local/tmp/agent.so").await {
        return output
            .stdout
            .split_whitespace()
            .next()
            .is_some_and(|value| value.eq_ignore_ascii_case(&profile.sha256));
    }
    let pulled = async {
        let directory = tempdir().ok()?;
        let local = directory.path().join("agent.so");
        hdc.receive_file("/data/local/tmp/agent.so", &local)
            .await
            .ok()?;
        let bytes = tokio::fs::read(local).await.ok()?;
        Some(hex_sha256(&bytes) == profile.sha256)
    }
    .await;
    if let Some(matches) = pulled {
        return matches;
    }
    tracing::warn!(
        target: "hm_driver_rs::compatibility",
        "设备端缺少 SHA-256 工具且无法拉取 Agent，使用版本标记、大小和 ELF 架构降级校验"
    );
    let marker = hdc
        .shell("cat /data/local/tmp/.hm_driver_agent.sha256")
        .await
        .ok()
        .is_some_and(|output| output.stdout.trim() == profile.sha256);
    let size = hdc
        .shell("stat -c %s /data/local/tmp/agent.so")
        .await
        .ok()
        .and_then(|output| output.stdout.trim().parse::<u64>().ok())
        == Some(profile.size);
    let architecture = hdc
        .shell("file /data/local/tmp/agent.so")
        .await
        .ok()
        .is_some_and(|output| {
            let lower = output.stdout.to_ascii_lowercase();
            if profile.architecture == "x86_64" {
                lower.contains("x86-64") || lower.contains("x86_64")
            } else {
                lower.contains("aarch64") || lower.contains("arm")
            }
        });
    marker && size && architecture
}

async fn establish_session(
    hdc: &HdcRunner,
    transport: &HarmonyTransport,
    config: &DriverConfig,
    api_level: Option<u32>,
) -> Result<EstablishedSession> {
    let remote = match transport {
        HarmonyTransport::Tcp { remote_port } => format!("tcp:{remote_port}"),
        HarmonyTransport::LocalAbstract { socket_name } => format!("localabstract:{socket_name}"),
    };
    let mut last_error = None;
    for _ in 0..3 {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(DriverError::RpcConnect)?;
        let port = listener
            .local_addr()
            .map_err(DriverError::RpcConnect)?
            .port();
        drop(listener);
        if let Err(error) = hdc.forward(port, &remote).await {
            last_error = Some(error);
            let _ = hdc.remove_forward(port).await;
            continue;
        }
        match connect_and_create(port, config, api_level).await {
            Ok((rpc, dialect, driver_reference)) => {
                return Ok(EstablishedSession {
                    rpc,
                    dialect,
                    driver_reference,
                    local_port: port,
                });
            }
            Err(error) => {
                last_error = Some(error);
                let _ = hdc.remove_forward(port).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| DriverError::Forward("重试次数耗尽".into())))
}

async fn connect_and_create(
    port: u16,
    config: &DriverConfig,
    api_level: Option<u32>,
) -> Result<(RpcClient, ApiDialect, String)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let rpc = loop {
        match RpcClient::connect(
            port,
            Duration::from_millis(500),
            config.rpc_timeout,
            config.max_rpc_frame_size,
        )
        .await
        {
            Ok(rpc) => break rpc,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    };
    let preferred = if api_level.is_some_and(|level| level < 9) {
        ApiDialect::Legacy
    } else {
        ApiDialect::Modern
    };
    match create_remote_driver(&rpc, preferred).await {
        Ok(reference) => Ok((rpc, preferred, reference)),
        Err(DriverError::Hypium(message))
            if api_level.is_none() && is_method_not_found(&message) =>
        {
            let legacy = ApiDialect::Legacy;
            let reference = create_remote_driver(&rpc, legacy).await?;
            Ok((rpc, legacy, reference))
        }
        Err(error) => Err(error),
    }
}

async fn create_remote_driver(rpc: &RpcClient, dialect: ApiDialect) -> Result<String> {
    rpc.call(&format!("{}.create", dialect.driver()), None, json!([]))
        .await?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| DriverError::Protocol("Driver.create 未返回远端引用".into()))
}

fn is_method_not_found(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("method") && (message.contains("not found") || message.contains("undefined"))
}

async fn daemon_running(hdc: &HdcRunner) -> Result<bool> {
    Ok(singleness_pids(&hdc.shell("ps -ef").await?.stdout)
        .next()
        .is_some())
}

async fn stop_singleness_daemon(hdc: &HdcRunner) -> Result<()> {
    let output = hdc.shell("ps -ef").await?;
    let pids: Vec<_> = singleness_pids(&output.stdout).collect();
    for pid in pids {
        hdc.shell(format!("kill -9 {pid}")).await?;
    }
    Ok(())
}

fn singleness_pids(output: &str) -> impl Iterator<Item = &str> {
    output.lines().filter_map(|line| {
        if !line.contains("uitest start-daemon singleness") {
            return None;
        }
        let pid = line.split_whitespace().nth(1)?;
        pid.chars().all(|ch| ch.is_ascii_digit()).then_some(pid)
    })
}

fn next_operation_id() -> String {
    let counter = OPERATION_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{timestamp:x}{counter:x}")
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

fn find_string_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key).and_then(Value::as_str) {
                return Some(value.to_owned());
            }
            object
                .values()
                .find_map(|value| find_string_key(value, key))
        }
        Value::Array(values) => values.iter().find_map(|value| find_string_key(value, key)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_one_strict_four_part_version() {
        assert_eq!(
            extract_four_part_version("UITest version 6.0.2.2\n").unwrap(),
            "6.0.2.2"
        );
        assert!(matches!(
            extract_four_part_version("6.0.2"),
            Err(DriverError::InvalidUitestVersion)
        ));
        assert!(matches!(
            extract_four_part_version("6.0.2.2 6.0.2.3"),
            Err(DriverError::InvalidUitestVersion)
        ));
    }

    #[test]
    fn only_matches_exact_singleness_process() {
        let output = "shell 100 1 0 uitest start-daemon singleness\nshell 101 1 0 uitest start-daemon demo\n";
        assert_eq!(singleness_pids(output).collect::<Vec<_>>(), vec!["100"]);
    }
}
