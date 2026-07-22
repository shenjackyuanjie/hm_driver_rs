//! 同步阻塞门面（Blocking Facade）。
//!
//! 本模块提供 `HmDriver`、`Element`、`XPathElement` 的同步阻塞 API，
//! 将底层异步操作封装为同步调用。所有方法都复用进程级 Tokio runtime，
//! 避免重复创建 runtime 的开销。
//!
//! # 设计目标
//!
//! - 为需要同步编程模型的调用方（如 CLI 工具、脚本、不支持异步的框架）
//!   提供与异步 API 等价的功能。
//! - 通过全局 `OnceLock<Runtime>` 实现 runtime 的单例化，进程内只创建一个
//!   Tokio runtime 实例。
//!
//! # 注意事项
//!
//! - 若在 Tokio 异步上下文中调用本模块的方法，会返回
//!   [`crate::DriverError::BlockingInAsyncContext`] 错误，避免阻塞
//!   异步执行器导致死锁。

// ---------------------------------------------------------------------------
// 私有子模块
// ---------------------------------------------------------------------------

/// 底层 driver 实现，封装 `fantoccini::Client` 的同步操作。
mod driver;
/// 元素（`Element`）的同步操作封装。
mod element;
/// XPath 相关扩展与元素查找的同步封装。
mod xpath;

// ---------------------------------------------------------------------------
// 公开的类型导出
// ---------------------------------------------------------------------------

/// WebDriver 客户端，提供同步的浏览器自动化操作。
pub use driver::{HmDriver, HmDriverBuilder};
/// 页面元素，提供同步的元素交互方法（点击、输入、属性获取等）。
pub use element::Element;
/// 通过 XPath 定位到的元素集合，提供批量操作方法。
pub use xpath::XPathElement;

use crate::Result;
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tracing::{debug, trace};

// ---------------------------------------------------------------------------
// 全局 Tokio Runtime（进程级单例）
// ---------------------------------------------------------------------------

/// 全局 Tokio runtime 实例，用于将异步操作转换为同步阻塞调用。
///
/// - 使用 `OnceLock` 保证只初始化一次，后续所有 `block_on` 调用直接复用。
/// - 延迟初始化：第一次调用 `block_on` 时才创建，避免进程启动时不必要的开销。
pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// 在同步上下文中阻塞执行一个 Future，返回其结果。
///
/// # 工作流程
///
/// 1. **上下文检测**：通过 `Handle::try_current()` 检查当前是否处于 Tokio
///    异步上下文。若处于异步上下文，直接返回 `BlockingInAsyncContext` 错误。
/// 2. **Runtime 获取**：检查全局 `RUNTIME` 是否已初始化。未初始化则创建
///    一个新的 `Runtime` 并存入 `OnceLock`。
/// 3. **阻塞执行**：使用全局 runtime 的 `block_on` 方法执行传入的 Future，
///    将异步操作同步化。
///
/// # 错误
///
/// - [`crate::DriverError::BlockingInAsyncContext`]：在 Tokio 异步上下文
///   中调用此函数。
/// - [`crate::DriverError::Io`]：创建 Tokio runtime 失败（如系统资源不足）。
///
/// # Panics
///
/// 在 `RUNTIME.set()` 成功但后续 `RUNTIME.get()` 返回 `None` 时 panic。
/// 这在当前实现逻辑下不应发生，属于防御性检查。
fn block_on<F: Future>(future: F) -> Result<F::Output> {
    // 检测是否已在 Tokio 异步上下文中 —— 避免在异步执行器内阻塞导致死锁
    if tokio::runtime::Handle::try_current().is_ok() {
        debug!(target: "hm_driver_rs::blocking", "在异步上下文中调用 block_on，拒绝执行");
        return Err(crate::DriverError::BlockingInAsyncContext);
    }

    // 获取或初始化全局 Tokio runtime
    let runtime = if let Some(runtime) = RUNTIME.get() {
        runtime
    } else {
        let runtime = Runtime::new().map_err(crate::DriverError::Io)?;
        // 忽略 set 失败：第一个成功设置后，后续并发调用会直接走上面分支
        let _ = RUNTIME.set(runtime);
        // 此时必定已初始化，unwrap 安全
        RUNTIME.get().expect("runtime 已初始化")
    };

    // 在全局 runtime 上阻塞执行 future
    trace!(target: "hm_driver_rs::blocking", "block_on 执行 future");
    Ok(runtime.block_on(future))
}

// ---------------------------------------------------------------------------
// 单元测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证在 Tokio 异步上下文中调用 `block_on` 会返回正确的错误。
    ///
    /// 测试用例在 `#[tokio::test]` 异步测试函数内调用 `block_on`，
    /// 预期返回 `Err(BlockingInAsyncContext)`，防止在异步上下文中
    /// 阻塞导致执行器死锁。
    #[tokio::test]
    async fn rejects_use_inside_async_context() {
        let result = block_on(async { 1_u8 });
        assert!(matches!(
            result,
            Err(crate::DriverError::BlockingInAsyncContext)
        ));
    }
}
