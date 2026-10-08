//! UI 树采集、远端 Selector 定位、主机 XPath 与显式等待。
//!
//! Selector 返回远端控件句柄，UI 树与 XPath 返回主机快照。
//! 异步条件/查询等待在 Future 上施加截止时间；UI 树谓词等待在每次采集前检查时间，
//! 单次采集使用 HDC 超时。匹配顺序、索引与各等待返回值在公开 API 中说明。

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
use tracing::{debug, trace};

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);

impl HmDriver {
    /// 获取当前界面的 UI 树（通过 `uitest dumpLayout`）。
    ///
    /// 在设备生成临时 JSON 文件并通过 HDC 拉回主机，返回完整根树快照；操作后清理临时文件。
    /// 采集/传输错误及 JSON 解析错误直接返回。
    pub async fn ui_tree(&self) -> Result<UiNode> {
        debug!(target: "hm_driver_rs::query", "获取 UI 树");
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

    /// 使用选择器查找指定索引的 UI 元素。
    ///
    /// 默认索引为 `0`；使用 [`Selector::index`] 选择其他匹配项。未找到返回 `None`。
    pub async fn find(&self, selector: &Selector) -> Result<Option<Element>> {
        trace!(target: "hm_driver_rs::query", ?selector, "查找元素");
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
    ///
    /// 遵循 [`Selector::index`]；查询失败返回错误，而不是 `false`。
    pub async fn exists(&self, selector: &Selector) -> Result<bool> {
        Ok(self.find(selector).await?.is_some())
    }

    /// 统计选择器匹配的元素数量。
    ///
    /// 统计全部匹配项，与 [`Selector::index`] 无关；无匹配返回 `0`。
    pub async fn count(&self, selector: &Selector) -> Result<usize> {
        Ok(self.find_all(selector).await?.len())
    }

    /// 找到选择器指定索引的元素后点击，返回 `true`；未找到返回 `false`。
    ///
    /// 查找或点击失败返回对应错误。
    pub async fn click_if_exists(&self, selector: &Selector) -> Result<bool> {
        let Some(element) = self.find(selector).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
    }

    /// 查找所有匹配选择器的 UI 元素。
    ///
    /// 保留远端返回顺序，忽略 [`Selector::index`]；无匹配返回空列表。
    pub async fn find_all(&self, selector: &Selector) -> Result<Vec<Element>> {
        trace!(target: "hm_driver_rs::query", ?selector, "查找所有元素");
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
    ///
    /// 默认间隔 100 毫秒，总截止时间约束单次异步查找和轮询休眠。`timeout` 为零立即返回
    /// [`DriverError::ElementNotFound`]；其他查询错误直接返回。
    pub async fn wait_for(&self, selector: &Selector, timeout: Duration) -> Result<Element> {
        debug!(target: "hm_driver_rs::query", ?selector, ?timeout, "wait_for");
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

    /// 等待文本内容匹配的节点出现（支持精确、包含、前后缀和正则表达式）。
    ///
    /// 内部使用 [`wait_for_ui`](Self::wait_for_ui) 轮询 UI 树，超时返回 `Err(ElementNotFound)`。
    ///
    /// `text` 是实际匹配字符串或正则，`pattern` 选择匹配模式，其内字符串以 `text` 为准。
    /// 非法正则返回 [`DriverError::InvalidArgument`]；采集与超时行为同 UI 树等待。
    pub async fn wait_for_text(
        &self,
        text: &str,
        pattern: MatchPattern,
        timeout: Duration,
    ) -> Result<UiNode> {
        debug!(target: "hm_driver_rs::query", text, ?pattern, ?timeout, "wait_for_text");
        if matches!(
            &pattern,
            MatchPattern::Regex(_) | MatchPattern::RegexCaseInsensitive(_)
        ) {
            let _ = pattern.matches_with("", text)?;
        }
        let owned = text.to_owned();
        self.wait_for_ui(timeout, move |node| {
            node.attribute("text")
                .is_some_and(|actual| pattern.matches_with(&actual, &owned).unwrap_or(false))
        })
        .await
    }

    /// 轮询 UI 树，返回深度优先遍历中第一个满足 `predicate` 的节点快照。
    ///
    /// 默认轮询间隔为 100 毫秒，采集前检查截止时间；详细超时语义见
    /// [`wait_for_ui_with_interval`](Self::wait_for_ui_with_interval)。
    pub async fn wait_for_ui(
        &self,
        timeout: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        debug!(target: "hm_driver_rs::query", ?timeout, "wait_for_ui");
        self.wait_for_ui_with_interval(timeout, DEFAULT_POLL_INTERVAL, predicate)
            .await
    }

    /// 使用指定的轮询间隔等待 UI 节点出现。
    ///
    /// 每次采集前检查 `timeout` 的截止时间，达到时返回 [`DriverError::ElementNotFound`]。
    /// 单次采集按 HDC 命令/传输超时执行，同步谓词执行至返回，所以一次采集和判断
    /// 可以越过此截止时间；匹配时返回该节点的独立快照。采集错误直接返回。
    /// 单次采集纳入总截止时间是 crate 对齐记录中的待补项。
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

    /// 轮询完整 UI 树，直到 `predicate` 对根节点返回 `true`。
    ///
    /// 返回完整根树，适合判断列表数量和兄弟节点关系；默认轮询间隔为 100 毫秒。
    /// 超时语义见 [`wait_for_ui_tree_with_interval`](Self::wait_for_ui_tree_with_interval)。
    pub async fn wait_for_ui_tree(
        &self,
        timeout: Duration,
        predicate: impl Fn(&UiNode) -> bool,
    ) -> Result<UiNode> {
        self.wait_for_ui_tree_with_interval(timeout, DEFAULT_POLL_INTERVAL, predicate)
            .await
    }

    /// 使用指定轮询间隔等待满足页面级条件的完整 UI 树。
    ///
    /// 截止时间在每次采集前检查，单次 HDC 采集和同步谓词执行至返回。
    /// 达到截止时间返回 [`DriverError::ElementNotFound`]，采集错误直接返回；
    /// 具体超时范围同 [`wait_for_ui_with_interval`](Self::wait_for_ui_with_interval)。
    pub async fn wait_for_ui_tree_with_interval(
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
            if predicate(&tree) {
                return Ok(tree);
            }
            sleep_until_next_poll(deadline, interval).await;
        }
    }

    /// 在总超时时间内轮询任意异步条件。
    ///
    /// 默认间隔 100 毫秒；总截止时间约束条件 Future 和轮询休眠。
    /// 条件为 `true` 时返回 `true`，超时（含零超时）返回 `false`，条件错误直接返回。
    pub async fn wait_until<F, Fut>(&self, timeout: Duration, condition: F) -> Result<bool>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<bool>>,
    {
        self.wait_until_with_interval(timeout, DEFAULT_POLL_INTERVAL, condition)
            .await
    }

    /// 使用指定轮询间隔等待任意异步条件。
    ///
    /// 语义同 [`wait_until`](Self::wait_until)。`interval` 是一次未满足条件后的休眠时长，
    /// 休眠以剩余截止时间为上限；异步条件应通过让出执行权完成耗时工作。
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
    ///
    /// 默认间隔 100 毫秒，总截止时间约束采集/查询及休眠；超时（含零超时）返回
    /// [`DriverError::XPathNotFound`]，表达式或采集错误直接返回。
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
    ///
    /// 每次重新采集并查询 UI 树；截止时间和错误语义同 [`wait_until`](Self::wait_until)。
    pub async fn wait_until_xpath_gone(&self, expression: &str, timeout: Duration) -> Result<bool> {
        self.wait_until(timeout, || async {
            Ok(self.xpath_optional(expression).await?.is_none())
        })
        .await
    }

    /// 等待指定应用进入前台，超时返回 `false`。
    ///
    /// 检查任意前台任务，支持多窗口；截止时间和错误语义同 [`wait_until`](Self::wait_until)。
    pub async fn wait_for_app(
        &self,
        bundle: &crate::AppIdentifier,
        timeout: Duration,
    ) -> Result<bool> {
        self.wait_until(timeout, || async { self.is_app_foreground(bundle).await })
            .await
    }

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `Err(XPathNotFound)`。
    ///
    /// 先采集 UI 树，在主机执行 XPath 1.0；表达式须返回节点集合，语法错误或标量结果返回
    /// [`DriverError::InvalidXPath`]。返回的属性与 bounds 是采集快照。
    pub async fn xpath(&self, expression: &str) -> Result<XPathElement> {
        trace!(target: "hm_driver_rs::query", expression, "XPath 查询");
        self.xpath_optional(expression)
            .await?
            .ok_or(DriverError::XPathNotFound)
    }

    /// 通过 XPath 表达式查找第一个匹配的 UI 元素，未找到返回 `None`。
    ///
    /// 表达式要求及快照语义同 [`xpath`](Self::xpath)，采集和表达式错误仍返回错误。
    pub async fn xpath_optional(&self, expression: &str) -> Result<Option<XPathElement>> {
        let root = self.ui_tree().await?;
        Ok(XPathElement::query(self.clone(), &root, expression)?
            .into_iter()
            .next())
    }

    /// 通过 XPath 表达式查找所有匹配的 UI 元素。
    ///
    /// 按文档顺序返回快照；无匹配为空列表。表达式要求同 [`xpath`](Self::xpath)。
    pub async fn xpath_all(&self, expression: &str) -> Result<Vec<XPathElement>> {
        trace!(target: "hm_driver_rs::query", expression, "XPath 查询所有");
        let root = self.ui_tree().await?;
        XPathElement::query(self.clone(), &root, expression)
    }

    /// 判断 XPath 表达式是否有匹配的元素。
    ///
    /// 重新采集并查询 UI 树；与 [`XPathElement::exists`] 的 bounds 快照检查不同。
    pub async fn xpath_exists(&self, expression: &str) -> Result<bool> {
        Ok(!self.xpath_all(expression).await?.is_empty())
    }

    /// 如果 XPath 匹配的元素存在则点击，返回是否点击成功。
    ///
    /// 未匹配返回 `false`，匹配后按快照中心点击；采集、表达式、bounds 或点击错误直接返回。
    pub async fn xpath_click_if_exists(&self, expression: &str) -> Result<bool> {
        let Some(element) = self.xpath_optional(expression).await? else {
            return Ok(false);
        };
        element.click().await?;
        Ok(true)
    }

    pub(crate) async fn find_remote_references(&self, selector: &Selector) -> Result<Vec<String>> {
        trace!(target: "hm_driver_rs::query", ?selector, "查找远端引用");
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
