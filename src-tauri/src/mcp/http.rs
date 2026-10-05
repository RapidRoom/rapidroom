use std::collections::HashMap;
use std::env;
use std::fmt::Display;
use std::fs;

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

pub fn start_server(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_server(app_handle).await {
            log::error!("MCP server stopped: {}", error);
        }
    });
}

pub fn initialize_runtime(app_handle: &AppHandle) -> Result<(), String> {
    let config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&config_dir).map_err(|error| error.to_string())?;

    Ok(())
}

async fn run_server(app_handle: AppHandle) -> Result<(), String> {
    let requested_port = env::var("RAPIDRAW_MCP_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let mut listener = None;
    let mut bound_port = requested_port;
    for port in requested_port..=requested_port.saturating_add(10) {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(bound) => {
                listener = Some(bound);
                bound_port = port;
                break;
            }
            Err(_) if env::var("RAPIDRAW_MCP_PORT").is_err() => continue,
            Err(error) => return Err(format!("failed to bind 127.0.0.1:{port}: {error}")),
        }
    }

    let listener = listener.ok_or_else(|| "no available MCP port found".to_string())?;
    let state = app_handle.state::<crate::AppState>();
    *state.mcp.port.lock().unwrap() = bound_port;

    if let Ok(config_dir) = app_handle.path().app_config_dir() {
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::write(
            config_dir.join("mcp-endpoint.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "url": format!("http://127.0.0.1:{bound_port}/mcp"),
                "protocolVersion": PROTOCOL_VERSION,
            }))
            .unwrap_or_default(),
        );
    }

    log::info!(
        "MCP server listening on http://127.0.0.1:{bound_port}/mcp (loopback-only, no auth)"
    );

    let service = create_http_service(app_handle);
    loop {
        let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let service_for_connection = service.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = handle_connection(stream, service_for_connection).await {
                log::debug!("MCP connection closed: {}", error);
            }
        });
    }
}

async fn handle_connection(mut stream: TcpStream, service: McpHttpService) -> Result<(), String> {
    let request = read_request(&mut stream).await?;

    // MCP clients are local processes, not web pages. Refusing anything a
    // browser sent keeps websites from driving the editor.
    if request.headers.contains_key("origin") {
        write_basic_response(&mut stream, 403, "forbidden", "text/plain").await?;
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
