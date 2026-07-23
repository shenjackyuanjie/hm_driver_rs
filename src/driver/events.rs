//! Toast 与 UI 事件监听。

use super::HmDriver;
use crate::{DriverError, MatchPattern, Result, UiEvent, UiEventType};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{debug, trace};

struct ListeningGuard<'a> {
    state: &'a AtomicBool,
    armed: bool,
}

impl<'a> ListeningGuard<'a> {
    fn new(state: &'a AtomicBool) -> Self {
        Self { state, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ListeningGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.state.store(false, Ordering::Release);
        }
    }
}

impl HmDriver {
    /// 开始一次 UI 事件监听。
    ///
    /// 同一个 Driver 同时只允许一个尚未读取的监听。调用后应使用
    /// [`get_latest_ui_event`](Self::get_latest_ui_event) 读取结果。
    pub async fn start_listen_ui_event(&self, event_type: UiEventType) -> Result<()> {
        trace!(target: "hm_driver_rs::events", event_type = event_type.as_str(), "开始监听 UI 事件");
        self.inner
            .ui_event_listening
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                DriverError::InvalidArgument(
                    "已有尚未读取的 UI 事件监听，请先调用 get_latest_ui_event".into(),
                )
            })?;
        let mut guard = ListeningGuard::new(&self.inner.ui_event_listening);
        self.driver_call("uiEventObserverOnce", json!([event_type.as_str()]))
            .await?;
        guard.disarm();
        Ok(())
    }

    /// 开始一次 Toast 监听。
    pub async fn start_listen_toast(&self) -> Result<()> {
        self.start_listen_ui_event(UiEventType::ToastShow).await
    }

    /// 等待并读取本次监听捕获的 UI 事件。
    ///
    /// 超时未捕获事件时返回 `None`。无论成功、超时、错误或调用被取消，本次监听状态
    /// 都会结束。
    pub async fn get_latest_ui_event(&self, timeout: Duration) -> Result<Option<UiEvent>> {
        debug!(target: "hm_driver_rs::events", ?timeout, "读取 UI 事件");
        if !self.inner.ui_event_listening.load(Ordering::Acquire) {
            return Err(DriverError::InvalidArgument("尚未开始 UI 事件监听".into()));
        }
        let _guard = ListeningGuard::new(&self.inner.ui_event_listening);
        if timeout.is_zero() {
            return Err(DriverError::InvalidArgument(
                "UI 事件等待超时必须大于 0".into(),
            ));
        }
        let seconds_u64 = timeout
            .as_secs()
            .saturating_add(u64::from(timeout.subsec_nanos() > 0))
            .max(1);
        let seconds = u32::try_from(seconds_u64)
            .map_err(|_| DriverError::InvalidArgument("UI 事件等待超时过大".into()))?;
        let rpc_timeout = timeout
            .checked_add(Duration::from_secs(1))
            .ok_or_else(|| DriverError::InvalidArgument("UI 事件等待超时过大".into()))?;
        let value = self
            .driver_call_with_timeout("getRecentUiEvent", json!([seconds]), rpc_timeout)
            .await?;
        parse_event(value)
    }

    /// 等待并读取本次 Toast 监听捕获的文本。
    pub async fn get_latest_toast(&self, timeout: Duration) -> Result<Option<String>> {
        Ok(self
            .get_latest_ui_event(timeout)
            .await?
            .and_then(|event| event.text))
    }

    /// 等待 Toast 并按给定规则检查文本。
    pub async fn check_toast(
        &self,
        expected: &str,
        pattern: MatchPattern,
        timeout: Duration,
    ) -> Result<bool> {
        let Some(actual) = self.get_latest_toast(timeout).await? else {
            return Ok(false);
        };
        pattern.matches_with(&actual, expected)
    }
}

fn parse_event(value: Value) -> Result<Option<UiEvent>> {
    let value = match value {
        Value::Null | Value::Bool(false) => return Ok(None),
        Value::String(value) if value.trim().is_empty() || value == "false" => return Ok(None),
        Value::String(value) => serde_json::from_str(&value)
            .map_err(|error| DriverError::Protocol(format!("UI 事件 JSON 无效：{error}")))?,
        value => value,
    };
    serde_json::from_value(value)
        .map(Some)
        .map_err(|error| DriverError::Protocol(format!("UI 事件格式无效：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{ApiDialect, RpcClient};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn parses_string_and_empty_event_results() {
        assert_eq!(parse_event(Value::Bool(false)).unwrap(), None);
        let event = parse_event(Value::String(
            r#"{"bundleName":"com.example","text":"完成","type":"Toast"}"#.into(),
        ))
        .unwrap()
        .unwrap();
        assert_eq!(event.bundle_name.as_deref(), Some("com.example"));
        assert_eq!(event.text.as_deref(), Some("完成"));
        assert_eq!(parse_event(json!(null)).unwrap(), None);
    }

    #[tokio::test]
    async fn toast_listener_uses_agent_event_apis() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let server_calls = calls.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            for index in 0..2 {
                let request: Value =
                    serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                server_calls.lock().await.push((
                    request["params"]["api"].as_str().unwrap().to_owned(),
                    request["params"]["args"].clone(),
                ));
                let result = if index == 0 {
                    Value::Null
                } else {
                    Value::String(r#"{"text":"发送成功","type":"Toast"}"#.into())
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
        let rpc = RpcClient::connect(port, Duration::from_secs(1), Duration::from_secs(1), 4096)
            .await
            .unwrap();
        let driver = HmDriver::with_test_rpc(rpc, ApiDialect::Modern);
        driver.start_listen_toast().await.unwrap();
        assert_eq!(
            driver
                .get_latest_toast(Duration::from_millis(50))
                .await
                .unwrap()
                .as_deref(),
            Some("发送成功")
        );
        assert_eq!(
            *calls.lock().await,
            vec![
                ("Driver.uiEventObserverOnce".into(), json!(["toastShow"])),
                ("Driver.getRecentUiEvent".into(), json!([1]))
            ]
        );
    }
}
