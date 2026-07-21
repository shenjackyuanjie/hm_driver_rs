use crate::types::{
    DeviceDescriptor, DeviceSelector, DeviceSerial, DeviceStatus, ForwardEndpoint, ForwardEntry,
};
use crate::{DriverError, Result};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
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
        let executable = resolve_hdc_path(config.path.as_deref())?;
        let server = match config.server.clone() {
            Some(server) => Some(validate_server(server)?),
            None => server_from_environment()?,
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

    pub async fn discover(&self) -> Result<Vec<DeviceDescriptor>> {
        let output = self
            .run(["list", "targets", "-v"], self.inner.config.command_timeout)
            .await?;
        parse_devices(&output.stdout)
    }

    pub async fn select(&self, selector: &DeviceSelector) -> Result<DeviceDescriptor> {
        let devices = self.discover().await?;
        let online: Vec<_> = devices
            .into_iter()
            .filter(|device| device.status == DeviceStatus::Online)
            .collect();
        match selector {
            DeviceSelector::Auto => match online.len() {
                0 => Err(DriverError::DeviceNotFound),
                1 => Ok(online.into_iter().next().expect("长度已经检查")),
                count => Err(DriverError::AmbiguousDevice { count }),
            },
            DeviceSelector::Serial(expected) => online
                .into_iter()
                .find(|device| device.serial == *expected)
                .ok_or(DriverError::DeviceOffline),
        }
    }

    pub async fn shell(&self, command: impl AsRef<OsStr>) -> Result<CommandOutput> {
        self.run(
            [OsString::from("shell"), command.as_ref().to_owned()],
            self.inner.config.command_timeout,
        )
        .await
    }

    pub async fn shell_timeout(
        &self,
        command: impl AsRef<OsStr>,
        duration: Duration,
    ) -> Result<CommandOutput> {
        self.run(
            [OsString::from("shell"), command.as_ref().to_owned()],
            duration,
        )
        .await
    }

    pub async fn send_file(&self, local: &Path, remote: &str) -> Result<CommandOutput> {
        self.run(
            [
                OsString::from("file"),
                OsString::from("send"),
                local.as_os_str().to_owned(),
                OsString::from(remote),
            ],
            self.inner.config.transfer_timeout,
        )
        .await
    }

    pub async fn receive_file(&self, remote: &str, local: &Path) -> Result<CommandOutput> {
        self.run(
            [
                OsString::from("file"),
                OsString::from("recv"),
                OsString::from(remote),
                local.as_os_str().to_owned(),
            ],
            self.inner.config.transfer_timeout,
        )
        .await
    }

    pub async fn install(&self, package: &Path) -> Result<CommandOutput> {
        self.run(
            [OsString::from("install"), package.as_os_str().to_owned()],
            self.inner.config.transfer_timeout,
        )
        .await
    }

    pub async fn uninstall(&self, bundle: &str) -> Result<CommandOutput> {
        self.run(
            [OsString::from("uninstall"), OsString::from(bundle)],
            self.inner.config.transfer_timeout,
        )
        .await
    }

    pub async fn forward(&self, local_port: u16, remote: &str) -> Result<()> {
        self.run(
            [
                OsString::from("fport"),
                OsString::from(format!("tcp:{local_port}")),
                OsString::from(remote),
            ],
            self.inner.config.command_timeout,
        )
        .await
        .map(|_| ())
        .map_err(|error| DriverError::Forward(error.to_string()))
    }

    pub async fn remove_forward(&self, local_port: u16, remote: &str) -> Result<()> {
        let remote_endpoint = parse_forward_endpoint(remote)?;
        if !self.forward_exists(local_port, &remote_endpoint).await? {
            return Ok(());
        }
        self.run(
            [
                OsString::from("fport"),
                OsString::from("rm"),
                OsString::from(format!("tcp:{local_port}")),
                OsString::from(remote),
            ],
            self.inner.config.command_timeout,
        )
        .await?;
        for _ in 0..3 {
            if !self.forward_exists(local_port, &remote_endpoint).await? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(DriverError::Forward(format!(
            "删除命令完成后映射 tcp:{local_port} -> {remote} 仍然存在"
        )))
    }

    pub async fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        let output = self
            .run(["fport", "ls"], self.inner.config.command_timeout)
            .await?;
        parse_forwards(&output.stdout)
    }

    async fn forward_exists(&self, local_port: u16, remote: &ForwardEndpoint) -> Result<bool> {
        Ok(self.list_forwards().await?.iter().any(|entry| {
            entry.local == ForwardEndpoint::Tcp(local_port) && entry.remote == *remote
        }))
    }

    async fn run<I, S>(&self, arguments: I, duration: Duration) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
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

fn resolve_hdc_path(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return canonical_hdc(path);
    }
    if let Some(path) = env::var_os("HDC_PATH") {
        return canonical_hdc(Path::new(&path));
    }
    let path_value = env::var_os("PATH").ok_or(DriverError::HdcNotFound)?;
    let candidates: &[&str] = if cfg!(windows) {
        &["hdc.exe", "hdc"]
    } else {
        &["hdc"]
    };
    for directory in env::split_paths(&path_value) {
        for candidate in candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return canonical_hdc(&path);
            }
        }
    }
    Err(DriverError::HdcNotFound)
}

