# 测试指南

[返回首页](../README.md)

本地单元测试：

```text
cargo test --all-features
```

验证关闭默认 feature 后仍可编译：

```text
cargo check --no-default-features
```

## 文档检查

公开项启用 `missing_docs` 警告。Rustdoc 检查缺失说明和内部链接，文档测试验证示例；
连接设备的示例使用 `no_run` 做编译验证，Selector、UI 快照、Gesture 和 Agent 解析
示例直接执行。

PowerShell：

```powershell
$env:RUSTDOCFLAGS = "-D warnings"
cargo doc --all-features --no-deps
cargo doc --no-default-features --no-deps
cargo doc --all-features --no-deps --document-private-items
Remove-Item Env:\RUSTDOCFLAGS
cargo test --doc --all-features
cargo test --doc --no-default-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

最低 Rust 版本为 `1.88`，使用 `cargo +1.88.0 test --all-features` 验证；仅异步和外部
Agent 接入可用 `cargo +1.88.0 test --no-default-features` 验证。

每个 Rust 文件以 `//!` 记录模块职责、关键行为和关联 API，公开项以 `///` 说明；
`--document-private-items` 用于查阅内部拆分模块及其链接。

## 模拟与真机测试

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

完整验证记录见 [Hypium 对齐记录](hypium-alignment.md#验证记录)。
