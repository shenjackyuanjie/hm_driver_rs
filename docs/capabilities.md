# 功能与用法

[返回首页](../README.md)

## 设备、屏幕和输入

- `device_info()`：产品名、型号、品牌、API 版本、系统版本、CPU ABI、WLAN IP、
  显示尺寸和旋转方向；
- `display_size()`、API Level 18 以上可用的 `display_size_for(display_id)`、
  `display_rotation()`、`set_display_rotation()`；
- `screen_on()`、`screen_off()`、`toggle_screen_power()`、`screen_state()` 和
  `unlock()`；
- `press_key_code()` 使用完整的 `KeyCode` 按键枚举，`press_key(u32)` 用于未类型化的
  平台扩展码（接受 `0..=3200`）；
- `press_key_combination()` 支持两个或三个按键，另有 `go_back()` 和 `go_home()`；
- 绝对坐标和归一化坐标的点击、双击、长按，以及可指定持续时间的长按；
- 直线滑动、拖拽、抛滑、按方向滑动和 `wait_for_idle()`；
- `Gesture`/`GesturePath` 自定义多指轨迹，最多十根手指。采样间隔支持 10–100 毫秒，
  注入速度支持 200–40000；
- API Level 22 以上支持单/双指关节的单次或双次敲击、指关节自定义轨迹和闭合圈选，
  并同时提供绝对坐标、归一化位置、控件中心及阻塞接口；
- 鼠标左/中/右键点击、双击、长按、滚轮、移动和拖拽，点击与滚轮支持最多两个组合键；
- 触控笔点击、双击、带压力长按、滑动和自定义轨迹；
- 触控板多指四向滑动及终点停留，手表表冠正反向旋转；
- `hide_keyboard()` 隐藏软键盘，`clear_text_on_current_cursor()` 清空当前焦点输入框。

`hide_keyboard()` 使用设备端 `testhelper`；工具缺失时返回 `DriverError::Unsupported`。

`Position::normalized(x, y)` 的两个值都必须位于 `0.0..=1.0`，会按当前显示区域换算
到有效像素范围 `[0, width - 1]` 和 `[0, height - 1]`。方向滑动支持全屏、绝对坐标区域
和归一化坐标区域。

```rust,no_run
use hm_driver_rs::{Gesture, GesturePath, Position, Result, SwipeArea, SwipeDirection};
use std::time::Duration;

async fn gestures(driver: &hm_driver_rs::HmDriver) -> Result<()> {
    driver
        .click_position(Position::normalized(0.5, 0.5)?)
        .await?;
    driver
        .swipe_direction(SwipeDirection::Up, SwipeArea::FullScreen, 0.8, 2_000)
        .await?;
    driver
        .knuckle_knock_positions(&[Position::normalized(0.5, 0.5)?], 1)
        .await?;
    driver
        .knuckle_select_position(Position::normalized(0.5, 0.5)?, 80, 2_000)
        .await?;

    let path = GesturePath::new(
        Position::normalized(0.4, 0.5)?,
        Duration::from_millis(100),
    )?
    .move_to(
        Position::normalized(0.2, 0.5)?,
        Duration::from_millis(300),
    )?;
    driver.perform_gesture(&Gesture::new(path)).await
}
```

## 系统辅助能力（Hypium 26.0.0.500）

以下接口均提供异步和 `blocking` 门面，通过原生 Rust 调用设备端工具：

| 能力 | 接口 | 官方标注的设备要求 |
| --- | --- | --- |
| 深浅色模式 | `set_view_mode(ViewMode::Dark / Light)` | `testhelper >= 1.0.0` |
| 系统时间 | `set_system_time()`、`system_time()` | `testhelper`，系统版本 >= 7.0.0 |
| IANA 时区 | `set_timezone()`、`timezone()` | `testhelper`，系统版本 >= 7.0.0 |
| 文本剪贴板 | `write_clipboard()`、`read_clipboard()`、`clear_clipboard()` | `testhelper`，系统版本 >= 7.0.0 |
| 字体管理 | `font_name()`、`install_font()`、`uninstall_font()` | `testhelper >= 1.0.0` |
| 网络模拟 | `enable_network_simulation()`、`disable_network_simulation()`、`network_scenarios()`，以及场景启停/删除接口 | `netcopilot`，API Level >= 20、系统版本 >= 6.0.0 |

