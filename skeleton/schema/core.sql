-- =============================================================================
-- 律师 Agent 桌面客户端 · 本地加密案卷库 核心 Schema（SQLite / SQLCipher）
-- 对应 架构定档-v5：D1「数据主权在端」+ D4 三案由 + v4 §5 迁移最小化
-- 红线（CI 强制校验）：
--   · 本机为案件内容唯一权威副本；云端禁止持久化原始内容
--   · 期限计算与原件相关形式要件判断永远本地求值（见 deadline 表，绝不交 LLM）
--   · 原件哈希在采集时刻本地盖戳，写入监管链 custody_event（append-only）
-- 迁移纪律：核心表钉死最小稳定；快变字段一律进 attributes JSON 列（加字段免迁移）；
--   结构迁移 forward-only / 幂等 / 原子，在事务内 bump user_version；只 roll-forward。
-- =============================================================================

PRAGMA user_version = 1;          -- 迁移门禁：旧客户端见到更高版本 → 强制升级/只读，绝不脏写
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ---- 元信息：schema 指纹 / 同步通道指纹 ------------------------------------
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- 约定写入：schema_hash（同步通道表结构指纹，不匹配触发 re-bootstrap）、
--          last_sync_id（断点续传游标）、client_id（用于离线 mint ID）

-- =============================================================================
-- 五张核心表（case / evidence / fact / deadline / attestation 存证）
-- 通用同步列：_rev 单调递增本地修订号；_updated_at；_deleted 软删；_dirty 待同步
-- ID 约定：TEXT，形如 "{client_id}-{ulid}"，离线可独立 mint 不撞（Figma 式）
-- attributes：JSON，承接一切快变/试验字段；字段语义由 server-config 的字段目录 catalog 定义
-- =============================================================================

-- 案件 ----------------------------------------------------------------------
CREATE TABLE "case" (
  id                  TEXT PRIMARY KEY,
  case_no             TEXT,                       -- 案号（可空，立案前未知）
  title               TEXT NOT NULL,
  case_type           TEXT NOT NULL,              -- 要件 caseTypeProfile 的 key，如 private_lending
  jurisdiction        TEXT,                       -- 管辖法院 / 地域，用于期限规则集继承覆盖
  rule_set_version    TEXT,                       -- 绑定的期限规则集版本（可重算追溯）
  stage               TEXT NOT NULL DEFAULT 'intake', -- intake/evidence/pretrial/trial/post/closed
  status              TEXT NOT NULL DEFAULT 'active',
  -- 当事人暂存 attributes.parties（JSON 数组）；满足"晋升为正式列"判定后再独立成 party 表
  attributes          TEXT NOT NULL DEFAULT '{}', -- JSON
  created_at          TEXT NOT NULL,
  _rev                INTEGER NOT NULL DEFAULT 1,
  _updated_at         TEXT NOT NULL,
  _deleted            INTEGER NOT NULL DEFAULT 0,
  _dirty              INTEGER NOT NULL DEFAULT 1
);

-- 证据 ----------------------------------------------------------------------
CREATE TABLE evidence (
  id                  TEXT PRIMARY KEY,
  case_id             TEXT NOT NULL REFERENCES "case"(id),
  seq                 INTEGER,                    -- 证据序号（fractional indexing 可放 attributes）
  name                TEXT NOT NULL,
  form                TEXT,                       -- 形式：书证/物证/视听资料/电子数据/证人证言/鉴定意见/勘验笔录/当事人陈述
  source              TEXT,                       -- 来源
  proof_target        TEXT,                       -- 证明对象（缺口诊断据此对照要件）
  original_hash       TEXT,                       -- ★ 原件采集时刻本地 SHA-256，可采性命脉，写入即不可改
  captured_at         TEXT,                       -- 采集/固定时刻
  file_ref            TEXT,                       -- 指向本地加密 blob（原件不出本机，绝不进同步通道）
  ocr_status          TEXT DEFAULT 'pending',     -- pending/done/low_confidence/manual
  three_natures       TEXT NOT NULL DEFAULT '{}', -- JSON：真实性/合法性/关联性 质证标注
  linked_element_ids  TEXT NOT NULL DEFAULT '[]', -- JSON：关联到哪些要件 element id
  attestation_id      TEXT REFERENCES attestation(id), -- 存证（可空）
  attributes          TEXT NOT NULL DEFAULT '{}',
  _rev                INTEGER NOT NULL DEFAULT 1,
  _updated_at         TEXT NOT NULL,
  _deleted            INTEGER NOT NULL DEFAULT 0,
  _dirty              INTEGER NOT NULL DEFAULT 1
);

