//! Drives `claybroom --mcp` over stdin/stdout like an AI tool would: handshake, tool list, a
//! duplicate scan of a throw-away home folder, and its results. Every line on stdout must be
//! JSON-RPC (anything else would break the client).
#![cfg(all(unix, feature = "mcp"))]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn an_ai_tool_can_scan_and_read_results() {
    let home = std::env::temp_dir().join(format!("claybroom-mcp-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("Documents")).unwrap();
    std::fs::create_dir_all(home.join("Downloads")).unwrap();
    let copy = vec![7u8; 600_000]; // over the default 500 KB duplicate limit
    std::fs::write(home.join("Documents/report.pdf"), &copy).unwrap();
    std::fs::write(home.join("Downloads/report (1).pdf"), &copy).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_claybroom"))
        .arg("--mcp")
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start claybroom --mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    // id 0 sends a notification (no reply expected).
    let mut ask = |id: u64, method: &str, params: Value| -> Value {
        if id == 0 {
            writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "method": method})).unwrap();
            return Value::Null;
        }
        writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        loop {
            let line = lines.next().expect("server closed stdout").unwrap();
            let msg: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("not JSON-RPC on stdout: {line}"));
            if msg["id"] == id {
                assert!(msg.get("error").is_none(), "{method} failed: {msg}");
                return msg["result"].clone();
            }
        }
    };

    let init = ask(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}));
    assert_eq!(init["serverInfo"]["name"], "claybroom");
    assert!(init["instructions"].as_str().unwrap().contains("untrusted"));
    ask(0, "notifications/initialized", Value::Null);

    let tools = ask(2, "tools/list", json!({}));
    let tools = tools["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 10);
    for t in tools {
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{} must be read-only in phase 1", t["name"]);
    }

    let call = |r: Value| -> Value {
        assert_ne!(r["isError"], true, "tool error: {r}");
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    };
    let scan = call(ask(3, "tools/call", json!({"name": "scan_start", "arguments": {"scope": "duplicates", "wait_secs": 20}})));
    assert_eq!(scan["state"], "done", "{scan}");
    let dups = call(ask(4, "tools/call", json!({"name": "list_duplicates", "arguments": {"scan_id": scan["scan_id"]}})));
    assert_eq!(dups["total"], 1, "{dups}");
    let copies = dups["groups"][0]["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 2);
    assert!(copies.iter().all(|c| c["location"].as_str().unwrap().starts_with("~/")), "home stays private: {copies:?}");
    assert_eq!(copies.iter().filter(|c| c["suggested_keep"] == true).count(), 1);

    drop(ask);
    let _ = child.kill();
    let _ = child.wait();
    // Read-only: the files are still there.
    assert!(home.join("Downloads/report (1).pdf").exists());
    let _ = std::fs::remove_dir_all(&home);
}
