//! 设备 Driver 与连接 Builder 的同步门面。
//!
//! 公开方法对应 [`crate::HmDriver`] 和 [`crate::HmDriverBuilder`]，设备操作经共享
//! Tokio runtime 同步等待完成；同步条件轮询在调用线程执行。
//! 每个转发方法链接相应异步入口，统一说明参数、返回值、设备要求和生命周期。

use super::{Element, UiWindow, XPathElement, block_on};
use crate::{
    AbilityInfo, AgentProfile, AgentSource, AppIdentifier, BuiltinNetworkScenario, CommandOutput,
    DeviceDescriptor, DeviceInfo, DeviceSelector, DisplayRotation, DisplaySize, DriverConfig,
    ForwardEntry, Gesture, HdcConfig, KeyCode, MatchPattern, MouseButton, NetworkScenario,
    NetworkScenarioInfo, OpenUrlMode, Point, Position, Result, ScreenState, ScreenshotMethod,
    Selector, SwipeArea, SwipeDirection, UiEvent, UiEventType, UiNode, ViewMode, WindowFilter,
};
use serde_json::Value;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::trace;

/// 阻塞 Driver 的 Builder，默认值和设置顺序见 [`crate::HmDriverBuilder`]。
#[derive(Clone, Debug, Default)]
pub struct HmDriverBuilder {
    /// 底层异步 Driver Builder 实例。
    inner: crate::HmDriverBuilder,
}

impl HmDriverBuilder {
    /// 设置目标设备选择器。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::device`]。
    pub fn device(mut self, selector: DeviceSelector) -> Self {
        self.inner = self.inner.device(selector);
        self
    }

    /// 设置 hdc 可执行文件的路径。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::hdc_path`]。
    pub fn hdc_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.inner = self.inner.hdc_path(path);
        self
    }

    /// 设置 hdc server 的地址和端口。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::hdc_server`]。
    pub fn hdc_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.inner = self.inner.hdc_server(host, port);
        self
    }

    /// 直接使用完整的 HDC 配置。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::hdc_config`]。
    pub fn hdc_config(mut self, config: HdcConfig) -> Self {
        self.inner = self.inner.hdc_config(config);
        self
    }

    /// 设置官方 Agent 动态库来源（内嵌资源或外部目录）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::agent_source`]。
    pub fn agent_source(mut self, source: AgentSource) -> Self {
        self.inner = self.inner.agent_source(source);
        self
    }

    /// 设置 Driver 运行时配置。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::driver_config`]。
    pub fn driver_config(mut self, config: DriverConfig) -> Self {
        self.inner = self.inner.driver_config(config);
        self
    }

    /// 连接设备并建立 Hypium RPC 会话。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriverBuilder::connect`]。
    pub fn connect(self) -> Result<HmDriver> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 HmDriver::connect");
        block_on(self.inner.connect())?.map(|inner| HmDriver { inner })
    }
}

/// 与异步 Driver 能力对应的阻塞门面。
///
/// RPC/HDC 方法等待异步操作完成，返回对应结果；`Clone` 共享底层会话。
/// 生命周期和错误说明见 [`crate::HmDriver`]，同步条件等待见 [`Self::wait_until`]。
#[derive(Clone, Debug)]
pub struct HmDriver {
    /// 底层异步 Driver 实例。
    inner: crate::HmDriver,
}

impl HmDriver {
    /// 创建一个新的 [`HmDriverBuilder`]。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::builder`]。
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    /// 使用当前 HDC 配置发现设备，不建立 Agent 会话。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::discover_devices`]。
    pub fn discover_devices(config: HdcConfig) -> Result<Vec<DeviceDescriptor>> {
        block_on(crate::HmDriver::discover_devices(config))?
    }

    /// 返回当前使用的 Agent 版本、架构、文件校验和传输信息。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::agent_profile`]。
    pub fn agent_profile(&self) -> &AgentProfile {
        self.inner.agent_profile()
    }

    /// 返回当前会话的代际编号，用于区分远端引用归属的会话。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::generation`]。
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    /// 返回当前会话协商出的 Hypium API 方言（Modern/Legacy）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::dialect`]。
    pub fn dialect(&self) -> Result<crate::ApiDialect> {
        block_on(self.inner.dialect())?
    }

