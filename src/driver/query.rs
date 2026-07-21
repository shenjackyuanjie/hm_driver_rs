//! UI 树抓取、选择器查找与 XPath 查询。

use super::{HmDriver, RemoteFileGuard, next_operation_id};
use crate::selector::{Element, Selector};
use crate::ui::UiNode;
use crate::xpath::XPathElement;
use crate::{DriverError, Result};
use serde_json::{Value, json};
use std::future::Future;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::{Instant, timeout_at};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

impl HmDriver {
    pub async fn ui_tree(&self) -> Result<UiNode> {
        let directory = tempdir()?;
        let local = directory.path().join("layout.json");
        let remote = format!("/data/local/tmp/hm_driver_{}.json", next_operation_id());
        let remote_guard = RemoteFileGuard::new(self.inner.hdc.clone(), remote.clone());
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
        remote_guard.cleanup().await;
        result
    }

    pub async fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        let index = selector.selected_index();
        let references = self.find_remote_references(selector).await?;
        let generation = self.generation();
        let mut selected = None;
        for (reference_index, reference) in references.into_iter().enumerate() {
            if reference_index == index {
                selected = Some(reference);
            } else {
                self.queue_remote_reference(reference, generation);
            }
        }
        Ok(selected.map(|reference| {
            Element::new(self.clone(), selector.clone(), index, reference, generation)
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
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(DriverError::ElementNotFound);
            }
            match timeout_at(deadline, self.find(selector)).await {
                Ok(Ok(Some(element))) => return Ok(element),
                Ok(Ok(None)) => sleep_until_next_poll(deadline, DEFAULT_POLL_INTERVAL).await,
                Ok(Err(error)) => return Err(error),
                Err(_) => return Err(DriverError::ElementNotFound),
            }
        }
    }

    /// 在总超时时间内轮询任意异步条件。
    pub async fn wait_until<F, Fut>(&self, timeout: Duration, condition: F) -> Result<bool>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<bool>>,
    {
        self.wait_until_with_interval(timeout, DEFAULT_POLL_INTERVAL, condition)
            .await
    }

    /// 使用指定轮询间隔等待任意异步条件。
    pub async fn wait_until_with_interval<F, Fut>(
        &self,
        timeout: Duration,
        interval: Duration,
        mut condition: F,
    ) -> Result<bool>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<bool>>,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            match timeout_at(deadline, condition()).await {
                Ok(Ok(true)) => return Ok(true),
                Ok(Ok(false)) => sleep_until_next_poll(deadline, interval).await,
                Ok(Err(error)) => return Err(error),
                Err(_) => return Ok(false),
            }
        }
    }

    /// 等待 XPath 节点出现。
    pub async fn wait_for_xpath(
        &self,
        expression: &str,
        timeout: Duration,
    ) -> Result<XPathElement> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(DriverError::XPathNotFound);
            }
            match timeout_at(deadline, self.xpath_optional(expression)).await {
                Ok(Ok(Some(element))) => return Ok(element),
                Ok(Ok(None)) => sleep_until_next_poll(deadline, DEFAULT_POLL_INTERVAL).await,
                Ok(Err(error)) => return Err(error),
                Err(_) => return Err(DriverError::XPathNotFound),
            }
        }
    }

    /// 等待 XPath 节点消失，超时返回 `false`。
    pub async fn wait_until_xpath_gone(&self, expression: &str, timeout: Duration) -> Result<bool> {
        self.wait_until(timeout, || async {
            Ok(self.xpath_optional(expression).await?.is_none())
        })
        .await
    }

    /// 等待指定应用进入前台，超时返回 `false`。
    pub async fn wait_for_app(
        &self,
        bundle: &crate::AppIdentifier,
        timeout: Duration,
    ) -> Result<bool> {
        self.wait_until(timeout, || async {
            Ok(self
                .current_app()
                .await?
                .is_some_and(|(current, _)| current == *bundle))
        })
        .await
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
            .await;
        self.queue_remote_reference(selector_reference, self.generation());
        let result = result?;
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

async fn sleep_until_next_poll(deadline: Instant, interval: Duration) {
    let now = Instant::now();
    if now < deadline {
        tokio::time::sleep_until(std::cmp::min(now + interval, deadline)).await;
    }
}
