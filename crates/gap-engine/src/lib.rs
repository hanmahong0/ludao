//! 证据链缺口诊断引擎 · A 段本地实现 (rust-core / standalone)
//!
//! 对应 `gap-diagnosis-spec.md` 的 A 段：**确定的形式要件门槛**，走本地 CEL
//! 规则（mutation-free / 可终止 / 可审计 / 离线可用 / 绝不交 LLM）。引擎只解释
//! `claim-elements` 配置，不内嵌任何具体案由的判定逻辑。
//!
//! 红线（spec §0/§5）：
//!   1. 纯本地求值——不调用任何 LLM/云端；CEL 仅注入白名单宿主函数与脱敏上下文。
//!   2. CEL 求值失败（未定义符号/类型错/配置缺位）→ **不产出该 gap**，标 DEGRADED，
//!      绝不当 false 静默跳过（避免漏报缺口）。
//!   3. 引擎不读 file_ref / 原文 / OCR——只取已编目证据的脱敏字段。
//!
//! 依赖：`serde`/`serde_json`（DSL 反序列化 + 脱敏 claim.* 上下文）、
//!       `cel-interpreter`（gapRules[].when 的 CEL `when` 求值）。

use cel_interpreter::{Context, Program, Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// 1. claim-elements DSL（serde 反序列化，对齐 private-lending.json 形状）
// ─────────────────────────────────────────────────────────────────────────────

/// 某 `caseType` 的整份要件 DSL（`config_cache(kind='claim-elements')`）。
#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    #[serde(rename = "caseType")]
    pub case_type: String,
    #[serde(rename = "schemaVersion", default)]
    pub schema_version: u32,
    #[serde(default)]
    pub version: String,
    pub elements: Vec<Element>,
    #[serde(rename = "gapRules", default)]
    pub gap_rules: Vec<GapRule>,
}

/// satisfyRule 判定模式（spec §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SatisfyRule {
    /// k >= 1
    Any,
    /// 覆盖 requiredEvidence 全部必备类目（按类目而非条数）
    All,
    /// k >= satisfyN
    N,
}

/// 一个要件（`elements[]`）。
#[derive(Debug, Clone, Deserialize)]
pub struct Element {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "requiredEvidence", default)]
    pub required_evidence: Vec<String>,
    #[serde(rename = "satisfyRule")]
    pub satisfy_rule: SatisfyRule,
    /// satisfyRule == "n" 时的阈值；缺省视为 1（spec §3.3）。
    #[serde(rename = "satisfyN", default)]
    pub satisfy_n: Option<u32>,
    #[serde(default)]
    pub severity: String,
    /// 非空且 CEL 求 false → 该要件 applicable=false，跳过（spec §3.1）。
    #[serde(rename = "appliesWhen", default)]
    pub applies_when: Option<String>,
    #[serde(rename = "gapPrompt", default)]
    pub gap_prompt: String,
    #[serde(default)]
    pub suggest: String,
}

/// 一条缺口规则（`gapRules[]`）——显式声明的危险差集（spec §4）。
#[derive(Debug, Clone, Deserialize)]
pub struct GapRule {
    pub id: String,
    pub severity: String,
    /// CEL 布尔表达式，注入 has/evidenceForm/evidenceHas + claim.* 等。
    pub when: String,
    pub explain: String,
    #[serde(rename = "elementRef")]
    pub element_ref: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. 输入：已编目证据 + 脱敏上下文
// ─────────────────────────────────────────────────────────────────────────────

/// 一条已编目证据（`evidence` 表节选，仅脱敏字段；绝不含 file_ref/原文）。
#[derive(Debug, Clone)]
pub struct Evidence {
    pub id: String,
    /// 归一形式值（book/electronic/cash/bank_transfer/witness/...）。
    pub form: String,
    /// 关联到的 element.id 集合（`linked_element_ids`）。
    pub linked_element_ids: Vec<String>,
    /// 名称/类目（供 evidenceHas 与 satisfyRule="all" 类目覆盖判定）。
    pub names: Vec<String>,
    /// 三性是否被致命否定（true=该条不计入满足，如 authenticity 被否定）。
    pub fatally_rejected: bool,
}

impl Evidence {
    /// 便捷构造（测试/调用方用）。
    pub fn new(id: impl Into<String>, form: impl Into<String>, linked: &[&str]) -> Self {
        Evidence {
            id: id.into(),
            form: form.into(),
            linked_element_ids: linked.iter().map(|s| s.to_string()).collect(),
            names: Vec::new(),
            fatally_rejected: false,
        }
    }
    pub fn with_names(mut self, names: &[&str]) -> Self {
        self.names = names.iter().map(|s| s.to_string()).collect();
        self
    }
    pub fn rejected(mut self) -> Self {
        self.fatally_rejected = true;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. 输出
// ─────────────────────────────────────────────────────────────────────────────

/// 单个要件满足判定结果（elementStatus[]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementStatus {
    pub element_ref: String,
    pub applicable: bool,
    pub satisfied: bool,
    /// evidenceForm 归一值；无满足证据为 "none"。
    pub form: String,
}

/// 一条缺口项（gaps[]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapFinding {
    pub element_ref: String,
    pub severity: String,
    pub explain: String,
    pub suggest: String,
    /// 触发它的 gapRule id（可审计）。
    pub fired_by: String,
}

/// 诊断报告。
#[derive(Debug, Clone)]
pub struct DiagnosisReport {
    pub case_type: String,
    pub used_profile_version: String,
    pub used_schema_version: u32,
    pub element_status: Vec<ElementStatus>,
    pub gaps: Vec<GapFinding>,
    /// 标 DEGRADED 时的告警摘要（CEL 求值失败的规则）。
    pub degraded: Vec<String>,
    pub trace: Vec<String>,
}

/// 诊断错误 = 拒诊信号（spec §5）。任一变体均不产出"已满足/无缺口"结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnoseError {
    /// CEL 表达式编译失败（语法错），整体拒诊。
    CelCompile { rule_id: String, reason: String },
}