    /// 恢复已断开的会话，重新部署 Agent、建立转发并创建远端 Driver。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::recover`]。
    pub fn recover(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 HmDriver::recover");
        block_on(self.inner.recover())?
    }

    /// 主动关闭共享会话并等待资源清理。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::close`]。
    pub fn close(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::blocking", "阻塞 HmDriver::close");
        block_on(self.inner.close())?
    }

    /// 直接调用任意 Hypium RPC API。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::call_hypium_api`]。
    pub fn call_hypium_api(&self, api: &str, this: Option<&str>, args: Value) -> Result<Value> {
        block_on(self.inner.call_hypium_api(api, this, args))?
    }

    /// 获取当前显示设备的尺寸（宽度 x 高度，单位为像素）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::display_size`]。
    pub fn display_size(&self) -> Result<DisplaySize> {
        block_on(self.inner.display_size())?
    }

    /// 获取指定显示设备的尺寸（宽度 x 高度，单位为像素）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::display_size_for`]。
    pub fn display_size_for(&self, display_id: u32) -> Result<DisplaySize> {
        block_on(self.inner.display_size_for(display_id))?
    }

    /// 获取当前显示旋转角度。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::display_rotation`]。
    pub fn display_rotation(&self) -> Result<DisplayRotation> {
        block_on(self.inner.display_rotation())?
    }

