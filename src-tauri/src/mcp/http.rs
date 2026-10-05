use rapidroom_mcp_client::{BearerVerifier, Endpoint, write_endpoint};
use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::fs;
use std::sync::Arc;

use ::http::{
    HeaderName, HeaderValue, Method, Request,
    header::{CONNECTION, CONTENT_LENGTH, TRANSFER_ENCODING},
};
use bytes::Bytes;
use http_body::Body;
use http_body_util::{BodyExt, Full};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::server::{McpHttpService, create_http_service};
use super::{DEFAULT_PORT, MAX_BODY_BYTES, MAX_HEADER_BYTES, PROTOCOL_VERSION};

#[derive(Debug)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn fresh_token() -> String {
    let mut random = [0u8; 32];
    rand::fill(&mut random);
    hex::encode(random)
}

pub fn start_server(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = set_enabled(app_handle, true, false).await {
            log::warn!("Unable to enable MCP control: {}", error);
        }
    });
}

fn endpoint_path(app_handle: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(app_handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("mcp-endpoint.json"))
}

fn remove_endpoint(app_handle: &AppHandle) -> Result<(), String> {
    match fs::remove_file(endpoint_path(app_handle)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Cannot remove MCP endpoint: {error}")),
    }
}

pub fn initialize_runtime(app_handle: &AppHandle) -> Result<(), String> {
    remove_endpoint(app_handle)
}

pub fn cleanup_endpoint(app_handle: &AppHandle) {
    if let Err(error) = remove_endpoint(app_handle) {
        log::warn!("{}", error);
    }
}

struct EndpointGuard(AppHandle);
impl Drop for EndpointGuard {
    fn drop(&mut self) {
        *self.0.state::<crate::AppState>().mcp.port.lock().unwrap() = 0;
        cleanup_endpoint(&self.0);
    }
}

pub async fn set_enabled(
    app_handle: AppHandle,
    enabled: bool,
    persist: bool,
) -> Result<(), String> {
    let state = app_handle.state::<crate::AppState>();
    let mut slot = state.mcp.server.lock().await;
    if enabled && slot.as_ref().is_some_and(|task| !task.is_finished()) {
        if persist {
            crate::mcp_control::persist_enabled(&app_handle, true)?;
        }
        return Ok(());
    }
    if let Some(task) = slot.take() {
        task.abort();
        let _ = task.await;
    }
    *state.mcp.port.lock().unwrap() = 0;
    state.mcp.ui_waiters.lock().unwrap().clear();
    remove_endpoint(&app_handle)?;
    if !enabled {
        if persist {
            crate::mcp_control::persist_enabled(&app_handle, false)?;
        }
        return Ok(());
    }

    let requested_port = env::var("RAPIDRAW_MCP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);
    let mut bound = None;
    for port in requested_port..=requested_port.saturating_add(10) {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                bound = Some((listener, port));
                break;
            }
            Err(_) if env::var("RAPIDRAW_MCP_PORT").is_err() => continue,
            Err(error) => return Err(format!("failed to bind 127.0.0.1:{port}: {error}")),
        }
    }
    let (listener, port) = bound.ok_or("no available MCP port found")?;
    let guard = EndpointGuard(app_handle.clone());
    let token = fresh_token();
    let verifier = Arc::new(BearerVerifier::new(&token));
    let endpoint = Endpoint {
        url: format!("http://127.0.0.1:{port}/mcp"),
        token,
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    write_endpoint(&endpoint_path(&app_handle)?, &endpoint)?;
    if persist {
        crate::mcp_control::persist_enabled(&app_handle, true)?;
    }
    drop(endpoint);
    *state.mcp.port.lock().unwrap() = port;
    let service = create_http_service(app_handle.clone());
    *slot = Some(tokio::spawn(async move {
        let _guard = guard;
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = match accepted { Ok(value) => value, Err(error) => { log::warn!("MCP listener stopped: {error}"); break; } };
                    let service = service.clone();
                    let verifier = verifier.clone();
                    connections.spawn(async move {
                        if let Err(error) = handle_connection(stream, service, verifier).await {
                            log::debug!("MCP connection closed: {error}");
                        }
                    });
                }
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
    }));
    log::info!("MCP control enabled on authenticated loopback port {port}");
    Ok(())
}

