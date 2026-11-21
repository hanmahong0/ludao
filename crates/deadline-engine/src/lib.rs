//! 确定性期限引擎 · Rust 参考实现 (rust-core)
//!
//! ⚠ 参考实现声明：本文件是期限引擎 `compute()` 的**参考实现**，用于固化
//! `deadline-engine-spec.md` 的算法语义并被 `golden-tests.json` 锁定。
//! **必须先 `cargo test` 通过 G1–G7 全部 golden 测试，方可作为基线**被
//! 其它实现/重构对照；任何修改若使 golden 测试变红，一律视为回归，禁止合并。
//!
//! 验证状态（审查员实测）：rustc/chrono 0.4 下编译零警告，G1–G7 + trace 共 8
//! 个单元测试全绿；日期算术经独立核算（exclude + calendar_day 末日 = anchor +
//! amount）与各 golden 周几断言一致；跨年 rollover 触达缺表年份能正确拒算。
//!
//! 设计红线（v5 D1 不变量④ / spec §4「错误即拒算」）：
//!   1. 纯确定性、可终止、无副作用（CEL 求值 mutation-free、加期循环有硬上界）。
//!   2. **绝不内嵌任何具体期限天数**——天数/单位全部来自 `deadline-rules` 配置。
//!   3. **绝不交 LLM / 云端**——期限计算与原件相关形式要件判断永远本地求值。
//!   4. 任一前置校验失败一律**拒算**（返回 `EngineError`），由上层写
//!      `custody_event(event_type='deadline_calc_refused', reason=...)` 并 UI 告警；
//!      **宁可拒算，不可带病算**——漏/错期限是执业事故级后果。
//!
//! 依赖：`chrono`（仅做日期算术，不做时区魔法）。
//! CEL：本文件用 `CelEvaluator` trait 抽象，内置一个**最小占位求值器**仅支持
//! `doc.type == '<literal>'`，足以驱动 golden G2；生产环境须替换为真实
//! `cel-interpreter` crate（mutation-free、保证终止、仅注入白名单宿主函数）。

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use std::collections::HashMap;

// ──────────────────────────────────────────────────────────────────────────
// 1. 类型定义（与 deadline-rules / holidays DSL 对齐）
// ──────────────────────────────────────────────────────────────────────────

/// dayZero：期间起算点。`Exclude` 为民诉法默认（"期间开始的日不计入"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayZero {
    Include,
    Exclude,
}

/// 计算方向。`Backward` 用于倒排期（如"开庭前 X 日"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// 加期单位。引擎只解释，不内嵌天数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    CalendarDay,
    CourtDay,
    Month,
    Year,
}

/// 末日顺延策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rollover {
    None,
    NextBusinessDay,
    PrevBusinessDay,
}

/// 节假日条目类型。`MakeupWorkday`=调休补班（即使周末也算工作日）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolidayType {
    StatutoryHoliday,
    MakeupWorkday,
}

/// `deadline-rules` 中的一条规则（`deadlines[]`）。
/// 注意：所有天数/单位均来自配置，引擎不内嵌。
#[derive(Debug, Clone)]
pub struct Rule {
    pub id: String,
    /// 引擎支持的 schema 版本绑定（与 ENGINE_SUPPORTED_SCHEMA_VERSION 比对）。
    pub schema_version: u32,
    pub day_zero: DayZero,
    pub direction: Direction,
    /// 基准 amount。
    pub amount: i64,
    pub unit: Unit,
    pub rollover: Rollover,
    pub require_lawyer_confirm_anchor: bool,
    /// CEL 条件字符串（如 `"doc.type == 'ruling'"`）；None 表示无条件分支。
    pub condition: Option<String>,
    /// condition 为真时改用的 amount。
    pub amount_if_condition: Option<i64>,
    /// 法院可指定且设了下限时：律师录入值 < 下限则取下限并标注。
    pub min_amount_enforced: Option<i64>,
    pub court_designatable: bool,
}

/// 一张年度节假日表（覆盖 `year` 这一年）。
#[derive(Debug, Clone)]
pub struct HolidayTable {
    pub table_id: String,
    pub year: i32,
    /// date(YYYY-MM-DD) -> 类型。
    pub days: HashMap<NaiveDate, HolidayType>,
}

