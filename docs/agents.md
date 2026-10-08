# Agent 兼容性与来源

[返回首页](../README.md)

仓库随附五个 Agent。来源包、wheel 内原始路径、文件大小和 SHA-256 固定记录在
[`assets/agents.json`](../assets/agents.json)，连接时会严格校验。当前解析规则如下：

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

## Agent 来源

- `AgentSource::Embedded`：使用编译进 crate 的官方 Agent。驱动会先验证内嵌字节，
  再以 SHA-256 为目录名写入私有缓存，并在使用前后重新校验；写入过程使用临时文件
  和重命名。
- `AgentSource::Directory(path)`：从 `path/<file_name>` 读取与 catalog 同名的外部
  Agent 文件，并同样校验大小和 SHA-256。关闭 `embedded-agents` 后必须使用此方式。

## 来源与许可

本 crate 以 Apache-2.0 许可发布。五个官方 UITest Agent 来自 Hypium
`26.0.0.500` 软件包，来源路径、包摘要和许可记录见
[Agent 资源说明](../assets/README.md)及[第三方记录](../THIRD_PARTY_NOTICES.md)。
官方 ZIP、wheel 与 Python 源码用于本地分析，Git 和 Cargo 发布包仅收录所需 Agent。
