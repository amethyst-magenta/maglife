use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockStatus {
    Planned,
    Active,
    Completed,
    Skipped,
}

impl BlockStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Skipped => "skipped",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "Запланировано",
            Self::Active => "В процессе",
            Self::Completed => "Завершено",
            Self::Skipped => "Пропущено вручную",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DomainError(pub &'static str);

#[derive(Debug, Clone)]
pub struct BlockDraft {
    pub title: String,
    pub category: Option<String>,
    pub planned_start: DateTime<Utc>,
    pub planned_end: DateTime<Utc>,
}

impl BlockDraft {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.title.trim().is_empty() {
            return Err(DomainError("Введите название блока."));
        }
        if self.planned_end <= self.planned_start {
            return Err(DomainError("Конец блока должен быть позже начала."));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ActivityBlock {
    pub id: Uuid,
    pub title: String,
    pub category: Option<String>,
    pub planned_start: DateTime<Utc>,
    pub planned_end: DateTime<Utc>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub status: BlockStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ActivityBlock {
    pub fn new(draft: BlockDraft, now: DateTime<Utc>) -> Result<Self, DomainError> {
        draft.validate()?;
        Ok(Self {
            id: Uuid::new_v4(),
            title: draft.title.trim().to_owned(),
            category: draft
                .category
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_owned()),
            planned_start: draft.planned_start,
            planned_end: draft.planned_end,
            actual_start: None,
            actual_end: None,
            status: BlockStatus::Planned,
            created_at: now,
            updated_at: now,
        })
    }
    pub fn is_missed(&self, now: DateTime<Utc>) -> bool {
        self.status == BlockStatus::Planned && self.planned_end <= now
    }
    pub fn is_scheduled_now(&self, now: DateTime<Utc>) -> bool {
        self.planned_start <= now && now < self.planned_end
    }
    pub fn result_label(&self, now: DateTime<Utc>) -> &'static str {
        if self.is_missed(now) {
            "Не начато вовремя"
        } else {
            self.status.label()
        }
    }
    pub fn draft(&self) -> BlockDraft {
        BlockDraft {
            title: self.title.clone(),
            category: self.category.clone(),
            planned_start: self.planned_start,
            planned_end: self.planned_end,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HistoryEdit {
    pub status: BlockStatus,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
}

impl HistoryEdit {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), DomainError> {
        match self.status {
            BlockStatus::Planned | BlockStatus::Skipped
                if self.actual_start.is_some() || self.actual_end.is_some() =>
            {
                return Err(DomainError(
                    "У не начатого или пропущенного блока не должно быть фактического времени.",
                ));
            }
            BlockStatus::Active if self.actual_start.is_none() || self.actual_end.is_some() => {
                return Err(DomainError(
                    "Для активного блока укажите только фактическое начало.",
                ));
            }
            BlockStatus::Completed if self.actual_start.is_none() || self.actual_end.is_none() => {
                return Err(DomainError(
                    "Для завершённого блока укажите фактические начало и конец.",
                ));
            }
            _ => {}
        }
        if let (Some(start), Some(end)) = (self.actual_start, self.actual_end)
            && end < start
        {
            return Err(DomainError(
                "Фактический конец не может быть раньше начала.",
            ));
        }
        if self.actual_start.is_some_and(|t| t > now) || self.actual_end.is_some_and(|t| t > now) {
            return Err(DomainError("Фактическое время не может быть в будущем."));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    fn block() -> ActivityBlock {
        let start = DateTime::parse_from_rfc3339("2026-10-11T08:00:00Z")
            .unwrap()
            .to_utc();
        ActivityBlock::new(
            BlockDraft {
                title: " Сон ".into(),
                category: Some(" ".into()),
                planned_start: start,
                planned_end: start + Duration::hours(1),
            },
            start,
        )
        .unwrap()
    }
    #[test]
    fn time_boundaries_and_missed_are_derived() {
        let mut b = block();
        assert_eq!(b.title, "Сон");
        assert_eq!(b.category, None);
        assert!(b.is_scheduled_now(b.planned_start));
        assert!(!b.is_scheduled_now(b.planned_end));
        assert!(b.is_missed(b.planned_end));
        b.status = BlockStatus::Skipped;
        assert!(!b.is_missed(b.planned_end));
    }
    #[test]
    fn rejects_invalid_plan_and_inconsistent_history() {
        let b = block();
        let mut draft = b.draft();
        draft.planned_end = draft.planned_start;
        assert!(draft.validate().is_err());
        assert!(
            HistoryEdit {
                status: BlockStatus::Completed,
                actual_start: None,
                actual_end: None
            }
            .validate(b.planned_end)
            .is_err()
        );
        assert!(
            HistoryEdit {
                status: BlockStatus::Completed,
                actual_start: Some(b.planned_end),
                actual_end: Some(b.planned_start)
            }
            .validate(b.planned_end)
            .is_err()
        );
        assert!(
            HistoryEdit {
                status: BlockStatus::Completed,
                actual_start: Some(b.planned_start),
                actual_end: Some(b.planned_end)
            }
            .validate(b.planned_end)
            .is_ok()
        );
    }
}