    /// 设置当前显示旋转角度（0°/90°/180°/270°）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::set_display_rotation`]。
    pub fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        block_on(self.inner.set_display_rotation(rotation))?
    }

    /// 收集完整的设备信息（型号、系统版本、CPU ABI、WLAN IP、显示尺寸与旋转角度等）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::device_info`]。
    pub fn device_info(&self) -> Result<DeviceInfo> {
        block_on(self.inner.device_info())?
    }

    /// 点亮屏幕（通过 `power-shell wakeup`）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screen_on`]。
    pub fn screen_on(&self) -> Result<()> {
        block_on(self.inner.screen_on())?
    }

    /// 熄灭屏幕。仅在屏幕当前为亮屏状态时发送电源键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screen_off`]。
    pub fn screen_off(&self) -> Result<()> {
        block_on(self.inner.screen_off())?
    }

    /// 无条件发送一次电源键，用于显式切换屏幕电源状态。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::toggle_screen_power`]。
    pub fn toggle_screen_power(&self) -> Result<()> {
        block_on(self.inner.toggle_screen_power())?
    }

    /// 获取当前屏幕电源状态（Awake / Sleep / Inactive）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screen_state`]。
    pub fn screen_state(&self) -> Result<ScreenState> {
        block_on(self.inner.screen_state())?
    }

    /// 获取 WLAN 接口的非回环 IPv4/IPv6 地址。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wlan_ip`]。
    pub fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        block_on(self.inner.wlan_ip())?
    }

    /// 解锁屏幕：先亮屏，再从底部向上滑动。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::unlock`]。
    pub fn unlock(&self) -> Result<()> {
        block_on(self.inner.unlock())?
    }

    /// 通过按键码（原始值）发送按键事件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::press_key`]。
    pub fn press_key(&self, key_code: u32) -> Result<()> {
        block_on(self.inner.press_key(key_code))?
    }

    /// 通过 [`KeyCode`] 枚举发送按键事件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::press_key_code`]。
    pub fn press_key_code(&self, key_code: KeyCode) -> Result<()> {
        block_on(self.inner.press_key_code(key_code))?
    }

    /// 同时触发两个或三个组合键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::press_key_combination`]。
    pub fn press_key_combination(&self, key_codes: &[KeyCode]) -> Result<()> {
        block_on(self.inner.press_key_combination(key_codes))?
    }

    /// 发送返回键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::go_back`]。
    pub fn go_back(&self) -> Result<()> {
        block_on(self.inner.go_back())?
    }

    /// 发送主页键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::go_home`]。
    pub fn go_home(&self) -> Result<()> {
        block_on(self.inner.go_home())?
    }

    /// 在指定绝对坐标处点击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::click`]。
    pub fn click(&self, point: Point) -> Result<()> {
        block_on(self.inner.click(point))?
    }

    /// 在指定绝对或归一化坐标处点击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::click_position`]。
    pub fn click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.click_position(position))?
    }

    /// 在指定绝对坐标处双击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::double_click`]。
    pub fn double_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.double_click(point))?
    }

    /// 在指定绝对或归一化坐标处双击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::double_click_position`]。
    pub fn double_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.double_click_position(position))?
    }

    /// 在指定绝对坐标处长按。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::long_click`]。
    pub fn long_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.long_click(point))?
    }

    /// 在指定绝对或归一化坐标处长按。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::long_click_position`]。
    pub fn long_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.long_click_position(position))?
    }

    /// 在指定绝对坐标处长按给定时长。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::long_click_for`]。
    pub fn long_click_for(&self, point: Point, duration: Duration) -> Result<()> {
        block_on(self.inner.long_click_for(point, duration))?
    }

    /// 在指定绝对或归一化位置长按给定时长。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::long_click_position_for`]。
    pub fn long_click_position_for(&self, position: Position, duration: Duration) -> Result<()> {
        block_on(self.inner.long_click_position_for(position, duration))?
    }

    /// 从起点滑动到终点，`speed` 为滑动速度（200–40000 像素/秒）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::swipe`]。
    pub fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.swipe(from, to, speed))?
    }

    /// 从起点滑动到终点，`duration_ms` 为滑动持续时间（毫秒）。
    /// 内部根据距离与持续时间自动计算所需速度，再调用底层 `swipe` API。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::swipe_with_duration_ms`]。
    pub fn swipe_with_duration_ms(&self, from: Point, to: Point, duration_ms: u32) -> Result<()> {
        block_on(self.inner.swipe_with_duration_ms(from, to, duration_ms))?
    }

    /// 从归一化或绝对坐标位置滑动到目标位置。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::swipe_positions`]。
    pub fn swipe_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        block_on(self.inner.swipe_positions(from, to, speed))?
    }

    /// 从一个绝对坐标拖拽到另一个绝对坐标。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::drag`]。
    pub fn drag(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.drag(from, to, speed))?
    }

    /// 接受绝对或归一化坐标的拖拽操作。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::drag_positions`]。
    pub fn drag_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        block_on(self.inner.drag_positions(from, to, speed))?
    }

    /// 执行带固定步长的抛滑操作。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::fling`]。
    pub fn fling(&self, from: Point, to: Point, step_length: u32, speed: u32) -> Result<()> {
        block_on(self.inner.fling(from, to, step_length, speed))?
    }

    /// 接受绝对或归一化坐标的抛滑操作。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::fling_positions`]。
    pub fn fling_positions(
        &self,
        from: Position,
        to: Position,
        step_length: u32,
        speed: u32,
    ) -> Result<()> {
        block_on(self.inner.fling_positions(from, to, step_length, speed))?
    }

    /// 在指定区域内按方向滑动一定比例。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::swipe_direction`]。
    pub fn swipe_direction(
        &self,
        direction: SwipeDirection,
        area: SwipeArea,
        scale: f64,
        speed: u32,
    ) -> Result<()> {
        block_on(self.inner.swipe_direction(direction, area, scale, speed))?
    }

    /// 执行一个多指手势轨迹。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::perform_gesture`]。
    pub fn perform_gesture(&self, gesture: &Gesture) -> Result<()> {
        block_on(self.inner.perform_gesture(gesture))?
    }

    /// 使用一个或两个指关节点执行单次或双次敲击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::knuckle_knock`]。
    pub fn knuckle_knock(&self, points: &[Point], times: u8) -> Result<()> {
        block_on(self.inner.knuckle_knock(points, times))?
    }

    /// 使用绝对或归一化位置执行指关节敲击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::knuckle_knock_positions`]。
    pub fn knuckle_knock_positions(&self, positions: &[Position], times: u8) -> Result<()> {
        block_on(self.inner.knuckle_knock_positions(positions, times))?
    }

    /// 使用指关节注入自定义轨迹。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::perform_knuckle_gesture`]。
    pub fn perform_knuckle_gesture(&self, gesture: &Gesture) -> Result<()> {
        block_on(self.inner.perform_knuckle_gesture(gesture))?
    }

    /// 以指定中心、半径和速度执行指关节闭合圈选。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::knuckle_select`]。
    pub fn knuckle_select(&self, center: Point, radius: u32, speed: u32) -> Result<()> {
        block_on(self.inner.knuckle_select(center, radius, speed))?
    }

    /// 以绝对或归一化中心位置执行指关节闭合圈选。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::knuckle_select_position`]。
    pub fn knuckle_select_position(&self, center: Position, radius: u32, speed: u32) -> Result<()> {
        block_on(self.inner.knuckle_select_position(center, radius, speed))?
    }

    /// 鼠标单击，支持同时按住最多两个键盘按键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_click`]。
    pub fn mouse_click(&self, point: Point, button: MouseButton, keys: &[KeyCode]) -> Result<()> {
        block_on(self.inner.mouse_click(point, button, keys))?
    }

    /// 在绝对或归一化位置执行鼠标单击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_click_position`]。
    pub fn mouse_click_position(
        &self,
        position: Position,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        block_on(self.inner.mouse_click_position(position, button, keys))?
    }

    /// 鼠标双击，支持同时按住最多两个键盘按键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_double_click`]。
    pub fn mouse_double_click(
        &self,
        point: Point,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        block_on(self.inner.mouse_double_click(point, button, keys))?
    }

    /// 鼠标长按，支持同时按住最多两个键盘按键。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_long_click`]。
    pub fn mouse_long_click(
        &self,
        point: Point,
        button: MouseButton,
        keys: &[KeyCode],
    ) -> Result<()> {
        block_on(self.inner.mouse_long_click(point, button, keys))?
    }

    /// 通过系统输入命令执行指定时长的鼠标长按。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_long_click_for`]。
    pub fn mouse_long_click_for(
        &self,
        point: Point,
        button: MouseButton,
        duration: Duration,
    ) -> Result<()> {
        block_on(self.inner.mouse_long_click_for(point, button, duration))?
    }

    /// 滚动鼠标滚轮。正数向前/向上，负数向后/向下。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_scroll`]。
    pub fn mouse_scroll(&self, point: Point, distance: i32, keys: &[KeyCode]) -> Result<()> {
        block_on(self.inner.mouse_scroll(point, distance, keys))?
    }

    /// 将鼠标指针直接移动到指定坐标。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_move_to`]。
    pub fn mouse_move_to(&self, point: Point) -> Result<()> {
        block_on(self.inner.mouse_move_to(point))?
    }

    /// 按给定速度沿轨迹移动鼠标。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_move`]。
    pub fn mouse_move(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.mouse_move(from, to, speed))?
    }

    /// 按住鼠标左键拖拽。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::mouse_drag`]。
    pub fn mouse_drag(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.mouse_drag(from, to, speed))?
    }

    /// 触控笔点击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::pen_click`]。
    pub fn pen_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.pen_click(point))?
    }

    /// 触控笔双击。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::pen_double_click`]。
    pub fn pen_double_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.pen_double_click(point))?
    }

    /// 触控笔长按，可指定 0 到 1 的压力。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::pen_long_click`]。
    pub fn pen_long_click(&self, point: Point, pressure: Option<f64>) -> Result<()> {
        block_on(self.inner.pen_long_click(point, pressure))?
    }

    /// 触控笔滑动，可指定速度和压力。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::pen_swipe`]。
    pub fn pen_swipe(
        &self,
        from: Point,
        to: Point,
        speed: u32,
        pressure: Option<f64>,
    ) -> Result<()> {
        block_on(self.inner.pen_swipe(from, to, speed, pressure))?
    }

    /// 使用触控笔注入自定义轨迹。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::perform_pen_gesture`]。
    pub fn perform_pen_gesture(&self, gesture: &Gesture, pressure: Option<f64>) -> Result<()> {
        block_on(self.inner.perform_pen_gesture(gesture, pressure))?
    }

    /// 模拟触控板多指滑动。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::touchpad_swipe`]。
    pub fn touchpad_swipe(
        &self,
        direction: SwipeDirection,
        fingers: u8,
        hold_at_end: bool,
        speed: Option<u32>,
    ) -> Result<()> {
        block_on(
            self.inner
                .touchpad_swipe(direction, fingers, hold_at_end, speed),
        )?
    }

    /// 旋转手表表冠。正步数为顺时针，负步数为逆时针。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::rotate_crown`]。
    pub fn rotate_crown(&self, steps: i32, speed: Option<u16>) -> Result<()> {
        block_on(self.inner.rotate_crown(steps, speed))?
    }

    /// 通过 `Driver.inputText` 在默认坐标 `(1, 1)` 处输入文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::input_text`]。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    /// 隐藏当前系统软键盘。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::hide_keyboard`]。
    pub fn hide_keyboard(&self) -> Result<()> {
        block_on(self.inner.hide_keyboard())?
    }

    /// 清空当前获得焦点的输入框。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::clear_text_on_current_cursor`]。
    pub fn clear_text_on_current_cursor(&self) -> Result<()> {
        block_on(self.inner.clear_text_on_current_cursor())?
    }

    /// 设置系统界面为深色或浅色模式。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::set_view_mode`]。
    pub fn set_view_mode(&self, mode: ViewMode) -> Result<()> {
        block_on(self.inner.set_view_mode(mode))?
    }

    /// 将系统时间设置为 `YYYY-MM-DD HH:MM:SS`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::set_system_time`]。
    pub fn set_system_time(&self, value: &str) -> Result<()> {
        block_on(self.inner.set_system_time(value))?
    }

    /// 读取系统时间，返回 `YYYY-MM-DD HH:MM:SS`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::system_time`]。
    pub fn system_time(&self) -> Result<String> {
        block_on(self.inner.system_time())?
    }

    /// 设置 IANA 时区，例如 `Asia/Shanghai` 或 `Etc/UTC`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::set_timezone`]。
    pub fn set_timezone(&self, timezone: &str) -> Result<()> {
        block_on(self.inner.set_timezone(timezone))?
    }

    /// 读取当前 IANA 时区标识。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::timezone`]。
    pub fn timezone(&self) -> Result<String> {
        block_on(self.inner.timezone())?
    }

    /// 将文本写入系统剪贴板。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::write_clipboard`]。
    pub fn write_clipboard(&self, value: &str) -> Result<()> {
        block_on(self.inner.write_clipboard(value))?
    }

    /// 读取系统剪贴板文本；剪贴板为空时返回空字符串。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::read_clipboard`]。
    pub fn read_clipboard(&self) -> Result<String> {
        block_on(self.inner.read_clipboard())?
    }

    /// 清空系统剪贴板。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::clear_clipboard`]。
    pub fn clear_clipboard(&self) -> Result<()> {
        block_on(self.inner.clear_clipboard())?
    }

    /// 读取本地字体文件声明的字体名称。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::font_name`]。
    pub fn font_name(&self, local: impl AsRef<Path>) -> Result<String> {
        block_on(self.inner.font_name(local))?
    }

    /// 安装本地字体文件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::install_font`]。
    pub fn install_font(&self, local: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.install_font(local))?
    }

    /// 按字体名称卸载字体。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::uninstall_font`]。
    pub fn uninstall_font(&self, font_name: &str) -> Result<()> {
        block_on(self.inner.uninstall_font(font_name))?
    }

    /// 启用设备端网络模拟工具。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::enable_network_simulation`]。
    pub fn enable_network_simulation(&self) -> Result<()> {
        block_on(self.inner.enable_network_simulation())?
    }

    /// 禁用设备端网络模拟工具。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::disable_network_simulation`]。
    pub fn disable_network_simulation(&self) -> Result<()> {
        block_on(self.inner.disable_network_simulation())?
    }

    /// 列出设备端可用的网络模拟场景。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::network_scenarios`]。
    pub fn network_scenarios(&self) -> Result<Vec<NetworkScenarioInfo>> {
        block_on(self.inner.network_scenarios())?
    }

    /// 启动已有网络模拟场景。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_network_scenario`]。
    pub fn start_network_scenario(&self, scenario_id: u32) -> Result<()> {
        block_on(self.inner.start_network_scenario(scenario_id))?
    }

    /// 启动官方内置网络模拟场景。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_builtin_network_scenario`]。
    pub fn start_builtin_network_scenario(&self, scenario: BuiltinNetworkScenario) -> Result<()> {
        block_on(self.inner.start_builtin_network_scenario(scenario))?
    }

    /// 新建并启动自定义网络模拟场景，返回设备分配的场景 ID。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_custom_network_scenario`]。
    pub fn start_custom_network_scenario(&self, scenario: &NetworkScenario) -> Result<u32> {
        block_on(self.inner.start_custom_network_scenario(scenario))?
    }

    /// 停止指定网络模拟场景。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::stop_network_scenario`]。
    pub fn stop_network_scenario(&self, scenario_id: u32) -> Result<()> {
        block_on(self.inner.stop_network_scenario(scenario_id))?
    }

    /// 删除指定自定义网络模拟场景。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::delete_network_scenario`]。
    pub fn delete_network_scenario(&self, scenario_id: u32) -> Result<()> {
        block_on(self.inner.delete_network_scenario(scenario_id))?
    }

    /// 等待 UI 连续空闲指定时长，最长等待 `timeout`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_idle`]。
    pub fn wait_for_idle(&self, idle_time: Duration, timeout: Duration) -> Result<()> {
        block_on(self.inner.wait_for_idle(idle_time, timeout))?
    }

    /// 开始一次 UI 事件监听。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_listen_ui_event`]。
    pub fn start_listen_ui_event(&self, event_type: UiEventType) -> Result<()> {
        block_on(self.inner.start_listen_ui_event(event_type))?
    }

    /// 开始一次 Toast 监听。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_listen_toast`]。
    pub fn start_listen_toast(&self) -> Result<()> {
        block_on(self.inner.start_listen_toast())?
    }

    /// 等待并读取本次监听捕获的 UI 事件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::get_latest_ui_event`]。
    pub fn get_latest_ui_event(&self, timeout: Duration) -> Result<Option<UiEvent>> {
        block_on(self.inner.get_latest_ui_event(timeout))?
    }

    /// 等待并读取本次 Toast 监听捕获的文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::get_latest_toast`]。
    pub fn get_latest_toast(&self, timeout: Duration) -> Result<Option<String>> {
        block_on(self.inner.get_latest_toast(timeout))?
    }

    /// 等待 Toast 并按给定规则检查文本。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::check_toast`]。
    pub fn check_toast(
        &self,
        expected: &str,
        pattern: MatchPattern,
        timeout: Duration,
    ) -> Result<bool> {
        block_on(self.inner.check_toast(expected, pattern, timeout))?
    }

    /// 通过 `hdc install` 安装主机上的应用包文件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::install_app`]。
    pub fn install_app(&self, package: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.install_app(package))?
    }

    /// 卸载指定包名的应用。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::uninstall_app`]。
    pub fn uninstall_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.uninstall_app(bundle))?
    }

    /// 启动应用。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::start_app`]。
    pub fn start_app(&self, bundle: &AppIdentifier, ability: Option<&str>) -> Result<()> {
        block_on(self.inner.start_app(bundle, ability))?
    }

    /// 使用系统浏览器或默认方式打开 URL。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::open_url`]。
    pub fn open_url(&self, value: &str, mode: OpenUrlMode) -> Result<()> {
        block_on(self.inner.open_url(value, mode))?
    }

    /// 强制停止指定应用的进程。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::stop_app`]。
    pub fn stop_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.stop_app(bundle))?
    }

    /// 清除指定应用的用户缓存和数据。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::clear_app`]。
    pub fn clear_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.clear_app(bundle))?
    }

    /// 查询应用的 main ability 名称。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::main_ability`]。
    pub fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        block_on(self.inner.main_ability(bundle))?
    }

    /// 查询应用的详细信息，返回 `bm dump` 的 JSON 输出。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::app_info`]。
    pub fn app_info(&self, bundle: &AppIdentifier) -> Result<Value> {
        block_on(self.inner.app_info(bundle))?
    }

    /// 解析应用的 Ability 列表。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::app_abilities`]。
    pub fn app_abilities(&self, bundle: &AppIdentifier) -> Result<Vec<AbilityInfo>> {
        block_on(self.inner.app_abilities(bundle))?
    }

    /// 查询应用的 main ability 详情。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::main_ability_info`]。
    pub fn main_ability_info(&self, bundle: &AppIdentifier) -> Result<Option<AbilityInfo>> {
        block_on(self.inner.main_ability_info(bundle))?
    }

    /// 获取第一个前台任务的 `(包名, Ability 名称)`；无前台任务返回 `None`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::current_app`]。
    pub fn current_app(&self) -> Result<Option<(AppIdentifier, String)>> {
        block_on(self.inner.current_app())?
    }

    /// 获取所有前台任务的应用和 Ability。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::foreground_apps`]。
    pub fn foreground_apps(&self) -> Result<Vec<(AppIdentifier, String)>> {
        block_on(self.inner.foreground_apps())?
    }

    /// 判断指定应用是否处于任一前台任务中。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::is_app_foreground`]。
    pub fn is_app_foreground(&self, bundle: &AppIdentifier) -> Result<bool> {
        block_on(self.inner.is_app_foreground(bundle))?
    }

    /// 将本地文件发送到设备端。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::push_file`]。
    pub fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        block_on(self.inner.push_file(local, remote))?
    }

    /// 从设备端拉取文件到本地。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::pull_file`]。
    pub fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.pull_file(remote, local))?
    }

    /// 显式执行设备端 shell。字符串不会交给主机 shell。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::raw_shell`]。
    pub fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        block_on(self.inner.raw_shell(command))?
    }

    /// 列出设备端所有已建立的端口转发规则。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::list_forwards`]。
    pub fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        block_on(self.inner.list_forwards())?
    }

    /// 建立一个自定义端口转发，与驱动自身使用的 RPC 转发互不影响。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::forward`]。
    pub fn forward(&self, local_port: u16, remote: &str) -> Result<()> {
        block_on(self.inner.forward(local_port, remote))?
    }

    /// 移除一个自定义端口转发。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::remove_forward`]。
    pub fn remove_forward(&self, local_port: u16, remote: &str) -> Result<()> {
        block_on(self.inner.remove_forward(local_port, remote))?
    }

    /// 截取当前屏幕（自动选择可用方式），返回 JPEG/PNG 字节。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screenshot`]。
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot())?
    }

    /// 使用指定的截图方式截取当前屏幕。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screenshot_with_method`]。
    pub fn screenshot_with_method(&self, method: ScreenshotMethod) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot_with_method(method))?
    }

    /// 截取屏幕并直接保存到本地文件（自动选择截图方式）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screenshot_to`]。
    pub fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.screenshot_to(path))?
    }

    /// 使用指定的截图方式截取屏幕并保存到本地文件。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::screenshot_to_with_method`]。
    pub fn screenshot_to_with_method(
        &self,
        path: impl AsRef<Path>,
        method: ScreenshotMethod,
    ) -> Result<()> {
        block_on(self.inner.screenshot_to_with_method(path, method))?
    }

    /// 获取当前界面的 UI 树（通过 `uitest dumpLayout`）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::ui_tree`]。
    pub fn ui_tree(&self) -> Result<UiNode> {
        block_on(self.inner.ui_tree())?
    }

    /// 按组合条件查找窗口。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::find_window`]。
    pub fn find_window(&self, filter: &WindowFilter) -> Result<Option<UiWindow>> {
        Ok(block_on(self.inner.find_window(filter))??.map(|inner| UiWindow { inner }))
    }

    /// 获取当前活动窗口，找不到时回退到聚焦窗口。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::current_window`]。
    pub fn current_window(&self) -> Result<Option<UiWindow>> {
        Ok(block_on(self.inner.current_window())??.map(|inner| UiWindow { inner }))
    }

    /// 获取当前窗口大小。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::window_size`]。
    pub fn window_size(&self) -> Result<Option<(u32, u32)>> {
        block_on(self.inner.window_size())?
    }

    /// 使用选择器查找指定索引的 UI 元素。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::find`]。
    pub fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let element = block_on(self.inner.find(selector))??;
        Ok(element.map(|inner| Element { inner }))
    }

    /// 查找所有匹配选择器的 UI 元素。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::find_all`]。
    pub fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        Ok(block_on(self.inner.find_all(selector))??
            .into_iter()
            .map(|inner| Element { inner })
            .collect())
    }

    /// 判断选择器是否有匹配的元素。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::exists`]。
    pub fn exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.exists(selector))?
    }

    /// 统计选择器匹配的元素数量。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::count`]。
    pub fn count(&self, selector: &Selector) -> Result<usize> {
        block_on(self.inner.count(selector))?
    }

    /// 找到选择器指定索引的元素后点击，返回 `true`；未找到返回 `false`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::click_if_exists`]。
    pub fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.click_if_exists(selector))?
    }

    /// 在总超时时间内等待元素出现，超时返回 `Err(ElementNotFound)`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for`]。
    pub fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        block_on(self.inner.wait_for(selector, timeout))?.map(|inner| Element { inner })
    }

    /// 在同步上下文轮询任意条件，默认间隔 100 毫秒。
    ///
    /// 条件返回 `true` 时成功，达到截止时间返回 `false`，条件错误直接返回。
    /// 每次调用条件前检查截止时间，单次同步闭包执行至返回；耗时由闭包自行控制。
    /// 闭包在普通调用线程运行，可以调用其他阻塞 API。Tokio 上下文中返回
    /// [`crate::DriverError::BlockingInAsyncContext`]。
    pub fn wait_until<F>(&self, timeout: Duration, condition: F) -> Result<bool>
    where
        F: FnMut() -> Result<bool>,
    {
        super::wait_until(timeout, Duration::from_millis(100), condition)
    }

    /// 使用指定轮询间隔等待同步条件。
    ///
    /// 语义同 [`wait_until`](Self::wait_until)，休眠以剩余截止时间为上限。
    /// 截止时间在每次条件调用前检查，单次同步条件执行至返回。
    pub fn wait_until_with_interval<F>(
        &self,
        timeout: Duration,
        interval: Duration,
        condition: F,
    ) -> Result<bool>
    where
        F: FnMut() -> Result<bool>,
    {
        super::wait_until(timeout, interval, condition)
    }

    /// 等待 XPath 节点出现。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_xpath`]。
    pub fn wait_for_xpath(&self, expression: &str, timeout: Duration) -> Result<XPathElement> {
        block_on(self.inner.wait_for_xpath(expression, timeout))?
            .map(|inner| XPathElement { inner })
    }

    /// 等待 XPath 节点消失，超时返回 `false`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_until_xpath_gone`]。
    pub fn wait_until_xpath_gone(&self, expression: &str, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_until_xpath_gone(expression, timeout))?
    }

    /// 等待指定应用进入前台，超时返回 `false`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_app`]。
    pub fn wait_for_app(&self, bundle: &AppIdentifier, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_for_app(bundle, timeout))?
    }

    /// 等待文本内容匹配的节点出现（支持精确、包含、前后缀和正则表达式）。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_text`]。
    pub fn wait_for_text(
        &self,
        text: &str,
        pattern: MatchPattern,
        timeout: Duration,
    ) -> Result<UiNode> {
        block_on(self.inner.wait_for_text(text, pattern, timeout))?
    }

    /// 轮询 UI 树，返回深度优先遍历中第一个满足 `predicate` 的节点快照。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_ui`]。
    pub fn wait_for_ui(
        &self,
        timeout: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        block_on(self.inner.wait_for_ui(timeout, predicate))?
    }

    /// 使用指定的轮询间隔等待 UI 节点出现。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_ui_with_interval`]。
    pub fn wait_for_ui_with_interval(
        &self,
        timeout: Duration,
        interval: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        block_on(
            self.inner
                .wait_for_ui_with_interval(timeout, interval, predicate),
        )?
    }

    /// 轮询完整 UI 树，直到 `predicate` 对根节点返回 `true`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_ui_tree`]。
    pub fn wait_for_ui_tree(
        &self,
        timeout: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        block_on(self.inner.wait_for_ui_tree(timeout, predicate))?
    }

    /// 使用指定轮询间隔等待满足页面级条件的完整 UI 树。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::wait_for_ui_tree_with_interval`]。
    pub fn wait_for_ui_tree_with_interval(
        &self,
        timeout: Duration,
        interval: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        block_on(
            self.inner
                .wait_for_ui_tree_with_interval(timeout, interval, predicate),
        )?
    }

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `Err(XPathNotFound)`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::xpath`]。
    pub fn xpath(&self, expression: &str) -> Result<XPathElement> {
        block_on(self.inner.xpath(expression))?.map(|inner| XPathElement { inner })
    }

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `None`。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::xpath_optional`]。
    pub fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        Ok(block_on(self.inner.xpath_optional(expression))??.map(|inner| XPathElement { inner }))
    }

    /// 通过 XPath 表达式查找所有匹配的 UI 元素。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::xpath_all`]。
    pub fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        Ok(block_on(self.inner.xpath_all(expression))??
            .into_iter()
            .map(|inner| XPathElement { inner })
            .collect())
    }

    /// 判断 XPath 表达式是否有匹配的元素。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::xpath_exists`]。
    pub fn xpath_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_exists(expression))?
    }

    /// 如果 XPath 匹配的元素存在则点击，返回是否点击成功。
    ///
    /// 参数、返回值和设备要求见 [`crate::HmDriver::xpath_click_if_exists`]。
    pub fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_click_if_exists(expression))?
    }
}
