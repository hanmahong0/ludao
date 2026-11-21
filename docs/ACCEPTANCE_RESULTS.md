# 验收结果（Grok 实现轮 · PR #1）

先前 A06 / D009 / D010 的 PASS 超过了证据：公开常量 HMAC 可重算、撤销后仍可验证、MCP 会创建数据库、`mcp_attached` 只看文件是否存在。本表按本轮实际命令改写。D011–D015 追加，不改旧行。

环境：Linux cloud agent。合成临时库。没有用户案件，没有模型密钥。

| ID | 结果 | 命令/证据 |
| --- | --- | --- |
| A00 | VERIFIED | 导入白名单 zip，见 D001 |
| A01 | PASS | feature 分支，未 merge |
| A02 | PARTIAL | 官方 tar.gz SHA256 已记；解压后 linux 二进制 SHA256 `6fe61f4c0288e355081a0a93619b51e7ce572412be99c2e8fb0a59244e0b496b` 已钉。Windows 解压 exe **未核** |
| A03 | PASS（本机 daemon） | `cargo test -p lurus-desktop --test legal_loop`：initialize、`session/new` `agent_alias=legal`、日志出现工具注册、空闲 `session/cancel` 返回错误、`session/state` 仍为 idle |
| A04-A05 | NOT RUN | 无法律模型密钥。设置 `LUDAO_SIM_LLM_URL` 时只标 `simulation_turn`，不能写成产品工具回合已验证 |
| A06 | PASS（本轮探针） | 同一 `legal_loop`：私案 A 命中、另一存在案件 `case-002` `scope_denied`、伪造环境变量票据不扩大范围、撤销文件/过期/错会话/空 registry 进程拒绝、未知工具 `tool_not_allowed`。不再把两个单测写成整条授权链已通过 |
| A13-A14 | PASS | `"借贷"` 与 `借贷` 同一命中；`%` / 超长拒绝；limit 0 与 9999 被夹紧 |
| A16 | NOT VERIFIED | 无 Tauri 显示。浏览器预览只标合成演示 |
| A19 | NOT VERIFIED | Windows named pipe 与 Windows 解压二进制都未跑 |

## 本轮已跑（实现机）

- `cargo test -p core --lib -- --test-threads=1`：8 passed
- `cargo test -p core --release --lib config::tests -- --test-threads=1`：3 passed（release 下 `LUDAO_CONFIG_DEV_TRUST=1` 仍不打开开发信任）
- `cargo test -p lurus-desktop --lib -- --test-threads=1`：19 passed，1 ignored（仅握手的旧测试；完整探针在 `legal_loop`）
- `cargo test -p lurus-desktop --test legal_loop -- --test-threads=1 --nocapture`：1 passed
- `npm --prefix apps/desktop run build`：exit 0
- GUI / Tauri / Windows：NOT VERIFIED

## 下一轮独立复验

```bash
bash scripts/verify_legal_loop.sh
# 或分开：
cargo test -p core --lib -- --test-threads=1
cargo test -p lurus-desktop --lib -- --test-threads=1
cargo test -p lurus-desktop --test legal_loop -- --test-threads=1 --nocapture
npm --prefix apps/desktop run build
```

`cargo test` 一次只能带一个测试过滤器。不要把两个过滤器写在同一条命令里。

## Runtime 证据分级

| 级别 | 含义 |
| --- | --- |
| disabled | 助手关 |
| rules_summary_only | 宿主检索拼规则摘要，没有模型回合 |
| product_mcp_enumerated | daemon 已登记法律 MCP 工具，没有模型回合 |
| simulation_turn | 显式 `LUDAO_SIM_LLM_URL` 的本地 HTTP fixture，不是法律结论 |
| product_tool_turn_verified | 保留给真正的法律模型回合；本轮没有 |
