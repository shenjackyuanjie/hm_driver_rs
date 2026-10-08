# 接入与会话指南

[返回首页](../README.md)

## 项目定位

`hm_driver_rs` 直接通过 HDC 启动设备端官方 UITest Agent，再使用原生 Rust 和
Hypium JSON RPC 与 Agent 通信。一次连接的大致流程如下：

1. 通过 HDC 发现并选择在线设备；
2. 探测设备架构和 UITest 版本，选择匹配的 Agent；
3. 将 Agent 推送到设备并校验文件大小和 SHA-256；
4. 建立 HDC forward 和本地 RPC 连接；
5. 创建远端 Driver，供后续设备、应用和 UI 操作使用。

本项目作为 Rust 库接入应用；调用方准备 HDC 和可用的 HarmonyOS 设备。

## 前置条件

- Rust **1.88+**，使用 **edition 2024**。
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
hm_driver_rs = "1.1.0"
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

- `blocking`：启用 `hm_driver_rs::blocking` 阻塞门面及其多线程 Tokio runtime；
- `embedded-agents`：将仓库内的五个官方 Agent 编译进 crate，并在首次使用时写入
  私有缓存目录。

异步应用按需配置 Tokio 的 runtime 和宏 features；上面的依赖示例已启用
`macros` 与 `rt-multi-thread`。库的异步核心使用 `rt`，测试通过开发依赖启用宏。

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

阻塞门面用于同步上下文；Tokio 异步上下文直接使用异步 API。
在异步上下文调用阻塞门面会返回 `DriverError::BlockingInAsyncContext`。

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

`call_hypium_api()` 支持直接调用 Agent 的原始接口：

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

`hm_driver_rs` 使用原生 Rust 与设备通信。驱动的 TCP 连接指向 `127.0.0.1`，
通过 HDC forward 连接设备端 UITest Agent；协议中的 `hypium` 和 `xdevice` 字段
沿用官方 Agent 的 RPC 消息格式。

仓库内嵌的 UITest Agent 来自官方 Hypium 软件包，属于第三方闭源二进制。当前静态检查
未发现其中包含独立的遥测上传逻辑；其来源与许可说明参见
[第三方记录](../THIRD_PARTY_NOTICES.md)。

设备序列号默认脱敏，但应用包名、Ability、UI 文本和调用方主动执行的设备命令仍可能
出现在调用方自己的日志或设备状态中；请按实际测试数据制定日志和隐私策略。
