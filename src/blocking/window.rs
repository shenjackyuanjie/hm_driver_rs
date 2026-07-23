//! 窗口对象的同步阻塞门面。

use super::block_on;
use crate::{Bounds, ResizeDirection, Result, WindowMode};

/// 阻塞窗口句柄。
#[derive(Debug)]
pub struct UiWindow {
    pub(super) inner: crate::UiWindow,
}

impl UiWindow {
    pub fn bundle_name(&self) -> Result<String> {
        block_on(self.inner.bundle_name())?
    }

    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    pub fn title(&self) -> Result<String> {
        block_on(self.inner.title())?
    }

    pub fn mode(&self) -> Result<WindowMode> {
        block_on(self.inner.mode())?
    }

    pub fn is_focused(&self) -> Result<bool> {
        block_on(self.inner.is_focused())?
    }

    pub fn is_active(&self) -> Result<bool> {
        block_on(self.inner.is_active())?
    }

    pub fn focus(&self) -> Result<()> {
        block_on(self.inner.focus())?
    }

    pub fn move_to(&self, x: i32, y: i32) -> Result<()> {
        block_on(self.inner.move_to(x, y))?
    }

    pub fn resize(&self, width: u32, height: u32, direction: ResizeDirection) -> Result<()> {
        block_on(self.inner.resize(width, height, direction))?
    }

    pub fn split(&self) -> Result<()> {
        block_on(self.inner.split())?
    }

    pub fn maximize(&self) -> Result<()> {
        block_on(self.inner.maximize())?
    }

    pub fn minimize(&self) -> Result<()> {
        block_on(self.inner.minimize())?
    }

    pub fn resume(&self) -> Result<()> {
        block_on(self.inner.resume())?
    }

    pub fn close(&self) -> Result<()> {
        block_on(self.inner.close())?
    }
}
