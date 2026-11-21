# Third-Party Notices（进行中）

本仓库第一方源码未经统一开源许可证授权；请勿视为 MIT/Apache 整库许可。

## 已固定引用

| 组件 | 版本/Commit | 许可 | 备注 |
| --- | --- | --- | --- |
| ZeroClaw | v0.9.0 / 6c78ba59… | MIT OR Apache-2.0 | 运行时二进制未随仓库分发；见 `vendor/zeroclaw.lock.json` |
| Tauri / React 等 | 见 lockfiles | 各上游许可 | 需按实际 lock 版本生成 NOTICE |
| rusqlite SQLCipher | Cargo.lock 0.31 | 见上游 + OpenSSL | 未执行二进制供应链审计 |
| PyMuPDF | — | 未引入 | legacy 未发布 |

未完成项保持「未验证」，不声明「全清」。