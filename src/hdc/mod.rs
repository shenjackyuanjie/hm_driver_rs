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

/// HDC 进程配置。
#[derive(Clone, Debug)]
pub struct HdcConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) server: Option<(String, u16)>,
    pub command_timeout: Duration,
    pub transfer_timeout: Duration,
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
}

/// HDC 命令的成功输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
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
        tracing::debug!(target: "hm_driver_rs::hdc", "执行 HDC 命令");
        let child = command.spawn().map_err(DriverError::HdcSpawn)?;
        let result = timeout(duration, child.wait_with_output())
            .await
            .map_err(|_| DriverError::HdcTimeout { timeout: duration })?
            .map_err(DriverError::HdcSpawn)?;
        let stdout = self.redact(String::from_utf8_lossy(&result.stdout).into_owned());
        let stderr = self.redact(String::from_utf8_lossy(&result.stderr).into_owned());
        let failed_marker = stdout.to_ascii_lowercase().contains("error:")
            || stdout.to_ascii_lowercase().contains("[fail]")
            || stderr.to_ascii_lowercase().contains("error:")
            || stderr.to_ascii_lowercase().contains("[fail]");
        if !result.status.success() || failed_marker {
            return Err(DriverError::HdcCommand {
                code: result.status.code(),
                message: "HDC 输出已隐藏，以避免泄露设备标识".into(),
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
