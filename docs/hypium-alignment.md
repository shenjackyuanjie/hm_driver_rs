# Hypium 对齐记录

对齐版本：`26.0.0.500`。核查日期：2026-10-08。

公开常量 `hm_driver_rs::HYPIUM_ALIGNMENT_VERSION` 保存当前对齐版本。
`AgentCatalog::load()` 使用它校验 `assets/agents.json` 的 ZIP 和 wheel 来源字段。
升级时同时更新常量、来源清单和本记录，并验证 Agent 大小、内容及 SHA-256。

本文件同时作为仓库 Markdown 文档和 crate 的 Rust 代码文档使用。

## 核查来源

依据本地官方 `devecotesting-hypium-26.0.0.500.zip` 中的 Python OHOS 设备/UI API：

- `hypium-26.0.0.500.zip/CHANGELOG.txt`：七类新增能力。
- `hypium-26.0.0.500-py3-none-any.whl`：`action/device/uidriver.py` 的 111 个
  公开入口，以及 `uidriver/ohos/uidriver.py`、`arkuidriver.py`、`by.py`、
  `uicomponent.py`、`uiwindow.py`、`model/basic_data_type.py`。
- OHOS 辅助模块：`clipboard.py`、`time_locale.py`、`system.py`、
  `font_manager.py`、`netcopilot.py`、`mouse.py`。
- `xdevice_devicetest-26.0.0.500-py3-none-any.whl`：五个 UITest Agent。

对照公开入口、底层命令/RPC、参数和生命周期，按下列功能清单记录当前实现。
官方 ZIP、wheel 和 Python 源码作为本地分析材料保存。

## 已有功能

| 功能 | Rust 接口与行为 |
| --- | --- |
| 设备发现与连接 | `discover_devices()`、`HmDriverBuilder::connect()`；选择设备、探测架构/UITest、推送 Agent、HDC forward、创建 RPC Driver |
| Agent 资源 | 五个 Agent 的内容、大小和 SHA-256 与 `.500` 官方包一致；按架构/版本选择，支持内嵌资源与外部目录 |
| 会话恢复与清理 | `recover()`、`close()`、远端引用清理；Element/Window 按代际重新定位 |
| 设备与屏幕信息 | `device_info()`、`display_size()`、`display_size_for()`、`display_rotation()`、`wlan_ip()`；指定显示尺寸要求 API >= 18 |
| 电源与解锁 | `screen_on()`、`screen_off()`、`toggle_screen_power()`、`screen_state()`、`unlock()` |
| 显示旋转角度 | `set_display_rotation()` 设置 0°/90°/180°/270° |
| 触控操作 | 点击、双击、长按、指定时长长按、滑动、拖拽、抛滑、方向/区域滑动；支持绝对和归一化坐标 |
| 按键 | `press_key()`、`press_key_code()`、两/三键组合、返回与主页键 |
| 多指手势 | `Gesture` / `GesturePath` 构造轨迹、停留和暂停，最多十根手指 |
| 指关节敲击 | `knuckle_knock()` 及位置/控件入口；单/双点、单/双次，API >= 22 |
| 指关节轨迹与圈选 | `perform_knuckle_gesture()`、`knuckle_select()` 及位置/控件入口，API >= 22；圈选半径显式校验为至少 50 像素 |
| 鼠标操作 | 点击、双击、长按及指定时长长按、滚轮、移动、轨迹移动、拖拽；点击/滚轮支持最多两个组合键 |
| 触控笔 | 点击、双击、带压力长按、滑动、注入自定义轨迹 |
| 触控板与表冠 | `touchpad_swipe()` 支持方向、手指数、速度和终点停留；`rotate_crown()` 支持正反向和速度 |
| 隐藏键盘与清空焦点文本 | `hide_keyboard()`；`clear_text_on_current_cursor()` 使用全选和删除键；隐藏键盘按官方系统 >= 7.0.0 条件使用 |
| 文本输入 | `Element::input_text()` 向控件输入；当前 `HmDriver::input_text()` 执行带固定 `(1, 1)` 坐标的 `Driver.inputText`，独立焦点输入列在待补项中 |
| 剪贴板 | `write_clipboard()`、`read_clipboard()`、`clear_clipboard()`；支持空字符串和多行，读取保留内容空格，未知回显返回协议错误 |
| 深浅色模式 | `set_view_mode(ViewMode::Dark / Light)` |
| 系统时间/时区 | `set_system_time()`、`system_time()`、`set_timezone()`、`timezone()`；有效日期时间字符串和 IANA 时区 |
| 字体 | `font_name()`、`install_font()`、`uninstall_font()`；唯一临时路径、清理守卫、重复安装视为成功 |
| 网络模拟 | 启用/禁用、场景查询、内置/自定义场景启动、按 ID 停止/删除；显式管理场景生命周期，API >= 20 |
| 应用管理 | 安装/卸载、启动/停止、清除数据、读取应用和 Ability 信息、前台应用列表、URL 打开 |
| 文件与截图 | `push_file()`、`pull_file()`、整屏截图及方式选择、本地保存、自定义 HDC forward |
| Selector 与控件 | 属性/布尔条件、字符串匹配、前后关系、within、窗口和索引；查找、状态/属性读取、控件交互、滚动搜索、捏合 |
| UI 树与 XPath | 布局快照、本地遍历/谓词/Selector 查询、层级/类型/相对路径、JSON 保存/加载、XPath 1.0 |
| 等待与事件 | 显式等待及截止时间、Toast/UI 事件监听和读取、Toast 文本检查 |
| 窗口 | 查找、属性读取、聚焦、移动、调整大小、分屏、最大化/最小化、恢复和关闭 |
| 原始调用 | `call_hypium_api()`、`raw_shell()`；供调用方直接使用设备能力 |
| 阻塞门面 | 默认 `blocking` feature 提供同步 API |

