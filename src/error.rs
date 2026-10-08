use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// 驱动的统一结果类型。
pub type Result<T> = std::result::Result<T, DriverError>;

/// 驱动建立连接或执行操作时可能返回的错误。
///
/// 支持标准 [`Error`] 错误链及 I/O、JSON 错误的自动转换。
#[derive(Debug)]
pub enum DriverError {
    /// 找不到 HDC 可执行文件。
    HdcNotFound,
    /// HDC 路径指向的不是一个文件。
    InvalidHdcPath(PathBuf),
    /// 启动 HDC 进程失败。
    HdcSpawn(std::io::Error),
    /// HDC 命令执行超时。
    HdcTimeout {
        /// 超时时间。
        timeout: Duration,
    },
    /// HDC 命令执行失败，包含退出码和错误消息。
    HdcCommand {
        /// 进程退出码。
        code: Option<i32>,
        /// 错误消息。
        message: String,
    },
    /// 未发现任何在线的 HarmonyOS 设备。
    DeviceNotFound,
    /// 发现多台在线设备，无法确定目标设备。
    AmbiguousDevice {
        /// 在线设备数量。
        count: usize,
    },
    /// 所选设备已不在线。
    DeviceOffline,
    /// 无法解析 HDC 设备列表输出。
    InvalidDeviceList,
    /// UITest 版本号格式无效（需要四段数字）。
    InvalidUitestVersion,
    /// 不支持的设备 CPU 架构。
    UnsupportedArchitecture(String),
    /// Agent catalog 配置或内容无效。
    InvalidAgentCatalog(String),
    /// 找不到指定的 Agent 文件。
    AgentNotFound(PathBuf),
    /// Agent 文件校验（大小或 SHA-256）失败。
    AgentVerification(String),
    /// Agent 启动或初始化失败。
    AgentStartup(String),
    /// 无法建立 HDC 端口转发。
    Forward(String),
    /// 清理 HDC 转发时发生错误。
    ForwardCleanup {
        /// 本地转发端口。
        local_port: u16,
        /// 远端转发目标。
        remote: String,
        /// 额外失败的清理操作数量。
        additional_failures: usize,
        /// 主要的清理失败原因。
        source: Box<DriverError>,
    },
    /// 操作失败后清理 HDC 转发也失败。
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
    RpcConnect(std::io::Error),
    /// RPC 数据读写失败。
    RpcIo(std::io::Error),
    /// RPC 请求超时未响应。
    RpcTimeout {
        /// 超时时间。
        timeout: Duration,
    },
    /// RPC 会话已失效，需调用 recover 恢复。
    SessionInvalid,
    /// RPC 协议层错误（帧格式或连接异常）。
    Protocol(String),
    /// Hypium API 调用返回了异常信息。
    Hypium(String),
    /// JSON 序列化或反序列化失败。
    Json(serde_json::Error),
    /// 文件或网络 I/O 操作失败。
    Io(std::io::Error),
    /// 应用包名或 Ability 名称格式不合法。
    InvalidIdentifier(String),
    /// URL 格式不合法。
    InvalidUrl(String),
    /// 坐标值不合法。
    InvalidCoordinate(String),
    /// 手势描述不合法。
    InvalidGesture(String),
    /// 函数参数不合法。
    InvalidArgument(String),
    /// 未找到匹配的 UI 控件。
    ElementNotFound,
    /// 未找到匹配的窗口。
    WindowNotFound,
    /// XPath 表达式语法无效。
    InvalidXPath(String),
    /// XPath 查询未匹配到任何节点。
    XPathNotFound,
    /// 阻塞 API 在 Tokio 异步上下文中被调用。
    BlockingInAsyncContext,
    /// Driver 实例已关闭，无法继续使用。
    DriverClosed,
    /// 当前 API 方言不支持该操作。
    Unsupported(String),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HdcNotFound => {
                f.write_str("找不到 HDC 可执行文件；请设置 Builder 路径、HDC_PATH 或 PATH")
            }
            Self::InvalidHdcPath(path) => write!(f, "HDC 路径不是文件：{}", path.display()),
            Self::HdcSpawn(error) => write!(f, "启动 HDC 失败：{error}"),
            Self::HdcTimeout { timeout } => write!(f, "HDC 命令在 {timeout:?} 后超时"),
            Self::HdcCommand { code, message } => {
                write!(f, "HDC 命令失败（退出码 {code:?}）：{message}")
            }
            Self::DeviceNotFound => f.write_str("未发现在线 HarmonyOS 设备"),
            Self::AmbiguousDevice { count } => {
                write!(f, "发现多台在线设备，请显式选择设备（数量：{count}）")
            }
            Self::DeviceOffline => f.write_str("所选设备不在线"),
            Self::InvalidDeviceList => f.write_str("设备列表输出无法解析"),
            Self::InvalidUitestVersion => f.write_str("UITest 版本必须是四段数字，实际输出已隐藏"),
            Self::UnsupportedArchitecture(value) => write!(f, "不支持的设备架构：{value}"),
            Self::InvalidAgentCatalog(value) => write!(f, "Agent catalog 无效：{value}"),
            Self::AgentNotFound(path) => write!(f, "找不到 Agent 文件：{}", path.display()),
            Self::AgentVerification(value) => write!(f, "Agent 校验失败：{value}"),
            Self::AgentStartup(value) => write!(f, "Agent 启动失败：{value}"),
            Self::Forward(value) => write!(f, "无法建立 HDC 转发：{value}"),
            Self::ForwardCleanup {
                local_port,
                remote,
                additional_failures,
                source,
            } => write!(
                f,
                "清理 HDC forward tcp:{local_port} -> {remote} 失败：{source}（另有 {additional_failures} 条清理失败）"
            ),
            Self::ForwardCleanupAfterOperation {
                local_port,
                remote,
                additional_failures,
                operation,
                cleanup,
            } => write!(
                f,
                "操作失败：{operation}；随后清理 HDC forward tcp:{local_port} -> {remote} 也失败：{cleanup}（另有 {additional_failures} 条清理失败）"
            ),
            Self::RpcConnect(error) => write!(f, "RPC 连接失败：{error}"),
            Self::RpcIo(error) => write!(f, "RPC I/O 失败：{error}"),
            Self::RpcTimeout { timeout } => write!(f, "RPC 请求在 {timeout:?} 后超时"),
            Self::SessionInvalid => f.write_str("RPC 会话已失效；请调用 recover()"),
            Self::Protocol(value) => write!(f, "RPC 协议错误：{value}"),
            Self::Hypium(value) => write!(f, "Hypium API 返回异常：{value}"),
            Self::Json(error) => write!(f, "JSON 解析失败：{error}"),
            Self::Io(error) => write!(f, "文件 I/O 失败：{error}"),
            Self::InvalidIdentifier(value) => write!(f, "应用或 Ability 标识不合法：{value}"),
            Self::InvalidUrl(value) => write!(f, "URL 不合法：{value}"),
            Self::InvalidCoordinate(value) => write!(f, "坐标不合法：{value}"),
            Self::InvalidGesture(value) => write!(f, "手势不合法：{value}"),
            Self::InvalidArgument(value) => write!(f, "参数不合法：{value}"),
            Self::ElementNotFound => f.write_str("未找到控件"),
            Self::WindowNotFound => f.write_str("未找到窗口"),
            Self::InvalidXPath(value) => write!(f, "XPath 表达式无效：{value}"),
            Self::XPathNotFound => f.write_str("XPath 未找到节点"),
            Self::BlockingInAsyncContext => f.write_str("阻塞 API 不能在 Tokio 异步上下文中调用"),
            Self::DriverClosed => f.write_str("Driver 已关闭"),
            Self::Unsupported(value) => write!(f, "操作不受当前 API 方言支持：{value}"),
        }
    }
}

impl Error for DriverError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::HdcSpawn(error)
            | Self::RpcConnect(error)
            | Self::RpcIo(error)
            | Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::ForwardCleanup { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DriverError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for DriverError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}
