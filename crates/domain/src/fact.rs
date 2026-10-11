use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactStatus {
    Running,
    Completed,
    Skipped,
}

impl FactStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Skipped => "skipped",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "В процессе",
            Self::Completed => "Завершено",
            Self::Skipped => "Пропущено",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FactEntry {
    pub id: Uuid,
    pub day: NaiveDate,
    pub plan_block_id: Option<Uuid>,
    pub title: String,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub status: FactStatus,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl FactEntry {
    pub fn is_running(&self) -> bool {
        self.status == FactStatus::Running
    }
    pub fn draft(&self) -> FactDraft {
        FactDraft {
            title: self.title.clone(),
            plan_block_id: self.plan_block_id,
            actual_start: self.actual_start,
            actual_end: self.actual_end,
            status: self.status,
            note: self.note.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FactDraft {
    pub title: String,
    pub plan_block_id: Option<Uuid>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub status: FactStatus,
    pub note: Option<String>,
}

impl FactDraft {
    pub fn validate(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), &'static str> {
        if self.title.trim().is_empty() {
            return Err("Введите название факта.");
        }
        match self.status {
            FactStatus::Skipped if self.actual_start.is_some() || self.actual_end.is_some() => {
                return Err("Пропуск не имеет фактического интервала. Очистите начало и конец.");
            }
            FactStatus::Running if self.actual_start.is_none() || self.actual_end.is_some() => {
                return Err("Для активного факта нужно только начало.");
            }
            FactStatus::Completed if self.actual_start.is_none() || self.actual_end.is_none() => {
                return Err("Для завершённого факта нужны начало и конец.");
            }
            _ => {}
        }
        if let Some(start) = self.actual_start {
            if start < from || start >= to {
                return Err("Фактическое начало должно находиться в выбранном дне.");
            }
            if start > now {
                return Err("Фактическое время не может быть в будущем.");
            }
        }
        if let Some(end) = self.actual_end {
            if end > to || end < from {
                return Err(
                    "Фактический конец должен находиться в выбранном дне (до следующей полуночи).",
                );
            }
            if end > now {
                return Err("Фактическое время не может быть в будущем.");
            }
            if self.actual_start.is_some_and(|start| end < start) {
                return Err("Конец не может быть раньше начала.");
            }
        }
        Ok(())
    }
}
