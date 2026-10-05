//! Fresh implementation of the authenticated local endpoint and transport.
//! No GUI dependency or filesystem MCP tools are present in this crate.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};
use subtle::ConstantTimeEq;

pub const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;
pub const APP_ID: &str = "io.github.CyberTimon.RapidRAW";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub url: String,
    pub token: String,
    pub protocol_version: String,
}

impl Endpoint {
    pub fn port(&self) -> Result<u16, String> {
        let port = self
            .url
            .strip_prefix("http://127.0.0.1:")
            .and_then(|v| v.strip_suffix("/mcp"))
            .and_then(|v| v.parse::<u16>().ok())
            .filter(|v| *v != 0)
            .ok_or("Endpoint must be the running app's IPv4 loopback MCP URL")?;
        if self.token.len() != 64 || !self.token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid endpoint token".into());
        }
        Ok(port)
    }
}

pub fn endpoint_path() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join(APP_ID).join("mcp-endpoint.json"))
        .ok_or_else(|| "No per-user application config directory".into())
}

pub fn read_endpoint(path: &Path) -> Result<Endpoint, String> {
    let file = fs::File::open(path).map_err(|_| "Start an MCP-enabled RapidRoom editor first")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Cannot inspect the MCP endpoint")?;
    if !metadata.is_file() || metadata.len() > 8192 {
        return Err("Invalid MCP endpoint file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err("MCP endpoint must be owned by this user with mode 0600".into());
        }
    }
    let endpoint: Endpoint =
        serde_json::from_reader(file).map_err(|_| "Invalid MCP endpoint JSON")?;
    endpoint.port()?;
    Ok(endpoint)
}

