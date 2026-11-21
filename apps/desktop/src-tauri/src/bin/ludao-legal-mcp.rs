//! 只读法律 MCP。只打开已存在的快照，不播种、不迁移、不打开开发信任。
//! 授权来自宿主写入的 grant 文件。伪造的环境变量票据无效。票据不进入 argv。

use lurus_desktop_lib::commands;
use lurus_desktop_lib::db;
use lurus_desktop_lib::scope::ScopeRegistry;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

fn respond(id: Value, result: Value) {
    let line = json!({"jsonrpc":"2.0","id":id,"result":result});
    writeln!(io::stdout(), "{line}").ok();
    let _ = io::stdout().flush();
}

fn respond_err(id: Value, message: &str) {
    let line = json!({"jsonrpc":"2.0","id":id,"error":{"message":message}});
    writeln!(io::stdout(), "{line}").ok();
    let _ = io::stdout().flush();
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let db_path = match std::env::var("LUDAO_SNAPSHOT_DB") {
        Ok(v) => v,
        Err(_) => return fail("LUDAO_SNAPSHOT_DB required"),
    };
    let grant_path = match std::env::var("LUDAO_GRANT_FILE") {
        Ok(v) => v,
        Err(_) => return fail("LUDAO_GRANT_FILE required"),
    };
    let session = match std::env::var("LUDAO_HOST_SESSION") {
        Ok(v) => v,
        Err(_) => return fail("LUDAO_HOST_SESSION required"),
    };
    if !Path::new(&db_path).is_file() {
        return fail("snapshot_missing");
    }
    let scope = match ScopeRegistry::from_grant_file(Path::new(&grant_path), &session) {
        Ok(s) => s,
        Err(e) => return fail(&e),
    };
    let text = match std::fs::read_to_string(&grant_path) {
        Ok(t) => t,
        Err(e) => return fail(&e.to_string()),
    };
    let meta: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return fail(&e.to_string()),
    };
    let matter_id = meta.get("matter_id").and_then(|m| m.as_str()).unwrap_or("").to_string();
    let token = meta.get("token").and_then(|t| t.as_str()).unwrap_or("").to_string();
    if scope.validate(&token, &matter_id, &session).is_err() {
        return fail("scope_denied: snapshot grant rejected");
    }
    let conn = match db::open_readonly_snapshot(&db_path) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let id = v.get("id").cloned().unwrap_or(Value::Null);
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        match method {
            "initialize" => respond(
                id,
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}),
            ),
            "tools/list" => respond(
                id,
                json!({"tools":[
                  {"name":"search_private_documents","description":"Read-only search inside the host snapshot","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}},
                  {"name":"search_public_cases","description":"Licensed public corpus","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"]}}
                ]}),
            ),
            "tools/call" => {
                let params = v.get("params").cloned().unwrap_or(json!({}));
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if let Some(asked) = args
                    .get("case_id")
                    .and_then(|c| c.as_str())
                    .or_else(|| args.get("matter_id").and_then(|c| c.as_str()))
                {
                    if asked != matter_id {
                        respond_err(id, "scope_denied: tool cannot widen matter");
                        continue;
                    }
                }
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|l| l.as_i64()).unwrap_or(5);
                let out = match name {
                    "search_private_documents" => {
                        match commands::search_private_documents(
                            &conn, &scope, &token, &session, &matter_id, query, limit,
                        ) {
                            Ok(hits) => json!({"content":[{"type":"text","text":serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into())}]}),
                            Err(e) => {
                                respond_err(id, &e);
                                continue;
                            }
                        }
                    }
                    "search_public_cases" => match commands::search_public_cases(&conn, query, limit)
                    {
                        Ok(hits) => json!({"content":[{"type":"text","text":serde_json::to_string(&hits).unwrap_or_else(|_| "[]".into())}]}),
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
            "notifications/initialized" | "initialized" => {}
            _ => respond_err(id, "method_not_supported"),
        }
    }
    ExitCode::SUCCESS
}
