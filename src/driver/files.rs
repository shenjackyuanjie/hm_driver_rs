//! 文件推拉、设备 shell、端口转发与截图。
//!
//! HDC 以主机参数数组执行命令，`raw_shell` 的命令内容由设备 shell 解释。
//! 截图使用唯一临时路径，经文件传输返回 JPEG/PNG；自定义转发由调用方显式移除，
//! 驱动自己的会话转发由生命周期管理。

use super::{HmDriver, RemoteFileGuard, next_operation_id};
use crate::hdc::CommandOutput;
use crate::types::{ForwardEntry, ScreenshotMethod};
use crate::{DriverError, Result};
use std::path::Path;
use tempfile::tempdir;
use tracing::{debug, trace};

impl HmDriver {
    /// 将本地文件发送到设备端。
    ///
    /// `local` 是主机路径，`remote` 是设备路径，使用文件传输超时；失败返回 HDC 错误。
    pub async fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        debug!(target: "hm_driver_rs::files", local = %local.as_ref().display(), remote, "推送文件");
        self.inner
            .hdc
            .send_file(local.as_ref(), remote)
            .await
            .map(|_| ())
    }

    /// 从设备端拉取文件到本地。
    ///
    /// `remote` 是设备路径，`local` 是主机目标路径，使用文件传输超时；失败返回 HDC 错误。
    pub async fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        debug!(target: "hm_driver_rs::files", remote, local = %local.as_ref().display(), "拉取文件");
        self.inner
            .hdc
            .receive_file(remote, local.as_ref())
            .await
            .map(|_| ())
    }

    /// 显式执行设备端 shell。字符串不会交给主机 shell。
    ///
    /// 字符串作为一项参数传给 `hdc shell`，其 shell 语法在设备端解释。
    /// 使用普通命令超时，返回脱敏的标准输出/错误输出；失败返回 HDC 错误。
    pub async fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        trace!(target: "hm_driver_rs::files", command, "原始 Shell 命令");
        self.inner.hdc.shell(command).await
    }

    /// 列出设备端所有已建立的端口转发规则。
    ///
    /// 读取当前 HDC `fport ls` 列表，包含驱动、自定义及其他调用方建立的规则。
    pub async fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        self.inner.hdc.list_forwards().await
    }

    /// 建立一个自定义端口转发，与驱动自身使用的 RPC 转发互不影响。
    ///
    /// `local_port` 映射为 `tcp:端口`，`remote` 使用 HDC 端点格式，如 `tcp:端口` 或
    /// `localabstract:名称`。这类转发由调用方用 [`remove_forward`](Self::remove_forward) 清理。
    pub async fn forward(&self, local_port: u16, remote: &str) -> Result<()> {
        self.inner.hdc.forward(local_port, remote).await
    }

    /// 移除一个自定义端口转发。
    ///
    /// 删除前后核对指定规则，已不存在时返回成功；规则持续存在时返回 [`DriverError::Forward`]。
    pub async fn remove_forward(&self, local_port: u16, remote: &str) -> Result<()> {
        self.inner.hdc.remove_forward(local_port, remote).await
    }

    /// 截取当前屏幕（自动选择可用方式），返回 JPEG/PNG 字节。
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        debug!(target: "hm_driver_rs::files", "截取屏幕");
        self.screenshot_with_method(ScreenshotMethod::Auto).await
    }

    /// 使用指定的截图方式截取当前屏幕。
    ///
    /// `SnapshotDisplay` 返回 JPEG，`ScreenCap` 返回 PNG；`Auto` 优先尝试前者，
    /// 失败后回退后者。使用主机/设备临时文件，结束后执行清理；采集或拉取失败返回错误。
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
    ///
    /// 主机路径已有文件会被覆盖；图像字节格式由截图方式决定，与文件扩展名无关。
    pub async fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        tokio::fs::write(path, self.screenshot().await?).await?;
        Ok(())
    }

    /// 使用指定的截图方式截取屏幕并保存到本地文件。
    ///
    /// 主机文件会被覆盖；扩展名不转换图像格式，字节格式见
    /// [`screenshot_with_method`](Self::screenshot_with_method)。写入失败返回 [`DriverError::Io`]。
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
