# hm_driver_rs

基于 HDC、官方 HarmonyOS UITest Agent 和 Hypium JSON RPC 的原生 Rust UI
自动化库，提供异步 API 和默认启用的阻塞门面。

- 设备与应用管理、文件传输、截图和屏幕操作；
- Selector、UI 树、XPath、控件交互、窗口和事件监听；
- 多指、指关节、鼠标、触控笔、触控板和表冠输入；
- 剪贴板、深浅色模式、时间/时区、字体和网络模拟。

当前 crate 版本：`1.1.0`。Hypium 对齐版本：`26.0.0.500`，可通过
`hm_driver_rs::HYPIUM_ALIGNMENT_VERSION` 读取。
已有功能和待补功能见 [Hypium 对齐记录](docs/hypium-alignment.md)。

## 快速开始

准备 Rust **1.86+** 和已授权、在线的 HarmonyOS 设备，确认 HDC 能发现设备：

```text
hdc list targets -v
```

添加依赖：

```toml
[dependencies]
hm_driver_rs = "1.1.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

默认启用 `blocking`（同步 API）和 `embedded-agents`（内嵌官方 Agent）。
HDC 可通过 builder、`HDC_PATH` 环境变量或 `PATH` 配置。

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

## 文档

| 文档 | 内容 |
| --- | --- |
| [接入与会话指南](docs/usage.md) | 依赖与 features、阻塞 API、设备/HDC 配置、等待、恢复与清理、原始调用及隐私 |
| [功能与用法](docs/capabilities.md) | 输入手势、系统辅助、应用/文件、Selector、UI 树、XPath、事件和窗口 |
| [Agent 兼容性与来源](docs/agents.md) | 架构/版本选择、transport、内嵌缓存与外部目录 |
| [测试指南](docs/testing.md) | 单元测试、feature 检查与真机冒烟测试 |
| [Hypium 对齐记录](docs/hypium-alignment.md) | 已有功能、逐项待补功能和验证记录（同样收录在 crate 文档中） |
| [API 文档](https://docs.rs/hm_driver_rs) | 类型、方法和参数说明 |

## 致谢与许可

感谢 [hmdriver2](https://github.com/codematrixer/hmdriver2) 提供 API 与线协议参考，
感谢华为 [DevEco Testing Hypium](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/hypium-python-guidelines)
提供官方 Agent 及整体方案。

本 crate 采用 [Apache-2.0](LICENSE) 许可。官方 Agent 的来源与第三方许可记录见
[Agent 资源说明](assets/README.md)和[第三方记录](THIRD_PARTY_NOTICES.md)。
