//! An in-process OpenAI-compatible stub server for the client tests: a
//! minimal HTTP/1.1 keep-alive server on 127.0.0.1 in its own thread and
//! runtime. It records every request body and the MAXIMUM number of
//! requests in flight at once (the concurrency gate's measurement), and
//! answers from a handler after an optional delay.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

pub struct StubResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl StubResponse {
    pub fn ok(body: impl Into<String>) -> Self {
        StubResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// Close the connection without answering (a transport failure the
    /// client sees as "Connection error.").
    pub fn hang_up() -> Self {
        StubResponse::status(0, "")
    }

    pub fn status(status: u16, body: impl Into<String>) -> Self {
        StubResponse {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }
}

/// A chat-completions body whose first choice carries `content`.
pub fn completion(content: Option<&str>) -> String {
    let content = match content {
        Some(c) => serde_json::Value::String(c.to_string()),
        None => serde_json::Value::Null,
    };
    serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
    })
    .to_string()
}

pub type Handler = Arc<dyn Fn(usize, &str) -> StubResponse + Send + Sync>;

#[derive(Default)]
pub struct StubStats {
    pub requests: AtomicUsize,
    pub in_flight: AtomicUsize,
    pub max_in_flight: AtomicUsize,
    pub bodies: Mutex<Vec<String>>,
    pub headers: Mutex<Vec<Vec<(String, String)>>>,
}

pub struct StubServer {
    pub base_url: String,
    pub stats: Arc<StubStats>,
}

impl StubServer {
    /// Start a server answering every request with `handler(index, body)`
    /// after `delay`.
    pub fn start(delay: Duration, handler: Handler) -> StubServer {
        let stats = Arc::new(StubStats::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let thread_stats = stats.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(listener.local_addr().unwrap()).unwrap();
                loop {
                    let (socket, _) = listener.accept().await.unwrap();
                    let stats = thread_stats.clone();
                    let handler = handler.clone();
                    tokio::spawn(serve_connection(socket, stats, handler, delay));
                }
            });
        });
        let addr = rx.recv().unwrap();
        StubServer {
            base_url: format!("http://{addr}/v1"),
            stats,
        }
    }

    pub fn max_in_flight(&self) -> usize {
        self.stats.max_in_flight.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> usize {
        self.stats.requests.load(Ordering::SeqCst)
    }

    pub fn bodies(&self) -> Vec<String> {
        self.stats.bodies.lock().unwrap().clone()
    }
}

async fn serve_connection(
    socket: tokio::net::TcpStream,
    stats: Arc<StubStats>,
    handler: Handler,
    delay: Duration,
) {
    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    loop {
        // Request line + headers.
        let mut headers = Vec::new();
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return;
        }
        let mut content_length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).await.unwrap_or(0) == 0 {
                return;
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                let (name, value) = (name.trim().to_lowercase(), value.trim().to_string());
                if name == "content-length" {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.push((name, value));
            }
        }
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).await.is_err() {
            return;
        }
        let body = String::from_utf8_lossy(&body).to_string();

        let index = stats.requests.fetch_add(1, Ordering::SeqCst);
        let now = stats.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        stats.max_in_flight.fetch_max(now, Ordering::SeqCst);
        stats.bodies.lock().unwrap().push(body.clone());
        stats.headers.lock().unwrap().push(headers);
        tokio::time::sleep(delay).await;
        let response = handler(index, &body);
        stats.in_flight.fetch_sub(1, Ordering::SeqCst);
        if response.status == 0 {
            return;
        }

        let mut out = format!(
            "HTTP/1.1 {} STUB\r\ncontent-type: application/json\r\ncontent-length: {}\r\n",
            response.status,
            response.body.len()
        );
        for (name, value) in &response.headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        out.push_str("\r\n");
        out.push_str(&response.body);
        if write.write_all(out.as_bytes()).await.is_err() {
            return;
        }
    }
}
