//! 异步 HarmonyOS Driver：连接生命周期与核心状态管理。
//!
//! 具体的能力按领域拆分到子模块中：
//! - [`session`]：Agent 探测、部署与 RPC 会话建立/恢复。
//! - [`device`]：设备与屏幕信息。
//! - [`input`]：点击、滑动、按键与手势注入。
//! - [`app`]：应用安装、启停与信息查询。
//! - [`files`]：文件推拉、原始 shell 与截图。
//! - [`query`]：UI 树、选择器查找与 XPath。

mod app;
mod device;
mod files;
mod input;
mod query;
mod session;

#[cfg(test)]
mod tests;

use crate::agent::{AgentProfile, AgentSource, materialize_agent};
use crate::hdc::{HdcConfig, HdcRunner};
use crate::rpc::{ApiDialect, RpcClient};
use crate::types::DeviceSelector;
use crate::{DriverError, Result};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

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
    owned_forwards: Vec<session::OwnedForward>,
    generation: u64,
    closed: bool,
    api_level: Option<u32>,
}

struct QueuedReference {
    value: String,
    generation: u64,
}

impl HmDriver {
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    /// 使用当前 HDC 配置发现设备，不建立 Agent 会话。
    pub async fn discover_devices(
        config: HdcConfig,
    ) -> Result<Vec<crate::types::DeviceDescriptor>> {
        HdcRunner::new(config)?.discover().await
    }

    pub fn agent_profile(&self) -> &AgentProfile {
        &self.inner.profile
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    /// 返回当前会话协商出的 Hypium API 方言（Modern/Legacy）。
    pub async fn dialect(&self) -> Result<ApiDialect> {
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
            session::cleanup_owned_forwards(&self.inner.hdc, &mut state.owned_forwards).await;
        if !cleanup_issues.is_empty() {
            return Err(session::forward_cleanup_error(cleanup_issues));
        }
        if self.inner.config.kill_daemon_on_close {
            session::stop_singleness_daemon(&self.inner.hdc).await?;
        }
        state.closed = true;
        Ok(())
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

    async fn absolute_position(&self, position: crate::types::Position) -> Result<crate::types::Point> {
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
                    api_level: Some(9),
                }),
                cleaner: StdMutex::new(Vec::new()),
                generation: AtomicU64::new(1),
            }),
        }
    }
}
