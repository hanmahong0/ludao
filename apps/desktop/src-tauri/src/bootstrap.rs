//! 首启把内置默认 server-config 走正规 verify→cache 管线落库（离线即用、断网回退）。
//! DEV：用 DEV 信任根自签（vault::config::dev_seal_envelope）后正规安装（仍经验签）。
//! 生产（PINNED 公钥非空）：dev_seal 返回 None，须改为加载发布方**预签名**的 .envelope.json。

use rusqlite::Connection;

// 内置默认配置文本（编译期内联，单一事实源仍是 skeleton/config）。
const DEADLINE_RULES_JSON: &str =
    include_str!("../../../../skeleton/config/deadline-rules/cn-civil-first-instance.json");
const HOLIDAYS_JSON: &str =
    include_str!("../../../../skeleton/config/holidays/cn-statutory-2026.json");
const PRIVATE_LENDING_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/private-lending.json");
const SALES_CONTRACT_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/sales-contract.json");
const TRAFFIC_ACCIDENT_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/traffic-accident.json");
const LEASE_JSON: &str = include_str!("../../../../skeleton/config/claim-elements/lease.json");
const PROPERTY_MANAGEMENT_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/property-management.json");
const LABOR_DISPUTE_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/labor-dispute.json");
const CONSTRUCTION_CONTRACT_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/construction-contract.json");
const FINANCIAL_LOAN_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/financial-loan.json");
const CREDIT_CARD_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/credit-card.json");
const LABOR_SERVICE_INJURY_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/labor-service-injury.json");
const HOUSE_SALE_JSON: &str =
    include_str!("../../../../skeleton/config/claim-elements/house-sale.json");
const CIVIL_COMPLAINT_TEMPLATE_JSON: &str =
    include_str!("../../../../skeleton/config/templates/civil-complaint.json");
const CIVIL_POA_TEMPLATE_JSON: &str =
    include_str!("../../../../skeleton/config/templates/civil-power-of-attorney.json");

struct Builtin {
    kind: &'static str,
    config_id: &'static str,
    version: &'static str,
    schema_version: i64,
    effective_from: Option<&'static str>,
    payload: &'static str,
}

// meta 须与各 payload 内联字段一致（install 时 check_payload_meta_consistency 校验）。
const BUILTINS: &[Builtin] = &[
    Builtin {
        kind: "deadline-rules",
        config_id: "cn-civil-first-instance",
        version: "2026.06.1",
        schema_version: 3,
        effective_from: Some("2026-01-01"),
        payload: DEADLINE_RULES_JSON,
    },
    Builtin {
        kind: "holidays",
        config_id: "cn-statutory-2026",
        version: "2026.1",
        schema_version: 1,
        effective_from: Some("2026-01-01"),
        payload: HOLIDAYS_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "private_lending",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: PRIVATE_LENDING_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "sales_contract",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: SALES_CONTRACT_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "traffic_accident",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: TRAFFIC_ACCIDENT_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "lease",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: LEASE_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "property_management",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: PROPERTY_MANAGEMENT_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "labor_dispute",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: LABOR_DISPUTE_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "construction_contract",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: CONSTRUCTION_CONTRACT_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "financial_loan",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: FINANCIAL_LOAN_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "credit_card",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: CREDIT_CARD_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "labor_service_injury",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: LABOR_SERVICE_INJURY_JSON,
    },
    Builtin {
        kind: "claim-elements",
        config_id: "house_sale",
        version: "2026.06.1",
        schema_version: 2,
        effective_from: Some("2026-06-01"),
        payload: HOUSE_SALE_JSON,
    },
    Builtin {
        kind: "template",
        config_id: "civil_complaint",
        version: "2026.06.1",
        schema_version: 1,
        effective_from: Some("2026-06-01"),
        payload: CIVIL_COMPLAINT_TEMPLATE_JSON,
    },
    Builtin {
        kind: "template",
        config_id: "civil_power_of_attorney",
        version: "2026.06.1",
        schema_version: 1,
        effective_from: Some("2026-06-01"),
        payload: CIVIL_POA_TEMPLATE_JSON,
    },
];

/// 幂等：每个内置配置若缓存中尚无有效版本则装入。返回新装入数量。
/// `now`: RFC3339（调用方注入，便于测试确定性）。
pub fn install_builtin_configs(conn: &Connection, now: &str) -> Result<usize, String> {
    let mut installed = 0usize;
    for b in BUILTINS {
        if vault::config::active_config(conn, b.kind, b.config_id)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue; // 已有有效版本（含日后 server-config 热更的更高版本）→ 不覆盖
        }
        let envelope = vault::config::dev_seal_envelope(
            b.kind,
            b.config_id,
            b.version,
            b.schema_version,
            b.effective_from,
            b.payload,
        )
        .ok_or_else(|| {
            "dev_seal_envelope 不可用（生产构建须改用发布方预签名 .envelope.json）".to_string()
        })?;
        match vault::config::install_config(conn, &envelope, now).map_err(|e| e.to_string())? {
            vault::config::InstallOutcome::Applied { .. } => installed += 1,
            other => {
                // 内置配置应永远 Applied；否则是 supported_schema_version 落后于内置配置的 bug。
                return Err(format!("内置配置未生效（schema 版本不匹配？）：{other:?}"));
            }
        }
    }
    Ok(installed)
}
