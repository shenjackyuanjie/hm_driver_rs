use crate::{DriverError, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;

const MODULE: &str = "com.ohos.devicetest.hypiumApiHelper";
const METHOD: &str = "callHypiumApi";

/// 设备端 Hypium API 的方言：区分现代 `Driver`/`On`/`Component`
/// 体系与旧版 `UiDriver`/`By`/`UiComponent` 体系。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiDialect {
    Modern,
    Legacy,
}

impl ApiDialect {
    pub fn driver(self) -> &'static str {
        match self {
            Self::Modern => "Driver",
            Self::Legacy => "UiDriver",
        }
    }

    pub fn selector(self) -> &'static str {
        match self {
            Self::Modern => "On",
            Self::Legacy => "By",
        }
    }

    pub fn component(self) -> &'static str {
        match self {
            Self::Modern => "Component",
            Self::Legacy => "UiComponent",
        }
    }
}

#[derive(Clone)]
pub(crate) struct RpcClient {
    inner: Arc<RpcClientInner>,
}

struct RpcClientInner {
    connection: Mutex<RpcConnection>,
    next_id: AtomicU64,
    timeout: Duration,
    max_frame_size: usize,
    valid: AtomicBool,
}

struct RpcConnection {
    stream: TcpStream,
    buffer: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    exception: Value,
}

impl RpcClient {
    pub async fn connect(
        port: u16,
        connect_timeout: Duration,
        rpc_timeout: Duration,
        max_frame_size: usize,
    ) -> Result<Self> {
        let stream = timeout(connect_timeout, TcpStream::connect(("127.0.0.1", port)))
            .await
            .map_err(|_| DriverError::RpcTimeout {
                timeout: connect_timeout,
            })?
            .map_err(DriverError::RpcConnect)?;
        stream.set_nodelay(true).map_err(DriverError::RpcIo)?;
        Ok(Self {
            inner: Arc::new(RpcClientInner {
                connection: Mutex::new(RpcConnection {
                    stream,
                    buffer: Vec::new(),
                }),
                next_id: AtomicU64::new(1),
                timeout: rpc_timeout,
                max_frame_size,
                valid: AtomicBool::new(true),
            }),
        })
    }

    pub fn is_valid(&self) -> bool {
        self.inner.valid.load(Ordering::Acquire)
    }

    pub fn invalidate(&self) {
        self.inner.valid.store(false, Ordering::Release);
    }

    pub async fn call(&self, api: &str, this: Option<&str>, args: Value) -> Result<Value> {
        if !self.is_valid() {
            return Err(DriverError::SessionInvalid);
        }
        let request_id = self
            .inner
            .next_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let request_id = format!("{request_id:0>20}");
        let request = json!({
            "module": MODULE,
            "method": METHOD,
            "params": {
                "api": api,
                "this": this,
                "args": args,
                "message_type": "hypium"
            },
            "request_id": request_id,
            "call": "xdevice",
            "client": "127.0.0.1"
        });
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        let request_timeout = self.inner.timeout;
        let mut connection = self.inner.connection.lock().await;
        let operation = async {
            connection
                .stream
                .write_all(&bytes)
                .await
                .map_err(DriverError::RpcIo)?;
            connection
                .stream
                .flush()
                .await
                .map_err(DriverError::RpcIo)?;
            for _ in 0..32 {
                let frame = read_frame(&mut connection, self.inner.max_frame_size).await?;
                let response: RpcResponse = serde_json::from_slice(&frame)?;
                // v1.2.2 的 bin Agent 响应不带 request_id。单连接只允许一个在途请求，
                // 因此缺失 ID 时可安全归属于当前请求；存在 ID 时仍严格匹配。
                if !response.request_id.is_empty() && response.request_id != request_id {
                    continue;
                }
                if !response.exception.is_null() {
                    let message = match response.exception {
                        Value::String(message) => message,
                        other => other.to_string(),
                    };
                    return Err(DriverError::Hypium(message));
                }
                return Ok(response.result);
            }
            Err(DriverError::Protocol("连续收到 32 个无关响应".into()))
        };
        match timeout(request_timeout, operation).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error @ DriverError::Hypium(_))) => Err(error),
            Ok(Err(error)) => {
                self.invalidate();
                Err(error)
            }
            Err(_) => {
                self.invalidate();
                Err(DriverError::RpcTimeout {
                    timeout: request_timeout,
                })
            }
        }
    }
}

