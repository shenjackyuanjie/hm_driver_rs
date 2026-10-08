//! 异步 HarmonyOS Driver：连接生命周期与核心状态管理。
//!
//! 具体的能力按领域拆分到子模块中：
//! - [`session`]：Agent 探测、部署与 RPC 会话建立/恢复。
//! - [`device`]：设备与屏幕信息。
//! - [`input`]：点击、滑动、按键与手势注入。
//! - [`app`]：应用安装、启停与信息查询。
//! - [`files`]：文件推拉、原始 shell 与截图。
//! - [`query`]：UI 树、选择器查找与 XPath。
//! - [`events`]：一次性 Toast/UI 事件监听。
//! - [`window`]：窗口定位与当前窗口尺寸。
//! - [`system`]：剪贴板、显示模式、时间/时区、字体与网络模拟。

mod app;
mod device;
mod events;
mod files;
mod input;
mod query;
mod session;
mod system;
mod window;

#[cfg(test)]
mod tests;

use crate::agent::{AgentProfile, AgentSource, materialize_agent};
use crate::hdc::{HdcConfig, HdcRunner};
use crate::rpc::{ApiDialect, RpcClient};
use crate::types::DeviceSelector;
use crate::{DriverError, Result};
use serde_json::{Value, json};
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::{debug, info, trace, warn};

static OPERATION_ID: AtomicU64 = AtomicU64::new(1);

fn next_operation_id() -> String {
    let counter = OPERATION_ID.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{timestamp:x}{counter:x}")
}

/// Driver 的运行时配置。
#[derive(Clone, Debug)]
pub struct DriverConfig {
    /// 单次 RPC 请求写入与响应读取的超时时间（默认 20 秒），不含等待其他请求释放连接的时间。
    pub rpc_timeout: Duration,
    /// RPC 帧的最大字节数（默认 8 MiB）。
    pub max_rpc_frame_size: usize,
    /// 关闭 Driver 时是否同时杀死设备端的 singleness daemon。
    pub kill_daemon_on_close: bool,
    /// 批量释放远端引用的队列阈值（默认 20）；在后续 RPC 前达到阈值时触发清理。
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
///
/// 默认自动选择唯一在线设备，使用 [`HdcConfig::default`]、[`DriverConfig::default`]
/// 和 [`AgentSource::Embedded`]。链式设置按调用顺序生效。
#[derive(Clone, Debug, Default)]
pub struct HmDriverBuilder {
    /// 目标设备选择器（自动选择 / 指定序列号）。
    selector: DeviceSelector,
    /// HDC 连接配置（路径 / 服务地址）。
    hdc: HdcConfig,
    /// 官方 Agent 动态库来源（内嵌资源 / 外部目录）。
    agent_source: AgentSource,
    /// 驱动运行时配置（超时、帧大小等）。
    config: DriverConfig,
}

impl HmDriverBuilder {
    /// 设置目标设备选择器。
    ///
    /// `Auto` 要求恰好一台在线设备；多设备时用 [`DeviceSelector::Serial`] 指定目标。
    pub fn device(mut self, selector: DeviceSelector) -> Self {
        self.selector = selector;
        self
    }

    /// 设置 hdc 可执行文件的路径。
    ///
    /// 优先于 `HDC_PATH` 和 `PATH`；连接时解析并固定可执行文件绝对路径。
    pub fn hdc_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.hdc.path = Some(path.into());
        self
    }

