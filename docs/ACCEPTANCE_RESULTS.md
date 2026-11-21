# 验收结果（Composer 运行）

环境：Linux cloud agent，rustc 1.100，合成 DB。

| ID | 结果 | 命令/证据 |
| --- | --- | --- |
| A00 | VERIFIED（归档） | zip SHA256 与用户给定一致 |
| A01 | PASS | `cursor-github get_repository` + feature 分支导入 |
| A06 | PASS | `cargo test cross_matter_private_search_denied` |
| A13-A14 | PARTIAL | 合成 fixture + LIKE/FTS；无实时采集 |
| A02-A03 | PASS（Linux REAL_RUNTIME_STUB） | 下载 tar.gz SHA256=ce9cdfd3…；`initialize` 返回 server_version 0.9.0 |
| A04-A05 | NOT RUN | 无本地 LLM 配置 |
| A16 | NOT RUN | 无 GUI 交互验收 |
| A19 | NOT RUN | Windows named pipe |

## 已运行测试摘录

```
cargo test -p core -p deadline-engine -p gap-engine -p lurus-desktop --lib
# core 5 passed, gap-engine 6 passed, lurus-desktop 4 passed
cd apps/desktop && npm run build
# vite build ok
```