# 第三方软件声明

## HarmonyOS UITest Agent

文件：`assets/agents/*.so`

来源：官方 `devecotesting-hypium-26.0.0.500.zip` 软件包中的
`xdevice_devicetest-26.0.0.500-py3-none-any.whl`。

本次复核确认五个 Agent 与此前 `26.0.0.400` 来源的文件逐字节一致，未替换二进制。
ZIP、wheel 和官方 Python 源码仅用于本地行为对照，不包含在仓库或 crate 发布包中。

本 crate 以 Apache-2.0 许可发布；这些二进制文件来自华为官方 Hypium 测试框架，来源与校验信息见 `assets/agents.json`。

## hmdriver2

版本 1.4.4，MIT 许可。仅用作 API 行为参考，未包含其源代码实现。
