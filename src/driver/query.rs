//! UI 树抓取、选择器查找与 XPath 查询。

use super::{HmDriver, next_operation_id};
use crate::selector::{Element, Selector};
use crate::ui::UiNode;
use crate::xpath::XPathElement;
use crate::{DriverError, Result};
use serde_json::{Value, json};
use std::time::Duration;
use tempfile::tempdir;

impl HmDriver {
    pub async fn ui_tree(&self) -> Result<UiNode> {
        let directory = tempdir()?;
        let local = directory.path().join("layout.json");
        let remote = format!("/data/local/tmp/hm_driver_{}.json", next_operation_id());
        self.inner
            .hdc
            .shell(format!("uitest dumpLayout -p {remote}"))
            .await?;
        let result = async {
            self.inner.hdc.receive_file(&remote, &local).await?;
            let bytes = tokio::fs::read(&local).await?;
            UiNode::from_layout_json(serde_json::from_slice(&bytes)?)
        }
        .await;
        let _ = self.inner.hdc.shell(format!("rm -f {remote}")).await;
        result
    }

    pub async fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let index = selector.selected_index();
        let references = self.find_remote_references(selector).await?;
        Ok(references.get(index).cloned().map(|reference| {
            Element::new(
                self.clone(),
                selector.clone(),
                index,
                reference,
                self.generation(),
            )
        }))
    }

    pub async fn exists(&self, selector: &Selector) -> Result<bool> {
        Ok(self.find(selector).await?.is_some())
    }

    pub async fn count(&self, selector: &Selector) -> Result<usize> {
        Ok(self.find_all(selector).await?.len())
    }

    pub async fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        let Some(element) = self.find(selector).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
    }

    pub async fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        let generation = self.generation();
        Ok(self
            .find_remote_references(selector)
            .await?
            .into_iter()
            .enumerate()
            .map(|(index, reference)| {
                Element::new(self.clone(), selector.clone(), index, reference, generation)
            })
            .collect())
    }

    pub async fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(element) = self.find(selector).await? {
                return Ok(element);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DriverError::ElementNotFound);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn xpath(&self, expression: &str) -> Result<XPathElement> {
        self.xpath_optional(expression)
            .await?
            .ok_or(DriverError::XPathNotFound)
    }

    pub async fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        let root = self.ui_tree().await?;
        Ok(XPathElement::query(self.clone(), &root, expression)?
            .into_iter()
            .next())
    }

    pub async fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)
    }

    pub async fn xpath_exists(&self, expression: &str) -> Result<bool> {
        Ok(!self.xpath_all(expression).await?.is_empty())
    }

    pub async fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        let Some(element) = self.xpath_optional(expression).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
    }

    pub(crate) async fn find_remote_references(&self, selector: &Selector) -> Result<Vec<String>> {
        let selector_reference = selector.build_remote(self).await?;
        let dialect = self.dialect().await?;
        let driver_reference = {
            let state = self.inner.state.lock().await;
            state
                .driver_reference
                .clone()
                .ok_or(DriverError::SessionInvalid)?
        };
        let result = self
            .call_api_raw(
                &format!("{}.findComponents", dialect.driver()),
                Some(&driver_reference),
                json!([selector_reference]),
            )
            .await?;
        self.queue_remote_reference(selector_reference, self.generation());
        match result {
            Value::Null => Ok(Vec::new()),
            Value::String(reference) => Ok(vec![reference]),
            Value::Array(values) => values
                .into_iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        DriverError::Protocol("findComponents 返回了非引用值".into())
                    })
                })
                .collect(),
            _ => Err(DriverError::Protocol("findComponents 响应类型无效".into())),
        }
    }
}