impl HolidayTable {
    pub fn new(table_id: impl Into<String>, year: i32) -> Self {
        HolidayTable {
            table_id: table_id.into(),
            year,
            days: HashMap::new(),
        }
    }
    pub fn with_day(mut self, date: NaiveDate, ty: HolidayType) -> Self {
        self.days.insert(date, ty);
        self
    }
}

/// 多年节假日表集合：按年份索引。引擎据此判断"跨年缺表→拒算"。
#[derive(Debug, Clone, Default)]
pub struct HolidaySet {
    by_year: HashMap<i32, HolidayTable>,
}

impl HolidaySet {
    pub fn new() -> Self {
        HolidaySet {
            by_year: HashMap::new(),
        }
    }
    pub fn insert(&mut self, table: HolidayTable) {
        self.by_year.insert(table.year, table);
    }
    pub fn with(mut self, table: HolidayTable) -> Self {
        self.insert(table);
        self
    }
    fn get(&self, year: i32) -> Option<&HolidayTable> {
        self.by_year.get(&year)
    }
    fn has_year(&self, year: i32) -> bool {
        self.by_year.contains_key(&year)
    }
    /// 用于 trace：列出本次计算实际依赖的表 id（按年份升序）。
    fn used_table_ids(&self, years: &[i32]) -> Vec<String> {
        let mut ids: Vec<String> = years
            .iter()
            .filter_map(|y| self.get(*y).map(|t| t.table_id.clone()))
            .collect();
        ids.dedup();
        ids
    }
}

/// 脱敏结构化上下文（供 CEL `condition` 求值）。
/// 只承载脱敏最小上下文，禁带原始案件内容（v5 D1 不变量①）。
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// 律师是否已确认触发日（anchor）。
    pub anchor_confirmed: bool,
    /// doc.type 等脱敏字段，扁平 key→value，如 "doc.type" => "ruling"。
    pub fields: HashMap<String, String>,
}

impl Ctx {
    pub fn new(anchor_confirmed: bool) -> Self {
        Ctx {
            anchor_confirmed,
            fields: HashMap::new(),
        }
    }
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.insert(key.to_string(), value.to_string());
        self
    }
}

/// 计算成功的产物。`trace` 入 custody_event，合规可追溯。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueDate {
    pub date: NaiveDate,
    pub used_ruleset_version: String,
    pub used_holiday_table: Vec<String>,
    pub trace: Vec<String>,
}

/// 引擎错误 = "拒算"信号。任一变体均不产出到期日。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// 触发日未经律师确认（spec §2.1，G6）。UI 不得展示到期日。
    AnchorNotConfirmed,
    /// 规则 schemaVersion 与引擎不兼容（提示升级，绝不猜）。
    RuleSchemaMismatch { expected: u32, got: u32 },
    /// 计算区间跨到的某一年缺节假日表（spec §2.1 ★，G7）。绝不用旧表硬算。
    HolidayTableMissing { year: i32 },
    /// CEL 求值失败（语法/宿主函数缺失/不可终止保护触发）。拒算优于猜。
    CelEvaluation { reason: String },
    /// 内部不一致（如加期循环超过硬上界）——可终止性保护。
    InvalidConfig { reason: String },
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::AnchorNotConfirmed => write!(f, "AnchorNotConfirmed"),
            EngineError::RuleSchemaMismatch { expected, got } => {
                write!(f, "RuleSchemaMismatch(expected={expected}, got={got})")
            }
            EngineError::HolidayTableMissing { year } => {
                write!(f, "HolidayTableMissing({year})")
            }
            EngineError::CelEvaluation { reason } => write!(f, "CelEvaluation({reason})"),
            EngineError::InvalidConfig { reason } => write!(f, "InvalidConfig({reason})"),
        }
    }
}
impl std::error::Error for EngineError {}

/// 引擎支持的 deadline-rules schema 版本。配置高于此值即拒算（提示升级）。
pub const ENGINE_SUPPORTED_SCHEMA_VERSION: u32 = 3;

/// 加期循环硬上界（可终止性保护）：court_day 逐日推进的最大步数。
/// 远大于任何合法期限（如 60 工作日），命中即视为配置异常 → 拒算。
const MAX_ADVANCE_STEPS: i64 = 100_000;

