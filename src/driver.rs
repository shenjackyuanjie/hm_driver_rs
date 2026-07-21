use crate::agent::{
    AgentProfile, AgentResolver, AgentSource, CompatibilityStatus, HarmonyTransport,
    materialize_agent,
};
use crate::gesture::Gesture;
use crate::hdc::{CommandOutput, HdcConfig, HdcRunner};
use crate::keycode::KeyCode;
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
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use url::Url;

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
                    owned_forwards: session.owned_forwards,
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
    owned_forwards: Vec<OwnedForward>,
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
    owned_forwards: Vec<OwnedForward>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct OwnedForward {
    local_port: u16,
    remote: String,
}

struct ForwardCleanupIssue {
    forward: OwnedForward,
    error: DriverError,
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
        state.dialect = None;
        state.driver_reference = None;
        let cleanup_issues =
            cleanup_owned_forwards(&self.inner.hdc, &mut state.owned_forwards).await;
        if !cleanup_issues.is_empty() {
            return Err(forward_cleanup_error(cleanup_issues));
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
        state.owned_forwards = session.owned_forwards;
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
        state.dialect = None;
        state.driver_reference = None;
        let cleanup_issues =
            cleanup_owned_forwards(&self.inner.hdc, &mut state.owned_forwards).await;
        if !cleanup_issues.is_empty() {
            return Err(forward_cleanup_error(cleanup_issues));
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
            wlan_ip: self.wlan_ip().await?,
            display_size: self.display_size().await?,
            display_rotation: self.display_rotation().await?,
        })
    }

    pub async fn screen_on(&self) -> Result<()> {
        self.inner.hdc.shell("power-shell wakeup").await.map(|_| ())
    }

    pub async fn screen_off(&self) -> Result<()> {
        self.press_key_code(KeyCode::Power).await
    }

    pub async fn screen_state(&self) -> Result<ScreenState> {
        let output = self
            .inner
            .hdc
            .shell("hidumper -s PowerManagerService -a -s")
            .await?;
        parse_screen_state(&output.stdout)
    }

    pub async fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        let output = self.inner.hdc.shell("ifconfig").await?;
        parse_wlan_ip(&output.stdout)
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
        self.send_key_code(i32::try_from(key_code).expect("按键码已经限制为 3200 以下"))
            .await
    }

    async fn send_key_code(&self, key_code: i32) -> Result<()> {
        if !(-1..=3200).contains(&key_code) {
            return Err(DriverError::InvalidCoordinate(
                "按键码必须位于 -1 到 3200".into(),
            ));
        }
        self.inner
            .hdc
            .shell(format!("uitest uiInput keyEvent {key_code}"))
            .await
            .map(|_| ())
    }

    pub async fn press_key_code(&self, key_code: KeyCode) -> Result<()> {
        self.send_key_code(key_code.value()).await
    }

    pub async fn go_back(&self) -> Result<()> {
        self.press_key_code(KeyCode::Back).await
    }

    pub async fn go_home(&self) -> Result<()> {
        self.press_key_code(KeyCode::Home).await
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

    pub async fn double_click_position(&self, position: Position) -> Result<()> {
        self.double_click(self.absolute_position(position).await?)
            .await
    }

    pub async fn long_click(&self, point: Point) -> Result<()> {
        self.coordinate_call("longClick", json!([point.x, point.y]))
            .await
    }

    pub async fn long_click_position(&self, position: Position) -> Result<()> {
        self.long_click(self.absolute_position(position).await?)
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

    pub async fn swipe_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        let size = self.display_size().await?;
        self.swipe(from.resolve(size)?, to.resolve(size)?, speed)
            .await
    }

    pub async fn swipe_direction(
        &self,
        direction: SwipeDirection,
        area: SwipeArea,
        scale: f64,
        speed: u32,
    ) -> Result<()> {
        if !scale.is_finite() || !(0.0..=1.0).contains(&scale) || scale == 0.0 {
            return Err(DriverError::InvalidCoordinate(
                "方向滑动比例必须位于 0 到 1 之间".into(),
            ));
        }
        let bounds = area.resolve(self.display_size().await?)?;
        let center = bounds.center();
        let horizontal = (f64::from(bounds.width()) * scale / 2.0).round() as i32;
        let vertical = (f64::from(bounds.height()) * scale / 2.0).round() as i32;
        let (from, to) = match direction {
            SwipeDirection::Up => (
                Point::new(center.x, center.y + vertical),
                Point::new(center.x, center.y - vertical),
            ),
            SwipeDirection::Down => (
                Point::new(center.x, center.y - vertical),
                Point::new(center.x, center.y + vertical),
            ),
            SwipeDirection::Left => (
                Point::new(center.x + horizontal, center.y),
                Point::new(center.x - horizontal, center.y),
            ),
            SwipeDirection::Right => (
                Point::new(center.x - horizontal, center.y),
                Point::new(center.x + horizontal, center.y),
            ),
        };
        self.swipe(from, to, speed).await
    }

    pub async fn perform_gesture(&self, gesture: &Gesture) -> Result<()> {
        let matrix = gesture.compile(self.display_size().await?)?;
        let total_points = matrix.first().map(Vec::len).unwrap_or_default();
        let reference = self
            .call_api_raw(
                "PointerMatrix.create",
                None,
                json!([matrix.len(), total_points]),
            )
            .await?
            .as_str()
            .ok_or_else(|| DriverError::Protocol("PointerMatrix.create 未返回远端引用".into()))?
            .to_owned();
        let result = async {
            for (finger_index, points) in matrix.iter().enumerate() {
                for (point_index, point) in points.iter().enumerate() {
                    self.call_api_raw(
                        "PointerMatrix.setPoint",
                        Some(&reference),
                        json!([
                            finger_index,
                            point_index,
                            {"x": point.encoded_x()?, "y": point.point.y}
                        ]),
                    )
                    .await?;
                }
            }
            self.driver_call(
                "injectMultiPointerAction",
                json!([reference, gesture.injection_speed_value()]),
            )
            .await
            .map(|_| ())
        }
        .await;
        self.queue_remote_reference(reference, self.generation());
        result
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

    pub async fn open_url(&self, value: &str, mode: OpenUrlMode) -> Result<()> {
        let url = Url::parse(value).map_err(|error| DriverError::InvalidUrl(error.to_string()))?;
        if url.scheme().is_empty() {
            return Err(DriverError::InvalidUrl("URL 缺少 scheme".into()));
        }
        let url = shell_quote(url.as_str());
        let command = match mode {
            OpenUrlMode::SystemBrowser => {
                format!("aa start -A ohos.want.action.viewData -e entity.system.browsable -U {url}")
            }
            OpenUrlMode::Default => format!("aa start -U {url}"),
        };
        self.inner.hdc.shell(command).await.map(|_| ())
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

    pub async fn app_info(&self, bundle: &AppIdentifier) -> Result<Value> {
        let output = self
            .inner
            .hdc
            .shell(format!("bm dump -n {}", bundle.as_str()))
            .await?;
        let Some(start) = output.stdout.find('{') else {
            return Err(DriverError::Protocol("应用信息不包含 JSON 对象".into()));
        };
        let Some(end) = output.stdout.rfind('}') else {
            return Err(DriverError::Protocol("应用信息 JSON 不完整".into()));
        };
        serde_json::from_str(&output.stdout[start..=end]).map_err(DriverError::Json)
    }

    pub async fn app_abilities(&self, bundle: &AppIdentifier) -> Result<Vec<AbilityInfo>> {
        Ok(parse_ability_infos(&self.app_info(bundle).await?))
    }

    pub async fn main_ability_info(&self, bundle: &AppIdentifier) -> Result<Option<AbilityInfo>> {
        let value = self.app_info(bundle).await?;
        Ok(select_main_ability(parse_ability_infos(&value)))
    }

    pub async fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        let value = self.app_info(bundle).await?;
        let abilities = parse_ability_infos(&value);
        Ok(select_main_ability(abilities)
            .map(|ability| ability.name)
            .or_else(|| find_string_key(&value, "mainAbility")))
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

    pub async fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        self.inner.hdc.list_forwards().await
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        self.screenshot_with_method(ScreenshotMethod::Auto).await
    }

    pub async fn screenshot_with_method(&self, method: ScreenshotMethod) -> Result<Vec<u8>> {
        let directory = tempdir()?;
        let local = directory.path().join("screen.bin");
        let operation_id = next_operation_id();
        let snapshot_remote = format!("/data/local/tmp/hm_driver_{operation_id}.jpeg");
        let screen_cap_remote = format!("/data/local/tmp/hm_driver_{operation_id}.png");
        match method {
            ScreenshotMethod::Auto => {
                let first = self
                    .capture_screenshot(&snapshot_remote, &local, ScreenshotMethod::SnapshotDisplay)
                    .await;
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {snapshot_remote}"))
                    .await;
                match first {
                    Ok(bytes) => Ok(bytes),
                    Err(_) => {
                        let result = self
                            .capture_screenshot(
                                &screen_cap_remote,
                                &local,
                                ScreenshotMethod::ScreenCap,
                            )
                            .await;
                        let _ = self
                            .inner
                            .hdc
                            .shell(format!("rm -f {screen_cap_remote}"))
                            .await;
                        result
                    }
                }
            }
            ScreenshotMethod::SnapshotDisplay => {
                let result = self
                    .capture_screenshot(&snapshot_remote, &local, ScreenshotMethod::SnapshotDisplay)
                    .await;
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {snapshot_remote}"))
                    .await;
                result
            }
            ScreenshotMethod::ScreenCap => {
                let result = self
                    .capture_screenshot(&screen_cap_remote, &local, ScreenshotMethod::ScreenCap)
                    .await;
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {screen_cap_remote}"))
                    .await;
                result
            }
        }
    }

    pub async fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.screenshot().await?).await?;
        Ok(())
    }

    pub async fn screenshot_to_with_method(
        &self,
        path: impl AsRef<Path>,
        method: ScreenshotMethod,
    ) -> Result<()> {
        tokio::fs::write(path, self.screenshot_with_method(method).await?).await?;
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

    pub async fn exists(&self, selector: &Selector) -> Result<bool> {
        Ok(self.find(selector).await?.is_some())
    }

    pub async fn count(&self, selector: &Selector) -> Result<usize> {
        Ok(self.find_all(selector).await?.len())
    }

    pub async fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        let Some(element) = self.find(selector).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
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
        self.xpath_optional(expression)
            .await?
            .ok_or(DriverError::XPathNotFound)
    }

    pub async fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        let root = self.ui_tree().await?;
        Ok(XPathElement::query(self.clone(), &root, expression)?
            .into_iter()
            .next())
    }

    pub async fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)
    }

    pub async fn xpath_exists(&self, expression: &str) -> Result<bool> {
        Ok(!self.xpath_all(expression).await?.is_empty())
    }

    pub async fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        let Some(element) = self.xpath_optional(expression).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
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
            .call_direct("BackendObjectsCleaner", None, json!(references))
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
        position.resolve(self.display_size().await?)
    }

    async fn capture_screenshot(
        &self,
        remote: &str,
        local: &Path,
        method: ScreenshotMethod,
    ) -> Result<Vec<u8>> {
        let command = match method {
            ScreenshotMethod::SnapshotDisplay => format!("snapshot_display -f {remote}"),
            ScreenshotMethod::ScreenCap => format!("uitest screenCap -p {remote}"),
            ScreenshotMethod::Auto => {
                return Err(DriverError::Protocol(
                    "内部截图方法不能再次使用 Auto".into(),
                ));
            }
        };
        self.inner.hdc.shell(command).await?;
        self.inner.hdc.receive_file(remote, local).await?;
        tokio::fs::read(local).await.map_err(DriverError::Io)
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
                    owned_forwards: Vec::new(),
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
    let remote = transport_endpoint(transport);
    let mut last_error = None;
    let mut owned_forwards = Vec::new();
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
            continue;
        }
        owned_forwards.push(OwnedForward {
            local_port: port,
            remote: remote.clone(),
        });
        match connect_and_create(port, config, api_level).await {
            Ok((rpc, dialect, driver_reference)) => {
                return Ok(EstablishedSession {
                    rpc,
                    dialect,
                    driver_reference,
                    owned_forwards,
                });
            }
            Err(error) => {
                let cleanup_issues = cleanup_owned_forwards(hdc, &mut owned_forwards).await;
                if !cleanup_issues.is_empty() {
                    return Err(forward_cleanup_after_operation(error, cleanup_issues));
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| DriverError::Forward("重试次数耗尽".into())))
}

