//! 同步阻塞门面。
//!
//! 所有方法都复用进程级 Tokio runtime。若在 Tokio 异步上下文中调用，会返回明确错误。

mod driver;
mod element;
mod xpath;

pub use driver::{HmDriver, HmDriverBuilder};
pub use element::Element;
pub use xpath::XPathElement;

use crate::Result;
use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn block_on<F: Future>(future: F) -> Result<F::Output> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(crate::DriverError::BlockingInAsyncContext);
    }
    let runtime = if let Some(runtime) = RUNTIME.get() {
        runtime
    } else {
        let runtime = Runtime::new().map_err(crate::DriverError::Io)?;
        let _ = RUNTIME.set(runtime);
        RUNTIME.get().expect("runtime 已初始化")
    };
    Ok(runtime.block_on(future))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_use_inside_async_context() {
        let result = block_on(async { 1_u8 });
        assert!(matches!(
            result,
            Err(crate::DriverError::BlockingInAsyncContext)
        ));
    }
}
