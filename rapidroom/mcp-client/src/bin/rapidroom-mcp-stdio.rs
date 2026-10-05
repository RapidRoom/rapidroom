use rapidroom_mcp_client::{MAX_JSON_BYTES, endpoint_path, forward, read_endpoint};
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!(
            "rapidroom-mcp-stdio: authenticated MCP stdio proxy for the running editor\nUsage: rapidroom-mcp-stdio\nReads the per-user RapidRoom mcp-endpoint.json; never starts the GUI."
        );
        return Ok(());
    }
    if !args.is_empty() {
        return Err("Unknown argument; use --help".into());
    }
    let endpoint = read_endpoint(&endpoint_path()?)?;
    let mut protocol = endpoint.protocol_version.clone();
    let mut capabilities = json!({});
    let input = io::stdin();
    let mut input = input.lock();
    let output = io::stdout();
    let mut output = output.lock();
    loop {
        let mut line = Vec::new();
        let count = input
            .by_ref()
            .take((MAX_JSON_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|_| "Cannot read MCP stdin")?;
        if count == 0 {
            return Ok(());
        }
        if count > MAX_JSON_BYTES {
            return Err("MCP stdin message exceeds the transport limit".into());
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let mut request: Value =
            serde_json::from_slice(&line).map_err(|_| "Invalid JSON on MCP stdin")?;
        if request["jsonrpc"] != "2.0" || !request.is_object() {
            return Err("Expected JSON-RPC 2.0 request".into());
        }
        let method = request["method"]
            .as_str()
            .ok_or("Missing MCP method")?
            .to_owned();
        if method == "initialize" {
            protocol = request["params"]["protocolVersion"]
                .as_str()
                .ok_or("Missing protocol version")?
                .to_owned();
            capabilities = request["params"]["capabilities"].clone();
        } else if protocol.as_str() >= "2026-07-28" {
            if request.get("params").is_none() {
                request["params"] = json!({});
            }
            let params = request["params"]
                .as_object_mut()
                .ok_or("Expected object MCP params")?;
            let meta = params
                .entry("_meta")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or("Expected object MCP metadata")?;
            meta.insert(
                "io.modelcontextprotocol/protocolVersion".into(),
                json!(protocol),
            );
            meta.insert(
                "io.modelcontextprotocol/clientCapabilities".into(),
                capabilities.clone(),
            );
        }
        let response = match forward(&endpoint, &request, &protocol) {
            Ok(value) => value,
            Err(error) if request.get("id").is_some() => Some(
                json!({"jsonrpc":"2.0","id":request["id"],"error":{"code":-32000,"message":error}}),
            ),
            Err(error) => return Err(error),
        };
        if let Some(response) = response {
            if method == "initialize"
                && let Some(version) = response["result"]["protocolVersion"].as_str()
            {
                protocol = version.to_owned();
            }
            serde_json::to_writer(&mut output, &response).map_err(|_| "Cannot write MCP stdout")?;
            output
                .write_all(b"\n")
                .and_then(|_| output.flush())
                .map_err(|_| "Cannot flush MCP stdout")?;
        }
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("rapidroom-mcp-stdio: {error}");
        std::process::exit(1);
    }
}
