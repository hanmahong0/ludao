# 律道（ludao）

本地优先法律工作台：Tauri + React + Rust（SQLCipher），含受控 ZeroClaw 助手接入点（默认关闭）与合成类案检索 fixture。

## 状态

- 源码来自经白名单审查的快照重建；**非**原机全量仓库。
- 第一方未选定整库开源许可证；第三方见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。

## 开发

```bash
cargo test -p core -p lurus-desktop --lib
cd apps/desktop && npm ci && npm run build
```

## 助手与 ZeroClaw

固定 `v0.9.0`（commit `6c78ba59`）。设置 `LUDAO_ZEROCLAW_BIN` 与可选 `LUDAO_ZEROCLAW_SOCKET` 后于 UI 启用。无本地模型时仅提供工具检索 + 规则化草稿，UI 标明 runtime 级别。

## 数据

仅使用合成案件/类案 fixture；不得导入真实案卷。