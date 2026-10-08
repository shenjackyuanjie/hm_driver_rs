//! 窗口对象的同步阻塞门面。
//!
//! [`UiWindow`] 包装 [`crate::UiWindow`]，在共享 runtime 上同步等待窗口 RPC。
//! 窗口定位来自 [`super::HmDriver::find_window`]，恢复与清理由底层句柄统一管理。

use super::block_on;
use crate::{Bounds, ResizeDirection, Result, WindowMode};

/// 阻塞窗口句柄。
///
/// 定位、恢复及引用清理语义见 [`crate::UiWindow`]，所有窗口方法同步等待 RPC 完成。
#[derive(Debug)]
pub struct UiWindow {
    pub(super) inner: crate::UiWindow,
}

impl UiWindow {
    /// 获取窗口所属应用包名。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::bundle_name`]。
    pub fn bundle_name(&self) -> Result<String> {
        block_on(self.inner.bundle_name())?
    }

    /// 获取窗口边界。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::bounds`]。
    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    /// 获取窗口标题。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::title`]。
    pub fn title(&self) -> Result<String> {
        block_on(self.inner.title())?
    }

    /// 获取窗口显示模式。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::mode`]。
    pub fn mode(&self) -> Result<WindowMode> {
        block_on(self.inner.mode())?
    }

    /// 判断窗口是否拥有焦点。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::is_focused`]。
    pub fn is_focused(&self) -> Result<bool> {
        block_on(self.inner.is_focused())?
    }

    /// 判断窗口是否处于活动状态。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::is_active`]。
    pub fn is_active(&self) -> Result<bool> {
        block_on(self.inner.is_active())?
    }

    /// 让窗口获得焦点。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::focus`]。
    pub fn focus(&self) -> Result<()> {
        block_on(self.inner.focus())?
    }

    /// 将窗口左上角移动到指定坐标。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::move_to`]。
    pub fn move_to(&self, x: i32, y: i32) -> Result<()> {
        block_on(self.inner.move_to(x, y))?
    }

    /// 调整窗口大小。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::resize`]。
    pub fn resize(&self, width: u32, height: u32, direction: ResizeDirection) -> Result<()> {
        block_on(self.inner.resize(width, height, direction))?
    }

    /// 切换到分屏模式。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::split`]。
    pub fn split(&self) -> Result<()> {
        block_on(self.inner.split())?
    }

    /// 最大化窗口。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::maximize`]。
    pub fn maximize(&self) -> Result<()> {
        block_on(self.inner.maximize())?
    }

    /// 最小化窗口。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::minimize`]。
    pub fn minimize(&self) -> Result<()> {
        block_on(self.inner.minimize())?
    }

    /// 恢复窗口之前的显示模式。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::resume`]。
    pub fn resume(&self) -> Result<()> {
        block_on(self.inner.resume())?
    }

    /// 关闭窗口。
    ///
    /// 参数、返回值和设备要求见 [`crate::UiWindow::close`]。
    pub fn close(&self) -> Result<()> {
        block_on(self.inner.close())?
    }
}
