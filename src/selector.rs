use crate::driver::HmDriver;
use crate::rpc::ApiDialect;
use crate::ui::parse_bounds_value;
use crate::{Bounds, DriverError, Result};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::Duration;

/// 字符串属性的匹配方式。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchPattern {
    Equals(String),
    Contains(String),
    StartsWith(String),
    EndsWith(String),
}

impl MatchPattern {
    fn argument(&self) -> Value {
        let (value, mode) = match self {
            Self::Equals(value) => (value, 0),
            Self::Contains(value) => (value, 1),
            Self::StartsWith(value) => (value, 2),
            Self::EndsWith(value) => (value, 3),
        };
        json!([value, mode])
    }
}

impl From<&str> for MatchPattern {
    fn from(value: &str) -> Self {
        Self::Equals(value.to_owned())
    }
}

impl From<String> for MatchPattern {
    fn from(value: String) -> Self {
        Self::Equals(value)
    }
}

#[derive(Clone, Debug)]
enum SelectorCondition {
    String {
        name: &'static str,
        pattern: MatchPattern,
    },
    Boolean {
        name: &'static str,
        value: bool,
    },
    Before(Box<Selector>),
    After(Box<Selector>),
}

/// 可串联多个官方 On/By 条件的控件选择器。
#[derive(Clone, Debug, Default)]
pub struct Selector {
    conditions: Vec<SelectorCondition>,
    index: usize,
}

impl Selector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("id", pattern.into())
    }

    pub fn key(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("key", pattern.into())
    }

    pub fn text(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("text", pattern.into())
    }

    pub fn type_name(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("type", pattern.into())
    }

    pub fn description(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("description", pattern.into())
    }

    pub fn hint(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("hint", pattern.into())
    }

    pub fn selected(self, value: bool) -> Self {
        self.boolean("selected", value)
    }

    pub fn checked(self, value: bool) -> Self {
        self.boolean("checked", value)
    }

    pub fn enabled(self, value: bool) -> Self {
        self.boolean("enabled", value)
    }

    pub fn focused(self, value: bool) -> Self {
        self.boolean("focused", value)
    }

    pub fn checkable(self, value: bool) -> Self {
        self.boolean("checkable", value)
    }

    pub fn clickable(self, value: bool) -> Self {
        self.boolean("clickable", value)
    }

    pub fn long_clickable(self, value: bool) -> Self {
        self.boolean("longClickable", value)
    }

    pub fn scrollable(self, value: bool) -> Self {
        self.boolean("scrollable", value)
    }

    pub fn before(mut self, selector: Selector) -> Self {
        self.conditions
            .push(SelectorCondition::Before(Box::new(selector)));
        self
    }

    pub fn after(mut self, selector: Selector) -> Self {
        self.conditions
            .push(SelectorCondition::After(Box::new(selector)));
        self
    }

    pub fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    fn string(mut self, name: &'static str, pattern: MatchPattern) -> Self {
        self.conditions
            .push(SelectorCondition::String { name, pattern });
        self
    }

    fn boolean(mut self, name: &'static str, value: bool) -> Self {
        self.conditions
            .push(SelectorCondition::Boolean { name, value });
        self
    }

    pub(crate) async fn build_remote(&self, driver: &HmDriver) -> Result<String> {
        let dialect = driver.dialect().await?;
        let prefix = dialect.selector();
        let mut current = format!("{prefix}#seed");
        for condition in &self.conditions {
            let (method, args) = match condition {
                SelectorCondition::String { name, pattern } => {
                    (mapped_attribute(dialect, name), pattern.argument())
                }
                SelectorCondition::Boolean { name, value } => (*name, json!([value])),
                SelectorCondition::Before(selector) => {
                    let other = Box::pin(selector.build_remote(driver)).await?;
                    ("isBefore", json!([other]))
                }
                SelectorCondition::After(selector) => {
                    let other = Box::pin(selector.build_remote(driver)).await?;
                    ("isAfter", json!([other]))
                }
            };
            let result = driver
                .call_api_raw(&format!("{prefix}.{method}"), Some(&current), args)
                .await?;
            current = result
                .as_str()
                .ok_or_else(|| DriverError::Protocol("Selector API 未返回远端引用".into()))?
                .to_owned();
        }
        Ok(current)
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.index
    }
}

fn mapped_attribute(dialect: ApiDialect, name: &'static str) -> &'static str {
    match (dialect, name) {
        (ApiDialect::Legacy, "id" | "key") => "key",
        _ => name,
    }
}

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
        let reference = references
            .get(self.index)
            .cloned()
            .ok_or(DriverError::ElementNotFound)?;
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
        parse_bounds_value(&value)
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
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if self.driver.find(&self.selector).await?.is_none() {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as TokioMutex;

    #[test]
    fn legacy_maps_id_and_key_to_key() {
        assert_eq!(mapped_attribute(ApiDialect::Legacy, "id"), "key");
        assert_eq!(mapped_attribute(ApiDialect::Legacy, "key"), "key");
        assert_eq!(mapped_attribute(ApiDialect::Modern, "id"), "id");
    }

    #[test]
    fn selector_preserves_condition_order() {
        let selector = Selector::new()
            .id("title")
            .text(MatchPattern::Contains("设置".into()))
            .enabled(true)
            .index(2);
        assert_eq!(selector.conditions.len(), 3);
        assert_eq!(selector.selected_index(), 2);
    }

    #[tokio::test]
    async fn remote_selector_chains_from_previous_result() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(TokioMutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            for index in 1..=3 {
                let line = lines.next_line().await.unwrap().unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                server_calls.lock().await.push((
                    request["params"]["api"].as_str().unwrap().to_owned(),
                    request["params"]["this"].as_str().unwrap().to_owned(),
                ));
                let response = json!({
                    "request_id": request["request_id"],
                    "result": format!("On#{index}"),
                    "exception": null
                });
                writer
                    .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                    .await
                    .unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
        });
        let rpc = crate::rpc::RpcClient::connect(
            port,
            Duration::from_secs(1),
            Duration::from_secs(1),
            1024,
        )
        .await
        .unwrap();
        let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
        let selector = Selector::new().id("title").text("设置").enabled(true);
        assert_eq!(selector.build_remote(&driver).await.unwrap(), "On#3");
        assert_eq!(
            *calls.lock().await,
            vec![
                ("On.id".into(), "On#seed".into()),
                ("On.text".into(), "On#1".into()),
                ("On.enabled".into(), "On#2".into()),
            ]
        );
    }
}
