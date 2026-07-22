//! UI 树抓取、选择器查找与 XPath 查询。

use super::{HmDriver, RemoteFileGuard, next_operation_id};
use crate::selector::{Element, MatchPattern, Selector};
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
    /// 获取当前界面的 UI 树（通过 `uitest dumpLayout`）。
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

    /// 使用选择器查找第一个匹配的 UI 元素。
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

    /// 判断选择器是否有匹配的元素。
    pub async fn exists(&self, selector: &Selector) -> Result<bool> {
        Ok(self.find(selector).await?.is_some())
    }

    /// 统计选择器匹配的元素数量。
    pub async fn count(&self, selector: &Selector) -> Result<usize> {
        Ok(self.find_all(selector).await?.len())
    }

    /// 如果元素存在则点击，返回是否点击成功。
    pub async fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        let Some(element) = self.find(selector).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
    }

    /// 查找所有匹配选择器的 UI 元素。
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

    /// 在总超时时间内等待元素出现，超时返回 `Err(ElementNotFound)`。
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

    /// 等待文本内容匹配的节点出现（等于、包含、前缀或后缀）。
    ///
    /// 内部使用 [`wait_for_ui`] 轮询 UI 树，超时返回 `Err(ElementNotFound)`。
    pub async fn wait_for_text(
        &self,
        text: &str,
        pattern: MatchPattern,
        timeout: Duration,
    ) -> Result<UiNode> {
        let owned = text.to_owned();
        self.wait_for_ui(timeout, move |node| {
            let actual = node.attribute("text");
            match &pattern {
                MatchPattern::Equals(_) => actual.as_deref() == Some(&owned),
                MatchPattern::Contains(_) => actual.is_some_and(|v| v.contains(&owned)),
                MatchPattern::StartsWith(_) => actual.is_some_and(|v| v.starts_with(&owned)),
                MatchPattern::EndsWith(_) => actual.is_some_and(|v| v.ends_with(&owned)),
            }
        })
        .await
    }

    /// 在超时时间内轮询 UI 树，直到某个节点满足 `predicate`。
    ///
    /// 返回第一个匹配的节点。超时返回 `Err(ElementNotFound)`。
    pub async fn wait_for_ui(
        &self,
        timeout: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        self.wait_for_ui_with_interval(timeout, DEFAULT_POLL_INTERVAL, predicate)
            .await
    }

    /// 使用指定的轮询间隔等待 UI 节点出现。
    pub async fn wait_for_ui_with_interval(
        &self,
        timeout: Duration,
        interval: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(DriverError::ElementNotFound);
            }
            let tree = self.ui_tree().await?;
            if let Some(node) = tree.find(&predicate) {
                return Ok(node.clone());
            }
            sleep_until_next_poll(deadline, interval).await;
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

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `Err(XPathNotFound)`。
    pub async fn xpath(&self, expression: &str) -> Result<XPathElement> {
        self.xpath_optional(expression)
            .await?
            .ok_or(DriverError::XPathNotFound)
    }

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `None`。
    pub async fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        let root = self.ui_tree().await?;
        Ok(XPathElement::query(self.clone(), &root, expression)?
            .into_iter()
            .next())
    }

    /// 通过 XPath 表达式查找所有匹配的 UI 元素。
    pub async fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)
    }

    /// 判断 XPath 表达式是否有匹配的元素。
    pub async fn xpath_exists(&self, expression: &str) -> Result<bool> {
        Ok(!self.xpath_all(expression).await?.is_empty())
    }

    /// 如果 XPath 匹配的元素存在则点击，返回是否点击成功。
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
            Value::Array(values) => {
                let mut references = Vec::with_capacity(values.len());
                for value in values {
                    let Some(reference) = value.as_str() else {
                        let generation = self.generation();
                        for reference in references {
                            self.queue_remote_reference(reference, generation);
                        }
                        return Err(DriverError::Protocol(
                            "findComponents 返回了非引用值".into(),
                        ));
                    };
                    references.push(reference.to_owned());
                }
                Ok(references)
            }
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
