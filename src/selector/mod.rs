//! 控件选择器：可串联多个官方 On/By 条件，并可解析为远端已定位控件。

mod element;

pub use element::{Element, ElementInfo};

use crate::driver::HmDriver;
use crate::rpc::ApiDialect;
use crate::{DriverError, Result};
use serde_json::{Value, json};

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
        assert_eq!(driver.queued_reference_count(), 2);
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
