//! Agent 探测、部署以及 RPC 会话建立/恢复逻辑。

use super::{DriverConfig, RemoteFileGuard, next_operation_id, spawn_cleanup};
use crate::agent::{AgentProfile, HarmonyTransport};
use crate::hdc::HdcRunner;
use crate::rpc::{ApiDialect, RpcClient};
use crate::{DriverError, Result};
use regex::Regex;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tracing::{debug, info, trace, warn};

/// 成功建立 RPC 会话后的全部上下文。
pub(super) struct EstablishedSession {
    /// 已连接的 RPC 客户端。
    pub(super) rpc: RpcClient,
    /// RPC API 方言（Legacy / Modern）。
    pub(super) dialect: ApiDialect,
    /// 远端 Driver 对象的引用标识。
    pub(super) driver_reference: String,
    /// 会话持有的端口转发列表。
    pub(super) owned_forwards: Vec<OwnedForward>,
}

/// 已建立的本地-远程端口转发记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OwnedForward {
    /// 本地监听的端口号。
    pub(super) local_port: u16,
    /// 远端转发目标地址（如 `tcp:1234`）。
    pub(super) remote: String,
}

/// 端口转发清理失败的信息。
pub(super) struct ForwardCleanupIssue {
    forward: OwnedForward,
    error: DriverError,
}

/// 设备端探测结果：架构、uitest 版本与 API Level。
pub(super) struct DeviceProbe {
    /// CPU 架构标识（`arm64` / `x86_64`）。
    pub(super) architecture: String,
    /// uitest 工具的版本号（四段式，如 `6.0.2.2`）。
    pub(super) uitest_version: String,
    /// 系统 API Level（从 `const.ohos.apiversion` 获取）。
    pub(super) api_level: Option<u32>,
}

/// 探测设备端架构、uitest 版本与 API Level。
pub(super) async fn probe_device(hdc: &HdcRunner) -> Result<DeviceProbe> {
    info!(target: "hm_driver_rs::session", "探测设备架构和版本");
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
    info!(target: "hm_driver_rs::session", %architecture, %uitest_version, api_level, "设备探测完成");
    Ok(DeviceProbe {
        architecture,
        uitest_version,
        api_level,
    })
}

