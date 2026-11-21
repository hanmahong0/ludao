# 验收结果（Composer 重跑 · PR #1 修复后）

环境：Linux cloud agent，rustc 1.100，合成 DB。

| ID | 结果 | 命令/证据 |
| --- | --- | --- |
| A00 | VERIFIED | zip SHA256 与用户给定一致 |
| A01 | PASS | feature 分支导入 |
| A02 | PARTIAL | 官方 tar.gz SHA256 核对；产品客户端 `parses_snake_case_initialize_result` PASS |
| A03 | PARTIAL | sidecar spawn + `RpcClient::initialize` 集成测试（`#[ignore]`，需 `LUDAO_ZEROCLAW_BIN`） |
| A04-A05 | NOT RUN | 无本地 LLM |
| A06 | PASS | `cross_matter_private_search_denied_with_grant`、`wildcard_private_query_rejected` |
| A13-A14 | PARTIAL | FTS + LIKE 回退；`%` 拒绝而非 500 |
| A16 | NOT RUN | 无 Tauri GUI 点击验收 |
| A19 | NOT RUN | Windows named pipe |

## 已运行测试（本轮）

```bash
cargo test -p core -p lurus-desktop --lib
# core 7 passed, lurus-desktop 9 passed

cd apps/desktop && npm run build
```

## Runtime 证据分级（修复后）

| 级别 | 含义 | 何时使用 |
| --- | --- | --- |
| disabled | 助手关 | 默认 |
| rules_summary_only | 宿主工具检索拼规则摘要，无 LLM turn | 明确标注，非 stub_model |
| product_handshake_verified | 产品 RPC initialize + sidecar 存活 | 已发生握手，无 turn |
| product_tool_turn_verified | session 收到 turn_complete 文本 | 需本地模型 |