`.500` 更新日志里的指关节、网络模拟、深色模式、时间/时区、隐藏键盘、剪贴板和
字体管理，均在上述已有功能中有对应实现。

## 待补功能：设备与显示

| 待补功能 | 官方入口/行为 | 当前基础 |
| --- | --- | --- |
| 休眠超时设置 | `set_sleep_time()`：确保亮屏，再执行 `power-shell timeout -o <毫秒>` | 已有亮屏/关屏和电源状态 |
| 恢复休眠超时 | `restore_sleep_time()`：`power-shell timeout -r` | 已有 `raw_shell()` |
| 锁屏状态读取 | `is_display_locked()`：解析 `ScreenlockService` 的 `screenLocked` | `screen_state()` 当前读取电源状态 |
| 自动旋转开关 | `set_display_rotation_enabled()` / `setDisplayRotationEnabled(bool)`，API >= 9 | 已有旋转角度设置 |
| 显示密度读取 | `getDisplayDensity()`，API >= 9 | 已有显示像素尺寸 |
| 多屏坐标 | `Point.to_dict()` 可携带 `displayId` | 现有 `Point` 包含 x/y |
| 多屏控件筛选 | `BySelector.inDisplay()` | 已有 Selector、UI 树和指定屏幕尺寸 |
| 多屏布局/坐标操作链 | 指定屏幕的布局、坐标和操作目标 | 已有 `display_size_for()`；待将显示 ID 贯穿查询及操作 |

## 待补功能：输入与手势

| 待补功能 | 官方入口/行为 | 当前基础 |
| --- | --- | --- |
| 当前焦点输入专用路径 | `input_text_on_current_cursor()`：API >= 20，执行 `uitest uiInput text <引用文本>` | 当前 `HmDriver::input_text()` 使用 `(1, 1)` 坐标输入；应优先修正焦点语义并补测试 |
| XPath 文本输入目标保持 | 点击查询目标后向该目标输入 | 当前 `XPathElement::input_text()` 点击后调用 Driver 默认坐标输入；待接入焦点输入或目标控件输入路径 |
| 粘贴输入选项 | `InputTextMode.paste()`；携带模式的 RPC 要求 API >= 20 | 已有文本输入和剪贴板 |
| 追加输入选项 | `InputTextMode.addition()`，API >= 20 | `Element::input_text()` 当前只传文本 |
| 坐标目标文本输入 | 官方 `input_text(target=tuple, text, mode)` | 已有坐标点击、控件文本输入和原始 RPC |
| 按键长按 | `press_key(mode='long')` 使用 `uinput -K` | 已有普通按键和组合键 |
| 按键双击 | `press_key(mode='double')` 使用按下/抬起序列 | 已有普通按键 |
| 光标移动便利入口 | `move_cursor(direction, times)` | 可组合方向/Home/End 按键 |
| 鼠标光标样式读取 | `Mouse.get_pointer_style()`：`uinput -M -q`；API >= 22、系统 >= 6.0.2 | 已有鼠标输入 |
| 自定义光标图片导出 | 拉取原始光标数据，解析并保存 PNG/JPEG，可指定尺寸 | 已有文件传输；待补图像解析 |
| 鼠标滚轮速度选项 | `mouseScroll(..., speed=None)` | 已有 Point、方向、步数和组合键 |
| 拖拽按压/持续时间选项 | `drag(..., press_time, drag_time)`、`dragBetween(..., duration)` | 已有速度型拖拽与自定义手势 |
| 触控笔拖拽便利入口 | `pen_drag()` 生成包含起点保持和移动的手势 | 可用 `perform_pen_gesture()` 组合 |
| 区域/全屏捏合 | `pinch_in/out(area=Rect/None)` | 已有控件捏合和多指轨迹 |
| 捏合方向/dead zone 选项 | 官方便利层支持水平/对角线及边缘范围 | 已有控件捏合和自定义手势 |
| 回桌面手势 | `swipe_to_home()` | 已有主页键、方向滑动和手势 |
| 返回手势 | `swipe_to_back(side, height, times)` | 已有返回键和手势 |
| 最近任务手势 | `swipe_to_recent_task()` | 已有轨迹移动和终点暂停 |

