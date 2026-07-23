//! 远端 HarmonyOS 窗口句柄。

use crate::driver::HmDriver;
use crate::{Bounds, DriverError, ResizeDirection, Result, WindowFilter, WindowMode};
use serde_json::{Value, json};
use std::sync::Mutex;
use tracing::trace;

struct WindowState {
    remote_reference: Option<String>,
    generation: u64,
}

/// 通过 [`HmDriver::find_window`] 定位到的远端窗口。
///
/// Driver 恢复会话后，窗口会使用原始 [`WindowFilter`] 自动重新定位。
pub struct UiWindow {
    driver: HmDriver,
    filter: WindowFilter,
    state: Mutex<WindowState>,
}

impl std::fmt::Debug for UiWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiWindow")
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl UiWindow {
    pub(crate) fn new(
        driver: HmDriver,
        filter: WindowFilter,
        remote_reference: String,
        generation: u64,
    ) -> Self {
        Self {
            driver,
            filter,
            state: Mutex::new(WindowState {
                remote_reference: Some(remote_reference),
                generation,
            }),
        }
    }

    async fn reference(&self) -> Result<String> {
        let generation = self.driver.generation();
        {
            let state = self.state.lock().expect("窗口状态锁中毒");
            if state.generation == generation
                && let Some(reference) = &state.remote_reference
            {
                return Ok(reference.clone());
            }
        }
        let reference = self
            .driver
            .find_window_reference(&self.filter)
            .await?
            .ok_or(DriverError::WindowNotFound)?;
        let mut state = self.state.lock().expect("窗口状态锁中毒");
        state.remote_reference = Some(reference.clone());
        state.generation = generation;
        Ok(reference)
    }

    async fn operate(&self, method: &str, args: Value) -> Result<Value> {
        trace!(target: "hm_driver_rs::window", method, "窗口操作");
        let reference = self.reference().await?;
        let dialect = self.driver.dialect().await?;
        self.driver
            .call_api_raw(
                &format!("{}.{}", dialect.window(), method),
                Some(&reference),
                args,
            )
            .await
    }

    /// 获取窗口所属应用包名。
    pub async fn bundle_name(&self) -> Result<String> {
        value_string(
            self.operate("getBundleName", json!([])).await?,
            "bundleName",
        )
    }

    /// 获取窗口边界。
    pub async fn bounds(&self) -> Result<Bounds> {
        let value = self.operate("getBounds", json!([])).await?;
        Bounds::parse_value(&value)
            .ok_or_else(|| DriverError::Protocol("窗口 bounds 格式无效".into()))
    }

    /// 获取窗口标题。
    pub async fn title(&self) -> Result<String> {
        value_string(self.operate("getTitle", json!([])).await?, "title")
    }

    /// 获取窗口显示模式。
    pub async fn mode(&self) -> Result<WindowMode> {
        let value = self.operate("getWindowMode", json!([])).await?;
        let raw = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
            .ok_or_else(|| DriverError::Protocol("窗口模式不是整数".into()))?;
        WindowMode::try_from(raw)
    }

    /// 判断窗口是否拥有焦点。
    pub async fn is_focused(&self) -> Result<bool> {
        value_bool(self.operate("isFocused", json!([])).await?, "focused")
    }

    /// 判断窗口是否处于活动状态。
    pub async fn is_active(&self) -> Result<bool> {
        let value = match self.operate("isActived", json!([])).await {
            Ok(value) => value,
            Err(DriverError::Hypium(message)) if is_method_not_found(&message) => {
                self.operate("isActive", json!([])).await?
            }
            Err(error) => return Err(error),
        };
        value_bool(value, "active")
    }

    /// 让窗口获得焦点。
    pub async fn focus(&self) -> Result<()> {
        self.operate("focus", json!([])).await.map(|_| ())
    }

    /// 将窗口左上角移动到指定坐标。
    pub async fn move_to(&self, x: i32, y: i32) -> Result<()> {
        self.operate("moveTo", json!([x, y])).await.map(|_| ())
    }

    /// 调整窗口大小。
    pub async fn resize(&self, width: u32, height: u32, direction: ResizeDirection) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(DriverError::InvalidArgument(
                "窗口宽度和高度必须大于 0".into(),
            ));
        }
        self.operate("resize", json!([width, height, direction.value()]))
            .await
            .map(|_| ())
    }

    /// 切换到分屏模式。
    pub async fn split(&self) -> Result<()> {
        self.operate("split", json!([])).await.map(|_| ())
    }

    /// 最大化窗口。
    pub async fn maximize(&self) -> Result<()> {
        self.operate("maximize", json!([])).await.map(|_| ())
    }

    /// 最小化窗口。
    pub async fn minimize(&self) -> Result<()> {
        self.operate("minimize", json!([])).await.map(|_| ())
    }

    /// 恢复窗口之前的显示模式。
    pub async fn resume(&self) -> Result<()> {
        self.operate("resume", json!([])).await.map(|_| ())
    }

    /// 关闭窗口。
    pub async fn close(&self) -> Result<()> {
        self.operate("close", json!([])).await.map(|_| ())
    }
}

impl Drop for UiWindow {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock()
            && let Some(reference) = &state.remote_reference
        {
            self.driver
                .queue_remote_reference(reference.clone(), state.generation);
        }
    }
}

fn value_string(value: Value, name: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| DriverError::Protocol(format!("窗口属性 {name} 不是字符串")))
}

fn value_bool(value: Value, name: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| DriverError::Protocol(format!("窗口属性 {name} 不是布尔值")))
}

fn is_method_not_found(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("method") && (message.contains("not found") || message.contains("undefined"))
}
