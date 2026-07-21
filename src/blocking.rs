//! 同步阻塞门面。
//!
//! 所有方法都复用进程级 Tokio runtime。若在 Tokio 异步上下文中调用，会返回明确错误。

use crate::{
    AgentProfile, AgentSource, AppIdentifier, Bounds, CommandOutput, DeviceInfo, DeviceSelector,
    DisplayRotation, DisplaySize, DriverConfig, HdcConfig, Point, Result, Selector, UiNode,
};
use serde_json::Value;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn block_on<F: Future>(future: F) -> Result<F::Output> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(crate::DriverError::BlockingInAsyncContext);
    }
    let runtime = if let Some(runtime) = RUNTIME.get() {
        runtime
    } else {
        let runtime = Runtime::new().map_err(crate::DriverError::Io)?;
        let _ = RUNTIME.set(runtime);
        RUNTIME.get().expect("runtime 已初始化")
    };
    Ok(runtime.block_on(future))
}

/// 阻塞 Driver 的 Builder。
#[derive(Clone, Debug, Default)]
pub struct HmDriverBuilder {
    inner: crate::HmDriverBuilder,
}

impl HmDriverBuilder {
    pub fn device(mut self, selector: DeviceSelector) -> Self {
        self.inner = self.inner.device(selector);
        self
    }

    pub fn hdc_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.inner = self.inner.hdc_path(path);
        self
    }

    pub fn hdc_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.inner = self.inner.hdc_server(host, port);
        self
    }

    pub fn hdc_config(mut self, config: HdcConfig) -> Self {
        self.inner = self.inner.hdc_config(config);
        self
    }

    pub fn agent_source(mut self, source: AgentSource) -> Self {
        self.inner = self.inner.agent_source(source);
        self
    }

    pub fn driver_config(mut self, config: DriverConfig) -> Self {
        self.inner = self.inner.driver_config(config);
        self
    }

    pub fn connect(self) -> Result<HmDriver> {
        block_on(self.inner.connect())?.map(|inner| HmDriver { inner })
    }
}

/// 与异步 Driver 能力对应的阻塞门面。
#[derive(Clone, Debug)]
pub struct HmDriver {
    inner: crate::HmDriver,
}

impl HmDriver {
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    pub fn agent_profile(&self) -> &AgentProfile {
        self.inner.agent_profile()
    }

    pub fn recover(&self) -> Result<()> {
        block_on(self.inner.recover())?
    }

    pub fn close(&self) -> Result<()> {
        block_on(self.inner.close())?
    }

    pub fn call_hypium_api(&self, api: &str, this: Option<&str>, args: Value) -> Result<Value> {
        block_on(self.inner.call_hypium_api(api, this, args))?
    }

    pub fn display_size(&self) -> Result<DisplaySize> {
        block_on(self.inner.display_size())?
    }

    pub fn display_rotation(&self) -> Result<DisplayRotation> {
        block_on(self.inner.display_rotation())?
    }

    pub fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        block_on(self.inner.set_display_rotation(rotation))?
    }

    pub fn device_info(&self) -> Result<DeviceInfo> {
        block_on(self.inner.device_info())?
    }

    pub fn screen_on(&self) -> Result<()> {
        block_on(self.inner.screen_on())?
    }

    pub fn screen_off(&self) -> Result<()> {
        block_on(self.inner.screen_off())?
    }

    pub fn unlock(&self) -> Result<()> {
        block_on(self.inner.unlock())?
    }

    pub fn press_key(&self, key_code: u32) -> Result<()> {
        block_on(self.inner.press_key(key_code))?
    }

    pub fn click(&self, point: Point) -> Result<()> {
        block_on(self.inner.click(point))?
    }

    pub fn double_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.double_click(point))?
    }

    pub fn long_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.long_click(point))?
    }

    pub fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.swipe(from, to, speed))?
    }

    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    pub fn install_app(&self, package: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.install_app(package))?
    }

    pub fn uninstall_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.uninstall_app(bundle))?
    }

    pub fn start_app(&self, bundle: &AppIdentifier, ability: Option<&str>) -> Result<()> {
        block_on(self.inner.start_app(bundle, ability))?
    }

    pub fn stop_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.stop_app(bundle))?
    }

    pub fn clear_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.clear_app(bundle))?
    }

    pub fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        block_on(self.inner.main_ability(bundle))?
    }

    pub fn current_app(&self) -> Result<Option<(AppIdentifier, String)>> {
        block_on(self.inner.current_app())?
    }

    pub fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        block_on(self.inner.push_file(local, remote))?
    }

    pub fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.pull_file(remote, local))?
    }

    pub fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        block_on(self.inner.raw_shell(command))?
    }

    pub fn screenshot(&self) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot())?
    }

    pub fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.screenshot_to(path))?
    }

    pub fn ui_tree(&self) -> Result<UiNode> {
        block_on(self.inner.ui_tree())?
    }

    pub fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let element = block_on(self.inner.find(selector))??;
        Ok(element.map(|inner| Element { inner }))
    }

    pub fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        Ok(block_on(self.inner.find_all(selector))??
            .into_iter()
            .map(|inner| Element { inner })
            .collect())
    }

    pub fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        block_on(self.inner.wait_for(selector, timeout))?.map(|inner| Element { inner })
    }

    pub fn xpath(&self, expression: &str) -> Result<XPathElement> {
        block_on(self.inner.xpath(expression))?.map(|inner| XPathElement { inner })
    }

    pub fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        Ok(block_on(self.inner.xpath_all(expression))??
            .into_iter()
            .map(|inner| XPathElement { inner })
            .collect())
    }

    pub fn xpath_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_exists(expression))?
    }
}

/// 阻塞控件句柄。
#[derive(Debug)]
pub struct Element {
    inner: crate::Element,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Result<Value> {
        block_on(self.inner.attribute(name))?
    }

    pub fn bounds(&self) -> Result<Bounds> {
        block_on(self.inner.bounds())?
    }

    pub fn click(&self) -> Result<()> {
        block_on(self.inner.click())?
    }

    pub fn double_click(&self) -> Result<()> {
        block_on(self.inner.double_click())?
    }

    pub fn long_click(&self) -> Result<()> {
        block_on(self.inner.long_click())?
    }

    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    pub fn clear_text(&self) -> Result<()> {
        block_on(self.inner.clear_text())?
    }

    pub fn drag_to(&self, target: &Element) -> Result<()> {
        block_on(self.inner.drag_to(&target.inner))?
    }

    pub fn pinch_in(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_in(scale))?
    }

    pub fn pinch_out(&self, scale: f64) -> Result<()> {
        block_on(self.inner.pinch_out(scale))?
    }
}

/// 阻塞 XPath 查询结果。
#[derive(Clone, Debug)]
pub struct XPathElement {
    inner: crate::XPathElement,
}

impl XPathElement {
    pub fn exists(&self) -> bool {
        self.inner.exists()
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.inner.attribute(name)
    }

    pub fn bounds(&self) -> Option<Bounds> {
        self.inner.bounds()
    }

    pub fn click(&self) -> Result<()> {
        block_on(self.inner.click())?
    }

    pub fn double_click(&self) -> Result<()> {
        block_on(self.inner.double_click())?
    }

    pub fn long_click(&self) -> Result<()> {
        block_on(self.inner.long_click())?
    }

    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_use_inside_async_context() {
        let result = block_on(async { 1_u8 });
        assert!(matches!(
            result,
            Err(crate::DriverError::BlockingInAsyncContext)
        ));
    }
}
