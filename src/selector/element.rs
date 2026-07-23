//! 通过选择器定位到的远端 UI 控件句柄。

use super::Selector;
use crate::driver::HmDriver;
use crate::{Bounds, DriverError, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};
use tracing::{debug, trace};

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
    /// 控件的资源 ID。
    pub id: String,
    /// 控件的键值。
    pub key: String,
    /// 控件的类型名称。
    pub type_name: String,
    /// 控件的文本内容。
    pub text: String,
    /// 控件的描述内容。
    pub description: String,
    /// 控件的提示文本。
    pub hint: String,
    /// 控件是否被选中。
    pub selected: bool,
    /// 控件是否被勾选。
    pub checked: bool,
    /// 控件是否已启用。
    pub enabled: bool,
    /// 控件是否已获取焦点。
    pub focused: bool,
    /// 控件是否可被勾选。
    pub checkable: bool,
    /// 控件是否可被点击。
    pub clickable: bool,
    /// 控件是否可被长按。
    pub long_clickable: bool,
    /// 控件是否可滚动。
    pub scrollable: bool,
    /// 控件的边界矩形。
    pub bounds: Bounds,
    /// 控件边界矩形的中心点坐标。
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
        trace!(target: "hm_driver_rs::element", "解析元素引用");
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
        trace!(target: "hm_driver_rs::element", method = %method, "元素操作");
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

    /// 读取控件的指定属性原始值。
    ///
    /// 支持的属性名包括 `id`、`key`、`type`、`text`、`description`、`hint`、
    /// `selected`、`checked`、`enabled`、`focused`、`checkable`、`clickable`、
    /// `longClickable`、`scrollable`、`originalText`。
    pub async fn attribute(&self, name: &str) -> Result<Value> {
        if name == "originalText" {
            return self.original_text().await.map(Value::String);
        }
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

    /// 一次 RPC 读取控件公开的全部属性。
    pub async fn all_properties(&self) -> Result<BTreeMap<String, Value>> {
        if let Some(level) = self.driver.api_level().await?
            && level < 12
        {
            return Err(DriverError::Unsupported(format!(
                "Component.getAllProperties 需要 API Level 12，当前为 {level}"
            )));
        }
        let value = self.operate("getAllProperties", json!([])).await?;
        let object = value.as_object().ok_or_else(|| {
            DriverError::Protocol("Component.getAllProperties 未返回 JSON object".into())
        })?;
        Ok(object
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect())
    }

    /// 读取控件未经展示层转换的原始文本。
    pub async fn original_text(&self) -> Result<String> {
        match self.driver.api_level().await? {
            Some(20..) => value_string(
                self.operate("getOriginalText", json!([])).await?,
                "originalText",
            ),
            Some(12..=19) => original_text_from_properties(self.all_properties().await?),
            Some(level) => Err(DriverError::Unsupported(format!(
                "originalText 需要 API Level 12，当前为 {level}"
            ))),
            None => match self.operate("getOriginalText", json!([])).await {
                Ok(value) => value_string(value, "originalText"),
                Err(DriverError::Hypium(message)) if is_method_not_found(&message) => {
                    match self.all_properties().await {
                        Ok(properties) => original_text_from_properties(properties),
                        Err(DriverError::Hypium(message)) if is_method_not_found(&message) => {
                            Err(DriverError::Unsupported(
                                "设备不支持 Component.getOriginalText 或 Component.getAllProperties"
                                    .into(),
                            ))
                        }
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
        }
    }

    /// 读取控件的资源 ID。
    pub async fn id(&self) -> Result<String> {
        value_string(self.attribute("id").await?, "id")
    }

    /// 读取控件的键值。
    pub async fn key(&self) -> Result<String> {
        value_string(self.attribute("key").await?, "key")
    }

    /// 读取控件的类型名称。
    pub async fn type_name(&self) -> Result<String> {
        value_string(self.attribute("type").await?, "type")
    }

    /// 读取控件的文本内容。
    pub async fn text(&self) -> Result<String> {
        value_string(self.attribute("text").await?, "text")
    }

    /// 读取控件的描述内容。
    pub async fn description(&self) -> Result<String> {
        value_string(self.attribute("description").await?, "description")
    }

    /// 读取控件的提示文本。
    pub async fn hint(&self) -> Result<String> {
        value_string(self.attribute("hint").await?, "hint")
    }

    /// 判断控件是否已被选中。
    pub async fn is_selected(&self) -> Result<bool> {
        value_bool(self.attribute("selected").await?, "selected")
    }

    /// 判断控件是否已被勾选。
    pub async fn is_checked(&self) -> Result<bool> {
        value_bool(self.attribute("checked").await?, "checked")
    }

    /// 判断控件是否已启用。
    pub async fn is_enabled(&self) -> Result<bool> {
        value_bool(self.attribute("enabled").await?, "enabled")
    }

    /// 判断控件是否已获取焦点。
    pub async fn is_focused(&self) -> Result<bool> {
        value_bool(self.attribute("focused").await?, "focused")
    }

    /// 判断控件是否可被勾选。
    pub async fn is_checkable(&self) -> Result<bool> {
        value_bool(self.attribute("checkable").await?, "checkable")
    }

    /// 判断控件是否可被点击。
    pub async fn is_clickable(&self) -> Result<bool> {
        value_bool(self.attribute("clickable").await?, "clickable")
    }

    /// 判断控件是否可被长按。
    pub async fn is_long_clickable(&self) -> Result<bool> {
        value_bool(self.attribute("longClickable").await?, "longClickable")
    }

    /// 判断控件是否可滚动。
    pub async fn is_scrollable(&self) -> Result<bool> {
        value_bool(self.attribute("scrollable").await?, "scrollable")
    }

    /// 读取控件的边界矩形。
    pub async fn bounds(&self) -> Result<Bounds> {
        let value = self.operate("getBounds", json!([])).await?;
        Bounds::parse_value(&value)
            .ok_or_else(|| DriverError::Protocol("控件 bounds 格式无效".into()))
    }

    /// 读取控件边界矩形的中心点坐标。
    pub async fn bounds_center(&self) -> Result<crate::Point> {
        Ok(self.bounds().await?.center())
    }

    /// 按控件边界中的相对偏移计算坐标。
    ///
    /// `(0, 0)` 为左上角，`(0.5, 0.5)` 为中心，`(1, 1)` 为右下角。
    /// 与官方 Hypium 一致，大于 `1` 的值按像素偏移计算，负数按控件尺寸比例计算，
    /// 因而也可指定控件外的位置。
    pub async fn point_at(&self, offset_x: f64, offset_y: f64) -> Result<crate::Point> {
        if !offset_x.is_finite() || !offset_y.is_finite() {
            return Err(DriverError::InvalidCoordinate(
                "控件相对偏移必须是有限数值".into(),
            ));
        }
        let bounds = self.bounds().await?;
        let x_offset = if offset_x > 1.0 {
            offset_x
        } else {
            f64::from(bounds.width()) * offset_x
        };
        let y_offset = if offset_y > 1.0 {
            offset_y
        } else {
            f64::from(bounds.height()) * offset_y
        };
        Ok(crate::Point::new(
            (f64::from(bounds.left) + x_offset).round() as i32,
            (f64::from(bounds.top) + y_offset).round() as i32,
        ))
    }

    /// 点击控件内（或控件外）的相对偏移位置。
    pub async fn click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        self.driver
            .click(self.point_at(offset_x, offset_y).await?)
            .await
    }

    /// 双击控件内（或控件外）的相对偏移位置。
    pub async fn double_click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        self.driver
            .double_click(self.point_at(offset_x, offset_y).await?)
            .await
    }

    /// 长按控件内（或控件外）的相对偏移位置。
    pub async fn long_click_at(&self, offset_x: f64, offset_y: f64) -> Result<()> {
        self.driver
            .long_click(self.point_at(offset_x, offset_y).await?)
            .await
    }

    /// 一次性读取控件的全部属性。
    ///
    /// 与逐个调用属性方法相比，此方法在一次往返中获取所有信息，
    /// 但在默认实现中仍然是通过多次 API 调用完成的。
    pub async fn info(&self) -> Result<ElementInfo> {
        debug!(target: "hm_driver_rs::element", "获取元素完整信息");
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

    /// 点击控件。
    pub async fn click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::element", "点击元素");
        self.operate("click", json!([])).await.map(|_| ())
    }

    /// 双击控件。
    pub async fn double_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::element", "双击元素");
        self.operate("doubleClick", json!([])).await.map(|_| ())
    }

    /// 长按控件。
    pub async fn long_click(&self) -> Result<()> {
        trace!(target: "hm_driver_rs::element", "长按元素");
        self.operate("longClick", json!([])).await.map(|_| ())
    }

    /// 向控件输入文本。
    pub async fn input_text(&self, text: &str) -> Result<()> {
        self.operate("inputText", json!([text])).await.map(|_| ())
    }

    /// 清除控件中的文本。
    pub async fn clear_text(&self) -> Result<()> {
        self.operate("clearText", json!([])).await.map(|_| ())
    }

    /// 使用平台默认速度滚动到控件顶部。
    pub async fn scroll_to_top(&self) -> Result<()> {
        self.scroll_to_top_with_speed(600).await
    }

    /// 以指定速度滚动到控件顶部。
    ///
    /// `speed` 取值范围为 200 到 40000。
    pub async fn scroll_to_top_with_speed(&self, speed: u32) -> Result<()> {
        validate_operation_speed(speed)?;
        self.operate("scrollToTop", json!([speed]))
            .await
            .map(|_| ())
    }

    /// 使用平台默认速度滚动到控件底部。
    pub async fn scroll_to_bottom(&self) -> Result<()> {
        self.scroll_to_bottom_with_speed(600).await
    }

    /// 以指定速度滚动到控件底部。
    ///
    /// `speed` 取值范围为 200 到 40000。
    pub async fn scroll_to_bottom_with_speed(&self, speed: u32) -> Result<()> {
        validate_operation_speed(speed)?;
        self.operate("scrollToBottom", json!([speed]))
            .await
            .map(|_| ())
    }

    /// 在当前可滚动控件中查找目标控件。
    pub async fn scroll_search(&self, selector: &Selector) -> Result<Option<Element>> {
        self.scroll_search_raw(selector, json!([])).await
    }

    /// 指定滚动方向及可选边缘偏移后查找目标控件。
    pub async fn scroll_search_with_options(
        &self,
        selector: &Selector,
        vertical: bool,
        offset: Option<u32>,
    ) -> Result<Option<Element>> {
        let options = match offset {
            Some(offset) => json!([vertical, offset]),
            None => json!([vertical]),
        };
        self.scroll_search_raw(selector, options).await
    }

    async fn scroll_search_raw(
        &self,
        selector: &Selector,
        options: Value,
    ) -> Result<Option<Element>> {
        let selector_reference = selector.build_remote(&self.driver).await?;
        let mut args = vec![Value::String(selector_reference.clone())];
        let Value::Array(options) = options else {
            unreachable!("滚动搜索选项固定为数组")
        };
        args.extend(options);
        let generation = self.driver.generation();
        let result = self.operate("scrollSearch", Value::Array(args)).await;
        self.driver
            .queue_remote_reference(selector_reference, generation);
        match result? {
            Value::Null => Ok(None),
            Value::String(reference) => Ok(Some(Element::new(
                self.driver.clone(),
                selector.clone(),
                selector.selected_index(),
                reference,
                generation,
            ))),
            _ => Err(DriverError::Protocol("scrollSearch 未返回控件引用".into())),
        }
    }

    /// 将当前控件拖拽到目标控件位置。
    pub async fn drag_to(&self, target: &Element) -> Result<()> {
        let target = target.reference().await?;
        self.operate("dragTo", json!([target])).await.map(|_| ())
    }

    /// 在控件上执行捏合缩小手势。
    ///
    /// `scale` 为缩放比例，必须大于 0。
    pub async fn pinch_in(&self, scale: f64) -> Result<()> {
        validate_scale(scale)?;
        self.operate("pinchIn", json!([scale])).await.map(|_| ())
    }

    /// 在控件上执行捏合放大手势。
    ///
    /// `scale` 为缩放比例，必须大于 0。
    pub async fn pinch_out(&self, scale: f64) -> Result<()> {
        validate_scale(scale)?;
        self.operate("pinchOut", json!([scale])).await.map(|_| ())
    }

    /// 等待控件从界面上消失。
    ///
    /// 在指定超时时间内不断尝试查找控件，若控件已不存在则返回 `true`，
    /// 超时后仍未消失则返回 `false`。
    pub async fn wait_until_gone(&self, timeout: Duration) -> Result<bool> {
        debug!(target: "hm_driver_rs::element", ?timeout, "等待元素消失");
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
        debug!(target: "hm_driver_rs::element", name = %name, ?timeout, "等待元素属性");
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

fn validate_operation_speed(speed: u32) -> Result<()> {
    if (200..=40_000).contains(&speed) {
        Ok(())
    } else {
        Err(DriverError::InvalidArgument(
            "操作速度必须位于 200 到 40000".into(),
        ))
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

fn original_text_from_properties(properties: BTreeMap<String, Value>) -> Result<String> {
    let Some(value) = properties.get("originalText") else {
        return Err(DriverError::Unsupported(
            "Component.getAllProperties 未提供 originalText".into(),
        ));
    };
    value_string(value.clone(), "originalText")
}

fn is_method_not_found(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("method") && (message.contains("not found") || message.contains("undefined"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{ApiDialect, RpcClient};
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as TokioMutex;

    async fn scripted_element(
        dialect: ApiDialect,
        api_level: Option<u32>,
        responses: Vec<(Value, Value)>,
    ) -> (Element, Arc<TokioMutex<Vec<String>>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(TokioMutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            for (result, exception) in responses {
                let request: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                server_calls
                    .lock()
                    .await
                    .push(request["params"]["api"].as_str().unwrap().to_owned());
                let response = json!({
                    "request_id": request["request_id"],
                    "result": result,
                    "exception": exception
                });
                writer
                    .write_all(serde_json::to_string(&response).unwrap().as_bytes())
                    .await
                    .unwrap();
                writer.write_all(b"\n").await.unwrap();
            }
        });
        let rpc = RpcClient::connect(
            port,
            Duration::from_secs(1),
            Duration::from_secs(1),
            1024 * 1024,
        )
        .await
        .unwrap();
        let driver = HmDriver::with_test_rpc_api_level(rpc, dialect, api_level);
        let generation = driver.generation();
        (
            Element::new(
                driver,
                Selector::new(),
                0,
                format!("{}#1", dialect.component()),
                generation,
            ),
            calls,
        )
    }

    #[tokio::test]
    async fn api_20_uses_get_original_text_and_attribute_reuses_it() {
        let (element, calls) = scripted_element(
            ApiDialect::Modern,
            Some(20),
            vec![(json!("0.25"), Value::Null), (json!("0.50"), Value::Null)],
        )
        .await;

        assert_eq!(element.original_text().await.unwrap(), "0.25");
        assert_eq!(
            element.attribute("originalText").await.unwrap(),
            json!("0.50")
        );
        assert_eq!(
            *calls.lock().await,
            vec!["Component.getOriginalText", "Component.getOriginalText"]
        );
    }

    #[tokio::test]
    async fn api_12_to_19_reads_original_text_from_all_properties() {
        let (element, calls) = scripted_element(
            ApiDialect::Legacy,
            Some(15),
            vec![(json!({"originalText": "0.75", "text": "75%"}), Value::Null)],
        )
        .await;

        assert_eq!(element.original_text().await.unwrap(), "0.75");
        assert_eq!(*calls.lock().await, vec!["UiComponent.getAllProperties"]);
    }

    #[tokio::test]
    async fn unknown_api_falls_back_only_when_method_is_missing() {
        let (element, calls) = scripted_element(
            ApiDialect::Modern,
            None,
            vec![
                (Value::Null, json!("method getOriginalText not found")),
                (json!({"originalText": "0.9"}), Value::Null),
            ],
        )
        .await;

        assert_eq!(element.original_text().await.unwrap(), "0.9");
        assert_eq!(
            *calls.lock().await,
            vec!["Component.getOriginalText", "Component.getAllProperties"]
        );
    }

    #[tokio::test]
    async fn unknown_api_normalizes_two_missing_methods_to_unsupported() {
        let (element, _) = scripted_element(
            ApiDialect::Modern,
            None,
            vec![
                (Value::Null, json!("method getOriginalText is undefined")),
                (Value::Null, json!("method getAllProperties not found")),
            ],
        )
        .await;

        assert!(matches!(
            element.original_text().await,
            Err(DriverError::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn all_properties_validates_object_and_original_text() {
        let (element, _) =
            scripted_element(ApiDialect::Modern, Some(15), vec![(json!([]), Value::Null)]).await;
        assert!(matches!(
            element.all_properties().await,
            Err(DriverError::Protocol(_))
        ));

        let (element, _) = scripted_element(
            ApiDialect::Modern,
            Some(15),
            vec![(json!({"text": "missing"}), Value::Null)],
        )
        .await;
        assert!(matches!(
            element.original_text().await,
            Err(DriverError::Unsupported(_))
        ));

        let (element, _) = scripted_element(
            ApiDialect::Modern,
            Some(15),
            vec![(json!({"originalText": 1}), Value::Null)],
        )
        .await;
        assert!(matches!(
            element.original_text().await,
            Err(DriverError::Protocol(_))
        ));
    }
}