## 待补功能：应用、文件与查询便利入口

| 待补功能 | 官方入口/行为 | 当前基础 |
| --- | --- | --- |
| 应用启动参数 | `start_app(params=...)` | 当前接收 bundle 与 Ability；待设计结构化参数 |
| 安装附加选项 | `install_app(options=...)` | 当前接收安装包路径；待增加参数数组/选项结构 |
| 应用存在性查询 | `has_app()` | 已有 `app_info()` |
| 设备文件存在性查询 | `has_file()` | 已有文件传输与 `raw_shell()` |
| 区域截图裁剪 | `capture_screen(area=...)` | 已有整屏截图；待补本地图像裁剪 |
| 截图直接保存到设备 | `capture_screen(in_pc=False)` 的便利入口 | 已有主机截图和原始设备调用 |
| 控件状态切换便利入口 | `switch_component_status(component, checked)` | 可组合状态读取和点击 |
| 窗口属性断言便利入口 | `check_window()` / `check_current_window()` | 已有窗口查找和属性读取 |
| 窗口存在性断言便利入口 | `check_window_exist()` | 已有 `find_window()`，当前由调用方断言 |
| 控件属性断言便利入口 | `check_component()` | 已有控件属性和状态读取 |
| 控件存在性断言便利入口 | `check_component_exist()` | 已有 `exists()` / `wait_for()`，当前由调用方断言 |
| 托管网络场景会话 | 自动停止上一个场景，停止自定义场景时自动删除配置 | 已有按 ID 的显式启停/删除；托管式便利层待补 |

## 待补功能：上层服务与识别

| 待补功能 | 官方入口/服务 | 当前基础 |
| --- | --- | --- |
| 图像查找 | `find_image()` | 已有截图 |
| 图像点击 | `touch_image()` | 已有截图、坐标点击 |
| 图像存在性检查 | `check_image_exist()` | 待增加匹配算法 |
| AI 控件定位 | 官方 AI widget finder | 已有 Selector、UI 树和 XPath |
| 自动弹窗服务 | `PopWindowService`、弹窗规则 | 已有窗口/控件查询与事件监听 |
| Launcher 工作流 | `Launcher.start_app()`、`clear_recent_task()` 等桌面操作 | 已有应用启动和手势 |
| 动作 hooks | `add_hook()`、`remove_hook()`、`remove_all_hooks()` | 当前由调用方组织操作流程 |
| 全局隐式等待 | `set/get_implicit_wait_time()` | 已有显式等待和截止时间 |
| 框架断言服务 | `Assert` 及报告集成 | 已有状态读取，Rust 调用方可自行断言 |
| HAP / ABC Agent 部署 | 官方 HAP / ABC / bin 模式选择 | 当前部署 `.so` Agent |
| 录屏/Captures | 测试录屏扩展 | 现有整屏截图用于现场分析 |
| 任务调度与运行状态持久化 | 框架级任务服务 | 当前以设备/UI 操作库提供能力 |

## 其他待补扩展

| 待补功能 | 用途 | 当前基础 |
| --- | --- | --- |
| OCR | 从截图识别文字 | 现有 Selector 文本查询与截图 |
| Inspector | 界面检查工具 | 现有 UI 树、XPath 和属性读取 |

## 验证记录

- 已有自动验证：单元测试、模拟 HDC/RPC 调用测试、文档测试、feature 组合编译、
  Clippy、Cargo 打包检查和五个 Agent 的字节/哈希核验。
- 已有真机记录：ARM64、Agent `v1.2.3` 分支；具体设备操作见 `tests/smoke.rs`。
- 待补真机验证：剪贴板、深浅色、系统时间/时区、字体、网络模拟及不同系统工具
  版本的输出和参数单位；其余 Agent 分支的真机覆盖。

本次新增版本常量、来源一致性校验及对齐记录。后续优先补焦点输入路径、输入模式、
旋转开关、显示密度、休眠超时与锁屏状态；逐项补齐后同步更新此清单。