-- 事实（事实时间轴） --------------------------------------------------------
CREATE TABLE fact (
  id                  TEXT PRIMARY KEY,
  case_id             TEXT NOT NULL REFERENCES "case"(id),
  description         TEXT NOT NULL,
  fact_date           TEXT,                       -- 用于时间轴排序（冲突点高亮在 UI 做）
  fact_type           TEXT,                       -- 待证事实/背景事实/程序事实
  disputed            INTEGER NOT NULL DEFAULT 0, -- 是否争议焦点
  element_id          TEXT,                       -- 对应的法律要件 id（要件 DSL 的 element.id）
  source_evidence_ids TEXT NOT NULL DEFAULT '[]', -- JSON：支撑该事实的证据
  attributes          TEXT NOT NULL DEFAULT '{}',
  _rev                INTEGER NOT NULL DEFAULT 1,
  _updated_at         TEXT NOT NULL,
  _deleted            INTEGER NOT NULL DEFAULT 0,
  _dirty              INTEGER NOT NULL DEFAULT 1
);

-- 期限（确定性引擎的产物；★ 绝不交 LLM 计算） -------------------------------
CREATE TABLE deadline (
  id                       TEXT PRIMARY KEY,
  case_id                  TEXT NOT NULL REFERENCES "case"(id),
  rule_id                  TEXT NOT NULL,         -- 对应 deadline-rules DSL 的 deadlines[].id
  label                    TEXT NOT NULL,
  anchor_event             TEXT NOT NULL,         -- 触发事件
  anchor_date              TEXT,                  -- ★ 触发日（律师确认，权威输入）
  anchor_confirmed         INTEGER NOT NULL DEFAULT 0, -- requireLawyerConfirmAnchor 护栏
  -- 以下为"派生缓存"：源是 anchor_date + rule_set_version + holiday_table，可随时重算
  computed_due_date        TEXT,
  computed_with_ruleset    TEXT,                  -- 用了哪个规则集版本（合规可追溯）
  computed_with_holidays   TEXT,                  -- 用了哪张节假日表
  rule_outdated            INTEGER NOT NULL DEFAULT 0, -- 离线旧规则版本告警标记
  status                   TEXT NOT NULL DEFAULT 'pending', -- pending/done/dismissed
  reminders                TEXT NOT NULL DEFAULT '[]', -- JSON：分级提醒配置
  attributes               TEXT NOT NULL DEFAULT '{}',
  _rev                     INTEGER NOT NULL DEFAULT 1,
  _updated_at              TEXT NOT NULL,
  _deleted                 INTEGER NOT NULL DEFAULT 0,
  _dirty                   INTEGER NOT NULL DEFAULT 1
);

-- 存证（电子证据固定→存证→可采 闭环） --------------------------------------
CREATE TABLE attestation (
  id                  TEXT PRIMARY KEY,
  evidence_id         TEXT NOT NULL REFERENCES evidence(id),
  platform            TEXT,                       -- 至信链/蚂蚁链/易保全/公证云
  content_hash        TEXT NOT NULL,              -- 上链的内容哈希
  chain_tx_id         TEXT,                       -- 上链交易号/存证编号
  timestamp_token     TEXT,                       -- 可信时间戳
  cert_ref            TEXT,                       -- 存证证书本地引用
  status              TEXT NOT NULL DEFAULT 'pending', -- pending/anchored/verified/failed
  created_at          TEXT NOT NULL,
  attributes          TEXT NOT NULL DEFAULT '{}',
  _rev                INTEGER NOT NULL DEFAULT 1,
  _updated_at         TEXT NOT NULL,
  _deleted            INTEGER NOT NULL DEFAULT 0,
  _dirty              INTEGER NOT NULL DEFAULT 1
);

