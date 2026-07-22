//! HDC 进程封装：配置、进程执行核心与设备/传输/端口转发命令。

mod commands;
mod parse;

use crate::types::DeviceSerial;
use crate::{DriverError, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, trace, warn};

/// HDC 进程配置。
#[derive(Clone, Debug)]
pub struct HdcConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) server: Option<(String, u16)>,
    /// 命令执行的超时时间。
    pub command_timeout: Duration,
    /// 文件传输操作的超时时间。
    pub transfer_timeout: Duration,
    /// 与 HDC agent 通信的超时时间。
    pub agent_timeout: Duration,
}

impl Default for HdcConfig {
    fn default() -> Self {
        Self {
            path: None,
            server: None,
            command_timeout: Duration::from_secs(10),
            transfer_timeout: Duration::from_secs(60),
            agent_timeout: Duration::from_secs(10),
        }
    }
}

impl HdcConfig {
    /// 设置 HDC 可执行文件路径。
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// 设置 HDC server 地址。
    pub fn with_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.server = Some((host.into(), port));
        self
    }

    /// 读取已配置的 HDC 可执行文件路径（未显式设置时为 `None`，将在
    /// `HdcRunner::new` 时从 `HDC_PATH`/`PATH` 自动推导）。
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// 读取已配置的 HDC server 地址（未显式设置时为 `None`，将在
    /// `HdcRunner::new` 时从 `HDC_SERVER_HOST`/`HDC_SERVER_PORT` 自动推导）。
    pub fn server(&self) -> Option<(&str, u16)> {
        self.server
            .as_ref()
            .map(|(host, port)| (host.as_str(), *port))
    }
}

/// HDC 命令的成功输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    /// 命令的标准输出。
    pub stdout: String,
    /// 命令的错误输出。
    pub stderr: String,
    /// 命令的退出状态码。
    pub status: i32,
}

#[derive(Clone)]
pub(crate) struct HdcRunner {
    inner: Arc<HdcRunnerInner>,
}

struct HdcRunnerInner {
    executable: PathBuf,
    server: Option<(String, u16)>,
    serial: Option<DeviceSerial>,
    config: HdcConfig,
}

impl HdcRunner {
    pub fn new(config: HdcConfig) -> Result<Self> {
        let executable = parse::resolve_hdc_path(config.path.as_deref())?;
        let server = match config.server.clone() {
            Some(server) => Some(parse::validate_server(server)?),
            None => parse::server_from_environment()?,
        };
        Ok(Self {
            inner: Arc::new(HdcRunnerInner {
                executable,
                server,
                serial: None,
                config,
            }),
        })
    }

    pub fn with_serial(&self, serial: DeviceSerial) -> Self {
        Self {
            inner: Arc::new(HdcRunnerInner {
                executable: self.inner.executable.clone(),
                server: self.inner.server.clone(),
                serial: Some(serial),
                config: self.inner.config.clone(),
            }),
        }
    }

    pub fn agent_timeout(&self) -> Duration {
        self.inner.config.agent_timeout
    }

    async fn run<I, S>(&self, arguments: I, duration: Duration) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command = Command::new(&self.inner.executable);
        command.kill_on_drop(true);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some((host, port)) = &self.inner.server {
            command.arg("-s").arg(format!("{host}:{port}"));
        }
        if let Some(serial) = &self.inner.serial {
            command.arg("-t").arg(serial.expose_secret());
        }
        command.args(arguments);
        debug!(target: "hm_driver_rs::hdc", "执行 HDC 命令");
        let child = command.spawn().map_err(DriverError::HdcSpawn)?;
        let result = match timeout(duration, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => return Err(DriverError::HdcSpawn(error)),
            Err(_) => {
                warn!(target: "hm_driver_rs::hdc", ?duration, "HDC 命令超时");
                return Err(DriverError::HdcTimeout { timeout: duration });
            }
        };
        let stdout = self.redact(String::from_utf8_lossy(&result.stdout).into_owned());
        let stderr = self.redact(String::from_utf8_lossy(&result.stderr).into_owned());
        trace!(target: "hm_driver_rs::hdc", status = ?result.status.code(), "HDC 命令执行完毕");
        let failed_marker = contains_failure_marker(&stdout) || contains_failure_marker(&stderr);
        if !result.status.success() || failed_marker {
            return Err(DriverError::HdcCommand {
                code: result.status.code(),
                message: command_failure_message(&stdout, &stderr),
            });
        }
        Ok(CommandOutput {
            stdout,
            stderr,
            status: result.status.code().unwrap_or_default(),
        })
    }

    fn redact(&self, value: String) -> String {
        match &self.inner.serial {
            Some(serial) => value.replace(serial.expose_secret(), "<redacted>"),
            None => value,
        }
    }
}

const MAX_ERROR_OUTPUT_CHARS: usize = 4_096;

fn contains_failure_marker(value: &str) -> bool {
    value.lines().any(|line| {
        let line = line.trim_start().to_ascii_lowercase();
        line.starts_with("error:") || line.starts_with("[fail]")
    })
}

fn command_failure_message(stdout: &str, stderr: &str) -> String {
    let mut sections = Vec::new();
    if !stderr.trim().is_empty() {
        sections.push(format!("stderr: {}", stderr.trim()));
    }
    if !stdout.trim().is_empty() {
        sections.push(format!("stdout: {}", stdout.trim()));
    }
    if sections.is_empty() {
        return "HDC 未返回错误文本".into();
    }
    let message = sections.join("; ");
    if message.chars().count() <= MAX_ERROR_OUTPUT_CHARS {
        message
    } else {
        let mut truncated: String = message.chars().take(MAX_ERROR_OUTPUT_CHARS).collect();
        truncated.push_str("...[truncated]");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_markers_only_match_line_prefixes() {
        assert!(contains_failure_marker("Error: device offline"));
        assert!(contains_failure_marker("notice\n  [Fail] command"));
        assert!(!contains_failure_marker("payload contains error: as data"));
    }

    #[test]
    fn failure_output_is_preserved_and_bounded() {
        assert_eq!(
            command_failure_message("bad stdout", "bad stderr"),
            "stderr: bad stderr; stdout: bad stdout"
        );
        let message = command_failure_message(&"x".repeat(5_000), "");
        assert!(message.ends_with("...[truncated]"));
        assert!(message.chars().count() < 4_200);
    }
}
