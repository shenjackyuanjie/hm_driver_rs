//! 公开错误类型的集成回归测试。
//!
//! 覆盖所有错误消息、标准 source 链、复合清理错误、I/O/JSON 转换及 `?` 运算符，
//! 并验证 [`DriverError`] 的标准错误和跨线程约束。测试使用本地构造数据。

use hm_driver_rs::DriverError;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

fn io_error() -> io::Error {
    io::Error::other("模拟 I/O 错误")
}

fn json_error() -> serde_json::Error {
    serde_json::from_str::<serde_json::Value>("!").unwrap_err()
}

#[test]
fn display_preserves_error_messages() {
    use DriverError::*;

    let cases = [
        (
            HdcNotFound,
            "找不到 HDC 可执行文件；请设置 Builder 路径、HDC_PATH 或 PATH",
        ),
        (
            InvalidHdcPath(PathBuf::from("路径/hdc")),
            "HDC 路径不是文件：路径/hdc",
        ),
        (HdcSpawn(io_error()), "启动 HDC 失败：模拟 I/O 错误"),
        (
            HdcTimeout {
                timeout: Duration::from_secs(2),
            },
            "HDC 命令在 2s 后超时",
        ),
        (
            HdcCommand {
                code: Some(1),
                message: "详情".into(),
            },
            "HDC 命令失败（退出码 Some(1)）：详情",
        ),
        (DeviceNotFound, "未发现在线 HarmonyOS 设备"),
        (
            AmbiguousDevice { count: 2 },
            "发现多台在线设备，请显式选择设备（数量：2）",
        ),
        (DeviceOffline, "所选设备不在线"),
        (InvalidDeviceList, "设备列表输出无法解析"),
        (
            InvalidUitestVersion,
            "UITest 版本必须是四段数字，实际输出已隐藏",
        ),
        (
            UnsupportedArchitecture("详情".into()),
            "不支持的设备架构：详情",
        ),
        (
            InvalidAgentCatalog("详情".into()),
            "Agent catalog 无效：详情",
        ),
        (
            AgentNotFound(PathBuf::from("字体 agent.so")),
            "找不到 Agent 文件：字体 agent.so",
        ),
        (AgentVerification("详情".into()), "Agent 校验失败：详情"),
        (AgentStartup("详情".into()), "Agent 启动失败：详情"),
        (Forward("详情".into()), "无法建立 HDC 转发：详情"),
        (RpcConnect(io_error()), "RPC 连接失败：模拟 I/O 错误"),
        (RpcIo(io_error()), "RPC I/O 失败：模拟 I/O 错误"),
        (
            RpcTimeout {
                timeout: Duration::from_millis(150),
            },
            "RPC 请求在 150ms 后超时",
        ),
        (SessionInvalid, "RPC 会话已失效；请调用 recover()"),
        (Protocol("详情".into()), "RPC 协议错误：详情"),
        (Hypium("详情".into()), "Hypium API 返回异常：详情"),
        (Io(io_error()), "文件 I/O 失败：模拟 I/O 错误"),
        (
            InvalidIdentifier("详情".into()),
            "应用或 Ability 标识不合法：详情",
        ),
        (InvalidUrl("详情".into()), "URL 不合法：详情"),
        (InvalidCoordinate("详情".into()), "坐标不合法：详情"),
        (InvalidGesture("详情".into()), "手势不合法：详情"),
        (InvalidArgument("详情".into()), "参数不合法：详情"),
        (ElementNotFound, "未找到控件"),
        (WindowNotFound, "未找到窗口"),
        (InvalidXPath("详情".into()), "XPath 表达式无效：详情"),
        (XPathNotFound, "XPath 未找到节点"),
        (
            BlockingInAsyncContext,
            "阻塞 API 不能在 Tokio 异步上下文中调用",
        ),
        (DriverClosed, "Driver 已关闭"),
        (
            Unsupported("详情".into()),
            "操作不受当前 API 方言支持：详情",
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
        if matches!(&error, HdcSpawn(_) | RpcConnect(_) | RpcIo(_) | Io(_)) {
            assert!(error.source().unwrap().is::<io::Error>());
        } else {
            assert!(error.source().is_none());
        }
    }
    assert_eq!(
        HdcCommand {
            code: None,
            message: String::new()
        }
        .to_string(),
        "HDC 命令失败（退出码 None）："
    );
    let json = json_error();
    let expected = format!("JSON 解析失败：{json}");
    assert_eq!(Json(json).to_string(), expected);
}

#[test]
fn io_and_json_errors_preserve_sources_and_from_conversions() {
    let io = io_error();
    let converted = DriverError::from(io);
    assert!(matches!(&converted, DriverError::Io(_)));
    let mut cases = vec![converted];
    cases.extend([
        DriverError::HdcSpawn(io_error()),
        DriverError::RpcConnect(io_error()),
        DriverError::RpcIo(io_error()),
    ]);
    for error in cases {
        let source = error.source().unwrap();
        let io = source.downcast_ref::<io::Error>().unwrap();
        assert_eq!(io.kind(), io::ErrorKind::Other);
        assert_eq!(io.to_string(), "模拟 I/O 错误");
    }

    let json = DriverError::from(json_error());
    assert!(matches!(&json, DriverError::Json(_)));
    assert!(json.source().unwrap().is::<serde_json::Error>());

    let error = DriverError::Protocol("详情".into());
    assert!(error.source().is_none());
}

#[test]
fn cleanup_errors_preserve_messages_and_source_chains() {
    let error = DriverError::ForwardCleanup {
        local_port: 1234,
        remote: "tcp:8012".into(),
        additional_failures: 2,
        source: Box::new(DriverError::Io(io_error())),
    };
    assert_eq!(
        error.to_string(),
        "清理 HDC forward tcp:1234 -> tcp:8012 失败：文件 I/O 失败：模拟 I/O 错误（另有 2 条清理失败）"
    );
    let source = error.source().unwrap();
    assert!(source.is::<Box<DriverError>>());
    assert!(source.source().unwrap().is::<io::Error>());

    let error = DriverError::ForwardCleanupAfterOperation {
        local_port: 1234,
        remote: "localabstract:uitest_socket".into(),
        additional_failures: 0,
        operation: Box::new(DriverError::SessionInvalid),
        cleanup: Box::new(DriverError::Io(io_error())),
    };
    assert_eq!(
        error.to_string(),
        "操作失败：RPC 会话已失效；请调用 recover()；随后清理 HDC forward tcp:1234 -> localabstract:uitest_socket 也失败：文件 I/O 失败：模拟 I/O 错误（另有 0 条清理失败）"
    );
    // 此变体通过 operation / cleanup 字段保留两个错误。
    assert!(error.source().is_none());
}

#[test]
fn question_mark_converts_io_and_json_errors() {
    fn io_result() -> hm_driver_rs::Result<()> {
        Err::<(), _>(io_error())?;
        Ok(())
    }
    fn json_result() -> hm_driver_rs::Result<()> {
        serde_json::from_str::<serde_json::Value>("!")?;
        Ok(())
    }
    assert!(matches!(io_result(), Err(DriverError::Io(_))));
    assert!(matches!(json_result(), Err(DriverError::Json(_))));
}

#[test]
fn driver_error_implements_standard_error_and_thread_safety_traits() {
    fn assert_traits<T: Error + Send + Sync + 'static>() {}
    assert_traits::<DriverError>();
}