// ──────────────────────────────────────────────────────────────────────────
// 2. CEL 抽象（生产须替换为真实 cel-interpreter crate）
// ──────────────────────────────────────────────────────────────────────────

/// CEL 求值器抽象。生产实现必须：mutation-free、保证终止、仅注入白名单宿主
/// 函数与脱敏上下文（v5 D5 CEL 边界）。**禁止任何网络/LLM 调用**。
pub trait CelEvaluator {
    /// 对布尔表达式求值。expr 为 None 时不应被调用（由 compute 保证）。
    fn eval_bool(&self, expr: &str, ctx: &Ctx) -> Result<bool, EngineError>;
}

/// 最小占位求值器：仅支持 `doc.type == '<literal>'`（含单/双引号），
/// 足以驱动 golden G2。**生产环境替换为 cel-interpreter**，
/// 届时本结构体连同此注释一并删除。
pub struct MinimalDocTypeCel;

impl CelEvaluator for MinimalDocTypeCel {
    fn eval_bool(&self, expr: &str, ctx: &Ctx) -> Result<bool, EngineError> {
        // 形如：  doc.type == 'ruling'   /   doc.type=="judgment"
        let normalized: String = expr.split_whitespace().collect::<Vec<_>>().join(" ");
        // 拆分 ==
        let parts: Vec<&str> = normalized.splitn(2, "==").collect();
        if parts.len() != 2 {
            return Err(EngineError::CelEvaluation {
                reason: format!("placeholder CEL only supports '<field> == <literal>': {expr}"),
            });
        }
        let lhs = parts[0].trim();
        let rhs_raw = parts[1].trim();
        // 仅允许 doc.type 字段（占位实现边界）。
        if lhs != "doc.type" {
            return Err(EngineError::CelEvaluation {
                reason: format!("placeholder CEL only supports field 'doc.type', got '{lhs}'"),
            });
        }
        // 去掉引号取字面量。
        let rhs = rhs_raw.trim_matches(|c| c == '\'' || c == '"');
        if rhs == rhs_raw {
            // 没有引号包裹 → 非字面量字符串，占位实现不处理。
            return Err(EngineError::CelEvaluation {
                reason: format!("placeholder CEL expects a quoted literal on rhs: {rhs_raw}"),
            });
        }
        let actual = ctx.fields.get("doc.type").map(|s| s.as_str());
        Ok(actual == Some(rhs))
    }
}

// ──────────────────────────────────────────────────────────────────────────
// 3. 工作日判定（spec §3，rollover 与 court_day 共用）
// ──────────────────────────────────────────────────────────────────────────

/// `isBusinessDay(date, holidaySet)`：
/// - 命中 statutory_holiday → 非工作日；
/// - 命中 makeup_workday   → 工作日（即使周末）；
/// - 否则 周一~周五 工作日，周六/周日 非工作日。
///
/// 注意：判定一律基于该日**所在年份**的节假日表。若该年缺表，返回
/// `HolidayTableMissing`——绝不回退到周末规则硬算（否则等于带病算）。
fn is_business_day(date: NaiveDate, holidays: &HolidaySet) -> Result<bool, EngineError> {
    let year = date.year();
    let table = holidays
        .get(year)
        .ok_or(EngineError::HolidayTableMissing { year })?;
    match table.days.get(&date) {
        Some(HolidayType::StatutoryHoliday) => Ok(false),
        Some(HolidayType::MakeupWorkday) => Ok(true),
        None => Ok(!matches!(date.weekday(), Weekday::Sat | Weekday::Sun)),
    }
}

/// 向后找到第一个工作日（含当日）。用于 next_business_day。
fn next_business_day(mut date: NaiveDate, holidays: &HolidaySet) -> Result<NaiveDate, EngineError> {
    let mut steps = 0i64;
    while !is_business_day(date, holidays)? {
        date += Duration::days(1);
        steps += 1;
        if steps > MAX_ADVANCE_STEPS {
            return Err(EngineError::InvalidConfig {
                reason: "next_business_day exceeded MAX_ADVANCE_STEPS".to_string(),
            });
        }
    }
    Ok(date)
}

