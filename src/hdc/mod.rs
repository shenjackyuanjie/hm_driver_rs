//! HDC 进程封装：配置、进程执行核心与设备/传输/端口转发命令。
//!
//! [`HdcConfig`] 定义路径、服务地址及分类超时，内部执行器通过参数数组启动进程，
//! 完成超时控制、失败标记检查和设备标识脱敏，返回 [`CommandOutput`]。
//! 子模块分别负责命令构造和输出解析，Driver 复用已绑定目标设备的执行器。

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
use tracing::{debug, warn};

/// HDC 进程配置。
///
/// 路径优先级：显式设置 → `HDC_PATH` → `PATH`；发现或连接时固定为绝对路径。
/// 服务地址优先使用显式设置，否则读取成对的 `HDC_SERVER_HOST` / `HDC_SERVER_PORT`。
/// 只设置一个服务环境变量时交由 HDC 自身解释，全部未设置时使用默认服务。
#[derive(Clone, Debug)]
pub struct HdcConfig {
    pub(crate) path: Option<PathBuf>,
    pub(crate) server: Option<(String, u16)>,
    /// 单次普通 HDC 命令的超时时间，默认 10 秒。
    pub command_timeout: Duration,
    /// 单次文件传输及应用安装/卸载的超时时间，默认 60 秒。
    pub transfer_timeout: Duration,
    /// singleness daemon 启动命令及启动检查的超时时间，默认 10 秒。
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
    ///
    /// 发现或连接时验证地址和端口；主机名须非空，端口须大于零，
    /// 非法值返回 [`DriverError::InvalidIdentifier`]。
    pub fn with_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.server = Some((host.into(), port));
        self
    }

    /// 读取显式配置的 HDC 路径；未设置时为 `None`。
    ///
    /// 环境变量和 `PATH` 在设备发现/连接时解析，不写回本配置。
    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    /// 读取显式配置的 HDC 服务地址；未设置时为 `None`。
    ///
    /// `HDC_SERVER_HOST` / `HDC_SERVER_PORT` 在设备发现/连接时解析。
    pub fn server(&self) -> Option<(&str, u16)> {
        self.server
            .as_ref()
            .map(|(host, port)| (host.as_str(), *port))
    }
}

/// HDC 命令的成功输出。
///
/// 标准输出和错误输出按 UTF-8 有损解码，并将当前设备序列号替换为脱敏值。
/// 非零退出码或识别到 HDC 失败标记时，命令入口返回 [`DriverError::HdcCommand`]。
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
    #[cfg(test)]
    handler: Option<Arc<TestHandler>>,
}

#[cfg(test)]
type TestHandler = dyn Fn(&[std::ffi::OsString]) -> Result<CommandOutput> + Send + Sync;

impl HdcRunner {
    #[cfg(test)]
    pub(crate) fn with_test_handler(
        handler: impl Fn(&[std::ffi::OsString]) -> Result<CommandOutput> + Send + Sync + 'static,
    ) -> Self {
        let mut runner = Self::new(
            HdcConfig::default()
                .with_path(std::env::current_exe().expect("测试程序路径"))
                .with_server("127.0.0.1", 8710),
        )
        .expect("测试 HDC runner");
        Arc::get_mut(&mut runner.inner).unwrap().handler = Some(Arc::new(handler));
        runner
    }

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
                #[cfg(test)]
                handler: None,
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
                #[cfg(test)]
                handler: self.inner.handler.clone(),
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
        self.run_with_stdout_prefix(arguments, duration, None).await
    }

    async fn run_with_stdout_prefix<I, S>(
        &self,
        arguments: I,
        duration: Duration,
        data_prefix: Option<&str>,
    ) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        #[cfg(test)]
        let arguments: Vec<std::ffi::OsString> = arguments
            .into_iter()
            .map(|argument| argument.as_ref().to_owned())
            .collect();
        #[cfg(test)]
        if let Some(handler) = &self.inner.handler {
            return handler(&arguments);
        }
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
        debug!(target: "hm_driver_rs::hdc", command = ?command, "执行 HDC 命令");
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
        let failed_marker = contains_stdout_failure_marker(&stdout, data_prefix)
            || contains_failure_marker(&stderr);
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

fn contains_stdout_failure_marker(value: &str, data_prefix: Option<&str>) -> bool {
    // 有明确数据前缀的回显之后是用户内容，不能将其中的 Error: 等文本当作命令失败。
    // HDC 非零退出码与 stderr 错误仍由运行器独立检查。
    !data_prefix.is_some_and(|prefix| value.starts_with(prefix)) && contains_failure_marker(value)
}

fn contains_failure_marker(value: &str) -> bool {
    value.lines().any(|line| {
        let line = line.trim_start().to_ascii_lowercase();
        line.starts_with("error:")
            || line.starts_with("[fail]")
            || line.contains("msg:error:")
            || line.contains("failed to install")
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
    fn failure_markers_match_prefixes_and_embedded_errors() {
        assert!(contains_failure_marker("Error: device offline"));
        assert!(contains_failure_marker("notice\n  [Fail] command"));
        assert!(!contains_failure_marker("payload contains error: as data"));
        assert!(contains_failure_marker(
            "[Info]App install path:x.app msg:error: failed to install bundle. code:9568448 error: verify app signature failed."
        ));
    }

    #[test]
    fn framed_text_does_not_treat_clipboard_content_as_command_errors() {
        let output = "Pasteboard text: first line\nError: copied log\n[Fail] copied log\n";
        assert!(contains_stdout_failure_marker(output, None));
        assert!(!contains_stdout_failure_marker(
            output,
            Some("Pasteboard text:")
        ));
        assert!(contains_stdout_failure_marker(
            "Error: testhelper unavailable",
            Some("Pasteboard text:")
        ));
        assert!(contains_failure_marker("Error: device offline"));
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
