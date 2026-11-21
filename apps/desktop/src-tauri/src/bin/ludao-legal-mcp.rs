//! 只读法律 MCP（stdio JSON-RPC 行协议）。由宿主设置 LUDAO_DB_PATH 与 LUDAO_CAPABILITY_TOKEN。

use lurus_desktop_lib::commands;
use lurus_desktop_lib::db;
use lurus_desktop_lib::scope::ScopeRegistry;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn respond(id: Value, result: Value) {
    let line = json!({"jsonrpc":"2.0","id":id,"result":result});
    writeln!(io::stdout(), "{line}").ok();
    io::stdout().flush().ok();
}

fn respond_err(id: Value, message: &str) {
    let line = json!({"jsonrpc":"2.0","id":id,"error":{"message":message}});
    writeln!(io::stdout(), "{line}").ok();
    io::stdout().flush().ok();
}

fn main() {
    let db_path = std::env::var("LUDAO_DB_PATH").expect("LUDAO_DB_PATH required");
    let cap = std::env::var("LUDAO_CAPABILITY_TOKEN").expect("LUDAO_CAPABILITY_TOKEN required");
    let matter = std::env::var("LUDAO_MATTER_SCOPE").expect("LUDAO_MATTER_SCOPE required");
    db::enable_dev_config_trust_for_tests();
    let conn = db::open_and_init(&db_path).expect("open db");
    let scope = ScopeRegistry::new();
    scope
        .validate(&cap, &matter)
        .expect("invalid capability for MCP");
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let id = v.get("id").cloned().unwrap_or(Value::Null);
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        match method {
            "initialize" => respond(id, json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}}})),
            "tools/list" => respond(
                id,
                json!({"tools":[
                  {"name":"search_private_documents","description":"Read-only scoped search","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}},
                  {"name":"search_public_cases","description":"Licensed public corpus","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}}
                ]}),
            ),
            "tools/call" => {
                let params = v.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|l| l.as_i64()).unwrap_or(5);
                let out = match name {
                    "search_private_documents" => {
                        if cap.is_empty() {
                            respond_err(id, "scope_denied");
                            continue;
                        }
                        match commands::search_private_documents(
                            &conn, &scope, &cap, &matter, query, limit,
                        ) {
                            Ok(hits) => json!({"content":[{"type":"text","text":serde_json::to_string(&hits).unwrap()}]}),
                            Err(e) => {
                                respond_err(id, &e);
                                continue;
                            }
                        }
                    }
                    "search_public_cases" => match commands::search_public_cases(&conn, query, limit) {
                        Ok(hits) => json!({"content":[{"type":"text","text":serde_json::to_string(&hits).unwrap()}]}),
                        Err(e) => {
                            respond_err(id, &e);
                            continue;
                        }
                    },
                    _ => {
                        respond_err(id, "tool_not_allowed");
                        continue;
                    }
                };
                respond(id, out);
            }
            _ => respond_err(id, "method_not_supported"),
        }
    }
}