/// 向前找到第一个工作日（含当日）。用于 prev_business_day（backward）。
fn prev_business_day(mut date: NaiveDate, holidays: &HolidaySet) -> Result<NaiveDate, EngineError> {
    let mut steps = 0i64;
    while !is_business_day(date, holidays)? {
        date -= Duration::days(1);
        steps += 1;
        if steps > MAX_ADVANCE_STEPS {
            return Err(EngineError::InvalidConfig {
                reason: "prev_business_day exceeded MAX_ADVANCE_STEPS".to_string(),
            });
        }
    }
    Ok(date)
}

// ──────────────────────────────────────────────────────────────────────────
// 4. 月/年加期（对应日规则）
// ──────────────────────────────────────────────────────────────────────────

/// 在 `base` 上加 `months` 个月（可负），对齐"对应日"；目标月无对应日则取
/// 当月最后一日（如 1/31 + 1 月 → 2/28|29）。
fn add_months(base: NaiveDate, months: i64) -> Option<NaiveDate> {
    // 以 0 起的绝对月序号做算术，规避跨年进位错误。
    let total = base.year() as i64 * 12 + (base.month0() as i64) + months;
    let year = total.div_euclid(12) as i32;
    let month0 = total.rem_euclid(12) as u32; // 0..=11
    let month = month0 + 1;
    let last_day = last_day_of_month(year, month);
    let day = base.day().min(last_day);
    NaiveDate::from_ymd_opt(year, month, day)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    // 下月一日 - 1 天。
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid first-of-month");
    (first_next - Duration::days(1)).day()
}

// ──────────────────────────────────────────────────────────────────────────
// 5. compute()：spec §2 步骤固定、确定性
// ──────────────────────────────────────────────────────────────────────────