    /// 设置 hdc server 的地址和端口。
    ///
    /// 优先于 `HDC_SERVER_HOST` / `HDC_SERVER_PORT`；成对变量解析为显式服务地址，
    /// 只有单个变量时交由 HDC 自身解释。连接时校验地址，端口须大于零。
    pub fn hdc_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.hdc.server = Some((host.into(), port));
        self
    }

    /// 直接使用完整的 HDC 配置。
    ///
    /// 替换此前的全部 HDC 设置，之后的 `hdc_path` / `hdc_server` 继续覆盖对应字段。
    pub fn hdc_config(mut self, config: HdcConfig) -> Self {
        self.hdc = config;
        self
    }

    /// 设置官方 Agent 动态库来源（内嵌资源或外部目录）。
    ///
    /// 外部目录内文件须与 catalog 的文件名、大小和 SHA-256 一致，连接时进行校验。
    pub fn agent_source(mut self, source: AgentSource) -> Self {
        self.agent_source = source;
        self
    }

    /// 设置 Driver 运行时配置。
    pub fn driver_config(mut self, config: DriverConfig) -> Self {
        self.config = config;
        self
    }

    /// 连接设备并建立 Hypium RPC 会话。
    ///
    /// 内部流程：发现设备 → 探测架构/版本 → 推送 Agent → 建立端口转发 → 创建远端 Driver。
    ///
    /// 在 Tokio runtime 中执行。无在线设备返回 [`DriverError::DeviceNotFound`]，
    /// 自动选择遇到多台设备返回 [`DriverError::AmbiguousDevice`]，指定设备未在线返回
    /// [`DriverError::DeviceOffline`]。其余错误包括 HDC 配置、架构/版本解析、Agent 文件
    /// 校验、启动、转发及 RPC 初始化失败。
    pub async fn connect(self) -> Result<HmDriver> {
        info!(target: "hm_driver_rs::driver", "开始连接设备");
        let discovery = HdcRunner::new(self.hdc)?;
        let descriptor = discovery.select(&self.selector).await?;
        let hdc = discovery.with_serial(descriptor.serial.clone());
        let probe = session::probe_device(&hdc).await?;
        let profile = crate::agent::AgentResolver::new()?
            .resolve(&probe.architecture, &probe.uitest_version)?;
        if profile.compatibility == crate::agent::CompatibilityStatus::OfficialReferenceOnly {
            tracing::warn!(
                target: "hm_driver_rs::compatibility",
                agent_version = %profile.version,
                "所选 Agent 分支仅有官方参考验证，尚未完成本地真机验证"
            );
        }
        let agent_path = materialize_agent(&self.agent_source, &profile).await?;
        session::ensure_agent(&hdc, &profile, &agent_path).await?;
        let session =
            session::establish_session(&hdc, &profile.transport, &self.config, probe.api_level)
                .await?;
        info!(target: "hm_driver_rs::driver", "设备连接成功");
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
                ui_event_listening: AtomicBool::new(false),
            }),
        })
    }
}

/// 一个设备上的异步 HarmonyOS Driver。
///
/// `Clone` 共享同一 RPC 会话、引用清理队列与 UI 事件监听状态。
/// RPC 请求串行执行；在途请求超时、取消或连接断开后，通过 [`recover`](Self::recover)
/// 显式重建会话，再由调用方决定后续操作。
///
/// 控件和窗口是远端句柄，UI 树和 XPath 属性是采集时的主机快照。
/// 用完句柄后调用 [`close`](Self::close) 等待清理；最后一个引用释放时会安排后台清理。
/// HDC 命令采用 [`HdcConfig`] 的超时，RPC 默认采用 [`DriverConfig::rpc_timeout`]。
///
/// # 错误与设备要求
///
/// 参数错误在对应方法说明中列出；设备命令失败返回 [`DriverError::HdcCommand`]，
/// Agent API 异常返回 [`DriverError::Hypium`]，响应格式不符返回 [`DriverError::Protocol`]。
/// RPC 超时返回 [`DriverError::RpcTimeout`]，失效会话返回 [`DriverError::SessionInvalid`]。
/// 已知 API Level 不足时返回 [`DriverError::Unsupported`]；版本未知时直接尝试设备能力。
/// HDC 文件、shell 和应用辅助操作直接使用设备连接。
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
    ui_event_listening: AtomicBool,
}

struct SessionState {
    rpc: Option<RpcClient>,
    dialect: Option<ApiDialect>,
    driver_reference: Option<String>,
    owned_forwards: Vec<session::OwnedForward>,
    generation: u64,
    closed: bool,
    api_level: Option<u32>,
}

struct QueuedReference {
    value: String,
    generation: u64,
}

pub(super) struct RemoteFileGuard {
    hdc: HdcRunner,
    path: Option<String>,
}

impl RemoteFileGuard {
    pub(super) fn new(hdc: HdcRunner, path: String) -> Self {
        Self {
            hdc,
            path: Some(path),
        }
    }

    pub(super) fn disarm(&mut self) {
        self.path = None;
    }

    pub(super) async fn cleanup(mut self) {
        if let Some(path) = self.path.as_deref() {
            let _ = self.hdc.shell(format!("rm -f {path}")).await;
        }
        self.disarm();
    }
}

impl Drop for RemoteFileGuard {
    fn drop(&mut self) {
        let Some(path) = self.path.take() else {
            return;
        };
        let hdc = self.hdc.clone();
        spawn_cleanup(async move {
            let _ = hdc.shell(format!("rm -f {path}")).await;
        });
    }
}