async fn cleanup_owned_forwards(
    hdc: &HdcRunner,
    owned_forwards: &mut Vec<OwnedForward>,
) -> Vec<ForwardCleanupIssue> {
    let mut retained = Vec::new();
    let mut issues = Vec::new();
    for forward in std::mem::take(owned_forwards) {
        match hdc
            .remove_forward(forward.local_port, &forward.remote)
            .await
        {
            Ok(()) => {}
            Err(error) => {
                retained.push(forward.clone());
                issues.push(ForwardCleanupIssue { forward, error });
            }
        }
    }
    *owned_forwards = retained;
    issues
}

fn forward_cleanup_error(mut issues: Vec<ForwardCleanupIssue>) -> DriverError {
    let additional_failures = issues.len().saturating_sub(1);
    let issue = issues.swap_remove(0);
    DriverError::ForwardCleanup {
        local_port: issue.forward.local_port,
        remote: issue.forward.remote,
        additional_failures,
        source: Box::new(issue.error),
    }
}

fn forward_cleanup_after_operation(
    operation: DriverError,
    mut issues: Vec<ForwardCleanupIssue>,
) -> DriverError {
    let additional_failures = issues.len().saturating_sub(1);
    let issue = issues.swap_remove(0);
    DriverError::ForwardCleanupAfterOperation {
        local_port: issue.forward.local_port,
        remote: issue.forward.remote,
        additional_failures,
        operation: Box::new(operation),
        cleanup: Box::new(issue.error),
    }
}