async fn read_frame(connection: &mut RpcConnection, max_frame_size: usize) -> Result<Vec<u8>> {
    loop {
        if let Some(index) = connection.buffer.iter().position(|byte| *byte == b'\n') {
            if index > max_frame_size {
                return Err(DriverError::Protocol("RPC 帧超过大小限制".into()));
            }
            let mut remainder = connection.buffer.split_off(index + 1);
            std::mem::swap(&mut remainder, &mut connection.buffer);
            remainder.truncate(index);
            if remainder.last() == Some(&b'\r') {
                remainder.pop();
            }
            return Ok(remainder);
        }
        if connection.buffer.len() > max_frame_size {
            return Err(DriverError::Protocol("RPC 帧超过大小限制".into()));
        }
        let mut chunk = [0_u8; 8192];
        let count = connection
            .stream
            .read(&mut chunk)
            .await
            .map_err(DriverError::RpcIo)?;
        if count == 0 {
            return Err(DriverError::Protocol("RPC 连接意外结束".into()));
        }
        connection.buffer.extend_from_slice(&chunk[..count]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    async fn test_client(chunks: Vec<Vec<u8>>, max_frame_size: usize) -> RpcClient {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            let request: Value = serde_json::from_slice(&request).unwrap();
            let id = request["request_id"].as_str().unwrap();
            assert_eq!(id.len(), 20);
            assert!(id.chars().all(|ch| ch.is_ascii_digit()));
            assert_eq!(request["call"], "xdevice");
            for mut chunk in chunks {
                let marker = b"$ID$";
                if let Some(position) = chunk.windows(marker.len()).position(|item| item == marker)
                {
                    chunk.splice(position..position + marker.len(), id.bytes());
                }
                stream.write_all(&chunk).await.unwrap();
            }
        });
        RpcClient::connect(
            port,
            Duration::from_secs(1),
            Duration::from_secs(1),
            max_frame_size,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn handles_fragmented_frames() {
        let client = test_client(
            vec![
                b"{\"request_id\":\"$ID$\",\"res".to_vec(),
                b"ult\":{\"ok\":true},\"exception\":null}\n".to_vec(),
            ],
            1024,
        )
        .await;
        assert_eq!(
            client.call("Driver.create", None, json!([])).await.unwrap(),
            json!({"ok": true})
        );
    }

    #[tokio::test]
    async fn accepts_bin_agent_response_without_request_id() {
        let client = test_client(
            vec![b"{\"result\":\"Driver#0\",\"pts\":1}\n".to_vec()],
            1024,
        )
        .await;
        assert_eq!(
            client.call("Driver.create", None, json!([])).await.unwrap(),
            json!("Driver#0")
        );
    }

    #[tokio::test]
    async fn skips_unrelated_response_in_coalesced_data() {
        let client = test_client(
            vec![b"{\"request_id\":\"other\",\"result\":1,\"exception\":null}\n{\"request_id\":\"$ID$\",\"result\":2,\"exception\":null}\n".to_vec()],
            1024,
        )
        .await;
        assert_eq!(
            client.call("Driver.create", None, json!([])).await.unwrap(),
            json!(2)
        );
    }

    #[tokio::test]
    async fn rejects_large_frames() {
        let client = test_client(vec![vec![b'a'; 65]], 32).await;
        assert!(matches!(
            client.call("x", None, json!([])).await,
            Err(DriverError::Protocol(_))
        ));
        assert!(!client.is_valid());
    }

    #[tokio::test]
    async fn returns_hypium_exceptions_without_invalidating_transport() {
        let client = test_client(
            vec![b"{\"request_id\":\"$ID$\",\"result\":null,\"exception\":{\"code\":401,\"message\":\"bad call\"}}\n".to_vec()],
            1024,
        )
        .await;
        assert!(matches!(
            client.call("x", None, json!([])).await,
            Err(DriverError::Hypium(_))
        ));
        assert!(client.is_valid());
    }

    #[tokio::test]
    async fn eof_invalidates_session() {
        let client = test_client(Vec::new(), 1024).await;
        assert!(matches!(
            client.call("x", None, json!([])).await,
            Err(DriverError::Protocol(_))
        ));
        assert!(!client.is_valid());
    }

    #[tokio::test]
    async fn timeout_invalidates_session() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 256];
            let _ = stream.read(&mut request).await;
            std::future::pending::<()>().await;
        });
        let client = RpcClient::connect(
            port,
            Duration::from_secs(1),
            Duration::from_millis(20),
            1024,
        )
        .await
        .unwrap();
        assert!(matches!(
            client.call("x", None, json!([])).await,
            Err(DriverError::RpcTimeout { .. })
        ));
        assert!(!client.is_valid());
    }
}
