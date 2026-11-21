# 律道（ludao）— Agent 工作说明

## 构建与测试

```bash
rustup default stable   # rustc 1.100+
cd /workspace
cargo test -p core -p deadline-engine -p gap-engine -p lurus-desktop --lib
cd apps/desktop && npm ci && npm run build
```

Tauri 完整桌面构建在 Linux 需 GTK/WebKit 系统库；本环境可验证 Rust 逻辑与前端构建。

## 架构要点

- **台账事实源**：SQLCipher `lurus.db`（案件/证据/期限）；助手 Memory 不是业务事实库。
- **助手**：`AssistantSupervisor` 管理 ZeroClaw sidecar，**默认关闭**；`LUDAO_ZEROCLAW_BIN` / `LUDAO_ZEROCLAW_SOCKET`。
- **法律工具桥**：`search_private_documents` / `search_public_cases` 由宿主校验 `matter_id` scope。
- **配置签名**：`crates/core/src/config.rs`；生产设 `LUDAO_PINNED_CONFIG_PUBKEY` 后禁用 dev 自签。

## 验收证据

见 `docs/decisions.tsv` 与 `docs/ACCEPTANCE_RESULTS.md`（本 PR）。

## 禁止

- 不提交案卷/真实 DB/`.env`/legacy/anjuan。
- 不用云模型密钥或隐式 fallback。
- 不把 stub 标称 REAL_RUNTIME_LOCAL_MODEL。