/// 从 `uitest --version` 输出中提取严格唯一的四段式版本号。
pub(super) fn extract_four_part_version(output: &str) -> Result<String> {
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

/// 确保设备端 Agent 已部署并运行：校验 SHA-256 → 推送/更新 → 启动 singleness daemon。
pub(super) async fn ensure_agent(
    hdc: &HdcRunner,
    profile: &AgentProfile,
    local: &Path,
) -> Result<()> {
    info!(target: "hm_driver_rs::session", agent_version = %profile.version, "部署/启动 Agent");
    if remote_agent_matches(hdc, profile).await {
        if daemon_running(hdc).await.unwrap_or(false) {
            info!(target: "hm_driver_rs::session", "Agent 已就绪，跳过部署");
            return Ok(());
        }
        info!(target: "hm_driver_rs::session", "Agent 文件匹配但 daemon 未运行，重新启动");
    } else {
        stop_singleness_daemon(hdc).await?;
        let temporary = format!("/data/local/tmp/.hm_driver_{}.so", next_operation_id());
        let mut temporary_guard = RemoteFileGuard::new(hdc.clone(), temporary.clone());
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
        temporary_guard.disarm();
    }
    info!(target: "hm_driver_rs::session", "启动 singleness daemon");
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

/// 建立 Hypium RPC 会话：创建端口转发 → 连接 RPC → 创建远端 Driver 对象。
///
/// 内部会重试最多 3 次。
pub(super) async fn establish_session(
    hdc: &HdcRunner,
    transport: &HarmonyTransport,
    config: &DriverConfig,
    api_level: Option<u32>,
) -> Result<EstablishedSession> {
    info!(target: "hm_driver_rs::session", "建立 RPC 会话");
    let remote = transport_endpoint(transport);
    let mut last_error = None;
    let mut owned_forwards = ForwardCleanupGuard::new(hdc.clone(), Vec::new());
    for _ in 0..3 {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(DriverError::RpcConnect)?;
        let port = listener
            .local_addr()
            .map_err(DriverError::RpcConnect)?
            .port();
        drop(listener);
        owned_forwards.forwards.push(OwnedForward {
            local_port: port,
            remote: remote.clone(),
        });
        if let Err(error) = hdc.forward(port, &remote).await {
            let cleanup_issues = cleanup_guard_forwards(&mut owned_forwards).await;
            if !cleanup_issues.is_empty() {
                return Err(forward_cleanup_after_operation(error, cleanup_issues));
            }
            last_error = Some(error);
            continue;
        }
        match connect_and_create(port, config, api_level).await {
            Ok((rpc, dialect, driver_reference)) => {
                info!(target: "hm_driver_rs::session", "RPC 会话建立成功");
                return Ok(EstablishedSession {
                    rpc,
                    dialect,
                    driver_reference,
                    owned_forwards: owned_forwards.take(),
                });
            }
            Err(error) => {
                warn!(target: "hm_driver_rs::session", error = %error, "RPC 会话建立重试");
                let cleanup_issues = cleanup_guard_forwards(&mut owned_forwards).await;
                if !cleanup_issues.is_empty() {
                    return Err(forward_cleanup_after_operation(error, cleanup_issues));
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| DriverError::Forward("重试次数耗尽".into())))
}

/// 清理所有已记录的端口转发，返回清理失败的列表。
pub(super) async fn cleanup_owned_forwards(
    hdc: &HdcRunner,
    owned_forwards: &mut Vec<OwnedForward>,
) -> Vec<ForwardCleanupIssue> {
    debug!(target: "hm_driver_rs::session", "清理端口转发");
    let mut guard = ForwardCleanupGuard::new(hdc.clone(), std::mem::take(owned_forwards));
    let issues = cleanup_guard_forwards(&mut guard).await;
    *owned_forwards = guard.take();
    issues
}

async fn cleanup_guard_forwards(guard: &mut ForwardCleanupGuard) -> Vec<ForwardCleanupIssue> {
    let mut issues = Vec::new();
    let mut index = 0;
    while index < guard.forwards.len() {
        let forward = guard.forwards[index].clone();
        match guard
            .hdc
            .remove_forward(forward.local_port, &forward.remote)
            .await
        {
            Ok(()) => {
                guard.forwards.swap_remove(index);
            }
            Err(error) => {
                issues.push(ForwardCleanupIssue { forward, error });
                index += 1;
            }
        }
    }
    issues
}

struct ForwardCleanupGuard {
    hdc: HdcRunner,
    forwards: Vec<OwnedForward>,
}

impl ForwardCleanupGuard {
    fn new(hdc: HdcRunner, forwards: Vec<OwnedForward>) -> Self {
        Self { hdc, forwards }
    }

    fn take(&mut self) -> Vec<OwnedForward> {
        std::mem::take(&mut self.forwards)
    }
}

impl Drop for ForwardCleanupGuard {
    fn drop(&mut self) {
        let forwards = self.take();
        if forwards.is_empty() {
            return;
        }
        let hdc = self.hdc.clone();
        spawn_cleanup(async move {
            for forward in forwards {
                let _ = hdc
                    .remove_forward(forward.local_port, &forward.remote)
                    .await;
            }
        });
    }
}

/// 将端口转发清理失败列表合并为单个 [`DriverError::ForwardCleanup`]。
pub(super) fn forward_cleanup_error(mut issues: Vec<ForwardCleanupIssue>) -> DriverError {
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
    trace!(target: "hm_driver_rs::session", "检查 daemon 运行状态");
    Ok(singleness_pids(&hdc.shell("ps -ef").await?.stdout)
        .next()
        .is_some())
}

/// 停止设备端所有 singleness daemon 进程。
pub(super) async fn stop_singleness_daemon(hdc: &HdcRunner) -> Result<()> {
    debug!(target: "hm_driver_rs::session", "停止 singleness daemon");
    let output = hdc.shell("ps -ef").await?;
    let pids: Vec<_> = singleness_pids(&output.stdout).collect();
    for pid in pids {
        hdc.shell(format!("kill -9 {pid}")).await?;
    }
    Ok(())
}

/// 从 `ps -ef` 输出中筛选出 singleness daemon 的 PID。
pub(super) fn singleness_pids(output: &str) -> impl Iterator<Item = &str> {
    output.lines().filter_map(|line| {
        if !line.contains("uitest start-daemon singleness") {
            return None;
        }
        let pid = line.split_whitespace().nth(1)?;
        pid.chars().all(|ch| ch.is_ascii_digit()).then_some(pid)
    })
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(result, "{byte:02x}");
    }
    result
}
