//! The ONE stub LLM server for tests: a minimal OpenAI-compatible HTTP/1.1
//! keep-alive server on 127.0.0.1, on its own thread and its own runtime
//! (apart from the code under test). It records every request body and
//! header set and the MAXIMUM number of requests in flight at once (a
//! concurrency cap's measurement), and answers each request from a
//! handler after an optional delay. A handler can also answer with a
//! status, extra headers, or by hanging up (a transport failure).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// One response.
pub struct StubResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl StubResponse {
    pub fn ok(body: impl Into<String>) -> Self {
        StubResponse::status(200, body)
    }

    pub fn status(status: u16, body: impl Into<String>) -> Self {
        StubResponse {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// Close the connection without answering (a transport failure the
    /// client sees as "Connection error.").
    pub fn hang_up() -> Self {
        StubResponse::status(0, "")
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// Answers request `index` (0-based, arrival order) with body `body`.
pub type Handler = Arc<dyn Fn(usize, &str) -> StubResponse + Send + Sync>;

/// What the server saw.
#[derive(Default)]
pub struct StubStats {
    pub requests: AtomicUsize,
    pub in_flight: AtomicUsize,
    pub max_in_flight: AtomicUsize,
    pub bodies: Mutex<Vec<String>>,
    pub headers: Mutex<Vec<Vec<(String, String)>>>,
}

/// A running stub. It lives until the test process ends.
pub struct StubServer {
    /// The `--endpoint` value: `http://127.0.0.1:<port>/v1`.
    pub base_url: String,
    pub stats: Arc<StubStats>,
}

impl StubServer {
    /// Start a server answering every request with `handler(index, body)`
    /// after `delay`, on a free port.
    pub fn start(delay: Duration, handler: Handler) -> StubServer {
        StubServer::serve(0, delay, handler).expect("a local port")
    }

    /// [`StubServer::start`] on `port` of 127.0.0.1 (0: a free one); Err
    /// when the port cannot be bound (`cargo dev stub-llm serve --port`).
    // concurrency: test-only stub server (this crate is a dev-dependency): its own runtime and per-connection tasks on its own thread, apart from the code under test
    #[allow(clippy::disallowed_methods)]
    pub fn serve(port: u16, delay: Duration, handler: Handler) -> Result<StubServer, String> {
        let stats = Arc::new(StubStats::default());
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let thread_stats = stats.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the stub's runtime starts");
            runtime.block_on(async move {
                let listener = match TcpListener::bind(("127.0.0.1", port)).await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = tx.send(Err(format!("cannot bind 127.0.0.1:{port}: {e}")));
                        return;
                    }
                };
                tx.send(listener.local_addr().map_err(|e| e.to_string()))
                    .expect("the starter waits for the address");
                loop {
                    let Ok((socket, _)) = listener.accept().await else {
                        continue;
                    };
                    let stats = thread_stats.clone();
                    let handler = handler.clone();
                    tokio::spawn(serve_connection(socket, stats, handler, delay));
                }
            });
        });
        let addr = rx.recv().expect("the stub reports its address")?;
        Ok(StubServer {
            base_url: format!("http://{addr}/v1"),
            stats,
        })
    }

    /// Start a server answering every request with a chat completion
    /// whose content is `policy(body)` (see `crate::answers`).
    pub fn answering(policy: impl Fn(&str) -> String + Send + Sync + 'static) -> StubServer {
        StubServer::start(
            Duration::ZERO,
            Arc::new(move |_, body| {
                StubResponse::ok(crate::answers::completion(Some(&policy(body))))
            }),
        )
    }

    pub fn max_in_flight(&self) -> usize {
        self.stats.max_in_flight.load(Ordering::SeqCst)
    }

    pub fn requests(&self) -> usize {
        self.stats.requests.load(Ordering::SeqCst)
    }

    pub fn bodies(&self) -> Vec<String> {
        self.stats.bodies.lock().expect("not poisoned").clone()
    }

    pub fn headers(&self) -> Vec<Vec<(String, String)>> {
        self.stats.headers.lock().expect("not poisoned").clone()
    }
}

/// Read one request: (headers, body), or None when the peer closed.
async fn read_request(
    reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Option<(Vec<(String, String)>, String)> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
        return None;
    }
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await.unwrap_or(0) == 0 {
            return None;
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
    reader.read_exact(&mut body).await.ok()?;
    Some((headers, String::from_utf8_lossy(&body).to_string()))
}

async fn serve_connection(
    socket: tokio::net::TcpStream,
    stats: Arc<StubStats>,
    handler: Handler,
    delay: Duration,
) {
    let (read, mut write) = socket.into_split();
    let mut reader = BufReader::new(read);
    while let Some((headers, body)) = read_request(&mut reader).await {
        let index = stats.requests.fetch_add(1, Ordering::SeqCst);
        let now = stats.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        stats.max_in_flight.fetch_max(now, Ordering::SeqCst);
        stats
            .bodies
            .lock()
            .expect("not poisoned")
            .push(body.clone());
        stats.headers.lock().expect("not poisoned").push(headers);
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
