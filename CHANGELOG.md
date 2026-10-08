# 更新日志

按版本记录公开 API、行为、依赖和使用要求的变化；新版本记录放在最前面。
开发期间将每次变更补入当前未发布版本，按新增、变更、修复、依赖和文档分类。
发布时将「未发布」替换为实际发布日期（`YYYY-MM-DD`），并固定对应 Git tag。

## 1.1.0（2026-10-08）

### 新增

- 深浅色模式、系统时间和 IANA 时区设置/读取、文本剪贴板及字体管理接口。
- 网络模拟启用/禁用、场景查询、内置/自定义场景启动及按 ID 停止/删除接口。
- `ViewMode`、`BuiltinNetworkScenario`、`NetworkScenario` 和
  `NetworkScenarioInfo` 公开类型；上述系统辅助能力同时提供异步与阻塞 API。
- `HYPIUM_ALIGNMENT_VERSION` 公开常量，记录官方 Hypium 对齐版本并用于
  Agent catalog 来源校验。

### 变更

- 官方 Hypium 对齐版本更新至 `26.0.0.500`；五个 Agent 与 `.400` 的内容及
  SHA-256 一致，沿用现有架构/版本选择和 transport 规则。
- 最低 Rust 版本由 `1.86` 调整为 `1.88`，与现有 let chains 写法一致。

### 修复

- 剪贴板读取按数据前缀识别正文，正确读取含 `Error:`、`[Fail]` 等文本的内容。
- 隐藏键盘的工具探测在 `testhelper` 缺失时返回 `DriverError::Unsupported`。

### 依赖

- Tokio 宏改由开发依赖启用，多线程 runtime 随 `blocking` feature 启用，
  移除未使用的 `test-util`。
- 按使用场景精简 tracing 和 SHA-2 features，移除 `tracing-attributes` 和
  `const-oid` 依赖。
- 移除 `thiserror` 直接依赖，手写 `DriverError` 的 `Display`、`Error` 和
  `From` 实现，保留错误消息、source 链及 I/O、JSON 错误转换。

### 文档与测试

- 补齐阻塞窗口方法及同步门面的 `//!` 文档，统一链接异步 API 的详细说明，
  记录同步条件闭包的执行与超时语义；启用公开项缺失文档警告，补充 Rustdoc、
  文档测试与 MSRV 检查指南。
- 补齐公开 API 的参数、返回值、错误条件和生命周期说明，新增 crate 接入、Selector、
  UI 快照、Gesture 和 Agent 解析文档示例，补充领域模块的 `//!` 职责说明及入口导航。
- 修正恢复后句柄重新定位、`Element::info()` 多次 RPC、XPath 快照存在性、索引和
  类型路径等说明；按当前实现区分异步查询、UI 树采集与同步条件等待的超时范围，
  将 UI 树等待单次采集的截止时间列为待补扩展。
- 精简 README，将接入、功能、Agent 兼容性和测试指南拆到 `docs/`。
- 新增 [Hypium 对齐记录](docs/hypium-alignment.md)，逐项列出已有功能、待补
  功能和验证记录；同一份内容收录在 crate 文档中。
- 明确 Driver 默认坐标文本输入和 XPath 文本输入的现有调用路径，修正 Rustdoc 链接。
- 新增模拟 HDC 系统辅助测试，以及覆盖错误格式、source 链和 `?` 转换的回归测试。
- 建立本更新日志，并在 README 添加入口。

[查看相对 1.0.1 的变更](https://github.com/shenjackyuanjie/hm_driver_rs/compare/v1.0.1...v1.1.0)

## 1.0.1

### 修复

- HDC 失败检测支持行内 `msg:error:` 和 `failed to install` 标记，正确返回
  应用安装失败，并增加对应回归测试。

[查看相对 1.0.0 的变更](https://github.com/shenjackyuanjie/hm_driver_rs/compare/v1.0.0...v1.0.1)

## 1.0.0

### 新增

- 首个 `1.x` 版本，提供基于 HDC 和官方 UITest Agent 的原生 Rust 异步驱动，
  以及默认启用的阻塞门面和内嵌 Agent。
- 设备发现/连接、应用与文件管理、截图、会话恢复和资源清理。
- Selector、控件交互、UI 树、XPath、Toast/UI 事件和窗口管理。
- 触控、多指、指关节、鼠标、触控笔、触控板和表冠输入。

### 许可

- crate 采用 Apache-2.0 许可，官方 Agent 和参考项目记录在
  [第三方说明](THIRD_PARTY_NOTICES.md)中。

[查看 1.0.0 源码](https://github.com/shenjackyuanjie/hm_driver_rs/tree/v1.0.0)