async fn handle_connection(
    mut stream: TcpStream,
    service: McpHttpService,
    verifier: Arc<BearerVerifier>,
) -> Result<(), String> {
    let request = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        read_request(&mut stream),
    )
    .await
    .map_err(|_| "MCP request read timed out")??;

    // MCP clients are local processes, not web pages. Refusing anything a
    // browser sent keeps websites from driving the editor.
    if request.headers.contains_key("origin") {
        write_basic_response(&mut stream, 403, "forbidden", "text/plain").await?;
        return Ok(());
    }

    if !verifier.accepts(request.headers.get("authorization").map(String::as_str)) {
        write_basic_response(&mut stream, 401, "authentication required", "text/plain").await?;
        return Ok(());
    }

    if request.method == "OPTIONS" {
        write_basic_response(&mut stream, 204, "", "text/plain").await?;
        return Ok(());
    }

    if request.path != "/mcp" {
        write_basic_response(&mut stream, 404, "not found", "text/plain").await?;
        return Ok(());
    }

    // Stateless JSON responses only: no server-sent event stream on GET.
    if request.method != "POST" {
        let response = "HTTP/1.1 405 Method Not Allowed\r\nAllow: POST\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream
            .write_all(response.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let request = build_rmcp_request(request)?;
    let response = service.handle(request).await;
    write_rmcp_response(&mut stream, response).await
}

fn build_rmcp_request(request: HttpRequest) -> Result<Request<Full<Bytes>>, String> {
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;
    let mut builder = Request::builder().method(method).uri(request.path);
    let headers = builder
        .headers_mut()
        .ok_or_else(|| "unable to build MCP request headers".to_string())?;
    for (name, value) in request.headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("invalid HTTP header name: {error}"))?;
        let value = HeaderValue::from_str(&value)
            .map_err(|error| format!("invalid HTTP header value: {error}"))?;
        headers.append(name, value);
    }

    builder
        .body(Full::new(Bytes::from(request.body)))
        .map_err(|error| format!("unable to build MCP request: {error}"))
}

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> Result<HttpRequest, String> {
    let mut bytes = Vec::with_capacity(4096);
    let mut header_end = None;
    let mut chunk = [0_u8; 4096];

    while header_end.is_none() {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before HTTP headers".to_string());
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_HEADER_BYTES {
            return Err("HTTP headers exceed the MCP limit".to_string());
        }
        header_end = bytes.windows(4).position(|window| window == b"\r\n\r\n");
    }

    let header_end = header_end.unwrap();
    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|error| format!("invalid HTTP headers: {error}"))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or("missing HTTP request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or("missing HTTP method")?
        .to_string();
    let path = request_parts.next().ok_or("missing HTTP path")?.to_string();

    let mut headers = HashMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("invalid HTTP header")?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    let body_start = header_end + 4;
    let chunked = headers
        .get("transfer-encoding")
        .is_some_and(|value| value.to_ascii_lowercase().contains("chunked"));
    if chunked {
        let body = read_chunked_body(stream, bytes.split_off(body_start)).await?;
        headers.remove("transfer-encoding");
        headers.insert("content-length".to_string(), body.len().to_string());
        return Ok(HttpRequest {
            method,
            path,
            headers,
            body,
        });
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("MCP request body exceeds the limit".to_string());
    }

    while bytes.len() < body_start + content_length {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed before HTTP body".to_string());
        }
        bytes.extend_from_slice(&chunk[..read]);
    }

    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[body_start..body_start + content_length].to_vec(),
    })
}

