use super::{Element, XPathElement, block_on};
use crate::{
    AbilityInfo, AgentProfile, AgentSource, AppIdentifier, CommandOutput, DeviceInfo,
    DeviceSelector, DisplayRotation, DisplaySize, DriverConfig, ForwardEntry, Gesture, HdcConfig,
    KeyCode, OpenUrlMode, Point, Position, Result, ScreenState, ScreenshotMethod, Selector,
    SwipeArea, SwipeDirection, UiNode,
};
use serde_json::Value;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

    pub fn screen_state(&self) -> Result<ScreenState> {
        block_on(self.inner.screen_state())?
    }

    pub fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        block_on(self.inner.wlan_ip())?
    }

    pub fn unlock(&self) -> Result<()> {
        block_on(self.inner.unlock())?
    }

    pub fn press_key(&self, key_code: u32) -> Result<()> {
        block_on(self.inner.press_key(key_code))?
    }

    pub fn press_key_code(&self, key_code: KeyCode) -> Result<()> {
        block_on(self.inner.press_key_code(key_code))?
    }

    pub fn go_back(&self) -> Result<()> {
        block_on(self.inner.go_back())?
    }

    pub fn go_home(&self) -> Result<()> {
        block_on(self.inner.go_home())?
    }

    pub fn click(&self, point: Point) -> Result<()> {
        block_on(self.inner.click(point))?
    }

    pub fn click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.click_position(position))?
    }

    pub fn double_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.double_click(point))?
    }

    pub fn double_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.double_click_position(position))?
    }

    pub fn long_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.long_click(point))?
    }

    pub fn long_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.long_click_position(position))?
    }

    pub fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.swipe(from, to, speed))?
    }

    pub fn swipe_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        block_on(self.inner.swipe_positions(from, to, speed))?
    }

    pub fn swipe_direction(
        &self,
        direction: SwipeDirection,
        area: SwipeArea,
        scale: f64,
        speed: u32,
    ) -> Result<()> {
        block_on(self.inner.swipe_direction(direction, area, scale, speed))?
    }

    pub fn perform_gesture(&self, gesture: &Gesture) -> Result<()> {
        block_on(self.inner.perform_gesture(gesture))?
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

    pub fn open_url(&self, value: &str, mode: OpenUrlMode) -> Result<()> {
        block_on(self.inner.open_url(value, mode))?
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

    pub fn app_info(&self, bundle: &AppIdentifier) -> Result<Value> {
        block_on(self.inner.app_info(bundle))?
    }

    pub fn app_abilities(&self, bundle: &AppIdentifier) -> Result<Vec<AbilityInfo>> {
        block_on(self.inner.app_abilities(bundle))?
    }

    pub fn main_ability_info(&self, bundle: &AppIdentifier) -> Result<Option<AbilityInfo>> {
        block_on(self.inner.main_ability_info(bundle))?
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

    pub fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        block_on(self.inner.list_forwards())?
    }

    pub fn screenshot(&self) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot())?
    }

    pub fn screenshot_with_method(&self, method: ScreenshotMethod) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot_with_method(method))?
    }

    pub fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.screenshot_to(path))?
    }

    pub fn screenshot_to_with_method(
        &self,
        path: impl AsRef<Path>,
        method: ScreenshotMethod,
    ) -> Result<()> {
        block_on(self.inner.screenshot_to_with_method(path, method))?
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

    pub fn exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.exists(selector))?
    }

    pub fn count(&self, selector: &Selector) -> Result<usize> {
        block_on(self.inner.count(selector))?
    }

    pub fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.click_if_exists(selector))?
    }

    pub fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        block_on(self.inner.wait_for(selector, timeout))?.map(|inner| Element { inner })
    }

    pub fn xpath(&self, expression: &str) -> Result<XPathElement> {
        block_on(self.inner.xpath(expression))?.map(|inner| XPathElement { inner })
    }

    pub fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        Ok(block_on(self.inner.xpath_optional(expression))??.map(|inner| XPathElement { inner }))
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

    pub fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_click_if_exists(expression))?
    }
}
