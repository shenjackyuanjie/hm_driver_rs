//! 控件选择器：可串联多个官方 On/By 条件，并可解析为远端已定位控件。

mod element;

pub use element::{Element, ElementInfo};

use crate::driver::HmDriver;
use crate::rpc::ApiDialect;
use crate::{DriverError, Result};
use serde_json::{Value, json};
use tracing::trace;

/// 字符串属性的匹配方式。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchPattern {
    /// 精确匹配。
    Equals(String),
    /// 包含匹配。
    Contains(String),
    /// 前缀匹配。
    StartsWith(String),
    /// 后缀匹配。
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
    Within(Box<Selector>),
    InWindow(String),
}

/// 可串联多个官方 On/By 条件的控件选择器。
#[derive(Clone, Debug, Default)]
pub struct Selector {
    conditions: Vec<SelectorCondition>,
    index: usize,
}

impl Selector {
    /// 创建一个空的控件选择器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 按控件的 `id` 属性匹配。
    pub fn id(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("id", pattern.into())
    }

    /// 按控件的 `key` 属性匹配。
    pub fn key(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("key", pattern.into())
    }

    /// 按控件的文本内容匹配。
    pub fn text(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("text", pattern.into())
    }

    /// 按控件的原始文本内容匹配。
    pub fn original_text(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("originalText", pattern.into())
    }

    /// 按控件的类型名称匹配。
    pub fn type_name(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("type", pattern.into())
    }

    /// 按控件的描述内容匹配。
    pub fn description(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("description", pattern.into())
    }

    /// 按控件的提示文本匹配。
    pub fn hint(self, pattern: impl Into<MatchPattern>) -> Self {
        self.string("hint", pattern.into())
    }

    /// 按控件的选中状态匹配。
    pub fn selected(self, value: bool) -> Self {
        self.boolean("selected", value)
    }

    /// 按控件的勾选状态匹配。
    pub fn checked(self, value: bool) -> Self {
        self.boolean("checked", value)
    }

    /// 按控件的启用状态匹配。
    pub fn enabled(self, value: bool) -> Self {
        self.boolean("enabled", value)
    }

    /// 按控件的焦点状态匹配。
    pub fn focused(self, value: bool) -> Self {
        self.boolean("focused", value)
    }

    /// 按控件是否可勾选匹配。
    pub fn checkable(self, value: bool) -> Self {
        self.boolean("checkable", value)
    }

    /// 按控件是否可点击匹配。
    pub fn clickable(self, value: bool) -> Self {
        self.boolean("clickable", value)
    }

    /// 按控件是否可长按匹配。
    pub fn long_clickable(self, value: bool) -> Self {
        self.boolean("longClickable", value)
    }

    /// 按控件是否可滚动匹配。
    pub fn scrollable(self, value: bool) -> Self {
        self.boolean("scrollable", value)
    }

    /// 限定目标控件位于另一个控件之前。
    pub fn before(mut self, selector: Selector) -> Self {
        self.conditions
            .push(SelectorCondition::Before(Box::new(selector)));
        self
    }

    /// 限定目标控件位于另一个控件之后。
    pub fn after(mut self, selector: Selector) -> Self {
        self.conditions
            .push(SelectorCondition::After(Box::new(selector)));
        self
    }

    /// 限定目标控件位于另一个控件内部。
    pub fn within(mut self, selector: Selector) -> Self {
        self.conditions
            .push(SelectorCondition::Within(Box::new(selector)));
        self
    }

    /// 限定目标控件位于指定应用窗口。
    pub fn in_window(mut self, bundle: &crate::AppIdentifier) -> Self {
        self.conditions
            .push(SelectorCondition::InWindow(bundle.as_str().to_owned()));
        self
    }

    /// 设置目标控件在同级匹配结果中的索引（从 0 开始）。
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
        trace!(target: "hm_driver_rs::selector", conditions = self.conditions.len(), "构建远端选择器");
        let dialect = driver.dialect().await?;
        let prefix = dialect.selector();
        let mut current = format!("{prefix}#seed");
        let generation = driver.generation();
        for condition in &self.conditions {
            let (method, args, dependency) = match condition {
                SelectorCondition::String { name, pattern } => {
                    (mapped_attribute(dialect, name), pattern.argument(), None)
                }
                SelectorCondition::Boolean { name, value } => (*name, json!([value]), None),
                SelectorCondition::Before(selector) => {
                    let other = Box::pin(selector.build_remote(driver)).await?;
                    ("isBefore", json!([other]), Some(other))
                }
                SelectorCondition::After(selector) => {
                    let other = Box::pin(selector.build_remote(driver)).await?;
                    ("isAfter", json!([other]), Some(other))
                }
                SelectorCondition::Within(selector) => {
                    let other = Box::pin(selector.build_remote(driver)).await?;
                    ("within", json!([other]), Some(other))
                }
                SelectorCondition::InWindow(bundle) => ("inWindow", json!([bundle]), None),
            };
            let result = driver
                .call_api_raw(&format!("{prefix}.{method}"), Some(&current), args)
                .await;
            driver.queue_remote_reference(current, generation);
            if let Some(dependency) = dependency {
                driver.queue_remote_reference(dependency, generation);
            }
            current = result?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;
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
            for index in 1..=7 {
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
        let selector = Selector::new()
            .id("title")
            .text("设置")
            .original_text("原文")
            .enabled(true)
            .within(Selector::new().type_name("List"))
            .in_window(&crate::AppIdentifier::new("com.example.app").unwrap());
        assert_eq!(selector.build_remote(&driver).await.unwrap(), "On#7");
        assert_eq!(driver.queued_reference_count(), 6);
        assert_eq!(
            *calls.lock().await,
            vec![
                ("On.id".into(), "On#seed".into()),
                ("On.text".into(), "On#1".into()),
                ("On.originalText".into(), "On#2".into()),
                ("On.enabled".into(), "On#3".into()),
                ("On.type".into(), "On#seed".into()),
                ("On.within".into(), "On#4".into()),
                ("On.inWindow".into(), "On#6".into()),
            ]
        );
    }

    #[tokio::test]
    async fn element_scroll_operations_use_component_api() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(TokioMutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            for _ in 0..6 {
                let request: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                let api = request["params"]["api"].as_str().unwrap().to_owned();
                server_calls
                    .lock()
                    .await
                    .push((api.clone(), request["params"]["args"].clone()));
                let result = match api.as_str() {
                    "On.type" => json!("On#container"),
                    "Driver.findComponents" => json!(["Component#container"]),
                    "On.text" => json!("On#target"),
                    "Component.scrollSearch" => json!("Component#target"),
                    _ => Value::Null,
                };
                let response = json!({
                    "request_id": request["request_id"],
                    "result": result,
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
        let container = driver
            .find(&Selector::new().type_name("List"))
            .await
            .unwrap()
            .unwrap();
        container.scroll_to_top().await.unwrap();
        container.scroll_to_bottom_with_speed(1_000).await.unwrap();
        let target = container
            .scroll_search_with_options(&Selector::new().text("设置"), false, Some(20))
            .await
            .unwrap();
        assert!(target.is_some());
        let calls = calls.lock().await;
        assert_eq!(calls[2], ("Component.scrollToTop".into(), json!([600])));
        assert_eq!(calls[3], ("Component.scrollToBottom".into(), json!([1000])));
        assert_eq!(
            calls[5],
            (
                "Component.scrollSearch".into(),
                json!(["On#target", false, 20]),
            )
        );
    }
}