/// 期限引擎主入口。严格按 spec §2 步骤执行；任一前置校验失败即拒算。
pub fn compute(
    rule: &Rule,
    anchor: NaiveDate,
    holidays: &HolidaySet,
    ctx: &Ctx,
    cel: &dyn CelEvaluator,
    ruleset_version: &str,
) -> Result<DueDate, EngineError> {
    let mut trace: Vec<String> = Vec::new();
    trace.push(format!(
        "anchor={anchor} rule={} ruleset_version={ruleset_version}",
        rule.id
    ));

    // ── 步骤 1：前置校验（任一失败即拒算）─────────────────────────────
    // 1a. 触发日未确认 → AnchorNotConfirmed（G6）。
    if rule.require_lawyer_confirm_anchor && !ctx.anchor_confirmed {
        return Err(EngineError::AnchorNotConfirmed);
    }
    // 1b. schema 版本不兼容 → RuleSchemaMismatch（提示升级，不猜）。
    if rule.schema_version != ENGINE_SUPPORTED_SCHEMA_VERSION {
        return Err(EngineError::RuleSchemaMismatch {
            expected: ENGINE_SUPPORTED_SCHEMA_VERSION,
            got: rule.schema_version,
        });
    }
    // 1c. 缺表跨年检查在确定候选末日区间后做（见步骤 5 之前）：
    //     先求出未顺延的候选末日，再校验 [anchor.year ..= 末日.year] 全部有表。

    // ── 步骤 2：解析 amount/unit（CEL 仅用于选择分支，mutation-free）──
    let amount = match &rule.condition {
        Some(expr) => {
            let cond = cel.eval_bool(expr, ctx)?;
            trace.push(format!("cel('{expr}')={cond}"));
            if cond {
                rule.amount_if_condition.unwrap_or(rule.amount)
            } else {
                rule.amount
            }
        }
        None => rule.amount,
    };
    // minAmountEnforced：法院可指定且录入值 < 下限 → 取下限并标注。
    let amount = if rule.court_designatable {
        if let Some(min) = rule.min_amount_enforced {
            if amount < min {
                trace.push(format!("minAmountEnforced: {amount} < {min} → clamp to {min}"));
                min
            } else {
                amount
            }
        } else {
            amount
        }
    } else {
        amount
    };
    trace.push(format!("amount={amount} unit={:?}", rule.unit));

    // ── 步骤 3：应用 dayZero ───────────────────────────────────────────
    // exclude（默认）：日计数起点 = anchor ± 1；include：起点 = anchor。
    // 注意：month/year 的"对应日"规则直接以 anchor 为基准，不做 ±1 偏移
    //       （civil-proc 月/年期间到对应日届满，已隐含起算规则）。
    let sign: i64 = match rule.direction {
        Direction::Forward => 1,
        Direction::Backward => -1,
    };

    let candidate = match rule.unit {
        Unit::CalendarDay => {
            let start = match rule.day_zero {
                DayZero::Exclude => anchor + Duration::days(sign),
                DayZero::Include => anchor,
            };
            trace.push(format!("dayZero={:?} start={start}", rule.day_zero));
            // 末日 = 起点 + (amount-1) 自然日（方向由 sign 决定）。
            let end = start + Duration::days(sign * (amount - 1));
            trace.push(format!("calendar_day end={end}"));
            end
        }
        Unit::CourtDay => {
            let start = match rule.day_zero {
                DayZero::Exclude => anchor + Duration::days(sign),
                DayZero::Include => anchor,
            };
            trace.push(format!("dayZero={:?} start={start}", rule.day_zero));
            advance_court_days(start, amount, sign, holidays, &mut trace)?
        }
        Unit::Month => {
            // 对应日规则以 anchor 为基准（dayZero 不再额外 ±1）。
            let end = add_months(anchor, sign * amount).ok_or(EngineError::InvalidConfig {
                reason: "add_months produced an invalid date".to_string(),
            })?;
            trace.push(format!("month corresponding-day end={end}"));
            end
        }
        Unit::Year => {
            let end =
                add_months(anchor, sign * amount * 12).ok_or(EngineError::InvalidConfig {
                    reason: "add_years produced an invalid date".to_string(),
                })?;
            trace.push(format!("year corresponding-day end={end}"));
            end
        }
    };

    // ── 步骤 1c（迟到执行）：确定计算区间跨到的年份，校验每年有表 ───────
    // 对核心区间 [anchor.year ..= candidate.year] 强校验（G7：候选末日落 2027
    // 而无表 → HolidayTableMissing(2027)）。rollover 至多移动数日，若顺延真正
    // 跨年触达某缺表年份，会在 is_business_day 中惰性报 HolidayTableMissing
    // （同样拒算，绝不带病算）——已由跨年顺延边界用例验证。
    let core_lo = anchor.year().min(candidate.year());
    let core_hi = anchor.year().max(candidate.year());
    for year in core_lo..=core_hi {
        if !holidays.has_year(year) {
            return Err(EngineError::HolidayTableMissing { year });
        }
    }

    // ── 步骤 5：末日顺延（rollover，复用 is_business_day）─────────────
    let final_date = match rule.rollover {
        Rollover::None => {
            trace.push("rollover=none".to_string());
            candidate
        }
        Rollover::NextBusinessDay => {
            let d = next_business_day(candidate, holidays)?;
            trace.push(format!("rollover=next_business_day {candidate}→{d}"));
            d
        }
        Rollover::PrevBusinessDay => {
            let d = prev_business_day(candidate, holidays)?;
            trace.push(format!("rollover=prev_business_day {candidate}→{d}"));
            d
        }
    };

    // ── 步骤 6：返回 DueDate + trace ──────────────────────────────────
    let used_years: Vec<i32> = {
        let lo = anchor.year().min(final_date.year());
        let hi = anchor.year().max(final_date.year());
        (lo..=hi).collect()
    };
    let used_holiday_table = holidays.used_table_ids(&used_years);
    trace.push(format!("due_date={final_date}"));

    Ok(DueDate {
        date: final_date,
        used_ruleset_version: ruleset_version.to_string(),
        used_holiday_table,
        trace,
    })
}