fn canonical_hdc(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        return Err(DriverError::InvalidHdcPath(path.to_owned()));
    }
    path.canonicalize().map_err(DriverError::Io)
}

fn server_from_environment() -> Result<Option<(String, u16)>> {
    match (
        env::var("HDC_SERVER_HOST").ok(),
        env::var("HDC_SERVER_PORT").ok(),
    ) {
        (None, None) => Ok(None),
        (Some(host), Some(port)) => {
            let port = port
                .parse()
                .map_err(|_| DriverError::InvalidIdentifier("HDC server 端口".into()))?;
            validate_server((host, port)).map(Some)
        }
        // HDC CLI 支持只设置其中一个变量，此时保留环境并由 CLI 自行解释。
        _ => Ok(None),
    }
}

fn validate_server((host, port): (String, u16)) -> Result<(String, u16)> {
    if host.is_empty()
        || port == 0
        || !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | ':' | '[' | ']'))
    {
        return Err(DriverError::InvalidIdentifier("HDC server 地址".into()));
    }
    Ok((host, port))
}

pub(crate) fn parse_devices(output: &str) -> Result<Vec<DeviceDescriptor>> {
    let mut devices = Vec::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.to_ascii_lowercase().contains("empty") {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        let Some(serial) = parts.first() else {
            continue;
        };
        if serial.eq_ignore_ascii_case("serial") || serial.starts_with('[') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        let status = if lower.contains("offline") {
            DeviceStatus::Offline
        } else if lower.contains("unauthorized") {
            DeviceStatus::Unauthorized
        } else if lower.contains("connected") || lower.contains("online") || parts.len() == 1 {
            DeviceStatus::Online
        } else {
            DeviceStatus::Unknown(parts.get(1).copied().unwrap_or("unknown").to_owned())
        };
        let details = parts
            .iter()
            .skip(1)
            .filter(|value| !value.contains(':') && !value.contains('='))
            .take(4)
            .map(|value| (*value).to_owned())
            .collect();
        devices.push(DeviceDescriptor {
            serial: DeviceSerial::new((*serial).to_owned()),
            status,
            details,
        });
    }
    Ok(devices)
}

pub(crate) fn parse_forwards(output: &str) -> Result<Vec<ForwardEntry>> {
    let endpoint =
        regex::Regex::new(r"^(?:tcp|localabstract|localreserved|localfilesystem|dev|jdwp):\S+$")
            .map_err(|error| DriverError::Protocol(error.to_string()))?;
    let mut result = Vec::new();
    for line in output.lines() {
        let endpoints: Vec<_> = line
            .split_whitespace()
            .filter(|value| endpoint.is_match(value))
            .collect();
        if endpoints.len() >= 2 {
            result.push(ForwardEntry {
                local: parse_forward_endpoint(endpoints[0])?,
                remote: parse_forward_endpoint(endpoints[1])?,
            });
        }
    }
    Ok(result)
}

fn parse_forward_endpoint(value: &str) -> Result<ForwardEndpoint> {
    if let Some(port) = value.strip_prefix("tcp:") {
        return port
            .parse::<u16>()
            .map(ForwardEndpoint::Tcp)
            .map_err(|_| DriverError::Protocol("HDC forward TCP 端口无效".into()));
    }
    if let Some(name) = value.strip_prefix("localabstract:") {
        if name.is_empty() {
            return Err(DriverError::Protocol(
                "HDC forward localabstract 名称为空".into(),
            ));
        }
        return Ok(ForwardEndpoint::LocalAbstract(name.to_owned()));
    }
    Ok(ForwardEndpoint::Other(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_verbose_device_states_without_exposing_serials() {
        let devices = parse_devices("device-alpha Connected\ndevice-beta Offline\n").unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].status, DeviceStatus::Online);
        assert_eq!(devices[1].status, DeviceStatus::Offline);
        assert!(!format!("{:?}", devices).contains("device-alpha"));
    }

    #[test]
    fn empty_output_has_no_devices() {
        assert!(parse_devices("[Empty]\n").unwrap().is_empty());
    }

    #[test]
    fn parses_tcp_and_local_abstract_forwards() {
        let forwards = parse_forwards(
            "tcp:10001 tcp:8012\n<redacted> tcp:10002 localabstract:uitest_socket\n<redacted> tcp:10003 localfilesystem:/data/service.sock [Forward]\n",
        )
        .unwrap();
        assert_eq!(forwards.len(), 3);
        assert_eq!(forwards[0].local, ForwardEndpoint::Tcp(10001));
        assert_eq!(forwards[0].remote, ForwardEndpoint::Tcp(8012));
        assert_eq!(
            forwards[1].remote,
            ForwardEndpoint::LocalAbstract("uitest_socket".into())
        );
        assert_eq!(
            forwards[2].remote,
            ForwardEndpoint::Other("localfilesystem:/data/service.sock".into())
        );
    }
}
