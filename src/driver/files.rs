//! 文件推拉、原始 shell 执行与截图。

use super::{HmDriver, RemoteFileGuard, next_operation_id};
use crate::hdc::CommandOutput;
use crate::types::{ForwardEntry, ScreenshotMethod};
use crate::{DriverError, Result};
use std::path::Path;
use tempfile::tempdir;
use tracing::{debug, trace};

impl HmDriver {
    /// 将本地文件发送到设备端。
    pub async fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        debug!(target: "hm_driver_rs::files", local = %local.as_ref().display(), remote, "推送文件");
        self.inner
            .hdc
            .send_file(local.as_ref(), remote)
            .await
            .map(|_| ())
    }

    /// 从设备端拉取文件到本地。
    pub async fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        debug!(target: "hm_driver_rs::files", remote, local = %local.as_ref().display(), "拉取文件");
        self.inner
            .hdc
            .receive_file(remote, local.as_ref())
            .await
            .map(|_| ())
    }

    /// 显式执行设备端 shell。字符串不会交给主机 shell。
    pub async fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        trace!(target: "hm_driver_rs::files", command, "原始 Shell 命令");
        self.inner.hdc.shell(command).await
    }

    /// 列出设备端所有已建立的端口转发规则。
    pub async fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        self.inner.hdc.list_forwards().await
    }

    /// 建立一个自定义端口转发，与驱动自身使用的 RPC 转发互不影响。
    pub async fn forward(&self, local_port: u16, remote: &str) -> Result<()> {
        self.inner.hdc.forward(local_port, remote).await
    }

    /// 移除一个自定义端口转发。
    pub async fn remove_forward(&self, local_port: u16, remote: &str) -> Result<()> {
        self.inner.hdc.remove_forward(local_port, remote).await
    }

    /// 截取当前屏幕（自动选择可用方式），返回 JPEG/PNG 字节。
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        debug!(target: "hm_driver_rs::files", "截取屏幕");
        self.screenshot_with_method(ScreenshotMethod::Auto).await
    }

    /// 使用指定的截图方式截取当前屏幕。
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
                match first {
                    Ok(bytes) => Ok(bytes),
                    Err(_) => {
                        self.capture_screenshot(
                            &screen_cap_remote,
                            &local,
                            ScreenshotMethod::ScreenCap,
                        )
                        .await
                    }
                }
            }
            ScreenshotMethod::SnapshotDisplay => {
                self.capture_screenshot(&snapshot_remote, &local, ScreenshotMethod::SnapshotDisplay)
                    .await
            }
            ScreenshotMethod::ScreenCap => {
                self.capture_screenshot(&screen_cap_remote, &local, ScreenshotMethod::ScreenCap)
                    .await
            }
        }
    }

    /// 截取屏幕并直接保存到本地文件（自动选择截图方式）。
    pub async fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.screenshot().await?).await?;
        Ok(())
    }

    /// 使用指定的截图方式截取屏幕并保存到本地文件。
    pub async fn screenshot_to_with_method(
        &self,
        path: impl AsRef<Path>,
        method: ScreenshotMethod,
    ) -> Result<()> {
        tokio::fs::write(path, self.screenshot_with_method(method).await?).await?;
        Ok(())
    }

    async fn capture_screenshot(
        &self,
        remote: &str,
        local: &Path,
        method: ScreenshotMethod,
    ) -> Result<Vec<u8>> {
        trace!(target: "hm_driver_rs::files", remote, ?method, "捕获截图");
        let remote_guard = RemoteFileGuard::new(self.inner.hdc.clone(), remote.to_owned());
        let command = match method {
            ScreenshotMethod::SnapshotDisplay => format!("snapshot_display -f {remote}"),
            ScreenshotMethod::ScreenCap => format!("uitest screenCap -p {remote}"),
            ScreenshotMethod::Auto => {
                return Err(DriverError::Protocol(
                    "内部截图方法不能再次使用 Auto".into(),
                ));
            }
        };
        let result = async {
            self.inner.hdc.shell(command).await?;
            self.inner.hdc.receive_file(remote, local).await?;
            tokio::fs::read(local).await.map_err(DriverError::Io)
        }
        .await;
        remote_guard.cleanup().await;
        result
    }
}
