//! CLI client: add-todo subcommand calls the running MCP server so list creation and id generation stay on the server.
//! Usage: todo-mcp add [--server URL] [--port N] [--json] <list_name> <title> [body]. Env: MCP_PORT (default 8080).

const MCP_SESSION_ID: &str = "mcp-session-id";
const ACCEPT_HEADER: &str = "application/json, text/event-stream";

pub const EXIT_CONNECT: i32 = 1;
pub const EXIT_INPUT: i32 = 2;
pub const EXIT_MCP: i32 = 3;

#[derive(Debug)]
pub struct AddOptions {
    pub base_url: String,
    pub list_name: String,
    pub title: String,
    pub body: String,
    pub json: bool,
    pub json_pretty: bool,
    pub quiet: bool,
    pub timeout_secs: Option<u64>,
    pub summary: bool,
    pub dry_run: bool,
}

pub fn parse_add_args(args: &[String]) -> anyhow::Result<AddOptions> {
    let mut i = 0;
    let mut server_url: Option<String> = None;
    let mut port: Option<u16> = std::env::var("MCP_PORT").ok().and_then(|s| s.parse().ok());
    let mut json = false;
    let mut json_pretty = false;
    let mut quiet = false;
    let mut timeout_secs: Option<u64> = None;
    let mut summary = false;
    let mut interactive = false;
    let mut dry_run = false;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg == "--server" || arg == "-s" {
            i += 1;
            let url = args.get(i).ok_or_else(|| anyhow::anyhow!("--server requires URL"))?;
            let s = url.as_str();
            server_url = Some(if s.starts_with("http://") || s.starts_with("https://") {
                s.to_string()
            } else {
                format!("http://{}", s)
            });
            i += 1;
        } else if arg == "--port" || arg == "-p" {
            i += 1;
            let p = args.get(i).ok_or_else(|| anyhow::anyhow!("--port requires number"))?;
            port = Some(p.parse().map_err(|_| anyhow::anyhow!("invalid port: {}", p))?);
            i += 1;
        } else if arg == "--timeout" || arg == "-t" {
            i += 1;
            let t = args.get(i).ok_or_else(|| anyhow::anyhow!("--timeout requires seconds"))?;
            timeout_secs = Some(t.parse().map_err(|_| anyhow::anyhow!("invalid timeout: {}", t))?);
            i += 1;
        } else if arg == "--json" || arg == "-j" {
            json = true;
            i += 1;
        } else if arg == "--pretty" {
            json_pretty = true;
            i += 1;
        } else if arg == "--quiet" || arg == "-q" {
            quiet = true;
            i += 1;
        } else if arg == "--summary" {
            summary = true;
            i += 1;
        } else if arg == "--interactive" || arg == "-i" {
            interactive = true;
            i += 1;
        } else if arg == "--dry-run" {
            dry_run = true;
            i += 1;
        } else {
            break;
        }
    }
    let rest = &args[i..];
    let usage = "usage: add [--server URL] [--port N] [--timeout SECS] [--json] [--pretty] [--quiet] [--summary] [--interactive] <list_name> <title> [body|-]";
    let (list_name, title, body) = match rest {
        [a, b, c, ..] => (a.as_str(), b.as_str(), c.as_str()),
        [a, b] => (a.as_str(), b.as_str(), ""),
        [a] if interactive => (a.as_str(), "", ""),
        [_] => anyhow::bail!("{}", usage),
        [] if interactive => ("", "", ""),
        [] => anyhow::bail!("{}", usage),
    };
    let (list_name, title, body) = if interactive && (list_name.is_empty() || title.is_empty()) {
        let (l, t, b) = prompt_add(list_name, title, body)?;
        (l, t, b)
    } else {
        let body_str = if body == "-" {
            read_stdin_body()?
        } else {
            body.to_string()
        };
        (list_name.to_string(), title.to_string(), body_str)
    };
    if !interactive {
        if list_name.is_empty() {
            anyhow::bail!("list name is required (use -i/--interactive to prompt, or see usage)");
        }
        if title.is_empty() {
            anyhow::bail!("title is required (use -i/--interactive to prompt, or see usage)");
        }
    }
    let base_url = match (server_url, port) {
        (Some(u), _) => u,
        (None, Some(p)) => format!("http://127.0.0.1:{}", p),
        (None, None) => "http://127.0.0.1:8080".to_string(),
    };
    Ok(AddOptions {
        base_url,
        list_name,
        title,
        body,
        json,
        json_pretty,
        quiet,
        timeout_secs,
        summary,
        dry_run,
    })
}

pub fn print_dry_run(opts: &AddOptions) {
    eprintln!("dry-run: would add todo to server");
    eprintln!("  server: {}", opts.base_url);
    eprintln!("  list:   {}", opts.list_name);
    eprintln!("  title:  {}", opts.title);
    eprintln!("  body:   {} bytes", opts.body.len());
}

pub fn print_success_id(id: u32) {
    if color_enabled() {
        println!("\x1b[32m{}\x1b[0m", id);
    } else {
        println!("{}", id);
    }
}

pub fn print_success_summary(id: u32, list: &str, title: &str) {
    if color_enabled() {
        println!("\x1b[32mAdded todo #{} to {}: {}\x1b[0m", id, list, title);
    } else {
        println!("Added todo #{} to {}: {}", id, list, title);
    }
    println!("{}", id);
}

