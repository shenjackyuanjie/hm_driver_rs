//! 通过选择器定位到的远端 UI 控件句柄。

use super::Selector;
use crate::driver::HmDriver;
use crate::{Bounds, DriverError, Result};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};

struct ElementState {
    remote_reference: Option<String>,
    generation: u64,
}

/// 已定位的远端 UI 控件。
pub struct Element {
    driver: HmDriver,
    selector: Selector,
    index: usize,
    state: Mutex<ElementState>,
}

/// 一次性读取的完整控件属性。
#[derive(Clone, Debug, PartialEq)]
pub struct ElementInfo {
    pub id: String,
    pub key: String,
    pub type_name: String,
    pub text: String,
    pub description: String,
    pub hint: String,
    pub selected: bool,
    pub checked: bool,
    pub enabled: bool,
    pub focused: bool,
    pub checkable: bool,
    pub clickable: bool,
    pub long_clickable: bool,
    pub scrollable: bool,
    pub bounds: Bounds,
    pub bounds_center: crate::Point,
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element")
            .field("selector", &self.selector)
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Element {
    pub(crate) fn new(
        driver: HmDriver,
        selector: Selector,
        index: usize,
        remote_reference: String,
        generation: u64,
    ) -> Self {
        Self {
            driver,
            selector,
            index,
            state: Mutex::new(ElementState {
                remote_reference: Some(remote_reference),
                generation,
            }),
        }
    }

    async fn reference(&self) -> Result<String> {
        let generation = self.driver.generation();
        {
            let state = self.state.lock().expect("控件状态锁中毒");
            if state.generation == generation
                && let Some(reference) = &state.remote_reference
            {
                return Ok(reference.clone());
            }
        }
        let references = self.driver.find_remote_references(&self.selector).await?;
        let mut reference = None;
        for (index, candidate) in references.into_iter().enumerate() {
            if index == self.index {
                reference = Some(candidate);
            } else {
                self.driver.queue_remote_reference(candidate, generation);
            }
        }
        let reference = reference.ok_or(DriverError::ElementNotFound)?;
        let mut state = self.state.lock().expect("控件状态锁中毒");
        state.remote_reference = Some(reference.clone());
        state.generation = generation;
        Ok(reference)
    }

    async fn operate(&self, method: &str, args: Value) -> Result<Value> {
        let reference = self.reference().await?;
        let dialect = self.driver.dialect().await?;
        self.driver
            .call_api_raw(
                &format!("{}.{}", dialect.component(), method),
                Some(&reference),
                args,
            )
            .await
    }

    pub async fn attribute(&self, name: &str) -> Result<Value> {
        let method = match name {
            "id" | "key" => "getId",
            "type" => "getType",
            "text" => "getText",
            "description" => "getDescription",
            "hint" => "getHint",
            "selected" => "isSelected",
            "checked" => "isChecked",
            "enabled" => "isEnabled",
            "focused" => "isFocused",
            "checkable" => "isCheckable",
            "clickable" => "isClickable",
            "longClickable" => "isLongClickable",
            "scrollable" => "isScrollable",
            other => return Err(DriverError::Unsupported(format!("未知控件属性：{other}"))),
        };
        self.operate(method, json!([])).await
    }

    pub async fn id(&self) -> Result<String> {
        value_string(self.attribute("id").await?, "id")
    }

    pub async fn key(&self) -> Result<String> {
        value_string(self.attribute("key").await?, "key")
    }

    pub async fn type_name(&self) -> Result<String> {
        value_string(self.attribute("type").await?, "type")
    }

    pub async fn text(&self) -> Result<String> {
        value_string(self.attribute("text").await?, "text")
    }

    pub async fn description(&self) -> Result<String> {
        value_string(self.attribute("description").await?, "description")
    }

    pub async fn hint(&self) -> Result<String> {
        value_string(self.attribute("hint").await?, "hint")
    }

    pub async fn is_selected(&self) -> Result<bool> {
        value_bool(self.attribute("selected").await?, "selected")
    }

    pub async fn is_checked(&self) -> Result<bool> {
        value_bool(self.attribute("checked").await?, "checked")
    }

    pub async fn is_enabled(&self) -> Result<bool> {
        value_bool(self.attribute("enabled").await?, "enabled")
    }

    pub async fn is_focused(&self) -> Result<bool> {
        value_bool(self.attribute("focused").await?, "focused")
    }

    pub async fn is_checkable(&self) -> Result<bool> {
        value_bool(self.attribute("checkable").await?, "checkable")
    }

    pub async fn is_clickable(&self) -> Result<bool> {
        value_bool(self.attribute("clickable").await?, "clickable")
    }

    pub async fn is_long_clickable(&self) -> Result<bool> {
        value_bool(self.attribute("longClickable").await?, "longClickable")
    }

    pub async fn is_scrollable(&self) -> Result<bool> {
        value_bool(self.attribute("scrollable").await?, "scrollable")
    }

    pub async fn bounds(&self) -> Result<Bounds> {
        let value = self.operate("getBounds", json!([])).await?;
        Bounds::parse_value(&value)
            .ok_or_else(|| DriverError::Protocol("控件 bounds 格式无效".into()))
    }

    pub async fn bounds_center(&self) -> Result<crate::Point> {
        Ok(self.bounds().await?.center())
    }

    pub async fn info(&self) -> Result<ElementInfo> {
        let id = self.id().await?;
        let key = self.key().await?;
        let type_name = self.type_name().await?;
        let text = self.text().await?;
        let description = self.description().await?;
        let hint = self.hint().await?;
        let selected = self.is_selected().await?;
        let checked = self.is_checked().await?;
        let enabled = self.is_enabled().await?;
        let focused = self.is_focused().await?;
        let checkable = self.is_checkable().await?;
        let clickable = self.is_clickable().await?;
        let long_clickable = self.is_long_clickable().await?;
        let scrollable = self.is_scrollable().await?;
        let bounds = self.bounds().await?;
        Ok(ElementInfo {
            id,
            key,
            type_name,
            text,
            description,
            hint,
            selected,
            checked,
            enabled,
            focused,
            checkable,
            clickable,
            long_clickable,
            scrollable,
            bounds,
            bounds_center: bounds.center(),
        })
    }

    pub async fn click(&self) -> Result<()> {
        self.operate("click", json!([])).await.map(|_| ())
    }

    pub async fn double_click(&self) -> Result<()> {
        self.operate("doubleClick", json!([])).await.map(|_| ())
    }

    pub async fn long_click(&self) -> Result<()> {
        self.operate("longClick", json!([])).await.map(|_| ())
    }

    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.operate("inputText", json!([text])).await.map(|_| ())
    }

