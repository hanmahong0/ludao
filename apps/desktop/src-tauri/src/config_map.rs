//! config DSL（skeleton/config/*.json）→ 引擎输入类型的 serde 映射层。
//! **不改引擎**：引擎的 Rule/HolidaySet 非 Deserialize，故在此先用 serde 解析 DSL 的
//! 中间结构，再 map 成引擎结构。gap-engine 的 Profile 本身 derive(Deserialize)，
//! 直接 serde_json::from_str 即可，无需中间层。

use chrono::NaiveDate;
use deadline_engine::{
    DayZero, Direction, HolidaySet, HolidayTable, HolidayType, Rollover, Rule, Unit,
};
use serde::Deserialize;
use std::collections::HashMap;

// ── deadline-rules DSL 中间结构 ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DeadlineRuleSet {
    #[serde(rename = "ruleSetId")]
    pub rule_set_id: String,
    pub version: String,
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub deadlines: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
pub struct RawRule {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(rename = "dayZero", default)]
    pub day_zero: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub amount: Option<i64>,
    pub unit: String,
    #[serde(default)]
    pub rollover: Option<String>,
    #[serde(rename = "requireLawyerConfirmAnchor", default)]
    pub require_lawyer_confirm_anchor: bool,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(rename = "amountIfCondition", default)]
    pub amount_if_condition: Option<i64>,
    #[serde(rename = "minAmountEnforced", default)]
    pub min_amount_enforced: Option<i64>,
    #[serde(rename = "courtDesignatable", default)]
    pub court_designatable: bool,
}

impl DeadlineRuleSet {
    pub fn ruleset_version(&self) -> String {
        format!("{}@{}", self.rule_set_id, self.version)
    }

    /// 查 rule_id 对应的 deadlines[] 条目并映射成引擎 Rule。
    pub fn find_rule(&self, rule_id: &str) -> Result<Rule, String> {
        let raw = self
            .deadlines
            .iter()
            .find(|r| r.id == rule_id)
            .ok_or_else(|| format!("rule_id not found in ruleset: {rule_id}"))?;
        raw.to_engine_rule(self.schema_version)
    }

    /// 期限行落库所需的展示元信息：(label, 触发事件 anchor_event, 是否要求律师确认锚点)。
    /// 这些是文案/事件名，不是天数——天数永远由引擎读 rule.amount，绝不在此返回。
    pub fn rule_meta(&self, rule_id: &str) -> Result<(String, String, bool), String> {
        let raw = self
            .deadlines
            .iter()
            .find(|r| r.id == rule_id)
            .ok_or_else(|| format!("rule_id not found in ruleset: {rule_id}"))?;
        Ok((
            raw.label.clone().unwrap_or_else(|| raw.id.clone()),
            raw.trigger.clone().unwrap_or_default(),
            raw.require_lawyer_confirm_anchor,
        ))
    }
}

impl RawRule {
    fn to_engine_rule(&self, ruleset_schema_version: u32) -> Result<Rule, String> {
        let day_zero = match self.day_zero.as_deref() {
            Some("include") => DayZero::Include,
            // 默认 exclude（民诉「期间开始的日不计入」）；month/year 规则未显式给 dayZero 也走默认。
            _ => DayZero::Exclude,
        };
        let direction = match self.direction.as_deref() {
            Some("backward") => Direction::Backward,
            _ => Direction::Forward,
        };
        let unit = match self.unit.as_str() {
            "calendar_day" => Unit::CalendarDay,
            "court_day" => Unit::CourtDay,
            "month" => Unit::Month,
            "year" => Unit::Year,
            other => return Err(format!("unknown unit: {other}")),
        };
        let rollover = match self.rollover.as_deref() {
            Some("next_business_day") => Rollover::NextBusinessDay,
            Some("prev_business_day") => Rollover::PrevBusinessDay,
            _ => Rollover::None,
        };
        let amount = self
            .amount
            .ok_or_else(|| format!("rule {} missing amount", self.id))?;
        Ok(Rule {
            id: self.id.clone(),
            schema_version: ruleset_schema_version,
            day_zero,
            direction,
            amount,
            unit,
            rollover,
            require_lawyer_confirm_anchor: self.require_lawyer_confirm_anchor,
            condition: self.condition.clone(),
            amount_if_condition: self.amount_if_condition,
            min_amount_enforced: self.min_amount_enforced,
            court_designatable: self.court_designatable,
        })
    }
}

// ── holidays DSL 中间结构 ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct HolidayConfig {
    #[serde(rename = "tableId")]
    pub table_id: String,
    pub year: i32,
    pub days: Vec<RawHoliday>,
}

#[derive(Debug, Deserialize)]
pub struct RawHoliday {
    pub date: String,
    #[serde(rename = "type")]
    pub ty: String,
}

impl HolidayConfig {
    /// 映射成引擎 HolidaySet（仅该年一张表）。
    pub fn to_holiday_set(&self) -> Result<HolidaySet, String> {
        let mut days: HashMap<NaiveDate, HolidayType> = HashMap::new();
        for d in &self.days {
            let date = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                .map_err(|e| format!("bad holiday date {}: {e}", d.date))?;
            let ty = match d.ty.as_str() {
                "statutory_holiday" => HolidayType::StatutoryHoliday,
                "makeup_workday" => HolidayType::MakeupWorkday,
                other => return Err(format!("unknown holiday type: {other}")),
            };
            days.insert(date, ty);
        }
        let mut table = HolidayTable::new(self.table_id.clone(), self.year);
        table.days = days;
        Ok(HolidaySet::new().with(table))
    }
}