pub(super) fn spawn_cleanup<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    debug!(target: "hm_driver_rs::driver", "生成后台清理任务");
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(future);
    } else {
        let _ = std::thread::Builder::new()
            .name("hm-driver-cleanup".into())
            .spawn(move || {
                if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    runtime.block_on(future);
                }
            });
    }
}

impl Drop for HmDriverInner {
    fn drop(&mut self) {
        debug!(target: "hm_driver_rs::driver", "HmDriverInner 释放");
        let Ok(mut state) = self.state.try_lock() else {
            tracing::warn!(target: "hm_driver_rs::cleanup", "Driver 释放时会话仍被占用，无法执行兜底清理");
            return;
        };
        if state.closed {
            return;
        }
        let rpc = state.rpc.take();
        let forwards = std::mem::take(&mut state.owned_forwards);
        let references: Vec<_> = self
            .cleaner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .drain(..)
            .map(|item| item.value)
            .collect();
        let hdc = self.hdc.clone();
        let kill_daemon = self.config.kill_daemon_on_close;
        spawn_cleanup(async move {
            if let Some(rpc) = rpc {
                if !references.is_empty() && rpc.is_valid() {
                    let _ = rpc
                        .call("BackendObjectsCleaner", None, json!(references))
                        .await;
                }
                rpc.invalidate();
            }
            let mut forwards = forwards;
            let _ = session::cleanup_owned_forwards(&hdc, &mut forwards).await;
            if kill_daemon {
                let _ = session::stop_singleness_daemon(&hdc).await;
            }
        });
    }
}

impl HmDriver {
    /// 创建一个新的 [`HmDriverBuilder`]。
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    /// 使用当前 HDC 配置发现设备，不建立 Agent 会话。
    ///
    /// 返回 HDC 报告的设备列表（含非在线状态）；无设备时返回空列表。HDC 配置和命令错误直接返回。
    pub async fn discover_devices(
        config: HdcConfig,
    ) -> Result<Vec<crate::types::DeviceDescriptor>> {
        HdcRunner::new(config)?.discover().await
    }

    /// 返回当前使用的 Agent 版本、架构、文件校验和传输信息。
    pub fn agent_profile(&self) -> &AgentProfile {
        &self.inner.profile
    }

    /// 返回当前会话的代际编号，用于区分远端引用归属的会话。
    ///
    /// 初始为 `1`，每次成功 [`recover`](Self::recover) 后递增。
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    /// 返回当前会话协商出的 Hypium API 方言（Modern/Legacy）。
    ///
    /// 会话未建立或已关闭时返回 [`DriverError::SessionInvalid`]。
    pub async fn dialect(&self) -> Result<ApiDialect> {
        self.inner
            .state
            .lock()
            .await
            .dialect
            .ok_or(DriverError::SessionInvalid)
    }

    pub(crate) async fn api_level(&self) -> Result<Option<u32>> {
        let state = self.inner.state.lock().await;
        if state.closed {
            return Err(DriverError::DriverClosed);
        }
        Ok(state.api_level)
    }

    pub(crate) async fn require_api_level(&self, required: u32, capability: &str) -> Result<()> {
        if let Some(level) = self.api_level().await?
            && level < required
        {
            return Err(DriverError::Unsupported(format!(
                "{capability} 需要 API Level {required}，当前为 {level}"
            )));
        }
        Ok(())
    }

    /// 直接调用任意 Hypium RPC API。
    ///
    /// `api` 为完整方法名（如 `"Driver.click"`），`this` 为可选的远端对象引用，
    /// `args` 为 JSON 参数数组。适用于当前能力未覆盖的高级场景。
    ///
    /// 返回未经类型转换的 JSON 结果。调用方管理自行创建的远端引用；RPC 错误按原类型返回。
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
        trace!(target: "hm_driver_rs::driver", api, ?this, "调用原始 API");
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

