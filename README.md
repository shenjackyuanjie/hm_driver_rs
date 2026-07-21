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
        .hdc_path("D:/command-line-tools/hdc.exe")
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

首版包含设备与应用基础操作、文件传输、截图、UI 树、Selector、控件交互和 XPath
1.0。暂不包含录屏/Captures 流、Toast watcher、复杂多指轨迹、OCR、Inspector、调度器、
持久化、Android、iOS 或 Windows Service。

## 许可注意事项

官方 UITest Agent 的再分发许可尚未确认，因此本 crate 设置了 `publish = false`，完成
许可审查前不得发布。详细来源见 `assets/README.md` 和 `THIRD_PARTY_NOTICES.md`。