-- =============================================================================
-- 基础设施表（不进对象同步通道）
-- =============================================================================

-- 监管链：append-only 链式日志（chain-of-custody / EDRM）。★ 禁 UPDATE/DELETE（触发器兜底）
CREATE TABLE custody_event (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  entity_type   TEXT NOT NULL,                    -- evidence/deadline/case/attestation/config
  entity_id     TEXT NOT NULL,
  event_type    TEXT NOT NULL,                    -- captured/hashed/ocr_done/deadline_computed/anchor_confirmed/attested/exported...
  actor         TEXT NOT NULL,                    -- 律师/系统/agent
  ts            TEXT NOT NULL,
  prev_hash     TEXT,                             -- 前一条事件哈希，构成链
  payload_hash  TEXT NOT NULL,                    -- 本条内容哈希（不落原文/PII）
  signature     TEXT                              -- 可选签名
);
CREATE TRIGGER custody_event_no_update BEFORE UPDATE ON custody_event
  BEGIN SELECT RAISE(ABORT, 'custody_event is append-only'); END;
CREATE TRIGGER custody_event_no_delete BEFORE DELETE ON custody_event
  BEGIN SELECT RAISE(ABORT, 'custody_event is append-only'); END;

-- 离线写入事务队列（Linear 式 outbox）
CREATE TABLE __transactions (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  op            TEXT NOT NULL,                    -- create/update/delete/archive
  entity_type   TEXT NOT NULL,
  entity_id     TEXT NOT NULL,
  payload       TEXT NOT NULL,                    -- JSON（仅脱敏后可同步字段；fields_exclude 在序列化层强制）
  created_at    TEXT NOT NULL,
  last_sync_id  INTEGER,
  status        TEXT NOT NULL DEFAULT 'pending'   -- pending/acked/rejected
);

-- server-config 本地加密缓存 + bootstrap（断网用最后已知有效版本）
CREATE TABLE config_cache (
  kind           TEXT NOT NULL,                   -- deadline-rules/holidays/claim-elements/template/catalog/sync-shape/ui-layout/flag
  config_id      TEXT NOT NULL,
  version        TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  effective_from TEXT,
  signature      TEXT NOT NULL,                   -- Ed25519，应用前必须验签
  payload        TEXT NOT NULL,                   -- JSON
  fetched_at     TEXT NOT NULL,
  valid          INTEGER NOT NULL DEFAULT 1,      -- schema 不兼容则置 0；当前生效=最近 fetched_at 的 valid=1 行
  -- PK 含 version：同一 (kind,config_id) 多版本共存，便于回退/灰度；不兼容新版以 valid=0 落库而不覆盖上一有效版本
  PRIMARY KEY (kind, config_id, version)
);

-- =============================================================================
-- 索引（含 json_extract 表达式索引，近似 Postgres GIN，承接 attributes 查询）
-- =============================================================================
CREATE INDEX idx_evidence_case      ON evidence(case_id);
CREATE INDEX idx_fact_case_date     ON fact(case_id, fact_date);
CREATE INDEX idx_deadline_case_due  ON deadline(case_id, computed_due_date, status);
CREATE INDEX idx_attestation_evid   ON attestation(evidence_id);
CREATE INDEX idx_case_jurisdiction  ON "case"(json_extract(attributes, '$.jurisdiction_code'));
-- “今日分诊”热路径：未完成且有到期日的期限
CREATE INDEX idx_deadline_open      ON deadline(computed_due_date) WHERE status = 'pending' AND _deleted = 0;

-- =============================================================================
-- 扩展点（v5 D4：为推迟的高价值案由预留，现在不建表，盘清边界避免日后被迫破坏冻结）
--   · 刑事：羁押/侦查/审查起诉期限是独立"期限对象"族 → 未来新增 criminal_term 表，
--           通过 deadline.attributes.term_kind 先兜底，超出兜底能力时才迁移。
--   · 婚姻家事：财产分割明细/子女抚养是独立一等对象 → 未来新增 asset_item 表。
--   约定：新增此类表属一次性结构迁移，须 bump user_version 并发桌面版（联测期不做）。
-- =============================================================================
