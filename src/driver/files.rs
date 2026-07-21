//! 文件推拉、原始 shell 执行与截图。

use super::{HmDriver, next_operation_id};
use crate::hdc::CommandOutput;
use crate::types::{ForwardEntry, ScreenshotMethod};
use crate::{DriverError, Result};
use std::path::Path;
use tempfile::tempdir;

impl HmDriver {
    pub async fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        self.inner
            .hdc
            .send_file(local.as_ref(), remote)
            .await
            .map(|_| ())
    }

    pub async fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        self.inner
            .hdc
            .receive_file(remote, local.as_ref())
            .await
            .map(|_| ())
    }

    /// 显式执行设备端 shell。字符串不会交给主机 shell。
    pub async fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        self.inner.hdc.shell(command).await
    }

    pub async fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        self.inner.hdc.list_forwards().await
    }

    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        self.screenshot_with_method(ScreenshotMethod::Auto).await
    }

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
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {snapshot_remote}"))
                    .await;
                match first {
                    Ok(bytes) => Ok(bytes),
                    Err(_) => {
                        let result = self
                            .capture_screenshot(
                                &screen_cap_remote,
                                &local,
                                ScreenshotMethod::ScreenCap,
                            )
                            .await;
                        let _ = self
                            .inner
                            .hdc
                            .shell(format!("rm -f {screen_cap_remote}"))
                            .await;
                        result
                    }
                }
            }
            ScreenshotMethod::SnapshotDisplay => {
                let result = self
                    .capture_screenshot(&snapshot_remote, &local, ScreenshotMethod::SnapshotDisplay)
                    .await;
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {snapshot_remote}"))
                    .await;
                result
            }
            ScreenshotMethod::ScreenCap => {
                let result = self
                    .capture_screenshot(&screen_cap_remote, &local, ScreenshotMethod::ScreenCap)
                    .await;
                let _ = self
                    .inner
                    .hdc
                    .shell(format!("rm -f {screen_cap_remote}"))
                    .await;
                result
            }
        }
    }

    pub async fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.screenshot().await?).await?;
        Ok(())
    }

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
        let command = match method {
            ScreenshotMethod::SnapshotDisplay => format!("snapshot_display -f {remote}"),
            ScreenshotMethod::ScreenCap => format!("uitest screenCap -p {remote}"),
            ScreenshotMethod::Auto => {
                return Err(DriverError::Protocol(
                    "内部截图方法不能再次使用 Auto".into(),
                ));
            }
        };
        self.inner.hdc.shell(command).await?;
        self.inner.hdc.receive_file(remote, local).await?;
        tokio::fs::read(local).await.map_err(DriverError::Io)
    }
}