    pub async fn clear_text(&self) -> Result<()> {
        self.operate("clearText", json!([])).await.map(|_| ())
    }

    pub async fn drag_to(&self, target: &Element) -> Result<()> {
        let target = target.reference().await?;
        self.operate("dragTo", json!([target])).await.map(|_| ())
    }

    pub async fn pinch_in(&self, scale: f64) -> Result<()> {
        validate_scale(scale)?;
        self.operate("pinchIn", json!([scale])).await.map(|_| ())
    }

    pub async fn pinch_out(&self, scale: f64) -> Result<()> {
        validate_scale(scale)?;
        self.operate("pinchOut", json!([scale])).await.map(|_| ())
    }

    pub async fn wait_until_gone(&self, timeout: Duration) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            match timeout_at(deadline, self.driver.find(&self.selector)).await {
                Ok(Ok(None)) => return Ok(true),
                Ok(Ok(Some(_))) => {
                    tokio::time::sleep_until(std::cmp::min(
                        Instant::now() + Duration::from_millis(100),
                        deadline,
                    ))
                    .await;
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => return Ok(false),
            }
        }
    }

    /// 等待控件属性变为指定值，超时返回 `false`。
    pub async fn wait_for_attribute(
        &self,
        name: &str,
        expected: &Value,
        timeout: Duration,
    ) -> Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            match timeout_at(deadline, self.attribute(name)).await {
                Ok(Ok(actual)) if actual == *expected => return Ok(true),
                Ok(Ok(_)) => {
                    tokio::time::sleep_until(std::cmp::min(
                        Instant::now() + Duration::from_millis(100),
                        deadline,
                    ))
                    .await;
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => return Ok(false),
            }
        }
    }
}

impl Drop for Element {
    fn drop(&mut self) {
        if let Ok(state) = self.state.lock()
            && let Some(reference) = &state.remote_reference
        {
            self.driver
                .queue_remote_reference(reference.clone(), state.generation);
        }
    }
}

fn validate_scale(scale: f64) -> Result<()> {
    if scale.is_finite() && scale > 0.0 {
        Ok(())
    } else {
        Err(DriverError::InvalidCoordinate("缩放比例必须大于 0".into()))
    }
}

fn value_string(value: Value, name: &str) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| DriverError::Protocol(format!("控件属性 {name} 不是字符串")))
}

fn value_bool(value: Value, name: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| DriverError::Protocol(format!("控件属性 {name} 不是布尔值")))
}