工具不存在时返回 `DriverError::Unsupported`；网络模拟也会拒绝已知低于 20 的 API
Level，未知 API Level 时按工具能力探测。其余系统/工具版本要求是官方参考条件，
驱动先探测设备端工具，再执行对应子命令。命令执行失败返回 HDC 错误，无法识别的
回显返回 `DriverError::Protocol`。系统辅助能力的单元测试使用模拟 HDC 验证调用链；
真机验证列在[待补验证](hypium-alignment.md#验证记录)中。

- 时间使用有效的 `YYYY-MM-DD HH:MM:SS` 字符串，时区使用如 `Asia/Shanghai` 的
  IANA 标识；时区有效性最终由设备判断。
- 剪贴板允许多行文本和空字符串，拒绝 NUL；读取时保留内容空格与内部换行，只移除
  回显前缀和一组末尾行结束符。无法识别的输出返回协议错误。
- 字体文件推送到唯一命名的设备端临时路径，完成、错误或取消时尽力清理；已安装同一
  字体视为成功。卸载按字体名称调用，不是传入本地路径。
- `BuiltinNetworkScenario` 提供八个内置场景；`NetworkScenario::new()` 校验自定义
  场景名称与丢包率（`0.0..=1.0`），带宽和延迟使用非负整数，带宽单位沿用设备工具，
  延迟为毫秒。调用时会重新校验公开字段，拒绝 NaN、无穷大等非法参数。
- `start_custom_network_scenario()` 返回新场景 ID；启动失败时尽力删除刚创建的场景。
  通过 ID 显式 `stop_network_scenario()`、`delete_network_scenario()` 分别管理停止与删除。
  `close()` 清理会话资源；系统设置恢复和网络模拟停止由调用方安排。切换场景前先停止
  原场景，网络配置操作由调用方串行执行。

```rust,no_run
use hm_driver_rs::{HmDriver, NetworkScenario, Result, ViewMode};

async fn system_helpers(driver: &HmDriver) -> Result<()> {
    driver.set_view_mode(ViewMode::Dark).await?;
    driver.write_clipboard("Hello, HarmonyOS!").await?;
    let _text = driver.read_clipboard().await?;
    let _time = driver.system_time().await?;
    let _timezone = driver.timezone().await?;

    let scenario = NetworkScenario::new("弱网", 100_000, 500_000, 200, 200, 0.05, 0.01)?;
    let id = driver.start_custom_network_scenario(&scenario).await?;
    // 在此执行测试；实际应用应在测试失败时也安排这些清理操作。
    driver.stop_network_scenario(id).await?;
    driver.delete_network_scenario(id).await?;
    driver.disable_network_simulation().await
}
```

这些操作会改变系统设置、剪贴板、已安装字体或网络状态，应只在明确允许修改的测试
设备上调用，并由调用方恢复原状态。

## 应用管理

- `install_app()` / `uninstall_app()`：通过 HDC 安装和卸载应用；
- `start_app()`：指定 bundle 和可选 Ability；不指定 Ability 时自动选择 main Ability；
- `stop_app()`、`clear_app()`；
- `app_info()`：返回 `bm dump` 的原始 JSON；
- `app_abilities()`、`main_ability_info()`、`main_ability()`：解析所有模块的 Ability，
  兼容根级和外层结果对象，并保留每项原始 JSON；
- `current_app()`：读取当前前台应用及 Ability；
- `open_url()`：使用系统浏览器或系统默认路由打开 URL。

`AppIdentifier` 和 Ability 名称会在进入设备命令前校验，避免把非法标识符拼入命令。

## 文件、截图和 HDC forward

- `push_file()` / `pull_file()`：设备与主机之间传输文件；
- `raw_shell()`：执行设备端 shell 并返回 `stdout`、`stderr` 和退出状态；
- `screenshot()` / `screenshot_to()`：自动截图，优先使用 `snapshot_display`，失败时
  回退到 UITest `screenCap`；
- `screenshot_with_method()` / `screenshot_to_with_method()`：显式选择
  `ScreenshotMethod::SnapshotDisplay` 或 `ScreenshotMethod::ScreenCap`；
- `list_forwards()`、`forward()`、`remove_forward()`：查询和管理自定义 HDC forward，
  不与 Driver 自己建立的 RPC forward 混用。

截图和 UI 树抓取使用的设备端临时文件带有清理守卫。调用被取消或发生错误时，驱动会
尽力移除这些临时文件。

## UI 树、Selector 和控件

`ui_tree()` 通过 `uitest dumpLayout` 获取当前界面，适合分析整棵树、父子关系和保存失败
现场。它会传输并解析完整布局，开销明显高于 RPC 元素查询。只需按属性定位或高频轮询
控件时，应优先使用 `Selector` 配合 `find()`、`find_all()`、`exists()` 或 `wait_for()`；
这些接口直接调用 Hypium RPC，不抓取完整 UI 树。

UI 树节点支持属性读取、bounds 解析、深度优先 `find()` 和 `find_all()`；还支持
`/0/1/2` 层级路径、`/Column/Text[1]` 类型路径、以 `..` 表示父节点的相对路径，以及
按 Selector 在本地快照中查询。`save_json()` / `load_json()` 可保存和加载离线快照。
解析器同时接受直接根节点和带 `root` 包装层的 JSON。

`Selector` 支持以下条件的链式组合：

- `id()`、`key()`、`text()`、`original_text()`、`type_name()`、`description()`、`hint()`；
- `selected()`、`checked()`、`enabled()`、`focused()`、`checkable()`、`clickable()`、
  `long_clickable()`、`scrollable()`；
- `before()`、`after()`、`within()`、`in_window()` 和结果 `index()`；
- 字符串匹配支持精确、包含、前缀、后缀、正则表达式和忽略大小写正则匹配
  （`MatchPattern`）。

通过 Driver 可以调用：

- `find()`、`find_all()`、`exists()`、`count()`、`click_if_exists()`；
- `wait_for()`、`wait_for_text()`、`wait_for_ui()`；
- `Element` 的属性、布尔状态、bounds、`all_properties()`、`original_text()`、`info()`；
- 控件点击、双击、长按、相对控件边界偏移点击、输入/清除文本、滚动到顶部/底部、
  横向或纵向滚动搜索、拖到其他控件、捏合缩放以及等待控件消失或属性变化。

Selector 链、未选中的控件引用和滚动搜索产生的临时引用会分批释放，避免长时间轮询
耗尽 Agent 端对象。

`all_properties()` 需要 API Level 12 及以上。`original_text()` 在 API Level 20 及以上
调用 `Component.getOriginalText`，API Level 12 到 19 从 `Component.getAllProperties`
读取；低于 12 返回 `DriverError::Unsupported`。API Level 未知时会按这个顺序做能力探测，
且仅在方法不存在时回退，RPC、协议和设备通信错误仍原样返回。

## XPath

`xpath()`、`xpath_optional()`、`xpath_all()`、`xpath_exists()` 和
`xpath_click_if_exists()` 在当前 UI 树快照上执行 XPath 1.0 查询。`XPathElement` 保存
查询时的属性和 bounds 快照，并提供：

- 属性、全部属性、文本、bounds 和中心点读取；
- 点击、双击、长按和输入文本。

XPath 查询先抓取 UI 树，再在主机端构造 XML 并执行。

## Toast、UI 事件和窗口

- `start_listen_toast()` / `get_latest_toast()`：监听并读取下一条 Toast；
- `start_listen_ui_event()` / `get_latest_ui_event()`：监听 `toastShow` 或 `dialogShow`，
  返回包名、文本、事件类型和 Agent 的扩展字段；
- 同一 Driver 只允许一个尚未读取的事件监听，读取、错误或取消都会结束本次监听；
- `find_window()` 使用 `WindowFilter` 按标题、包名、焦点或活动状态查找窗口；
- `current_window()`、`window_size()` 读取当前窗口；
- `UiWindow` 可读取包名、标题、bounds、模式和状态，并支持聚焦、移动、调整大小、
  分屏、最大化、最小化、恢复和关闭。

`UiWindow` 与 `Element` 一样记录会话代际。调用 `recover()` 后，旧窗口句柄会按原
`WindowFilter` 重新定位。

## 待补功能

焦点输入、输入模式、多屏操作、图像识别、录屏、OCR、Inspector、调度和持久化等
待补功能逐项列在 [Hypium 对齐记录](hypium-alignment.md#待补功能设备与显示)。

其他平台扩展：

- Android 支持；
- iOS 支持；
- Windows Service 支持。
