# UITest Agent 资源

本目录中的五个动态库与官方
`devecotesting-hypium-26.0.0.500.zip` 软件包内的
`xdevice_devicetest-26.0.0.500-py3-none-any.whl` 逐字节一致，其 wheel 内原始目录为
`devicetest/res/prototype/native/`。

文件大小和 SHA-256 摘要固定记录在 [`agents.json`](agents.json)。2026-10-07 对本地
官方新包进行复核：五个 Agent 的名称、大小、SHA-256 和内容均与此前的 `26.0.0.400`
一致，因此本次只更新来源清单，不替换二进制，也不调整 Agent 选择或 transport 规则。

用于复核的包摘要：

- ZIP SHA-256：`1c66fb288a291e3cd1743a4bf82708752bcef58c230748088194182f304a097f`
- XDevice DeviceTest wheel SHA-256：`810b2d33bb86a6fe91c5f24fe9bc33cc3937e3af84ce1d760e3257bb648f9c06`

官方 ZIP、wheel 与 Python 源码仅供本地分析，不纳入 Git 或 Cargo 发布包。
仓库根目录中的下载包和 `.local-analysis/` 已加入 `.gitignore`。

本 crate 的 Cargo 元数据声明为 `license = "Apache-2.0"`、`publish = true`；
官方 Agent 是第三方二进制，该声明不替代其自身的许可条件。第三方记录见
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md)。

本项目将 MIT 许可的 `hmdriver2` 1.4.4 用作 API 和线协议参考，但没有复制其实现。