fn transport_endpoint(transport: &HarmonyTransport) -> String {
    match transport {
        HarmonyTransport::Tcp { remote_port } => format!("tcp:{remote_port}"),
        HarmonyTransport::LocalAbstract { socket_name } => format!("localabstract:{socket_name}"),
    }
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

fn parse_screen_state(output: &str) -> Result<ScreenState> {
    let pattern = Regex::new(r"Current State:\s*([A-Za-z_]+)")
        .map_err(|error| DriverError::Protocol(error.to_string()))?;
    let raw = pattern
        .captures(output)
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str().to_ascii_uppercase())
        .ok_or_else(|| DriverError::Protocol("无法解析屏幕电源状态".into()))?;
    Ok(match raw.as_str() {
        "INACTIVE" => ScreenState::Inactive,
        "SLEEP" => ScreenState::Sleep,
        "AWAKE" => ScreenState::Awake,
        _ => ScreenState::Unknown(raw),
    })
}

fn parse_wlan_ip(output: &str) -> Result<Option<IpAddr>> {
    let address_pattern = Regex::new(r"(?:inet addr:|inet\s+)([0-9A-Fa-f:.]+)")
        .map_err(|error| DriverError::Protocol(error.to_string()))?;
    let normalized = output.replace("\r\n", "\n");
    let preferred = normalized.split("\n\n").find(|block| {
        block
            .lines()
            .next()
            .map(str::trim_start)
            .is_some_and(|line| line.starts_with("wlan") || line.starts_with("wifi"))
    });
    Ok(
        parse_non_loopback_ip(preferred.unwrap_or(output), &address_pattern)
            .or_else(|| preferred.and_then(|_| parse_non_loopback_ip(output, &address_pattern))),
    )
}