impl std::fmt::Display for DiagnoseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnoseError::CelCompile { rule_id, reason } => {
                write!(f, "CelCompile(rule={rule_id}, reason={reason})")
            }
        }
    }
}
impl std::error::Error for DiagnoseError {}

// ─────────────────────────────────────────────────────────────────────────────
// 4. 要件满足判定（spec §3，A 段第一步）
// ─────────────────────────────────────────────────────────────────────────────

/// 严重度排序键：critical > warning > info > 其它（spec §4 末）。
fn severity_rank(s: &str) -> u8 {
    match s {
        "critical" => 0,
        "warning" => 1,
        "info" => 2,
        _ => 3,
    }
}

/// element 满足判定的内部快照（供 has/evidenceForm 闭包读取，求值无顺序依赖）。
struct ElementSnapshot {
    applicable: bool,
    satisfied: bool,
    form: String,
}

/// 先算全部 elementStatus（含 has/evidenceForm 缓存），再求 gapRules——
/// 保证 gapRules[].when 调用宿主函数时读到已收敛的快照（spec §3 末）。
fn compute_element_status(
    profile: &Profile,
    evidences: &[Evidence],
    claim: &serde_json::Value,
) -> Result<HashMap<String, ElementSnapshot>, DiagnoseError> {
    let mut snap: HashMap<String, ElementSnapshot> = HashMap::new();

    for e in &profile.elements {
        // 1) appliesWhen 门：非空且求 false → applicable=false，跳过。
        let applicable = match &e.applies_when {
            None => true,
            Some(expr) => {
                // appliesWhen 仅读 claim.* 等脱敏上下文，不需 has/evidenceForm。
                let mut ctx = Context::default();
                inject_readonly_ctx(&mut ctx, claim);
                eval_bool_strict(expr, &ctx).map_err(|reason| DiagnoseError::CelCompile {
                    rule_id: format!("appliesWhen:{}", e.id),
                    reason,
                })?
            }
        };

        if !applicable {
            // 不适用的要件：has() 视为已满足（spec §3.4），form=none。
            snap.insert(
                e.id.clone(),
                ElementSnapshot {
                    applicable: false,
                    satisfied: false,
                    form: "none".to_string(),
                },
            );
            continue;
        }

        // 2) 收集"计入满足"的证据：linked 含 e.id 且未被致命否定。
        let counting: Vec<&Evidence> = evidences
            .iter()
            .filter(|ev| !ev.fatally_rejected && ev.linked_element_ids.iter().any(|x| x == &e.id))
            .collect();
        let k = counting.len() as u32;

        // 3) 按 satisfyRule 判 satisfied。
        let satisfied = match e.satisfy_rule {
            SatisfyRule::Any => k >= 1,
            SatisfyRule::N => k >= e.satisfy_n.unwrap_or(1),
            SatisfyRule::All => {
                // 按类目覆盖：requiredEvidence 每个必备类目都需被某条证据的 names 命中。
                !e.required_evidence.is_empty()
                    && e.required_evidence
                        .iter()
                        .all(|req| counting.iter().any(|ev| ev.names.iter().any(|n| n == req)))
            }
        };

        // evidenceForm：满足时取主导项（首条计入证据的 form），否则 "none"。
        let form = if satisfied {
            counting
                .first()
                .map(|ev| ev.form.clone())
                .unwrap_or_else(|| "none".to_string())
        } else {
            "none".to_string()
        };

        snap.insert(
            e.id.clone(),
            ElementSnapshot {
                applicable: true,
                satisfied,
                form,
            },
        );
    }

    Ok(snap)
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. CEL 宿主函数注入与求值（spec §2）
// ─────────────────────────────────────────────────────────────────────────────

/// 把脱敏只读上下文注入 CEL Context：`claim` 作为整棵 serde_json Map 注入，
/// 表达式以 `claim.amount` / `claim.includes_interest` 访问。
fn inject_readonly_ctx(ctx: &mut Context, claim: &serde_json::Value) {
    // TryIntoValue 对任意 serde::Serialize 生效；serde_json::Value 直接落为 CEL Map。
    let _ = ctx.add_variable("claim", claim.clone());
}

/// 注入 has / evidenceForm / evidenceHas 三个白名单宿主函数。
/// 闭包捕获已收敛快照的 Arc（Send + Sync + 'static），mutation-free。
fn inject_host_functions(
    ctx: &mut Context,
    has_map: Arc<HashMap<String, bool>>,
    form_map: Arc<HashMap<String, String>>,
    names: Arc<Vec<String>>,
) {
    // has(elementId) -> bool：该要件是否已被满足（不适用要件视为已满足）。
    // 注意：CEL 的 `has` 是保留的字段存在宏（has(a.b)），无法接受字符串实参。
    // 故 DSL 仍按规范写 `has('id')`，求值前由 rewrite_has_calls 改写为内部名 `__has`。
    let h = has_map.clone();
    ctx.add_function(HAS_FN, move |id: Arc<String>| -> bool {
        *h.get(id.as_str()).unwrap_or(&false)
    });

    // evidenceForm(elementId) -> string：满足该要件的证据归一形式，无则 'none'。
    let f = form_map.clone();
    ctx.add_function("evidenceForm", move |id: Arc<String>| -> String {
        f.get(id.as_str())
            .cloned()
            .unwrap_or_else(|| "none".to_string())
    });

    // evidenceHas(name) -> bool：本案是否存在名称/类目为 name 的已编目证据。
    let n = names.clone();
    ctx.add_function("evidenceHas", move |name: Arc<String>| -> bool {
        n.iter().any(|x| x == name.as_str())
    });
}

/// 内部 has 函数名（规避 CEL 保留宏 `has`）。DSL 仍写 `has('id')`。
const HAS_FN: &str = "__has";

/// 把 DSL 里作为函数调用的 `has(` 改写为内部名 `__has(`，仅当 `has` 是独立标识符
/// （前后非标识符字符）时替换——绝不误伤 `evidenceHas(`。
fn rewrite_has_calls(expr: &str) -> String {
    let bytes = expr.as_bytes();
    let is_ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out: Vec<u8> = Vec::with_capacity(expr.len() + 8);
    let mut i = 0usize;
    while i < bytes.len() {
        // 匹配独立标识符 "has" 紧跟 "("（前一字节非标识符字符）。
        if bytes[i] == b'h'
            && bytes.get(i + 1) == Some(&b'a')
            && bytes.get(i + 2) == Some(&b's')
            && bytes.get(i + 3) == Some(&b'(')
            && (i == 0 || !is_ident(bytes[i - 1]))
        {
            out.extend_from_slice(HAS_FN.as_bytes());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    // 输入是合法 UTF-8，HAS_FN 是 ASCII，且仅在 ASCII 边界替换 → 结果仍是合法 UTF-8。
    String::from_utf8(out).expect("rewrite preserves UTF-8")
}

/// 严格布尔求值：编译错/求值错/非布尔结果一律视为失败（返回 Err(reason)）——
/// 绝不当 false 静默跳过（spec §2 类型纪律 / §4.2）。
fn eval_bool_strict(expr: &str, ctx: &Context) -> Result<bool, String> {
    let rewritten = rewrite_has_calls(expr);
    let program = Program::compile(&rewritten).map_err(|e| format!("compile: {e}"))?;
    match program.execute(ctx) {
        Ok(Value::Bool(b)) => Ok(b),
        Ok(other) => Err(format!("non-bool result: {other:?}")),
        Err(e) => Err(format!("exec: {e}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. 主入口：diagnose（spec §3 → §4）
// ─────────────────────────────────────────────────────────────────────────────

/// 缺口诊断主入口。先算 elementStatus，再求 gapRules 差集。
///
/// - appliesWhen 编译失败 → 整体拒诊（DiagnoseError），绝不猜。
/// - gapRule.when 求值失败 → 不产出该 gap，记入 `degraded`（DEGRADED），其余继续。
pub fn diagnose(
    profile: &Profile,
    evidences: &[Evidence],
    claim: &serde_json::Value,
) -> Result<DiagnosisReport, DiagnoseError> {
    let mut trace: Vec<String> = Vec::new();
    trace.push(format!(
        "profile={}@{} schema={}",
        profile.case_type, profile.version, profile.schema_version
    ));

    // ── 第一步：要件满足判定（spec §3）──
    let snap = compute_element_status(profile, evidences, claim)?;

    // 收敛快照 → 闭包可读的 has/evidenceForm 表（has(): 不适用视为满足）。
    let mut has_map: HashMap<String, bool> = HashMap::new();
    let mut form_map: HashMap<String, String> = HashMap::new();
    let mut element_status: Vec<ElementStatus> = Vec::new();
    let mut satisfied_count = 0usize;

    for e in &profile.elements {
        let s = snap.get(&e.id).expect("snapshot present for every element");
        // has(e.id) := applicable ? satisfied : true（spec §3.4）。
        let has = if s.applicable { s.satisfied } else { true };
        has_map.insert(e.id.clone(), has);
        form_map.insert(e.id.clone(), s.form.clone());
        if s.applicable && s.satisfied {
            satisfied_count += 1;
        }
        element_status.push(ElementStatus {
            element_ref: e.id.clone(),
            applicable: s.applicable,
            satisfied: s.satisfied,
            form: s.form.clone(),
        });
    }

    // evidenceHas 的名称全集（去否定证据外仍可被 catalog 命中，简单起见取全部名称）。
    let all_names: Vec<String> = evidences
        .iter()
        .flat_map(|ev| ev.names.iter().cloned())
        .collect();

    let has_arc = Arc::new(has_map);
    let form_arc = Arc::new(form_map);
    let names_arc = Arc::new(all_names);

    trace.push(format!(
        "elements={} satisfied={}",
        profile.elements.len(),
        satisfied_count
    ));

    // 要件名 → suggest 映射（gap.suggest 取被指要件的 suggest，spec §4.3）。
    let suggest_of: HashMap<&str, &str> = profile
        .elements
        .iter()
        .map(|e| (e.id.as_str(), e.suggest.as_str()))
        .collect();

    // ── 第二步：gapRules 求值（spec §4）──
    let mut gaps: Vec<GapFinding> = Vec::new();
    let mut degraded: Vec<String> = Vec::new();
    let mut fired = 0usize;

    for g in &profile.gap_rules {
        // 每条规则用新 Context，注入相同收敛快照（无求值顺序依赖）。
        let mut ctx = Context::default();
        inject_readonly_ctx(&mut ctx, claim);
        inject_host_functions(
            &mut ctx,
            has_arc.clone(),
            form_arc.clone(),
            names_arc.clone(),
        );

        match eval_bool_strict(&g.when, &ctx) {
            Ok(true) => {
                fired += 1;
                gaps.push(GapFinding {
                    element_ref: g.element_ref.clone(),
                    severity: g.severity.clone(),
                    explain: g.explain.clone(),
                    suggest: suggest_of
                        .get(g.element_ref.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                    fired_by: g.id.clone(),
                });
            }
            Ok(false) => {}
            Err(reason) => {
                // CEL 求值失败 → 不产出该 gap，标 DEGRADED（绝不当 false 漏报）。
                degraded.push(format!("{}: {}", g.id, reason));
            }
        }
    }

    trace.push(format!(
        "gapRules={} fired={} degraded={}",
        profile.gap_rules.len(),
        fired,
        degraded.len()
    ));

    // 按 severity 降序稳定排序（critical > warning > info）。
    gaps.sort_by_key(|g| severity_rank(&g.severity));

    Ok(DiagnosisReport {
        case_type: profile.case_type.clone(),
        used_profile_version: profile.version.clone(),
        used_schema_version: profile.schema_version,
        element_status,
        gaps,
        degraded,
        trace,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. 测试：private-lending 端到端（对照 spec §6）
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    /// 加载 private-lending.json profile（只读参考配置）。
    fn private_lending_profile() -> Profile {
        let raw = include_str!("../../../skeleton/config/claim-elements/private-lending.json");
        serde_json::from_str(raw).expect("private-lending.json must deserialize")
    }

    fn claim_ctx() -> serde_json::Value {
        serde_json::json!({
            "amount": 80000,
            "includes_interest": true,
            "case_stage": "first_instance"
        })
    }

    /// spec §6 缺口用例：缺『款项实际交付』+ 缺『利息约定』。
    /// 证据：ev-1 借条(book→loan_relationship)、ev-2 微信(electronic→loan_relationship,repayment_due)。
    #[test]
    fn private_lending_missing_delivery_fires_critical() {
        let profile = private_lending_profile();
        let evidences = vec![
            Evidence::new("ev-1", "book", &["loan_relationship"]).with_names(&["借条/借款合同"]),
            Evidence::new(
                "ev-2",
                "electronic",
                &["loan_relationship", "repayment_due"],
            )
            .with_names(&["微信/短信/通话中的借款确认"]),
        ];

        let report = diagnose(&profile, &evidences, &claim_ctx()).expect("must diagnose");

        // 无求值失败。
        assert!(
            report.degraded.is_empty(),
            "no DEGRADED rules: {:?}",
            report.degraded
        );

        // elementStatus 对照 spec §6 表。
        let status = |id: &str| {
            report
                .element_status
                .iter()
                .find(|s| s.element_ref == id)
                .unwrap_or_else(|| panic!("status for {id}"))
        };
        assert!(
            status("loan_relationship").satisfied,
            "loan_relationship satisfied"
        );
        assert_eq!(status("loan_relationship").form, "book");
        assert!(
            !status("actual_delivery").satisfied,
            "actual_delivery NOT satisfied"
        );
        assert_eq!(status("actual_delivery").form, "none");
        assert!(
            !status("interest_agreement").satisfied,
            "interest_agreement NOT satisfied"
        );
        assert!(
            status("interest_agreement").applicable,
            "interest applicable (includes_interest)"
        );
        assert!(status("repayment_due").satisfied, "repayment_due satisfied");

        // 必报 critical 的"缺款项实际交付凭证"缺口。
        let delivery = report
            .gaps
            .iter()
            .find(|g| g.element_ref == "actual_delivery")
            .expect("MUST fire actual_delivery gap");
        assert_eq!(delivery.severity, "critical", "delivery gap is critical");
        assert_eq!(delivery.fired_by, "gap_missing_delivery");
        assert!(
            delivery.explain.contains("款项实际交付"),
            "explain mentions 款项实际交付"
        );
        assert!(!delivery.suggest.is_empty(), "delivery gap carries suggest");

        // 利息缺口（info）也应触发（claim.includes_interest && !has(interest_agreement)）。
        let interest = report
            .gaps
            .iter()
            .find(|g| g.element_ref == "interest_agreement")
            .expect("MUST fire interest_agreement gap");
        assert_eq!(interest.severity, "info");
        assert_eq!(interest.fired_by, "gap_interest_no_agreement");

        // gap_cash_large_amount 不触发（无交付证据，form='none' != 'cash'）。
        assert!(
            !report
                .gaps
                .iter()
                .any(|g| g.fired_by == "gap_cash_large_amount"),
            "cash rule must NOT fire"
        );

        // 排序：critical 在 info 之前。
        assert_eq!(report.gaps.first().unwrap().severity, "critical");

        // 共触发 2 条。
        assert_eq!(report.gaps.len(), 2, "exactly 2 gaps");
    }

    /// 证据齐全用例：补齐交付凭证 + 利息约定 → 不报对应缺口。
    #[test]
    fn private_lending_complete_no_gaps() {
        let profile = private_lending_profile();
        let evidences = vec![
            Evidence::new("ev-1", "book", &["loan_relationship"]),
            Evidence::new("ev-2", "bank_transfer", &["actual_delivery"]),
            Evidence::new("ev-3", "book", &["interest_agreement"]),
            Evidence::new("ev-4", "electronic", &["repayment_due"]),
        ];

        let report = diagnose(&profile, &evidences, &claim_ctx()).expect("must diagnose");

        assert!(report.degraded.is_empty(), "no DEGRADED");
        assert!(
            report
                .element_status
                .iter()
                .all(|s| !s.applicable || s.satisfied),
            "all applicable elements satisfied"
        );
        // 关键：交付凭证齐全 → 不报 gap_missing_delivery。
        assert!(
            !report
                .gaps
                .iter()
                .any(|g| g.fired_by == "gap_missing_delivery"),
            "delivery gap must NOT fire when evidence present"
        );
        // 利息约定齐全 → 不报利息缺口。
        assert!(
            !report
                .gaps
                .iter()
                .any(|g| g.fired_by == "gap_interest_no_agreement"),
            "interest gap must NOT fire when agreement present"
        );
        // bank_transfer 非 cash → cash 规则不触发。
        assert!(
            report.gaps.is_empty(),
            "no gaps at all when complete: {:?}",
            report.gaps
        );
    }

    /// gap_cash_large_amount（warning）：大额现金交付应触发阈值规则。
    #[test]
    fn cash_large_amount_fires_warning() {
        let profile = private_lending_profile();
        let evidences = vec![
            Evidence::new("ev-1", "book", &["loan_relationship"]),
            Evidence::new("ev-2", "cash", &["actual_delivery"]),
            Evidence::new("ev-3", "book", &["interest_agreement"]),
        ];
        let report = diagnose(&profile, &evidences, &claim_ctx()).expect("must diagnose");
        assert!(report.degraded.is_empty());
        let cash = report
            .gaps
            .iter()
            .find(|g| g.fired_by == "gap_cash_large_amount")
            .expect("cash rule fires (form=cash && amount>=50000)");
        assert_eq!(cash.severity, "warning");
        // 交付证据已在 → 不报 missing_delivery。
        assert!(
            !report
                .gaps
                .iter()
                .any(|g| g.fired_by == "gap_missing_delivery")
        );
    }

    /// appliesWhen 关门：不主张利息时 interest_agreement 不适用 → 不报利息缺口。
    #[test]
    fn no_interest_claim_skips_interest_element() {
        let profile = private_lending_profile();
        let evidences = vec![
            Evidence::new("ev-1", "book", &["loan_relationship"]),
            Evidence::new("ev-2", "bank_transfer", &["actual_delivery"]),
        ];
        let claim = serde_json::json!({ "amount": 80000, "includes_interest": false });
        let report = diagnose(&profile, &evidences, &claim).expect("must diagnose");

        let interest = report
            .element_status
            .iter()
            .find(|s| s.element_ref == "interest_agreement")
            .unwrap();
        assert!(
            !interest.applicable,
            "interest not applicable when no interest claim"
        );
        assert!(
            !report
                .gaps
                .iter()
                .any(|g| g.element_ref == "interest_agreement"),
            "interest gap must NOT fire when not applicable"
        );
    }

    /// 三性致命否定的证据不计入满足。
    #[test]
    fn fatally_rejected_evidence_not_counted() {
        let profile = private_lending_profile();
        let evidences = vec![
            Evidence::new("ev-1", "book", &["loan_relationship"]),
            // 交付证据被致命否定 → 不计入 → actual_delivery 仍缺。
            Evidence::new("ev-2", "bank_transfer", &["actual_delivery"]).rejected(),
        ];
        let claim = serde_json::json!({ "amount": 10000, "includes_interest": false });
        let report = diagnose(&profile, &evidences, &claim).expect("must diagnose");
        assert!(
            !report
                .element_status
                .iter()
                .find(|s| s.element_ref == "actual_delivery")
                .unwrap()
                .satisfied,
            "rejected delivery evidence does not satisfy"
        );
        assert!(
            report
                .gaps
                .iter()
                .any(|g| g.fired_by == "gap_missing_delivery")
        );
    }

    /// CEL 求值失败 → 该 gap 不产出，记 DEGRADED（绝不漏报为无缺口）。
    #[test]
    fn cel_eval_failure_marks_degraded_not_silent() {
        let mut profile = private_lending_profile();
        // 注入一条引用未定义符号的规则。
        profile.gap_rules.push(GapRule {
            id: "bad_rule".into(),
            severity: "warning".into(),
            when: "undefined_symbol.foo == true".into(),
            explain: "bad".into(),
            element_ref: "actual_delivery".into(),
        });
        let evidences = vec![Evidence::new("ev-1", "book", &["loan_relationship"])];
        let claim = serde_json::json!({ "amount": 1000, "includes_interest": false });
        let report = diagnose(&profile, &evidences, &claim).expect("must diagnose");
        assert!(
            report.degraded.iter().any(|d| d.starts_with("bad_rule")),
            "failing rule recorded in degraded: {:?}",
            report.degraded
        );
    }
}
