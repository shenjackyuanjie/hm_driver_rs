# hm_driver_rs

`hm_driver_rs` 是原生 Rust HarmonyOS UI 自动化驱动。它通过 HDC 启动官方 UITest
Agent，并以 Hypium JSON RPC 提供异步 API 和可选的阻塞门面。

## 使用方式

```rust,no_run
use hm_driver_rs::{AgentSource, DeviceSelector, HmDriver, Selector};

#[tokio::main]
async fn main() -> hm_driver_rs::Result<()> {
    let driver = HmDriver::builder()
        .device(DeviceSelector::Auto)
        .agent_source(AgentSource::Embedded)
        .connect()
        .await?;

    let button = driver
        .wait_for(&Selector::new().text("确定").clickable(true), std::time::Duration::from_secs(3))
        .await?;
    button.click().await?;
    driver.close().await
}
```

阻塞代码使用 `hm_driver_rs::blocking::HmDriver`。阻塞门面共享进程级 Tokio runtime，
若在 Tokio 异步上下文中误用会返回 `BlockingInAsyncContext`，不会触发嵌套 runtime
panic。

## 新增设备与交互能力

坐标 API 使用 `Position` 明确区分绝对坐标与归一化坐标。归一化坐标范围为 `0.0` 到
`1.0`，并始终换算到屏幕内的有效像素。点击、双击、长按、滑动、方向滑动和自定义
手势均支持归一化坐标。

```rust,no_run
use hm_driver_rs::{Gesture, GesturePath, NormalizedPoint, Position, SwipeArea, SwipeDirection};
use std::time::Duration;

# async fn example(driver: &hm_driver_rs::HmDriver) -> hm_driver_rs::Result<()> {
driver.click_position(Position::normalized(0.5, 0.5)?).await?;
driver
    .swipe_direction(SwipeDirection::Up, SwipeArea::FullScreen, 0.8, 2_000)
    .await?;

let left = GesturePath::new(
    Position::Normalized(NormalizedPoint::new(0.4, 0.5)?),
    Duration::from_millis(100),
)?
.move_to(Position::normalized(0.2, 0.5)?, Duration::from_millis(300))?;
let right = GesturePath::new(
    Position::normalized(0.6, 0.5)?,
    Duration::from_millis(100),
)?
.move_to(Position::normalized(0.8, 0.5)?, Duration::from_millis(300))?;
driver.perform_gesture(&Gesture::new(left).add_path(right)?).await?;
# Ok(())
# }
```

`KeyCode` 包含 hmdriver2 参考中的完整 OpenHarmony 按键码，可通过
`press_key_code()` 发送；未类型化的平台扩展码仍可使用 `press_key(u32)`。设备信息另外提供
`screen_state()` 和 `wlan_ip()`，`list_forwards()` 返回已解析的 HDC forward 端点。

## 应用、截图与控件

`app_info()` 返回 `bm dump` 原始 JSON，`app_abilities()` 返回所有模块的 Ability，
`main_ability_info()` 按 launcher、模块主 Ability 和主模块选择入口。Ability 解析兼容根级
以及外层结果对象，且每项保留原始 JSON。`open_url()` 可选择系统浏览器或默认路由。

截图可显式选择 `ScreenshotMethod::SnapshotDisplay` 或 `ScreenshotMethod::ScreenCap`；
`ScreenshotMethod::Auto` 保持优先使用 `snapshot_display`、失败后回退的行为。

`HmDriver` 提供 `exists()`、`count()` 和 `click_if_exists()`。`Element` 提供 `id()`、
`key()`、`type_name()`、`text()`、`description()`、`hint()`、全部布尔状态、
`bounds_center()`、`info()` 和 `wait_until_gone()` 等便利方法。XPath 查询提供可选查询、
条件点击、文本、中心坐标和完整属性快照。

## HDC 与设备选择

HDC 路径按以下优先级解析，并在连接前固化为绝对路径：

1. Builder 显式设置的路径；
2. `HDC_PATH` 环境变量；
3. `PATH` 中的 `hdc`。

HDC server 可由 Builder 显式设置，也可同时设置 `HDC_SERVER_HOST` 和
`HDC_SERVER_PORT`。所有主机命令均使用参数数组调用，不经过主机 shell。

`DeviceSelector::Auto` 只在恰好有一台在线设备时成功。设备序列号的 `Debug` 和
`Display` 输出始终为 `<redacted>`，只有显式调用 `expose_secret()` 才能读取原值。

## Agent 兼容性

五个 Agent 的来源、wheel 内原始路径、大小、SHA-256、架构和严格版本边界记录在
`assets/agents.json`。目前只有 ARM64 `v1.2.2` 分支标记为本地真机已验证；其余官方
分支允许连接，但会产生不包含设备标识的兼容性警告。

内嵌 Agent 在使用前会原子写入私有缓存目录，并重新校验大小和 SHA-256。也可以使用
`AgentSource::Directory` 指向外部官方 Agent 安装目录。

## 会话语义

每个 Driver 使用单一 RPC 连接，同一时刻最多有一个在途请求。连接断开或超时后会话
立即失效，不会自动重放点击、输入等非幂等操作。调用 `recover()` 会重新检查 Agent、
建立 forward 和 Driver，并递增 session generation；旧 `Element` 下次使用时会按原
Selector 重新定位。

建议显式调用 `close()`。默认仅删除本次创建的 HDC forward 并保留 daemon；设置
`DriverConfig::kill_daemon_on_close` 后才会停止精确匹配的 singleness daemon。

## 当前范围

首版包含设备与应用基础操作、文件传输、截图、UI 树、Selector、控件交互、XPath 1.0
和最多十指的自定义轨迹。暂不包含录屏/Captures 流、Toast watcher、OCR、Inspector、
调度器、持久化、Android、iOS 或 Windows Service。

## 许可注意事项

官方 UITest Agent 的再分发许可尚未确认，因此本 crate 设置了 `publish = false`，完成
许可审查前不得发布。详细来源见 `assets/README.md` 和 `THIRD_PARTY_NOTICES.md`。