fn prompt_add(
    list_name: &str,
    title: &str,
    body: &str,
) -> anyhow::Result<(String, String, String)> {
    use std::io::{self, Write};
    let mut out = io::stderr();
    let read_line = || {
        let mut s = String::new();
        io::stdin().read_line(&mut s).map(|_| s.trim_end().to_string())
    };
    let list_name = if list_name.is_empty() {
        out.write_all(b"List name: ").ok();
        out.flush().ok();
        read_line()?
    } else {
        list_name.to_string()
    };
    let title = if title.is_empty() {
        out.write_all(b"Title: ").ok();
        out.flush().ok();
        read_line()?
    } else {
        title.to_string()
    };
    let body = if body == "-" {
        read_stdin_body()?
    } else if body.is_empty() {
        out.write_all(b"Body (optional, empty line to finish): ").ok();
        out.flush().ok();
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() || line.trim().is_empty() {
                break;
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    } else {
        body.to_string()
    };
    Ok((list_name, title, body))
}

pub fn print_add_help(bin: &str) {
    eprintln!(
        r#"todo-mcp - Todo list MCP server and CLI

USAGE:
  {bin}                    Start the MCP server (default: http://127.0.0.1:8080)
  {bin} add [OPTIONS] <list_name> <title> [body|-]

ADD OPTIONS:
  -s, --server <URL>   MCP server URL (default: http://127.0.0.1:8080)
  -p, --port <N>       Server port (overrides MCP_PORT env)
  -t, --timeout <SECS> Request timeout in seconds (avoids hanging in scripts)
  -j, --json           Print result as JSON (id, list, title, server) for scripting
  --pretty             Pretty-print JSON (use with --json)
  -q, --quiet          Id-only output (default when not --json)
  --summary            Print one-line summary and id on next line (script-friendly)
  -i, --interactive    Prompt for list/title/body when omitted
  --dry-run           Preview resolved server/list/title/body without sending request

ENVIRONMENT:
  MCP_PORT         Port when --port not set (default: 8080)

EXAMPLES:
  {bin} add mylist "Buy milk"
  MCP_PORT=8081 {bin} add work "Fix bug" "Description here"
  {bin} add notes "Title" --json
  echo "body text" | {bin} add mylist "Title" -

EXIT CODES (add command):
  0  success
  1  connection failed (server not running or unreachable)
  2  invalid input or usage
  3  MCP/server error (e.g. bad list name, validation)
"#,
        bin = bin
    );
}


fn read_stdin_body() -> anyhow::Result<String> {
    use std::io::Read;
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s)?;
    Ok(s.trim_end().to_string())
}

fn color_enabled() -> bool {
    use std::io::IsTerminal;
    std::env::var("NO_COLOR").is_err() && std::io::stderr().is_terminal()
}

pub fn report_add_error(e: &anyhow::Error) {
    if color_enabled() {
        eprintln!("\x1b[31mError:\x1b[0m {}", e);
    } else {
        eprintln!("Error: {}", e);
    }
}

pub fn exit_code_for_error(e: &anyhow::Error) -> i32 {
    let s = e.to_string();
    if s.contains("usage")
        || s.contains("invalid")
        || s.contains("must be")
        || s.contains("requires")
        || s.contains("required")
    {
        EXIT_INPUT
    } else if s.contains("connection")
        || s.contains("Connection")
        || s.contains("failed to connect")
        || s.contains("timeout")
        || s.contains("deadline")
    {
        EXIT_CONNECT
    } else {
        EXIT_MCP
    }
}

fn parse_sse_first_json(bytes: &[u8]) -> Option<serde_json::Value> {
    for line in std::str::from_utf8(bytes).ok()?.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            return serde_json::from_str(data).ok();
        }
    }
    None
}

pub async fn add_via_server(
    base_url: &str,
    list_name: &str,
    title: &str,
    body: &str,
    timeout_secs: Option<u64>,
) -> anyhow::Result<u32> {
    let mut builder = reqwest::Client::builder();
    if let Some(secs) = timeout_secs {
        builder = builder.timeout(std::time::Duration::from_secs(secs));
    }
    let client = builder.build()?;
    let init_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "todo-mcp-cli", "version": "0.1.0" }
        }
    });
    let init_resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .json(&init_body)
        .send()
        .await?;
    let session_id = init_resp
        .headers()
        .get(MCP_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("missing {} header", MCP_SESSION_ID))?;
    let init_bytes = init_resp.bytes().await?;
    let init_msg = parse_sse_first_json(&init_bytes).ok_or_else(|| anyhow::anyhow!("invalid initialize response"))?;
    if init_msg.get("error").is_some() {
        let msg = init_msg["error"]["message"].as_str().unwrap_or("unknown");
        anyhow::bail!("initialize failed: {}", msg);
    }
    let initialized_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let _ = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id.as_str())
        .json(&initialized_body)
        .send()
        .await?;
    let tool_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "add-todo",
            "arguments": { "list_name": list_name, "title": title, "body": body }
        }
    });
    let tool_resp = client
        .post(format!("{}/mcp", base_url))
        .header("Accept", ACCEPT_HEADER)
        .header("Content-Type", "application/json")
        .header(MCP_SESSION_ID, session_id.as_str())
        .json(&tool_body)
        .send()
        .await?;
    let tool_bytes = tool_resp.bytes().await?;
    let tool_msg = parse_sse_first_json(&tool_bytes).ok_or_else(|| anyhow::anyhow!("invalid tools/call response"))?;
    if let Some(err) = tool_msg.get("error") {
        let msg = err["message"].as_str().unwrap_or("unknown");
        anyhow::bail!("add-todo failed: {}", msg);
    }
    let text = tool_msg
        .get("result")
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing result content"))?;
    let after_hash = text
        .strip_prefix("Added todo #")
        .ok_or_else(|| anyhow::anyhow!("unexpected result format: {}", text))?;
    let digits: String = after_hash.chars().take_while(|c| c.is_ascii_digit()).collect();
    let id: u32 = digits.parse().map_err(|_| anyhow::anyhow!("could not parse id from: {}", text))?;
    Ok(id)
}
