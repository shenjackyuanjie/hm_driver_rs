use std::path::PathBuf;
use std::time::Duration;

/// 驱动的统一结果类型。
pub type Result<T> = std::result::Result<T, DriverError>;

/// 驱动建立连接或执行操作时可能返回的错误。
#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("找不到 HDC 可执行文件；请设置 Builder 路径、HDC_PATH 或 PATH")]
    HdcNotFound,
    #[error("HDC 路径不是文件：{0}")]
    InvalidHdcPath(PathBuf),
    #[error("启动 HDC 失败：{0}")]
    HdcSpawn(#[source] std::io::Error),
    #[error("HDC 命令在 {timeout:?} 后超时")]
    HdcTimeout { timeout: Duration },
    #[error("HDC 命令失败（退出码 {code:?}）：{message}")]
    HdcCommand { code: Option<i32>, message: String },
    #[error("未发现在线 HarmonyOS 设备")]
    DeviceNotFound,
    #[error("发现多台在线设备，请显式选择设备（数量：{count}）")]
    AmbiguousDevice { count: usize },
    #[error("所选设备不在线")]
    DeviceOffline,
    #[error("设备列表输出无法解析")]
    InvalidDeviceList,
    #[error("UITest 版本必须是四段数字，实际输出已隐藏")]
    InvalidUitestVersion,
    #[error("不支持的设备架构：{0}")]
    UnsupportedArchitecture(String),
    #[error("Agent catalog 无效：{0}")]
    InvalidAgentCatalog(String),
    #[error("找不到 Agent 文件：{0}")]
    AgentNotFound(PathBuf),
    #[error("Agent 校验失败：{0}")]
    AgentVerification(String),
    #[error("Agent 启动失败：{0}")]
    AgentStartup(String),
    #[error("无法建立 HDC 转发：{0}")]
    Forward(String),
    #[error("RPC 连接失败：{0}")]
    RpcConnect(#[source] std::io::Error),
    #[error("RPC I/O 失败：{0}")]
    RpcIo(#[source] std::io::Error),
    #[error("RPC 请求在 {timeout:?} 后超时")]
    RpcTimeout { timeout: Duration },
    #[error("RPC 会话已失效；请调用 recover()")]
    SessionInvalid,
    #[error("RPC 协议错误：{0}")]
    Protocol(String),
    #[error("Hypium API 返回异常：{0}")]
    Hypium(String),
    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("文件 I/O 失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("应用或 Ability 标识不合法：{0}")]
    InvalidIdentifier(String),
    #[error("URL 不合法：{0}")]
    InvalidUrl(String),
    #[error("坐标不合法：{0}")]
    InvalidCoordinate(String),
    #[error("手势不合法：{0}")]
    InvalidGesture(String),
    #[error("未找到控件")]
    ElementNotFound,
    #[error("XPath 表达式无效：{0}")]
    InvalidXPath(String),
    #[error("XPath 未找到节点")]
    XPathNotFound,
    #[error("阻塞 API 不能在 Tokio 异步上下文中调用")]
    BlockingInAsyncContext,
    #[error("Driver 已关闭")]
    DriverClosed,
    #[error("操作不受当前 API 方言支持：{0}")]
    Unsupported(String),
}
