use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayPlanState {
    Draft,
    Approved,
}

impl DayPlanState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Черновик",
            Self::Approved => "Утверждён",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanSource {
    Manual,
    Template,
}

impl PlanSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Template => "template",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DayPlan {
    pub day: NaiveDate,
    pub state: DayPlanState,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct PlanDraft {
    pub title: String,
    pub category: Option<String>,
    pub planned_start: DateTime<Utc>,
    pub planned_end: DateTime<Utc>,
}

impl PlanDraft {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.title.trim().is_empty() {
            return Err("Введите название планового блока.");
        }
        if self.planned_end <= self.planned_start {
            return Err("Конец блока должен быть позже начала.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PlanBlock {
    pub id: Uuid,
    pub day: NaiveDate,
    pub title: String,
    pub category: Option<String>,
    pub planned_start: DateTime<Utc>,
    pub planned_end: DateTime<Utc>,
    pub source: PlanSource,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DayTemplate {
    pub id: Uuid,
    pub name: String,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TemplateBlock {
    pub id: Uuid,
    pub template_id: Uuid,
    pub title: String,
    pub category: Option<String>,
    pub starts_at: NaiveTime,
    pub ends_at: NaiveTime,
    pub ends_next_day: bool,
}

#[derive(Debug, Clone)]
pub struct TemplateDraft {
    pub name: String,
    pub is_default: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

    #[test]
    fn plan_draft_rejects_empty_and_backwards_time() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        let start = day.and_hms_opt(8, 0, 0).unwrap().and_utc();
        let mut draft = PlanDraft {
            title: "".into(),
            category: None,
            planned_start: start,
            planned_end: start + Duration::hours(1),
        };
        assert!(draft.validate().is_err());
        draft.title = "Работа".into();
        draft.planned_end = start;
        assert!(draft.validate().is_err());
    }
}
