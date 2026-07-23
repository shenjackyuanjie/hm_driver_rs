//! 窗口查询。

use super::HmDriver;
use crate::{DriverError, Result, UiWindow, WindowFilter};
use serde_json::{Value, json};
use tracing::trace;

impl HmDriver {
    /// 按组合条件查找窗口。
    pub async fn find_window(&self, filter: &WindowFilter) -> Result<Option<UiWindow>> {
        trace!(target: "hm_driver_rs::window", ?filter, "查找窗口");
        if filter.is_empty() {
            return Err(DriverError::InvalidArgument(
                "窗口过滤器至少需要一个条件".into(),
            ));
        }
        let generation = self.generation();
        Ok(self
            .find_window_reference(filter)
            .await?
            .map(|reference| UiWindow::new(self.clone(), filter.clone(), reference, generation)))
    }

    /// 获取当前活动窗口，找不到时回退到聚焦窗口。
    pub async fn current_window(&self) -> Result<Option<UiWindow>> {
        if let Some(window) = self.find_window(&WindowFilter::new().active(true)).await? {
            return Ok(Some(window));
        }
        self.find_window(&WindowFilter::new().focused(true)).await
    }

    /// 获取当前窗口大小。
    pub async fn window_size(&self) -> Result<Option<(u32, u32)>> {
        let Some(window) = self.current_window().await? else {
            return Ok(None);
        };
        let bounds = window.bounds().await?;
        let width = u32::try_from(bounds.width())
            .map_err(|_| DriverError::Protocol("窗口宽度为负数".into()))?;
        let height = u32::try_from(bounds.height())
            .map_err(|_| DriverError::Protocol("窗口高度为负数".into()))?;
        Ok(Some((width, height)))
    }

    pub(crate) async fn find_window_reference(
        &self,
        filter: &WindowFilter,
    ) -> Result<Option<String>> {
        let result = self.driver_call("findWindow", json!([filter])).await?;
        match result {
            Value::Null | Value::Bool(false) => Ok(None),
            Value::String(reference) if reference.is_empty() || reference == "false" => Ok(None),
            Value::String(reference) => Ok(Some(reference)),
            _ => Err(DriverError::Protocol(
                "findWindow 未返回窗口引用或 null".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{ApiDialect, RpcClient};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn finds_window_and_calls_window_object_api() {
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
                    request["params"]["this"].as_str().unwrap().to_owned(),
                    request["params"]["args"].clone(),
                ));
                let response = json!({
                    "request_id": request["request_id"],
                    "result": if index == 0 { json!("UiWindow#4") } else { json!("设置") },
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
        let window = driver
            .find_window(&WindowFilter::new().active(true))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(window.title().await.unwrap(), "设置");
        assert_eq!(
            *calls.lock().await,
            vec![
                (
                    "Driver.findWindow".into(),
                    "Driver#0".into(),
                    json!([{"actived": true}])
                ),
                ("UiWindow.getTitle".into(), "UiWindow#4".into(), json!([]))
            ]
        );
    }
}
