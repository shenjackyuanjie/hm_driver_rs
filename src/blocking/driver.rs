use super::{Element, XPathElement, block_on};
use crate::{
    AbilityInfo, AgentProfile, AgentSource, AppIdentifier, CommandOutput, DeviceDescriptor,
    DeviceInfo, DeviceSelector, DisplayRotation, DisplaySize, DriverConfig, ForwardEntry, Gesture,
    HdcConfig, KeyCode, OpenUrlMode, Point, Position, Result, ScreenState, ScreenshotMethod,
    Selector, SwipeArea, SwipeDirection, UiNode,
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
    /// 设置目标设备选择器。
    ///
    /// 指定要连接的设备，可通过序列号、USB 或网络地址来标识。
    pub fn device(mut self, selector: DeviceSelector) -> Self {
        self.inner = self.inner.device(selector);
        self
    }

    /// 设置 hdc 可执行文件路径。
    ///
    /// 默认情况下会自动在系统 PATH 中查找 `hdc`，若需要指定特定版本或
    /// 自定义路径时使用此方法。
    pub fn hdc_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.inner = self.inner.hdc_path(path);
        self
    }

    /// 设置 hdc 服务地址与端口。
    ///
    /// 用于连接远程 hdc 服务端，而非使用本地 hdc 守护进程。
    pub fn hdc_server(mut self, host: impl Into<String>, port: u16) -> Self {
        self.inner = self.inner.hdc_server(host, port);
        self
    }

    /// 设置完整的 hdc 配置。
    ///
    /// 当需要同时配置多项 hdc 参数时，可使用此方法替代逐个设置。
    pub fn hdc_config(mut self, config: HdcConfig) -> Self {
        self.inner = self.inner.hdc_config(config);
        self
    }

    /// 设置 agent 来源（APK 或 Hap 包）。
    pub fn agent_source(mut self, source: AgentSource) -> Self {
        self.inner = self.inner.agent_source(source);
        self
    }

    /// 设置驱动配置项（超时、重试策略等）。
    pub fn driver_config(mut self, config: DriverConfig) -> Self {
        self.inner = self.inner.driver_config(config);
        self
    }

    /// 连接到设备并返回 [`HmDriver`] 实例。
    ///
    /// 此方法会在当前线程阻塞直到连接完成或超时失败。
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
    /// 创建一个默认配置的 `HmDriverBuilder`。
    ///
    /// 使用 builder 模式链式调用配置方法，最后通过 `connect` 建立连接。
    pub fn builder() -> HmDriverBuilder {
        HmDriverBuilder::default()
    }

    /// 静态方法：发现当前 hdc 服务可识别的设备列表。
    ///
    /// 无需建立连接即可枚举设备，返回所有已连接设备的基本描述信息。
    pub fn discover_devices(config: HdcConfig) -> Result<Vec<DeviceDescriptor>> {
        block_on(crate::HmDriver::discover_devices(config))?
    }

    /// 返回当前连接的 agent 配置信息。
    ///
    /// 包含 agent 的版本号、包名、支持的能力等元数据。
    pub fn agent_profile(&self) -> &AgentProfile {
        self.inner.agent_profile()
    }

    /// 返回当前 driver 实例的连接世代号。
    ///
    /// 每次重新连接后此值递增，可用于判断连接状态是否已变化。
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    /// 查询当前设备支持的 API 方言版本。
    pub fn dialect(&self) -> Result<crate::ApiDialect> {
        block_on(self.inner.dialect())?
    }

    /// 尝试恢复 driver 的连接状态。
    ///
    /// 当检测到连接异常时调用此方法可尝试重新建立连接。
    pub fn recover(&self) -> Result<()> {
        block_on(self.inner.recover())?
    }

    /// 关闭当前 driver 连接，释放相关资源。
    ///
    /// 调用后 driver 实例不应再被使用。
    pub fn close(&self) -> Result<()> {
        block_on(self.inner.close())?
    }

    /// 直接调用 Hypium 底层 API。
    ///
    /// 当现有封装方法无法满足需求时，可通过此方法调用任意 Hypium 接口。
    ///
    /// # 参数
    ///
    /// * `api` - Hypium API 名称
    /// * `this` - 可选的调用目标（如元素 ID）
    /// * `args` - 以 JSON Value 形式传入的参数
    pub fn call_hypium_api(&self, api: &str, this: Option<&str>, args: Value) -> Result<Value> {
        block_on(self.inner.call_hypium_api(api, this, args))?
    }

    /// 获取设备屏幕的宽高尺寸。
    pub fn display_size(&self) -> Result<DisplaySize> {
        block_on(self.inner.display_size())?
    }

    /// 获取设备屏幕当前的旋转方向。
    pub fn display_rotation(&self) -> Result<DisplayRotation> {
        block_on(self.inner.display_rotation())?
    }

    /// 设置设备屏幕的旋转方向。
    pub fn set_display_rotation(&self, rotation: DisplayRotation) -> Result<()> {
        block_on(self.inner.set_display_rotation(rotation))?
    }

    /// 获取设备的详细信息（型号、系统版本、分辨率等）。
    pub fn device_info(&self) -> Result<DeviceInfo> {
        block_on(self.inner.device_info())?
    }

    /// 点亮设备屏幕。
    pub fn screen_on(&self) -> Result<()> {
        block_on(self.inner.screen_on())?
    }

    /// 熄灭设备屏幕。
    pub fn screen_off(&self) -> Result<()> {
        block_on(self.inner.screen_off())?
    }

    /// 切换设备屏幕的开关状态（亮/灭）。
    pub fn toggle_screen_power(&self) -> Result<()> {
        block_on(self.inner.toggle_screen_power())?
    }

    /// 获取设备屏幕当前的亮灭状态。
    pub fn screen_state(&self) -> Result<ScreenState> {
        block_on(self.inner.screen_state())?
    }

    /// 获取设备 WLAN 接口的 IP 地址。
    ///
    /// 若设备未连接 WLAN，则返回 `None`。
    pub fn wlan_ip(&self) -> Result<Option<IpAddr>> {
        block_on(self.inner.wlan_ip())?
    }

    /// 解锁设备屏幕。
    ///
    /// 相当于执行滑动解锁操作，具体行为取决于设备当前的锁屏方式。
    pub fn unlock(&self) -> Result<()> {
        block_on(self.inner.unlock())?
    }

    /// 按下指定的原始键码（整数形式）。
    ///
    /// # 参数
    ///
    /// * `key_code` - 键码的整数值，请参考设备厂商提供的键码映射表
    pub fn press_key(&self, key_code: u32) -> Result<()> {
        block_on(self.inner.press_key(key_code))?
    }

    /// 按下指定的键码（使用预定义的 `KeyCode` 枚举）。
    pub fn press_key_code(&self, key_code: KeyCode) -> Result<()> {
        block_on(self.inner.press_key_code(key_code))?
    }

    /// 同时按下多个键（组合键）。
    ///
    /// 例如 `[KeyCode::Ctrl, KeyCode::C]` 可实现复制操作。
    pub fn press_key_combination(&self, key_codes: &[KeyCode]) -> Result<()> {
        block_on(self.inner.press_key_combination(key_codes))?
    }

    /// 模拟按下返回键。
    pub fn go_back(&self) -> Result<()> {
        block_on(self.inner.go_back())?
    }

    /// 模拟按下 Home 键回到桌面。
    pub fn go_home(&self) -> Result<()> {
        block_on(self.inner.go_home())?
    }

    /// 在指定绝对坐标处点击。
    pub fn click(&self, point: Point) -> Result<()> {
        block_on(self.inner.click(point))?
    }

    /// 在指定位置方向（如左上、中心等）处点击。
    ///
    /// 位置基于目标元素的边界框计算。
    pub fn click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.click_position(position))?
    }

    /// 在指定绝对坐标处双击。
    pub fn double_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.double_click(point))?
    }

    /// 在指定位置方向处双击。
    pub fn double_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.double_click_position(position))?
    }

    /// 在指定绝对坐标处长按。
    pub fn long_click(&self, point: Point) -> Result<()> {
        block_on(self.inner.long_click(point))?
    }

    /// 在指定位置方向处长按。
    pub fn long_click_position(&self, position: Position) -> Result<()> {
        block_on(self.inner.long_click_position(position))?
    }

    /// 从起点到终点执行滑动操作（使用绝对坐标）。
    ///
    /// # 参数
    ///
    /// * `from` - 起始坐标
    /// * `to` - 终止坐标
    /// * `speed` - 滑动速度（像素/秒）
    pub fn swipe(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.swipe(from, to, speed))?
    }

    /// 从起点到终点执行滑动操作（使用位置方向）。
    pub fn swipe_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        block_on(self.inner.swipe_positions(from, to, speed))?
    }

    /// 执行拖拽操作（从起点到终点，使用绝对坐标）。
    ///
    /// 拖拽与滑动的区别在于拖拽在终点处有短暂停留。
    pub fn drag(&self, from: Point, to: Point, speed: u32) -> Result<()> {
        block_on(self.inner.drag(from, to, speed))?
    }

    /// 执行拖拽操作（使用位置方向）。
    pub fn drag_positions(&self, from: Position, to: Position, speed: u32) -> Result<()> {
        block_on(self.inner.drag_positions(from, to, speed))?
    }

    /// 执行快速滑动（fling）操作（使用绝对坐标）。
    ///
    /// fling 是一种快速甩动操作，有步长参数控制滑动的粒度。
    pub fn fling(&self, from: Point, to: Point, step_length: u32, speed: u32) -> Result<()> {
        block_on(self.inner.fling(from, to, step_length, speed))?
    }

    /// 执行快速滑动（fling）操作（使用位置方向）。
    pub fn fling_positions(
        &self,
        from: Position,
        to: Position,
        step_length: u32,
        speed: u32,
    ) -> Result<()> {
        block_on(self.inner.fling_positions(from, to, step_length, speed))?
    }

    /// 按指定方向、区域和比例滑动。
    ///
    /// # 参数
    ///
    /// * `direction` - 滑动方向（上/下/左/右）
    /// * `area` - 滑动区域（全屏/局部等）
    /// * `scale` - 滑动距离占区域尺寸的比例（0.0 ~ 1.0）
    /// * `speed` - 滑动速度（像素/秒）
    pub fn swipe_direction(
        &self,
        direction: SwipeDirection,
        area: SwipeArea,
        scale: f64,
        speed: u32,
    ) -> Result<()> {
        block_on(self.inner.swipe_direction(direction, area, scale, speed))?
    }

    /// 执行自定义手势序列。
    ///
    /// 手势由一系列连续的触摸事件组成，可实现复杂交互如画圆、多点触控等。
    pub fn perform_gesture(&self, gesture: &Gesture) -> Result<()> {
        block_on(self.inner.perform_gesture(gesture))?
    }

    /// 在当前焦点处输入文本。
    ///
    /// 文本会直接输入到当前获得焦点的输入控件中。
    pub fn input_text(&self, text: &str) -> Result<()> {
        block_on(self.inner.input_text(text))?
    }

    /// 等待设备进入空闲状态。
    ///
    /// # 参数
    ///
    /// * `idle_time` - 持续空闲多久即视为空闲状态
    /// * `timeout` - 等待的总超时时间
    pub fn wait_for_idle(&self, idle_time: Duration, timeout: Duration) -> Result<()> {
        block_on(self.inner.wait_for_idle(idle_time, timeout))?
    }

    /// 安装应用包（APK/Hap）。
    ///
    /// # 参数
    ///
    /// * `package` - 本地安装包文件路径
    pub fn install_app(&self, package: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.install_app(package))?
    }

    /// 卸载指定应用。
    pub fn uninstall_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.uninstall_app(bundle))?
    }

    /// 启动指定应用的某个 Ability。
    ///
    /// # 参数
    ///
    /// * `bundle` - 应用标识符
    /// * `ability` - 可选的 Ability 名称，不传则启动 Main Ability
    pub fn start_app(&self, bundle: &AppIdentifier, ability: Option<&str>) -> Result<()> {
        block_on(self.inner.start_app(bundle, ability))?
    }

    /// 使用指定模式打开 URL。
    ///
    /// 模式（如浏览器打开、应用内打开等）由 `OpenUrlMode` 指定。
    pub fn open_url(&self, value: &str, mode: OpenUrlMode) -> Result<()> {
        block_on(self.inner.open_url(value, mode))?
    }

    /// 停止指定应用的后台运行。
    pub fn stop_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.stop_app(bundle))?
    }

    /// 清除指定应用的数据。
    pub fn clear_app(&self, bundle: &AppIdentifier) -> Result<()> {
        block_on(self.inner.clear_app(bundle))?
    }

    /// 查询指定应用的主 Ability 名称。
    ///
    /// 返回 `None` 表示未找到主 Ability。
    pub fn main_ability(&self, bundle: &AppIdentifier) -> Result<Option<String>> {
        block_on(self.inner.main_ability(bundle))?
    }

    /// 获取指定应用的详细信息（JSON 格式）。
    pub fn app_info(&self, bundle: &AppIdentifier) -> Result<Value> {
        block_on(self.inner.app_info(bundle))?
    }

    /// 获取指定应用的所有 Ability 列表。
    pub fn app_abilities(&self, bundle: &AppIdentifier) -> Result<Vec<AbilityInfo>> {
        block_on(self.inner.app_abilities(bundle))?
    }

    /// 获取指定应用的主 Ability 详细信息。
    pub fn main_ability_info(&self, bundle: &AppIdentifier) -> Result<Option<AbilityInfo>> {
        block_on(self.inner.main_ability_info(bundle))?
    }

    /// 获取当前前台运行的应用程序信息。
    ///
    /// 返回 `(AppIdentifier, 应用名称)` 元组，若无前台应用则返回 `None`。
    pub fn current_app(&self) -> Result<Option<(AppIdentifier, String)>> {
        block_on(self.inner.current_app())?
    }

    /// 将本地文件推送到设备。
    ///
    /// # 参数
    ///
    /// * `local` - 本地文件路径
    /// * `remote` - 设备上的目标路径
    pub fn push_file(&self, local: impl AsRef<Path>, remote: &str) -> Result<()> {
        block_on(self.inner.push_file(local, remote))?
    }

    /// 从设备拉取文件到本地。
    ///
    /// # 参数
    ///
    /// * `remote` - 设备上的源文件路径
    /// * `local` - 本地目标路径
    pub fn pull_file(&self, remote: &str, local: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.pull_file(remote, local))?
    }

    /// 在设备上执行原始 shell 命令并返回输出。
    ///
    /// # 参数
    ///
    /// * `command` - shell 命令字符串
    pub fn raw_shell(&self, command: &str) -> Result<CommandOutput> {
        block_on(self.inner.raw_shell(command))?
    }

    /// 列出当前所有端口转发规则。
    pub fn list_forwards(&self) -> Result<Vec<ForwardEntry>> {
        block_on(self.inner.list_forwards())?
    }

    /// 添加一条端口转发规则。
    ///
    /// # 参数
    ///
    /// * `local_port` - 本地端口号
    /// * `remote` - 设备上的目标地址（如 `tcp:8080`）
    pub fn forward(&self, local_port: u16, remote: &str) -> Result<()> {
        block_on(self.inner.forward(local_port, remote))?
    }

    /// 移除一条端口转发规则。
    ///
    /// 参数必须与添加时完全一致才能匹配。
    pub fn remove_forward(&self, local_port: u16, remote: &str) -> Result<()> {
        block_on(self.inner.remove_forward(local_port, remote))?
    }

    /// 截取当前屏幕并返回 PNG 字节数据。
    pub fn screenshot(&self) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot())?
    }

    /// 使用指定的截图方法截取屏幕并返回 PNG 字节数据。
    ///
    /// 不同截图方法（如 BMP 编码、JPEG 编码等）可能在速度和质量上有所差异。
    pub fn screenshot_with_method(&self, method: ScreenshotMethod) -> Result<Vec<u8>> {
        block_on(self.inner.screenshot_with_method(method))?
    }

    /// 截取当前屏幕并保存到本地文件。
    pub fn screenshot_to(&self, path: impl AsRef<Path>) -> Result<()> {
        block_on(self.inner.screenshot_to(path))?
    }

    /// 使用指定的截图方法截取屏幕并保存到本地文件。
    pub fn screenshot_to_with_method(
        &self,
        path: impl AsRef<Path>,
        method: ScreenshotMethod,
    ) -> Result<()> {
        block_on(self.inner.screenshot_to_with_method(path, method))?
    }

    /// 获取当前屏幕的 UI 树结构。
    ///
    /// 返回根节点，可通过递归遍历获取完整的界面元素层级。
    pub fn ui_tree(&self) -> Result<UiNode> {
        block_on(self.inner.ui_tree())?
    }

    /// 查找与选择器匹配的第一个控件元素。
    ///
    /// 返回 `None` 表示未找到匹配的元素。
    pub fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let element = block_on(self.inner.find(selector))??;
        Ok(element.map(|inner| Element { inner }))
    }

    /// 查找与选择器匹配的所有控件元素。
    pub fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        Ok(block_on(self.inner.find_all(selector))??
            .into_iter()
            .map(|inner| Element { inner })
            .collect())
    }

    /// 判断与选择器匹配的控件元素是否存在。
    pub fn exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.exists(selector))?
    }

    /// 统计与选择器匹配的控件元素数量。
    pub fn count(&self, selector: &Selector) -> Result<usize> {
        block_on(self.inner.count(selector))?
    }

    /// 如果匹配选择器的控件存在则点击它。
    ///
    /// 返回 `true` 表示已点击，`false` 表示未找到匹配元素。
    pub fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        block_on(self.inner.click_if_exists(selector))?
    }

    /// 等待直到匹配选择器的控件出现，超时时间内返回该控件。
    ///
    /// 超时后仍没有匹配元素则返回 `Err`。
    pub fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        block_on(self.inner.wait_for(selector, timeout))?.map(|inner| Element { inner })
    }

    /// 等待直到条件函数返回 `true`，超时时间内返回结果。
    ///
    /// 条件函数会在循环中被反复调用，函数应返回 `Result<bool>`。
    pub fn wait_until<F>(&self, timeout: Duration, mut condition: F) -> Result<bool>
    where
        F: FnMut() -> Result<bool>,
    {
        block_on(
            self.inner
                .wait_until(timeout, || std::future::ready(condition())),
        )?
    }

    /// 等待直到条件函数返回 `true`，可自定义轮询间隔。
    ///
    /// # 参数
    ///
    /// * `timeout` - 总超时时间
    /// * `interval` - 轮询间隔
    /// * `condition` - 条件判断函数
    pub fn wait_until_with_interval<F>(
        &self,
        timeout: Duration,
        interval: Duration,
        mut condition: F,
    ) -> Result<bool>
    where
        F: FnMut() -> Result<bool>,
    {
        block_on(
            self.inner
                .wait_until_with_interval(timeout, interval, || std::future::ready(condition())),
        )?
    }

    /// 等待直到 XPath 表达式匹配的元素出现。
    ///
    /// 超时后仍不出现则返回 `Err`。
    pub fn wait_for_xpath(&self, expression: &str, timeout: Duration) -> Result<XPathElement> {
        block_on(self.inner.wait_for_xpath(expression, timeout))?
            .map(|inner| XPathElement { inner })
    }

    /// 等待直到 XPath 表达式匹配的元素消失。
    ///
    /// 返回 `true` 表示元素已消失，`false` 表示超时后元素仍然存在。
    pub fn wait_until_xpath_gone(&self, expression: &str, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_until_xpath_gone(expression, timeout))?
    }

    /// 等待指定应用出现在前台（通过轮询判断）。
    ///
    /// 返回 `true` 表示应用已在超时时间内出现。
    pub fn wait_for_app(&self, bundle: &AppIdentifier, timeout: Duration) -> Result<bool> {
        block_on(self.inner.wait_for_app(bundle, timeout))?
    }

    /// 通过 XPath 表达式查找元素，找不到时返回 `Err`。
    pub fn xpath(&self, expression: &str) -> Result<XPathElement> {
        block_on(self.inner.xpath(expression))?.map(|inner| XPathElement { inner })
    }

    /// 通过 XPath 表达式查找元素，找不到时返回 `None`。
    pub fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        Ok(block_on(self.inner.xpath_optional(expression))??.map(|inner| XPathElement { inner }))
    }

    /// 通过 XPath 表达式查找所有匹配的元素。
    pub fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        Ok(block_on(self.inner.xpath_all(expression))??
            .into_iter()
            .map(|inner| XPathElement { inner })
            .collect())
    }

    /// 判断 XPath 表达式是否有任何匹配的元素。
    pub fn xpath_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_exists(expression))?
    }

    /// 如果 XPath 表达式匹配的元素存在则点击它。
    ///
    /// 返回 `true` 表示已点击，`false` 表示未找到匹配元素。
    pub fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        block_on(self.inner.xpath_click_if_exists(expression))?
    }
}
