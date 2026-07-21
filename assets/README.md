# UITest Agent 资源

本目录中的五个动态库逐字节提取自官方
`devecotesting-hypium-6.1.0.210.zip` 软件包内的
`xdevice_devicetest-6.1.0.210-py3-none-any.whl`，其 wheel 内原始目录为
`devicetest/res/prototype/native/`。

文件大小和 SHA-256 摘要固定记录在 `../agents.json`。目前尚未确认这些二进制文件的
再分发许可，因此 crate 设置了 `publish = false`；完成许可审查前不得发布。

本项目将 MIT 许可的 `hmdriver2` 1.4.4 用作 API 和线协议参考，但没有复制其实现。