pub fn write_endpoint(path: &Path, endpoint: &Endpoint) -> Result<(), String> {
    endpoint.port()?;
    let parent = path.parent().ok_or("Endpoint has no parent directory")?;
    fs::create_dir_all(parent).map_err(|_| "Cannot create MCP endpoint directory")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Invalid system clock")?
        .as_nanos();
    let temp = parent.join(format!(".mcp-endpoint-{}-{nonce}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options
            .open(&temp)
            .map_err(|_| "Cannot create private MCP endpoint")?;
        serde_json::to_writer_pretty(&mut file, endpoint)
            .map_err(|_| "Cannot serialize MCP endpoint")?;
        file.write_all(b"\n")
            .and_then(|_| file.sync_all())
            .map_err(|_| "Cannot persist MCP endpoint")?;
        fs::rename(&temp, path).map_err(|_| "Cannot atomically publish MCP endpoint")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub struct BearerVerifier([u8; 32]);
impl BearerVerifier {
    pub fn new(token: &str) -> Self {
        Self(Sha256::digest(token.as_bytes()).into())
    }
    pub fn accepts(&self, header: Option<&str>) -> bool {
        let Some(token) = header.and_then(|s| s.strip_prefix("Bearer ")) else {
            return false;
        };
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        bool::from(self.0.ct_eq(&supplied))
    }
}

pub fn forward(
    endpoint: &Endpoint,
    request: &serde_json::Value,
    protocol: &str,
) -> Result<Option<serde_json::Value>, String> {
    let port = endpoint.port()?;
    let body = serde_json::to_vec(request).map_err(|_| "Invalid JSON request")?;
    if body.len() > MAX_JSON_BYTES {
        return Err("MCP request exceeds the transport limit".into());
    }
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, port).into(),
        Duration::from_secs(5),
    )
    .map_err(|_| "The RapidRoom endpoint is stale or the editor is not running")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(90)))
        .map_err(|_| "Cannot set MCP read timeout")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|_| "Cannot set MCP write timeout")?;
    let method = request["method"].as_str().ok_or("Missing MCP method")?;
    if !method
        .bytes()
        .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'/' | b'_' | b'-'))
    {
        return Err("Invalid MCP method".into());
    }
    if !protocol.bytes().all(|v| v.is_ascii_digit() || v == b'-') {
        return Err("Invalid protocol version".into());
    }
    let mut header = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nAuthorization: Bearer {}\r\nMcp-Protocol-Version: {protocol}\r\nMcp-Method: {method}\r\nContent-Length: {}\r\nConnection: close\r\n",
        endpoint.token,
        body.len()
    );
    if method == "tools/call" {
        let name = request["params"]["name"]
            .as_str()
            .ok_or("Missing tool name")?;
        if !name
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'_' | b'-'))
        {
            return Err("Invalid tool name".into());
        }
        header.push_str(&format!("Mcp-Name: {name}\r\n"));
    }
    header.push_str("\r\n");
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|_| "MCP connection write failed")?;
    let mut response = Vec::new();
    stream
        .take((MAX_JSON_BYTES + 32769) as u64)
        .read_to_end(&mut response)
        .map_err(|_| "MCP connection read failed")?;
    if response.len() > MAX_JSON_BYTES + 32768 {
        return Err("MCP response exceeds the transport limit".into());
    }
    let split = response
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .filter(|v| *v <= 32768)
        .ok_or("Invalid MCP HTTP response")?;
    let headers =
        std::str::from_utf8(&response[..split]).map_err(|_| "Invalid MCP HTTP headers")?;
    let status = headers
        .lines()
        .next()
        .and_then(|v| v.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u16>().ok())
        .ok_or("Invalid MCP HTTP status")?;
    if status != 200 && status != 202 {
        return Err(format!(
            "RapidRoom refused MCP request (HTTP {status}); restart the proxy if the editor restarted"
        ));
    }
    let body = &response[split + 4..];
    if body.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(body)
        .map(Some)
        .map_err(|_| "Invalid MCP JSON response".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Endpoint {
        Endpoint {
            url: "http://127.0.0.1:7790/mcp".into(),
            token: "a".repeat(64),
            protocol_version: "2026-07-28".into(),
        }
    }
    #[test]
    fn refuses_missing_wrong_and_partial_bearers() {
        let token = fixture().token;
        let verifier = BearerVerifier::new(&token);
        assert!(verifier.accepts(Some(&format!("Bearer {token}"))));
        for header in [
            None,
            Some(""),
            Some("Basic invalid"),
            Some("Bearer "),
            Some("Bearer a"),
            Some(&format!("Bearer {}", "b".repeat(64))),
        ] {
            assert!(!verifier.accepts(header));
        }
        for i in 0..64 {
            let mut changed = token.as_bytes().to_vec();
            changed[i] = b'b';
            assert!(!verifier.accepts(Some(&format!(
                "Bearer {}",
                String::from_utf8(changed).unwrap()
            ))));
        }
    }
    #[test]
    fn refuses_remote_or_injected_endpoint_urls() {
        for url in [
            "http://example.com:7790/mcp",
            "https://127.0.0.1:7790/mcp",
            "http://127.0.0.1:0/mcp",
            "http://127.0.0.1:7790/mcp?other",
            "http://127.0.0.1:7790/mcp\r\nX: y",
            "http://127.0.0.1:7790@evil/mcp",
        ] {
            let mut endpoint = fixture();
            endpoint.url = url.into();
            assert!(endpoint.port().is_err());
        }
    }
    #[test]
    #[cfg(unix)]
    fn replaces_public_or_symlink_endpoint_with_private_file_and_rejects_public_reads() {
        use std::os::unix::{
            fs::MetadataExt,
            fs::{PermissionsExt, symlink},
        };
        let dir = std::env::temp_dir().join(format!(
            "rapidroom-endpoint-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let file = dir.join("endpoint.json");
        let victim = dir.join("victim");
        fs::write(&victim, b"unchanged").unwrap();
        symlink(&victim, &file).unwrap();
        write_endpoint(&file, &fixture()).unwrap();
        assert_eq!(fs::read(&victim).unwrap(), b"unchanged");
        assert!(!fs::symlink_metadata(&file).unwrap().is_symlink());
        assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
        assert_eq!(read_endpoint(&file).unwrap().token, fixture().token);
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_endpoint(&file).is_err());
        write_endpoint(&file, &fixture()).unwrap();
        assert!(read_endpoint(&file).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }
}
