use std::path::PathBuf;
use std::time::Duration;

/// 驱动的统一结果类型。
pub type Result<T> = std::result::Result<T, DriverError>;

/// 驱动建立连接或执行操作时可能返回的错误。
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    /// 找不到 HDC 可执行文件。
    #[error("找不到 HDC 可执行文件；请设置 Builder 路径、HDC_PATH 或 PATH")]
    HdcNotFound,
    /// HDC 路径指向的不是一个文件。
    #[error("HDC 路径不是文件：{0}")]
    InvalidHdcPath(PathBuf),
    /// 启动 HDC 进程失败。
    #[error("启动 HDC 失败：{0}")]
    HdcSpawn(#[source] std::io::Error),
    /// HDC 命令执行超时。
    #[error("HDC 命令在 {timeout:?} 后超时")]
    HdcTimeout {
        /// 超时时间。
        timeout: Duration,
    },
    /// HDC 命令执行失败，包含退出码和错误消息。
    #[error("HDC 命令失败（退出码 {code:?}）：{message}")]
    HdcCommand {
        /// 进程退出码。
        code: Option<i32>,
        /// 错误消息。
        message: String,
    },
    /// 未发现任何在线的 HarmonyOS 设备。
    #[error("未发现在线 HarmonyOS 设备")]
    DeviceNotFound,
    /// 发现多台在线设备，无法确定目标设备。
    #[error("发现多台在线设备，请显式选择设备（数量：{count}）")]
    AmbiguousDevice {
        /// 在线设备数量。
        count: usize,
    },
    /// 所选设备已不在线。
    #[error("所选设备不在线")]
    DeviceOffline,
    /// 无法解析 HDC 设备列表输出。
    #[error("设备列表输出无法解析")]
    InvalidDeviceList,
    /// UITest 版本号格式无效（需要四段数字）。
    #[error("UITest 版本必须是四段数字，实际输出已隐藏")]
    InvalidUitestVersion,
    /// 不支持的设备 CPU 架构。
    #[error("不支持的设备架构：{0}")]
    UnsupportedArchitecture(String),
    /// Agent catalog 配置或内容无效。
    #[error("Agent catalog 无效：{0}")]
    InvalidAgentCatalog(String),
    /// 找不到指定的 Agent 文件。
    #[error("找不到 Agent 文件：{0}")]
    AgentNotFound(PathBuf),
    /// Agent 文件校验（大小或 SHA-256）失败。
    #[error("Agent 校验失败：{0}")]
    AgentVerification(String),
    /// Agent 启动或初始化失败。
    #[error("Agent 启动失败：{0}")]
    AgentStartup(String),
    /// 无法建立 HDC 端口转发。
    #[error("无法建立 HDC 转发：{0}")]
    Forward(String),
    /// 清理 HDC 转发时发生错误。
    #[error(
        "清理 HDC forward tcp:{local_port} -> {remote} 失败：{source}（另有 {additional_failures} 条清理失败）"
    )]
    ForwardCleanup {
        /// 本地转发端口。
        local_port: u16,
        /// 远端转发目标。
        remote: String,
        /// 额外失败的清理操作数量。
        additional_failures: usize,
        /// 主要的清理失败原因。
        #[source]
        source: Box<DriverError>,
    },
    /// 操作失败后清理 HDC 转发也失败。
    #[error(
        "操作失败：{operation}；随后清理 HDC forward tcp:{local_port} -> {remote} 也失败：{cleanup}（另有 {additional_failures} 条清理失败）"
    )]
    ForwardCleanupAfterOperation {
        /// 本地转发端口。
        local_port: u16,
        /// 远端转发目标。
        remote: String,
        /// 额外失败的清理操作数量。
        additional_failures: usize,
        /// 导致清理失败的原操作错误。
        operation: Box<DriverError>,
        /// 清理操作的错误原因。
        cleanup: Box<DriverError>,
    },
    /// RPC 连接建立失败。
    #[error("RPC 连接失败：{0}")]
    RpcConnect(#[source] std::io::Error),
    /// RPC 数据读写失败。
    #[error("RPC I/O 失败：{0}")]
    RpcIo(#[source] std::io::Error),
    /// RPC 请求超时未响应。
    #[error("RPC 请求在 {timeout:?} 后超时")]
    RpcTimeout {
        /// 超时时间。
        timeout: Duration,
    },
    /// RPC 会话已失效，需调用 recover 恢复。
    #[error("RPC 会话已失效；请调用 recover()")]
    SessionInvalid,
    /// RPC 协议层错误（帧格式或连接异常）。
    #[error("RPC 协议错误：{0}")]
    Protocol(String),
    /// Hypium API 调用返回了异常信息。
    #[error("Hypium API 返回异常：{0}")]
    Hypium(String),
    /// JSON 序列化或反序列化失败。
    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),
    /// 文件或网络 I/O 操作失败。
    #[error("文件 I/O 失败：{0}")]
    Io(#[from] std::io::Error),
    /// 应用包名或 Ability 名称格式不合法。
    #[error("应用或 Ability 标识不合法：{0}")]
    InvalidIdentifier(String),
    /// URL 格式不合法。
    #[error("URL 不合法：{0}")]
    InvalidUrl(String),
    /// 坐标值不合法。
    #[error("坐标不合法：{0}")]
    InvalidCoordinate(String),
    /// 手势描述不合法。
    #[error("手势不合法：{0}")]
    InvalidGesture(String),
    /// 函数参数不合法。
    #[error("参数不合法：{0}")]
    InvalidArgument(String),
    /// 未找到匹配的 UI 控件。
    #[error("未找到控件")]
    ElementNotFound,
    /// 未找到匹配的窗口。
    #[error("未找到窗口")]
    WindowNotFound,
    /// XPath 表达式语法无效。
    #[error("XPath 表达式无效：{0}")]
    InvalidXPath(String),
    /// XPath 查询未匹配到任何节点。
    #[error("XPath 未找到节点")]
    XPathNotFound,
    /// 阻塞 API 在 Tokio 异步上下文中被调用。
    #[error("阻塞 API 不能在 Tokio 异步上下文中调用")]
    BlockingInAsyncContext,
    /// Driver 实例已关闭，无法继续使用。
    #[error("Driver 已关闭")]
    DriverClosed,
    /// 当前 API 方言不支持该操作。
    #[error("操作不受当前 API 方言支持：{0}")]
    Unsupported(String),
}