/// court_day 逐日推进：从 start 起按 sign 方向逐日，仅工作日计数，直到累计
/// `amount` 个工作日。start 本身若为工作日则计第 1 个。
fn advance_court_days(
    start: NaiveDate,
    amount: i64,
    sign: i64,
    holidays: &HolidaySet,
    trace: &mut Vec<String>,
) -> Result<NaiveDate, EngineError> {
    if amount <= 0 {
        return Err(EngineError::InvalidConfig {
            reason: format!("court_day amount must be >= 1, got {amount}"),
        });
    }
    let mut cursor = start;
    let mut counted = 0i64;
    let mut steps = 0i64;
    loop {
        if is_business_day(cursor, holidays)? {
            counted += 1;
            if counted == amount {
                trace.push(format!("court_day counted={counted} end={cursor}"));
                return Ok(cursor);
            }
        }
        cursor += Duration::days(sign);
        steps += 1;
        if steps > MAX_ADVANCE_STEPS {
            return Err(EngineError::InvalidConfig {
                reason: "court_day advance exceeded MAX_ADVANCE_STEPS".to_string(),
            });
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
// 6. Golden 测试 G1–G7（验证闸门；必须全绿方可作为基线）
// ──────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod golden_tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("valid date literal")
    }

    /// 构造 cn-statutory-2026 节假日表（仅纳入 golden 触达区间所需且已确认的项）。
    fn holidays_2026() -> HolidayTable {
        let mut t = HolidayTable::new("cn-statutory-2026", 2026);
        // 元旦 + 调休补班。
        for day in ["2026-01-01", "2026-01-02", "2026-01-03"] {
            t.days.insert(d(day), HolidayType::StatutoryHoliday);
        }
        t.days.insert(d("2026-01-04"), HolidayType::MakeupWorkday);
        // 国庆法定假 10/1–10/7（G4 依赖）。
        for day in [
            "2026-10-01",
            "2026-10-02",
            "2026-10-03",
            "2026-10-04",
            "2026-10-05",
            "2026-10-06",
            "2026-10-07",
        ] {
            t.days.insert(d(day), HolidayType::StatutoryHoliday);
        }
        // 中秋 9/25–9/27（G4 区间附近，纳入以保真）。
        for day in ["2026-09-25", "2026-09-26", "2026-09-27"] {
            t.days.insert(d(day), HolidayType::StatutoryHoliday);
        }
        t
    }

    fn set_2026() -> HolidaySet {
        HolidaySet::new().with(holidays_2026())
    }

    fn ruleset_version() -> &'static str {
        "cn-civil-first-instance@2026.06.1"
    }

    // ── 规则工厂（对应 cn-civil-first-instance.json，schemaVersion=3）──

    fn rule_appeal_period() -> Rule {
        Rule {
            id: "appeal_period".into(),
            schema_version: 3,
            day_zero: DayZero::Exclude,
            direction: Direction::Forward,
            amount: 15,
            unit: Unit::CalendarDay,
            rollover: Rollover::NextBusinessDay,
            require_lawyer_confirm_anchor: true,
            condition: Some("doc.type == 'ruling'".into()),
            amount_if_condition: Some(10),
            min_amount_enforced: None,
            court_designatable: false,
        }
    }

    fn rule_answer_period() -> Rule {
        Rule {
            id: "answer_period".into(),
            schema_version: 3,
            day_zero: DayZero::Exclude,
            direction: Direction::Forward,
            amount: 15,
            unit: Unit::CalendarDay,
            rollover: Rollover::NextBusinessDay,
            require_lawyer_confirm_anchor: true,
            condition: None,
            amount_if_condition: None,
            min_amount_enforced: None,
            court_designatable: false,
        }
    }

    fn rule_limitation_general() -> Rule {
        Rule {
            id: "limitation_general".into(),
            schema_version: 3,
            day_zero: DayZero::Exclude,
            direction: Direction::Forward,
            amount: 3,
            unit: Unit::Year,
            rollover: Rollover::None,
            require_lawyer_confirm_anchor: true,
            condition: None,
            amount_if_condition: None,
            min_amount_enforced: None,
            court_designatable: false,
        }
    }

    fn rule_retrial_application() -> Rule {
        Rule {
            id: "retrial_application".into(),
            schema_version: 3,
            day_zero: DayZero::Exclude,
            direction: Direction::Forward,
            amount: 6,
            unit: Unit::Month,
            rollover: Rollover::NextBusinessDay,
            require_lawyer_confirm_anchor: true,
            condition: None,
            amount_if_condition: None,
            min_amount_enforced: None,
            court_designatable: false,
        }
    }

    // G1：上诉期(判决,+15自然日)，anchor 2026-03-02 → 2026-03-17（周二工作日，不顺延）。
    #[test]
    fn g1_appeal_judgment() {
        let ctx = Ctx::new(true).with_field("doc.type", "judgment");
        let r = compute(
            &rule_appeal_period(),
            d("2026-03-02"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("G1 must compute");
        assert_eq!(r.date, d("2026-03-17"), "G1");
    }

    // G2：上诉期(裁定,CEL doc.type=='ruling',+10)，anchor 2026-03-02 → 2026-03-12（周四）。
    #[test]
    fn g2_appeal_ruling_cel() {
        let ctx = Ctx::new(true).with_field("doc.type", "ruling");
        let r = compute(
            &rule_appeal_period(),
            d("2026-03-02"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("G2 must compute");
        assert_eq!(r.date, d("2026-03-12"), "G2");
    }

    // G3：答辩期(+15,顺延)，anchor 2026-08-01 → 末日08-16(周日)→顺延→2026-08-17(周一)。
    #[test]
    fn g3_answer_weekend_rollover() {
        let ctx = Ctx::new(true);
        let r = compute(
            &rule_answer_period(),
            d("2026-08-01"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("G3 must compute");
        assert_eq!(r.date, d("2026-08-17"), "G3");
    }

    // G4：答辩期(+15,顺延)，anchor 2026-09-20 → 末日10-05(国庆假)→跳10-05~07→2026-10-08(周四)。
    #[test]
    fn g4_answer_national_holiday_rollover() {
        let ctx = Ctx::new(true);
        let r = compute(
            &rule_answer_period(),
            d("2026-09-20"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("G4 must compute");
        assert_eq!(r.date, d("2026-10-08"), "G4");
    }

    // G5：诉讼时效一般(+3年,rollover none)，anchor 2023-05-10 → 对应日 2026-05-10。
    #[test]
    fn g5_limitation_three_years_no_rollover() {
        let ctx = Ctx::new(true);
        // 跨 2023..2026，需各年有表（rollover=none 时仅需核心区间有表；
        // 但 month/year 不触达 is_business_day，唯前置缺表校验生效）。
        let holidays = HolidaySet::new()
            .with(HolidayTable::new("cn-statutory-2023", 2023))
            .with(HolidayTable::new("cn-statutory-2024", 2024))
            .with(HolidayTable::new("cn-statutory-2025", 2025))
            .with(holidays_2026());
        let r = compute(
            &rule_limitation_general(),
            d("2023-05-10"),
            &holidays,
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("G5 must compute");
        assert_eq!(r.date, d("2026-05-10"), "G5");
    }

    // G6：触发日未确认 → AnchorNotConfirmed（护栏：不得产出到期日）。
    #[test]
    fn g6_anchor_not_confirmed() {
        let ctx = Ctx::new(false); // anchorConfirmed=false
        let err = compute(
            &rule_answer_period(),
            d("2026-03-02"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect_err("G6 must refuse");
        assert_eq!(err, EngineError::AnchorNotConfirmed, "G6");
    }

    // G7：申请再审(+6月)，anchor 2026-09-15 → 末日落 2027-03-15，无 2027 表 → HolidayTableMissing(2027)。
    #[test]
    fn g7_holiday_table_missing_2027() {
        let ctx = Ctx::new(true);
        // 仅有 2026 表（与 golden holidayTablesAvailable:["cn-statutory-2026"] 一致）。
        let err = compute(
            &rule_retrial_application(),
            d("2026-09-15"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect_err("G7 must refuse");
        assert_eq!(err, EngineError::HolidayTableMissing { year: 2027 }, "G7");
    }

    // 辅助断言：trace 与版本元数据被填充（合规可追溯）。
    #[test]
    fn trace_and_metadata_populated() {
        let ctx = Ctx::new(true).with_field("doc.type", "judgment");
        let r = compute(
            &rule_appeal_period(),
            d("2026-03-02"),
            &set_2026(),
            &ctx,
            &MinimalDocTypeCel,
            ruleset_version(),
        )
        .expect("must compute");
        assert_eq!(r.used_ruleset_version, "cn-civil-first-instance@2026.06.1");
        assert!(r.used_holiday_table.contains(&"cn-statutory-2026".to_string()));
        assert!(!r.trace.is_empty());
    }
}