fn parse_non_loopback_ip(output: &str, pattern: &Regex) -> Option<IpAddr> {
    pattern
        .captures_iter(output)
        .filter_map(|capture| capture.get(1)?.as_str().parse::<IpAddr>().ok())
        .find(|address| !address.is_loopback() && !address.is_unspecified())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_ability_infos(value: &Value) -> Vec<AbilityInfo> {
    let mut result = Vec::new();
    collect_ability_infos(value, None, &mut result);
    result
}

fn collect_ability_infos(
    value: &Value,
    inherited_main_module: Option<&str>,
    result: &mut Vec<AbilityInfo>,
) {
    let Value::Object(object) = value else {
        if let Value::Array(values) = value {
            for value in values {
                collect_ability_infos(value, inherited_main_module, result);
            }
        }
        return;
    };
    let main_module = object
        .get("mainEntry")
        .and_then(Value::as_str)
        .or(inherited_main_module);
    if let Some(modules) = object.get("hapModuleInfos").and_then(Value::as_array) {
        collect_modules(modules, main_module, result);
    }
    for (key, child) in object {
        if key != "hapModuleInfos" {
            collect_ability_infos(child, main_module, result);
        }
    }
}

fn collect_modules(modules: &[Value], main_module: Option<&str>, result: &mut Vec<AbilityInfo>) {
    for module in modules {
        let module_main_ability = module
            .get("mainAbility")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let module_name = module
            .get("moduleName")
            .or_else(|| module.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(abilities) = module.get("abilityInfos").and_then(Value::as_array) else {
            continue;
        };
        for raw in abilities {
            let Some(name) = raw.get("name").and_then(Value::as_str) else {
                continue;
            };
            let ability_module = raw
                .get("moduleName")
                .and_then(Value::as_str)
                .unwrap_or(module_name);
            result.push(AbilityInfo {
                name: name.to_owned(),
                module_name: ability_module.to_owned(),
                module_main_ability: module_main_ability.clone(),
                main_module: main_module.map(str::to_owned),
                is_launcher: is_launcher_ability(raw),
                raw: raw.clone(),
            });
        }
    }
}

fn is_launcher_ability(value: &Value) -> bool {
    value
        .get("skills")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|skill| skill.get("actions").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .any(|action| action == "action.system.home")
}

fn select_main_ability(mut abilities: Vec<AbilityInfo>) -> Option<AbilityInfo> {
    abilities.sort_by_key(|ability| {
        let mut score = 0_u8;
        if ability.module_main_ability.as_deref() == Some(ability.name.as_str()) {
            score += 1;
        }
        if ability.main_module.as_deref() == Some(ability.module_name.as_str()) {
            score += 1;
        }
        (
            std::cmp::Reverse(ability.is_launcher),
            std::cmp::Reverse(score),
        )
    });
    abilities.into_iter().next()
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
    use crate::{GesturePath, NormalizedPoint};
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as TokioMutex;

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

    #[test]
    fn discovers_and_ranks_all_abilities() {
        let value = json!({
            "mainEntry": "entry",
            "hapModuleInfos": [
                {
                    "moduleName": "feature",
                    "mainAbility": "FeatureAbility",
                    "abilityInfos": [{"name": "FeatureAbility", "moduleName": "feature", "skills": []}]
                },
                {
                    "moduleName": "entry",
                    "mainAbility": "EntryAbility",
                    "abilityInfos": [
                        {"name": "OtherAbility", "moduleName": "entry", "skills": []},
                        {"name": "EntryAbility", "moduleName": "entry", "skills": [{"actions": ["action.system.home"]}]}
                    ]
                }
            ]
        });
        let abilities = parse_ability_infos(&value);
        assert_eq!(abilities.len(), 3);
        let selected = select_main_ability(abilities).unwrap();
        assert_eq!(selected.name, "EntryAbility");
        assert!(selected.is_launcher);
        assert_eq!(selected.raw["moduleName"], "entry");
    }

    #[test]
    fn discovers_abilities_inside_wrapped_result() {
        let value = json!({
            "result": {
                "mainEntry": "entry",
                "hapModuleInfos": [{
                    "moduleName": "entry",
                    "mainAbility": "EntryAbility",
                    "abilityInfos": [
                        {"name": "EntryAbility", "moduleName": "entry", "skills": []},
                        {"name": "ShareAbility", "moduleName": "entry", "skills": []}
                    ]
                }]
            }
        });
        let abilities = parse_ability_infos(&value);
        assert_eq!(abilities.len(), 2);
        assert_eq!(abilities[0].main_module.as_deref(), Some("entry"));
        assert_eq!(abilities[1].name, "ShareAbility");
    }

    #[test]
    fn parses_screen_state_and_non_loopback_ip() {
        assert_eq!(
            parse_screen_state("Current State: AWAKE\n").unwrap(),
            ScreenState::Awake
        );
        assert_eq!(
            parse_wlan_ip("inet addr:127.0.0.1\ninet 192.168.1.20 netmask 255.255.255.0").unwrap(),
            Some("192.168.1.20".parse().unwrap())
        );
        assert_eq!(
            parse_wlan_ip(
                "rmnet0 Link encap:Ethernet\n  inet addr:10.0.0.2\n\nwlan0 Link encap:Ethernet\n  inet addr:192.168.1.20\n"
            )
            .unwrap(),
            Some("192.168.1.20".parse().unwrap())
        );
    }

    #[test]
    fn quotes_device_shell_url_as_one_argument() {
        assert_eq!(
            shell_quote("https://example.com/a'b"),
            "'https://example.com/a'\\''b'"
        );
    }

    #[tokio::test]
    async fn submits_pointer_matrix_before_injecting_gesture() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(TokioMutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Some(line) = lines.next_line().await.unwrap() {
                let request: Value = serde_json::from_str(&line).unwrap();
                let api = request["params"]["api"].as_str().unwrap().to_owned();
                server_calls.lock().await.push(api.clone());
                let result = match api.as_str() {
                    "Driver.getDisplaySize" => json!({"x": 1000, "y": 2000}),
                    "PointerMatrix.create" => json!("PointerMatrix#1"),
                    _ => Value::Null,
                };
                let response = json!({
                    "request_id": request["request_id"],
                    "result": result,
                    "exception": null
                });
                writer
                    .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                    .await
                    .unwrap();
                writer.write_all(b"\n").await.unwrap();
                if api == "Driver.injectMultiPointerAction" {
                    break;
                }
            }
        });
        let rpc = RpcClient::connect(
            port,
            Duration::from_secs(1),
            Duration::from_secs(1),
            1024 * 1024,
        )
        .await
        .unwrap();
        let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
        let path = GesturePath::new(
            Position::Normalized(NormalizedPoint::new(0.2, 0.2).unwrap()),
            Duration::from_millis(50),
        )
        .unwrap()
        .move_to(
            Position::Normalized(NormalizedPoint::new(0.8, 0.8).unwrap()),
            Duration::from_millis(50),
        )
        .unwrap();
        driver.perform_gesture(&Gesture::new(path)).await.unwrap();
        let calls = calls.lock().await;
        assert_eq!(calls[0], "Driver.getDisplaySize");
        assert_eq!(calls[1], "PointerMatrix.create");
        assert_eq!(
            calls
                .iter()
                .filter(|api| api.as_str() == "PointerMatrix.setPoint")
                .count(),
            3
        );
        assert_eq!(calls.last().unwrap(), "Driver.injectMultiPointerAction");
    }
}
