//! 设备发现、shell/文件传输与端口转发命令。

use super::{CommandOutput, HdcRunner};
use crate::types::{DeviceDescriptor, DeviceSelector, DeviceStatus, ForwardEndpoint, ForwardEntry};
use crate::{DriverError, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;
use tracing::debug;

impl HdcRunner {
    pub async fn discover(&self) -> Result<Vec<DeviceDescriptor>> {
        debug!(target: "hm_driver_rs::hdc::commands", "发现设备");
        let output = self
            .run(["list", "targets", "-v"], self.inner.config.command_timeout)
            .await?;
        super::parse::parse_devices(&output.stdout)
    }

    pub async fn select(&self, selector: &DeviceSelector) -> Result<DeviceDescriptor> {
        debug!(target: "hm_driver_rs::hdc::commands", ?selector, "选择设备");
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
        debug!(target: "hm_driver_rs::hdc::commands", local = %local.display(), remote, "发送文件");
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
        debug!(target: "hm_driver_rs::hdc::commands", remote, local = %local.display(), "接收文件");
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
        debug!(target: "hm_driver_rs::hdc::commands", package = %package.display(), "安装");
        self.run(
            [OsString::from("install"), package.as_os_str().to_owned()],
            self.inner.config.transfer_timeout,
        )
        .await
    }

    pub async fn uninstall(&self, bundle: &str) -> Result<CommandOutput> {
        debug!(target: "hm_driver_rs::hdc::commands", bundle, "uninstall");
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
        let remote_endpoint = super::parse::parse_forward_endpoint(remote)?;
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
        super::parse::parse_forwards(&output.stdout)
    }

    async fn forward_exists(&self, local_port: u16, remote: &ForwardEndpoint) -> Result<bool> {
        Ok(self.list_forwards().await?.iter().any(|entry| {
            entry.local == ForwardEndpoint::Tcp(local_port) && entry.remote == *remote
        }))
    }
}