async fn read_chunked_body<S: AsyncRead + Unpin>(
    stream: &mut S,
    mut buffer: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let size_line_end = loop {
            if let Some(offset) = buffer.windows(2).position(|window| window == b"\r\n") {
                break offset;
            }
            if buffer.len() > MAX_HEADER_BYTES {
                return Err("invalid chunked HTTP body".to_string());
            }
            let read = stream
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("connection closed inside a chunked HTTP body".to_string());
            }
            buffer.extend_from_slice(&chunk[..read]);
        };
        if size_line_end > MAX_HEADER_BYTES {
            return Err("invalid chunked HTTP body".to_string());
        }
        let size_text = std::str::from_utf8(&buffer[..size_line_end])
            .map_err(|_| "invalid chunk size".to_string())?;
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| format!("invalid chunk size: {size_text}"))?;
        let data_start = size_line_end + 2;
        if size == 0 {
            // Trailers are not used by MCP clients; the body ends here.
            return Ok(body);
        }
        if size > MAX_BODY_BYTES - body.len() {
            return Err("MCP request body exceeds the limit".to_string());
        }
        while buffer.len() < data_start + size + 2 {
            let read = stream
                .read(&mut chunk)
                .await
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("connection closed inside a chunked HTTP body".to_string());
            }
            buffer.extend_from_slice(&chunk[..read]);
        }
        if &buffer[data_start + size..data_start + size + 2] != b"\r\n" {
            return Err("invalid chunk terminator".to_string());
        }
        body.extend_from_slice(&buffer[data_start..data_start + size]);
        buffer.drain(..data_start + size + 2);
    }
}

async fn write_basic_response(
    stream: &mut TcpStream,
    status: u16,
    body: &str,
    content_type: &str,
) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| error.to_string())
}

async fn write_rmcp_response<B>(
    stream: &mut TcpStream,
    response: ::http::Response<B>,
) -> Result<(), String>
where
    B: Body<Data = Bytes> + Send,
    B::Error: Display,
{
    let (parts, body) = response.into_parts();
    let body = body
        .collect()
        .await
        .map_err(|error| error.to_string())?
        .to_bytes();
    let reason = parts.status.canonical_reason().unwrap_or_default();
    let mut response = format!("HTTP/1.1 {} {reason}\r\n", parts.status.as_u16());
    for (name, value) in &parts.headers {
        if name == CONTENT_LENGTH || name == TRANSFER_ENCODING || name == CONNECTION {
            continue;
        }
        let value = value
            .to_str()
            .map_err(|error| format!("invalid MCP response header: {error}"))?;
        response.push_str(&format!("{}: {value}\r\n", name.as_str()));
    }
    response.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&body)
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn sessions_generate_distinct_full_length_tokens() {
        let first = fresh_token();
        let second = fresh_token();
        assert_eq!(first.len(), 64);
        assert_eq!(second.len(), 64);
        assert_ne!(first, second);
    }

    async fn parse(raw: &'static [u8]) -> Result<HttpRequest, String> {
        let (mut client, mut server) = tokio::io::duplex(64);
        let writer = tokio::spawn(async move {
            for piece in raw.chunks(7) {
                if client.write_all(piece).await.is_err() {
                    break;
                }
            }
        });
        let request = read_request(&mut server).await;
        drop(server);
        writer.await.unwrap();
        request
    }

    #[tokio::test]
    async fn reads_content_length_body() {
        let request =
            parse(b"POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 4\r\n\r\n{\"a\"")
                .await;
        let request = request.unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/mcp");
        assert_eq!(request.body, b"{\"a\"");
    }

    #[tokio::test]
    async fn decodes_chunked_body() {
        let request = parse(
            b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\nB;ext=1\r\n, MCP world\r\n0\r\n\r\n",
        )
        .await
        .unwrap();
        assert_eq!(request.body, b"hello, MCP world");
        assert!(!request.headers.contains_key("transfer-encoding"));
        assert_eq!(request.headers.get("content-length").unwrap(), "16");
    }

    #[tokio::test]
    async fn rejects_chunk_size_that_would_overflow_the_accumulated_length() {
        let raw = format!(
            "POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n{:X}\r\n",
            usize::MAX
        );
        let mut stream = raw.as_bytes();
        assert!(
            read_request(&mut stream)
                .await
                .unwrap_err()
                .contains("exceeds the limit")
        );
    }

    #[tokio::test]
    async fn rejects_chunk_data_without_a_crlf_terminator() {
        let request =
            parse(b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx!!0\r\n\r\n")
                .await;
        assert!(request.unwrap_err().contains("chunk terminator"));
    }

    #[tokio::test]
    async fn rejects_bad_chunk_size() {
        let request = parse(
            b"POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\nhello\r\n0\r\n\r\n",
        )
        .await;
        assert!(request.is_err());
    }
}
