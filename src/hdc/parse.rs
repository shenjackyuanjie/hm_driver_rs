//! HDC 可执行文件解析、server 环境变量与命令输出解析。

use crate::types::{DeviceDescriptor, DeviceSerial, DeviceStatus, ForwardEndpoint, ForwardEntry};
use crate::{DriverError, Result};
use std::env;
use std::path::{Path, PathBuf};

pub(super) fn resolve_hdc_path(explicit: Option<&Path>) -> Result<PathBuf> {
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

pub(super) fn server_from_environment() -> Result<Option<(String, u16)>> {
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

pub(super) fn validate_server((host, port): (String, u16)) -> Result<(String, u16)> {
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

pub(super) fn parse_devices(output: &str) -> Result<Vec<DeviceDescriptor>> {
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

pub(super) fn parse_forwards(output: &str) -> Result<Vec<ForwardEntry>> {
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

pub(super) fn parse_forward_endpoint(value: &str) -> Result<ForwardEndpoint> {
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