    /// 恢复已断开的会话，重新部署 Agent、建立转发并创建远端 Driver。
    ///
    /// 恢复成功后会话代际递增，重置 UI 事件监听状态。已有 [`Element`](crate::Element)
    /// 和 [`UiWindow`](crate::UiWindow) 在下一次操作时按原条件及索引重新定位；
    /// [`XPathElement`](crate::XPathElement) 保留原快照，需要重新查询以取得当前坐标。
    /// 恢复失败时返回部署、转发清理或 RPC 错误，调用方可再次恢复；已关闭的 Driver
    /// 返回 [`DriverError::DriverClosed`]。操作由调用方安排重试。
    pub async fn recover(&self) -> Result<()> {
        warn!(target: "hm_driver_rs::driver", "开始恢复会话");
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
            session::cleanup_owned_forwards(&self.inner.hdc, &mut state.owned_forwards).await;
        if !cleanup_issues.is_empty() {
            return Err(session::forward_cleanup_error(cleanup_issues));
        }
        self.inner.cleaner.lock().expect("清理队列锁中毒").clear();
        let path = materialize_agent(&self.inner.source, &self.inner.profile).await?;
        session::ensure_agent(&self.inner.hdc, &self.inner.profile, &path).await?;
        let session = session::establish_session(
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
        self.inner
            .ui_event_listening
            .store(false, Ordering::Release);
        info!(target: "hm_driver_rs::driver", "会话恢复成功");
        Ok(())
    }

    /// 主动关闭共享会话并等待资源清理。
    ///
    /// 先处理已排队的远端引用，再关闭 RPC、移除驱动创建的转发；
    /// [`DriverConfig::kill_daemon_on_close`] 为 `true` 时还会停止 singleness daemon。
    /// 自定义 [`forward`](Self::forward) 由调用方移除，系统设置和网络场景由调用方恢复。
    /// 清理错误会返回给调用方，会话仍标记为关闭；再次关闭返回 `Ok(())`。
    /// 共享此会话的克隆及远端句柄同时结束使用，后续 RPC 根据入口返回
    /// [`DriverError::DriverClosed`] 或 [`DriverError::SessionInvalid`]。
    pub async fn close(&self) -> Result<()> {
        debug!(target: "hm_driver_rs::driver", "关闭会话");
        let cleaner_error = self.flush_cleaner(true).await.err();
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
            session::cleanup_owned_forwards(&self.inner.hdc, &mut state.owned_forwards).await;
        let forward_error =
            (!cleanup_issues.is_empty()).then(|| session::forward_cleanup_error(cleanup_issues));
        let daemon_error = if self.inner.config.kill_daemon_on_close {
            session::stop_singleness_daemon(&self.inner.hdc).await.err()
        } else {
            None
        };
        state.closed = true;
        match cleaner_error.or(forward_error).or(daemon_error) {
            Some(error) => Err(error),
            None => Ok(()),
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
        trace!(target: "hm_driver_rs::driver", method, "Driver API 调用");
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

    async fn driver_call_with_timeout(
        &self,
        method: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Value> {
        trace!(target: "hm_driver_rs::driver", method, ?timeout, "Driver API 调用（带超时）");
        self.flush_cleaner(false).await?;
        let (rpc, dialect, reference) = {
            let state = self.inner.state.lock().await;
            if state.closed {
                return Err(DriverError::DriverClosed);
            }
            (
                state.rpc.clone().ok_or(DriverError::SessionInvalid)?,
                state.dialect.ok_or(DriverError::SessionInvalid)?,
                state
                    .driver_reference
                    .clone()
                    .ok_or(DriverError::SessionInvalid)?,
            )
        };
        rpc.call_with_timeout(
            &format!("{}.{}", dialect.driver(), method),
            Some(&reference),
            args,
            timeout,
        )
        .await
    }

    async fn absolute_position(
        &self,
        position: crate::types::Position,
    ) -> Result<crate::types::Point> {
        position.resolve(self.display_size().await?)
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
    pub(crate) fn queued_reference_count(&self) -> usize {
        self.inner.cleaner.lock().expect("清理队列锁中毒").len()
    }

    #[cfg(test)]
    pub(crate) fn with_test_rpc(rpc: RpcClient, dialect: ApiDialect) -> Self {
        Self::with_test_rpc_api_level(rpc, dialect, Some(9))
    }

    #[cfg(test)]
    pub(crate) fn with_test_rpc_api_level(
        rpc: RpcClient,
        dialect: ApiDialect,
        api_level: Option<u32>,
    ) -> Self {
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
                    transport: crate::agent::HarmonyTransport::Tcp { remote_port: 8012 },
                    condition: String::new(),
                    compatibility: crate::agent::CompatibilityStatus::OfficialReferenceOnly,
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
                    api_level,
                }),
                cleaner: StdMutex::new(Vec::new()),
                generation: AtomicU64::new(1),
                ui_event_listening: AtomicBool::new(false),
            }),
        }
    }
}
