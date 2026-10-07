# hm_driver_rs

基于 HDC、官方 HarmonyOS UITest Agent 和 Hypium JSON RPC 的原生 Rust UI
自动化驱动。项目以库的形式提供异步 API，并在默认 feature 下提供可选的阻塞门面。

> 当前 crate 版本为 `1.0.1`，本 crate 以 Apache-2.0 许可发布，
> 仓库内嵌的官方 UITest Agent 来源详见[许可注意事项](#许可注意事项)。

## 项目定位

`hm_driver_rs` 不依赖 Hypium 或 XDevice 的 Python 实现，而是直接通过 HDC 启动设备端
官方 UITest Agent，再使用 Hypium JSON RPC 与 Agent 通信。一次连接的大致流程如下：

1. 通过 HDC 发现并选择在线设备；
2. 探测设备架构和 UITest 版本，选择匹配的 Agent；
3. 将 Agent 推送到设备并校验文件大小和 SHA-256；
4. 建立 HDC forward 和本地 RPC 连接；
5. 创建远端 Driver，供后续设备、应用和 UI 操作使用。

它不是命令行工具，也不负责安装 DevEco/HDC；调用方需要先准备好 HDC 和可用的
HarmonyOS 设备。

## 前置条件

- Rust 工具链需要支持 **edition 2024**。
- 主机上需要安装可用的 `hdc`，并确保设备已经被 HDC 识别、在线且已授权。
- HDC 路径按以下优先级解析，并在连接前固化为绝对路径：
  1. `HmDriverBuilder::hdc_path()` 显式设置的路径；
  2. `HDC_PATH` 环境变量；
  3. `PATH` 中的 `hdc`（Windows 下也会查找 `hdc.exe`）。
- 如果使用远程 HDC server，可以通过 `HmDriverBuilder::hdc_server()` 配置，也可以
  同时设置 `HDC_SERVER_HOST` 和 `HDC_SERVER_PORT`。

可以先在终端确认 HDC 能看到设备：

```text
hdc list targets -v
```

## 接入项目
推荐从 crates.io 接入：

```toml
[dependencies]
hm_driver_rs = "1.0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

也可以使用 Git 路径依赖：

```toml
[dependencies]
hm_driver_rs = { git = "https://github.com/shenjackyuanjie/hm_driver_rs.git" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

默认启用两个 feature：

- `blocking`：启用 `hm_driver_rs::blocking` 阻塞门面；
- `embedded-agents`：将仓库内的五个官方 Agent 编译进 crate，并在首次使用时写入
  私有缓存目录。

如果只使用异步 API，可以关闭阻塞门面；如果同时关闭 `embedded-agents`，连接时必须
通过 `AgentSource::Directory` 提供外部 Agent 文件：

```toml
[dependencies]
hm_driver_rs = { path = "../hm_driver_rs", default-features = false }
```

也可以只保留内嵌 Agent 而关闭阻塞 API：

```toml
hm_driver_rs = { path = "../hm_driver_rs", default-features = false, features = ["embedded-agents"] }
```

## 快速开始：异步 API

```rust,no_run
use hm_driver_rs::{AgentSource, DeviceSelector, HmDriver, Result, Selector};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let driver = HmDriver::builder()
        .device(DeviceSelector::Auto)
        .agent_source(AgentSource::Embedded)
        .connect()
        .await?;

    let button = driver
        .wait_for(
            &Selector::new().text("确定").clickable(true),
            Duration::from_secs(3),
        )
        .await?;
    button.click().await?;

    driver.close().await
}
```

`DeviceSelector::Auto` 只会在恰好有一台在线设备时成功。连接多台设备时，应显式使用
`DeviceSelector::Serial(DeviceSerial::new(...))`。

## 阻塞 API

启用默认的 `blocking` feature 后，可以使用 `hm_driver_rs::blocking::HmDriver`。它
共享进程级 Tokio runtime，并将异步 Driver 的常用能力转换为同步调用：

```rust,no_run
use hm_driver_rs::blocking::HmDriver;
use hm_driver_rs::{DeviceSelector, Result, Selector};

fn main() -> Result<()> {
    let driver = HmDriver::builder()
        .device(DeviceSelector::Auto)
        .connect()?;

    if let Some(button) = driver.find(&Selector::new().text("确定"))? {
        button.click()?;
    }

    driver.close()
}
```

不要在 Tokio 异步上下文中调用阻塞门面；此时会返回
`DriverError::BlockingInAsyncContext`，而不是启动嵌套 runtime。

## 设备选择与 HDC

设备发现可以单独执行，不会建立 Agent 会话：

```rust,no_run
use hm_driver_rs::{HdcConfig, HmDriver, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let devices = HmDriver::discover_devices(HdcConfig::default()).await?;
    for device in devices {
        println!("{:?} {:?}", device.status, device.details);
    }
    Ok(())
}
```

设备序列号使用 `DeviceSerial` 包装。其 `Debug` 和 `Display` 输出始终是脱敏值
（分别为 `DeviceSerial(<redacted>)` 和 `<redacted>`）；只有显式调用
`expose_secret()` 才能取得原值，调用方不应把原值写入日志。

`HdcConfig` 可以设置：

- HDC 可执行文件路径和 HDC server 地址；
- 普通命令超时，默认 10 秒；
- 文件传输超时，默认 60 秒；
- Agent 通信超时，默认 10 秒。

Builder 还可通过 `DriverConfig` 调整 RPC 超时（默认 20 秒）、RPC 最大帧大小（默认
8 MiB）、关闭时是否停止设备端 singleness daemon，以及远端引用批量清理阈值。
主机命令使用参数数组调用，不经过主机 shell；`raw_shell()` 则是调用方主动请求在
设备端执行 shell 命令的接口。

## Agent 兼容性

仓库随附五个 Agent。来源包、wheel 内原始路径、文件大小和 SHA-256 固定记录在
[`assets/agents.json`](assets/agents.json)，连接时会严格校验。当前解析规则如下：

当前来源基线为 Hypium `26.0.0.500`。本地包复核确认其五个 Agent 与此前的
`26.0.0.400` 完全一致，因此无需替换二进制，版本分流与 transport 保持不变。

| 设备架构 | Agent | UITest 版本条件 | HDC transport | 验证状态 |
| --- | --- | --- | --- | --- |
| `arm64` | `v1.1.3` | `<= 5.1.1.2` | TCP `8012` | 仅官方参考 |
| `arm64` | `v1.1.5` | `> 5.1.1.2` 且 `<= 5.1.1.3` | TCP `8012` | 仅官方参考 |
| `arm64` | `v1.1.12` | `> 5.1.1.3` 且 `<= 6.0.2.1` | TCP `8012` | 仅官方参考 |
| `arm64` | `v1.2.3` | `> 6.0.2.1` | `localabstract:uitest_socket` | **API 26 真机已验证** |
| `x86_64` | `v1.1.12` | 架构优先于 UITest 版本 | TCP `8012` | 仅官方参考 |

架构会被标准化为 `arm64` 或 `x86_64`，UITest 版本要求为严格的四段式版本号。
`x86_64` 设备固定选择 `v1.1.12`；其他已识别架构按上表的版本边界选择。
使用未完成本地验证的分支时，驱动会发出不包含设备标识的兼容性警告。

### Agent 来源

- `AgentSource::Embedded`：使用编译进 crate 的官方 Agent。驱动会先验证内嵌字节，
  再以 SHA-256 为目录名写入私有缓存，并在使用前后重新校验；写入过程使用临时文件
  和重命名。
- `AgentSource::Directory(path)`：从 `path/<file_name>` 读取与 catalog 同名的外部
  Agent 文件，并同样校验大小和 SHA-256。关闭 `embedded-agents` 后必须使用此方式。

## 已实现的能力

### 设备、屏幕和输入

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

`hide_keyboard()` 依赖设备端 `testhelper`。系统未提供该工具时会返回
`DriverError::Unsupported`，不会把 shell 错误当作成功。

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

### 系统辅助能力（Hypium 26.0.0.500）

以下接口均提供异步和 `blocking` 门面；Rust 实现只调用设备端工具，不加载官方
Python 代码，也不新增 Python 依赖：

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
驱动不按版本字符串提前判定子命令可用性。命令执行失败继续返回 HDC 错误，无法识别的
回显返回 `DriverError::Protocol`，不会默认为成功。新增系统能力目前通过模拟 HDC
调用链验证，**尚未进行真机验证**，不代表所有设备均已支持。

- 时间使用有效的 `YYYY-MM-DD HH:MM:SS` 字符串，时区使用如 `Asia/Shanghai` 的
  IANA 标识；时区有效性最终由设备判断。
- 剪贴板允许多行文本和空字符串，拒绝 NUL；读取时保留内容空格与内部换行，只移除
  回显前缀和一组末尾行结束符。无法识别的输出返回错误，而不是伪装为空剪贴板。
- 字体文件推送到唯一命名的设备端临时路径，完成、错误或取消时尽力清理；已安装同一
  字体视为成功。卸载按字体名称调用，不是传入本地路径。
- `BuiltinNetworkScenario` 提供八个内置场景；`NetworkScenario::new()` 校验自定义
  场景名称与丢包率（`0.0..=1.0`），带宽和延迟使用非负整数，带宽单位沿用设备工具，
  延迟为毫秒。调用时会重新校验公开字段，拒绝 NaN、无穷大等非法参数。
- `start_custom_network_scenario()` 返回新场景 ID；启动失败时尽力删除刚创建的场景。
  通过 ID 显式 `stop_network_scenario()`、`delete_network_scenario()` 管理生命周期；
  停止不会自动删除，`close()` 也不还原系统设置或停止网络模拟。切换场景前应先停止
  原场景，网络配置操作应串行执行，不与其他调用方并发修改设备场景列表。

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

### 应用管理

- `install_app()` / `uninstall_app()`：通过 HDC 安装和卸载应用；
- `start_app()`：指定 bundle 和可选 Ability；不指定 Ability 时自动选择 main Ability；
- `stop_app()`、`clear_app()`；
- `app_info()`：返回 `bm dump` 的原始 JSON；
- `app_abilities()`、`main_ability_info()`、`main_ability()`：解析所有模块的 Ability，
  兼容根级和外层结果对象，并保留每项原始 JSON；
- `current_app()`：读取当前前台应用及 Ability；
- `open_url()`：使用系统浏览器或系统默认路由打开 URL。

`AppIdentifier` 和 Ability 名称会在进入设备命令前校验，避免把非法标识符拼入命令。

### 文件、截图和 HDC forward

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

### UI 树、Selector 和控件

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

### XPath

`xpath()`、`xpath_optional()`、`xpath_all()`、`xpath_exists()` 和
`xpath_click_if_exists()` 在当前 UI 树快照上执行 XPath 1.0 查询。`XPathElement` 保存
查询时的属性和 bounds 快照，并提供：

- 属性、全部属性、文本、bounds 和中心点读取；
- 点击、双击、长按和输入文本。

XPath 查询不是远端 XPath 执行；驱动会先抓取 UI 树，再在主机端构造 XML 并查询。

### Toast、UI 事件和窗口

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

## 等待、超时和会话恢复

- `wait_for()`、`wait_for_xpath()`、`wait_for_ui()` 等等待使用总截止时间；单次慢 RPC
  不会突破调用方给出的超时；
- `wait_for()`、`wait_for_xpath()` 和 UI 树等待在超时后分别返回
  `ElementNotFound` 或 `XPathNotFound`；
- `wait_until()` 和 `wait_until_with_interval()` 用于任意异步条件，超时返回 `false`；
- `wait_for_app()`、`wait_until_xpath_gone()` 和 `Element` 的等待方法覆盖常见状态等待。

每个 Driver 使用单一 RPC 连接，同一时刻最多有一个在途请求。连接断开、RPC 超时或
取消正在进行的请求后，会话立即失效；驱动不会自动重放点击、输入等非幂等操作。此时
应调用 `recover()`，它会重新检查/推送 Agent、建立 forward 和 RPC Driver，并递增
session generation。

`Element` 和 `UiWindow` 持有会话代际信息，恢复后下一次使用时会分别按原 Selector
或 WindowFilter 重新定位；`XPathElement` 是查询快照，恢复后应重新执行 XPath 查询，
不要依赖旧快照代表当前界面。

建议显式调用 `close()`：

- 默认只释放远端引用、删除本次 Driver 创建的 HDC forward，并保留设备端 daemon；
- 将 `DriverConfig::kill_daemon_on_close` 设为 `true` 后，关闭时才会停止精确匹配的
  singleness daemon；
- 如果最后一个 Driver/Element 句柄直接释放，驱动会尽力在后台清理远端引用、自有
  forward 和配置要求停止的 daemon，但该兜底无法返回清理错误，也不替代确定性的
  `close()`。

## 原始 Hypium API

已封装的 API 不足以覆盖场景时，可以使用：

```rust,no_run
use hm_driver_rs::{HmDriver, Result};
use serde_json::json;

async fn raw_api(driver: &HmDriver) -> Result<()> {
    let dialect = driver.dialect().await?;
    let api = format!("{}.getDisplaySize", dialect.driver());
    let _ = driver.call_hypium_api(&api, None, json!([])).await?;
    Ok(())
}
```

`call_hypium_api()` 接收完整 API 名称、可选的远端 `this` 引用和 JSON 参数数组。
连接时会自动协商 `ApiDialect`：现代方言使用 `Driver`/`On`/`Component`，旧方言使用
`UiDriver`/`By`/`UiComponent`。

## 遥测与隐私

`hm_driver_rs` 不加载 Hypium 或 XDevice 的 Python 代码，也不包含其 telemetry 事件
收集与上传逻辑，因此无需运行 `python -m hypium telemetry disable`。驱动的 TCP
连接仅指向 `127.0.0.1`，用于通过 HDC forward 与设备端 UITest Agent 通信；协议中的
`hypium` 和 `xdevice` 字段只是官方 Agent 所要求的 RPC 消息格式。

仓库内嵌的 UITest Agent 来自官方 Hypium 软件包，属于第三方闭源二进制。当前静态检查
未发现其中包含独立的遥测上传逻辑；其来源与许可说明参见
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。

设备序列号默认脱敏，但应用包名、Ability、UI 文本和调用方主动执行的设备命令仍可能
出现在调用方自己的日志或设备状态中；请按实际测试数据制定日志和隐私策略。

## 测试

本地单元测试：

```text
cargo test --all-features
```

验证关闭默认 feature 后仍可编译：

```text
cargo check --no-default-features
```

系统辅助能力的单元测试以模拟 HDC 验证命令参数、shell 引号、工具缺失、异常回显、
低 API Level、自定义网络场景启动失败后的清理，以及字体临时文件清理；不修改真机。
可单独执行 `cargo test --all-features driver::system::tests`。

仓库还提供一个默认忽略的真机冒烟测试。它固定验证 ARM64、UITest Agent `v1.2.3`，
会读取设备状态和 Ability，执行截图、按键、滑动、多指轨迹、UI 树、XPath 和 Selector
操作；测试设备需要安装 `com.chinadaily.har`，并包含 `EntryAbility`。

PowerShell：

```powershell
$env:HM_DRIVER_SMOKE = "1"
$env:HM_DRIVER_DEVICE = "<设备序列号>"
cargo test --test smoke arm64_v123_smoke -- --ignored --nocapture
```

类 Unix shell：

```sh
HM_DRIVER_SMOKE=1 HM_DRIVER_DEVICE=<设备序列号> \
  cargo test --test smoke arm64_v123_smoke -- --ignored --nocapture
```

真机测试会改变屏幕和前台界面。测试代码不会输出或持久化设备序列号。

## 当前范围

当前版本包含设备与应用基础操作、文件传输、截图、UI 树、Selector、控件交互、XPath
1.0、Toast/UI 事件、窗口管理、鼠标/触控笔/触控板/表冠输入、最多十指的自定义轨迹，
以及剪贴板、深浅色模式、时间/时区、字体和网络模拟系统辅助能力。

暂不包含：

- 录屏/Captures 流；
- OCR；
- Inspector；
- 调度器；
- 测试任务与运行状态持久化；
- Android、iOS 或 Windows Service 支持。

## 致谢

感谢 [hmdriver2](https://github.com/codematrixer/hmdriver2) 原始项目提供思路与参考。
本项目使用 MIT 许可的 `hmdriver2` 1.4.4 作为 API 行为和线协议参考，但没有复制其
源代码实现。

感谢华为 [DevEco Testing Hypium](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/hypium-python-guidelines)
提供 `agent.so` 及整体方案。

## 许可注意事项

本 crate 以 Apache-2.0 许可发布。内嵌的官方 UITest Agent 来自华为 Hypium 测试
框架，详细来源见 [`assets/README.md`](assets/README.md) 和
[`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。

`assets/agents/*.so` 逐字节提取自官方
`devecotesting-hypium-26.0.0.500.zip` 软件包内的
`xdevice_devicetest-26.0.0.500-py3-none-any.whl`，其 wheel 内原始目录为
`devicetest/res/prototype/native/`。

五个文件与此前 `26.0.0.400` 来源一致，本次只更新来源钉住信息。
官方下载包和官方 Python 源码仅用于本地分析，不纳入 Git 或 crate 发布包。

Cargo package metadata 当前声明为：

```toml
license = "Apache-2.0"
publish = true
```

第三方软件的具体记录见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)。其中，
`hmdriver2` 1.4.4 为 MIT 许可，仅用于 API/线协议参考；官方 UITest Agent 二进制来自
华为 Hypium 测试框架，本 crate 按 Apache-2.0 发布。
