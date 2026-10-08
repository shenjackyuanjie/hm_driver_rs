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
