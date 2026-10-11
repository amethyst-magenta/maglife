use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use maglife_domain::{
    DayPlan, DayPlanState, DayTemplate, FactDraft, FactEntry, FactStatus, PlanBlock, PlanDraft,
    PlanSource, TemplateBlock, TemplateDraft,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("{0}")]
    Validation(String),
    #[error("План этого дня уже утверждён и больше не редактируется.")]
    PlanApproved,
    #[error("Запись уже изменена или удалена. Обновите экран и повторите действие.")]
    Conflict,
}

impl StorageError {
    pub fn user_message(&self) -> String {
        if let Self::Database(sqlx::Error::Database(error)) = self {
            match error.constraint() {
                Some("no_plan_overlap") => {
                    return "Плановый блок пересекается с другим блоком.".into();
                }
                Some("one_running_fact") => {
                    return "Сначала завершите текущее фактическое занятие.".into();
                }
                Some("day_templates_name_key") => {
                    return "Шаблон с таким названием уже существует.".into();
                }
                _ => {}
            }
        }
        self.to_string()
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

pub async fn connect(database_url: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(database_url)
        .await?)
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct DayPlanRow {
    day: NaiveDate,
    state: String,
    approved_at: Option<DateTime<Utc>>,
}

impl TryFrom<DayPlanRow> for DayPlan {
    type Error = StorageError;

    fn try_from(row: DayPlanRow) -> Result<Self> {
        let state = match row.state.as_str() {
            "draft" => DayPlanState::Draft,
            "approved" => DayPlanState::Approved,
            _ => {
                return Err(StorageError::Validation(
                    "Неизвестное состояние плана.".into(),
                ));
            }
        };
        Ok(Self {
            day: row.day,
            state,
            approved_at: row.approved_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct PlanBlockRow {
    id: Uuid,
    day: NaiveDate,
    title: String,
    category: Option<String>,
    planned_start: DateTime<Utc>,
    planned_end: DateTime<Utc>,
    source: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<PlanBlockRow> for PlanBlock {
    type Error = StorageError;

    fn try_from(row: PlanBlockRow) -> Result<Self> {
        let source = match row.source.as_str() {
            "manual" => PlanSource::Manual,
            "template" => PlanSource::Template,
            _ => {
                return Err(StorageError::Validation(
                    "Неизвестный источник планового блока.".into(),
                ));
            }
        };
        Ok(Self {
            id: row.id,
            day: row.day,
            title: row.title,
            category: row.category,
            planned_start: row.planned_start,
            planned_end: row.planned_end,
            source,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct FactRow {
    id: Uuid,
    day: NaiveDate,
    plan_block_id: Option<Uuid>,
    title: String,
    actual_start: Option<DateTime<Utc>>,
    actual_end: Option<DateTime<Utc>>,
    status: String,
    note: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<FactRow> for FactEntry {
    type Error = StorageError;

    fn try_from(row: FactRow) -> Result<Self> {
        let status = match row.status.as_str() {
            "running" => FactStatus::Running,
            "completed" => FactStatus::Completed,
            "skipped" => FactStatus::Skipped,
            _ => return Err(StorageError::Validation("Неизвестный статус факта.".into())),
        };
        Ok(Self {
            id: row.id,
            day: row.day,
            plan_block_id: row.plan_block_id,
            title: row.title,
            actual_start: row.actual_start,
            actual_end: row.actual_end,
            status,
            note: row.note,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct TemplateRow {
    id: Uuid,
    name: String,
    is_default: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<TemplateRow> for DayTemplate {
    fn from(row: TemplateRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            is_default: row.is_default,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TemplateBlockRow {
    id: Uuid,
    template_id: Uuid,
    title: String,
    category: Option<String>,
    starts_at: NaiveTime,
    ends_at: NaiveTime,
    ends_next_day: bool,
}

impl From<TemplateBlockRow> for TemplateBlock {
    fn from(row: TemplateBlockRow) -> Self {
        Self {
            id: row.id,
            template_id: row.template_id,
            title: row.title,
            category: row.category,
            starts_at: row.starts_at,
            ends_at: row.ends_at,
            ends_next_day: row.ends_next_day,
        }
    }
}

pub async fn ensure_day(pool: &PgPool, day: NaiveDate) -> Result<DayPlan> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let inserted = sqlx::query("INSERT INTO day_plans (day, state, created_at, updated_at) VALUES ($1, 'draft', $2, $2) ON CONFLICT (day) DO NOTHING")
        .bind(day).bind(now).execute(&mut *tx).await?.rows_affected() == 1;
    if inserted {
        if let Some(template) = sqlx::query_as::<_, TemplateRow>("SELECT id, name, is_default, created_at, updated_at FROM day_templates WHERE is_default LIMIT 1").fetch_optional(&mut *tx).await? {
            let blocks = sqlx::query_as::<_, TemplateBlockRow>("SELECT id, template_id, title, category, starts_at, ends_at, ends_next_day FROM template_blocks WHERE template_id = $1 ORDER BY starts_at")
                .bind(template.id).fetch_all(&mut *tx).await?;
            for block in blocks {
                let start = Local.from_local_datetime(&day.and_time(block.starts_at)).single().ok_or_else(|| StorageError::Validation("Не удалось определить местное время шаблона.".into()))?.with_timezone(&Utc);
                let end_day = if block.ends_next_day { day.succ_opt().ok_or_else(|| StorageError::Validation("Дата шаблона выходит за допустимые границы.".into()))? } else { day };
                let end = Local.from_local_datetime(&end_day.and_time(block.ends_at)).single().ok_or_else(|| StorageError::Validation("Не удалось определить местное время шаблона.".into()))?.with_timezone(&Utc);
                sqlx::query("INSERT INTO plan_blocks (id, day, title, category, planned_start, planned_end, source, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,'template',$7,$7)")
                    .bind(Uuid::new_v4()).bind(day).bind(block.title).bind(block.category).bind(start).bind(end).bind(now).execute(&mut *tx).await?;
            }
        }
    }
    let row = sqlx::query_as::<_, DayPlanRow>(
        "SELECT day, state, approved_at FROM day_plans WHERE day = $1",
    )
    .bind(day)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    row.try_into()
}

pub async fn load_day_plan(pool: &PgPool, day: NaiveDate) -> Result<DayPlan> {
    ensure_day(pool, day).await
}

pub async fn approve_day(pool: &PgPool, day: NaiveDate) -> Result<()> {
    changed(sqlx::query("UPDATE day_plans SET state='approved', approved_at=COALESCE(approved_at, clock_timestamp()), updated_at=clock_timestamp() WHERE day=$1 AND state='draft'").bind(day).execute(pool).await?.rows_affected())
}

pub async fn load_plan_blocks(pool: &PgPool, day: NaiveDate) -> Result<Vec<PlanBlock>> {
    sqlx::query_as::<_, PlanBlockRow>("SELECT id, day, title, category, planned_start, planned_end, source, created_at, updated_at FROM plan_blocks WHERE day=$1 ORDER BY planned_start")
        .bind(day).fetch_all(pool).await?.into_iter().map(PlanBlock::try_from).collect()
}

async fn ensure_draft(pool: &PgPool, day: NaiveDate) -> Result<()> {
    if load_day_plan(pool, day).await?.state == DayPlanState::Approved {
        Err(StorageError::PlanApproved)
    } else {
        Ok(())
    }
}

pub async fn create_plan_block(pool: &PgPool, day: NaiveDate, draft: PlanDraft) -> Result<()> {
    draft
        .validate()
        .map_err(|e| StorageError::Validation(e.into()))?;
    ensure_draft(pool, day).await?;
    let now = Utc::now();
    sqlx::query("INSERT INTO plan_blocks (id,day,title,category,planned_start,planned_end,source,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,'manual',$7,$7)")
        .bind(Uuid::new_v4()).bind(day).bind(draft.title.trim()).bind(draft.category.filter(|v| !v.trim().is_empty())).bind(draft.planned_start).bind(draft.planned_end).bind(now).execute(pool).await?;
    Ok(())
}

pub async fn edit_plan_block(pool: &PgPool, block: &PlanBlock, draft: PlanDraft) -> Result<()> {
    draft
        .validate()
        .map_err(|e| StorageError::Validation(e.into()))?;
    ensure_draft(pool, block.day).await?;
    changed(sqlx::query("UPDATE plan_blocks SET title=$1,category=$2,planned_start=$3,planned_end=$4,updated_at=clock_timestamp() WHERE id=$5 AND updated_at=$6")
        .bind(draft.title.trim()).bind(draft.category.filter(|v| !v.trim().is_empty())).bind(draft.planned_start).bind(draft.planned_end).bind(block.id).bind(block.updated_at).execute(pool).await?.rows_affected())
}

pub async fn delete_plan_block(pool: &PgPool, block: &PlanBlock) -> Result<()> {
    ensure_draft(pool, block.day).await?;
    changed(
        sqlx::query("DELETE FROM plan_blocks WHERE id=$1 AND updated_at=$2")
            .bind(block.id)
            .bind(block.updated_at)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

pub async fn load_facts(pool: &PgPool, day: NaiveDate) -> Result<Vec<FactEntry>> {
    sqlx::query_as::<_, FactRow>("SELECT id,day,plan_block_id,title,actual_start,actual_end,status,note,created_at,updated_at FROM fact_entries WHERE day=$1 ORDER BY actual_start")
        .bind(day).fetch_all(pool).await?.into_iter().map(FactEntry::try_from).collect()
}

pub async fn start_fact(
    pool: &PgPool,
    day: NaiveDate,
    plan_block_id: Option<Uuid>,
    title: String,
    note: Option<String>,
) -> Result<()> {
    if title.trim().is_empty() {
        return Err(StorageError::Validation("Введите название факта.".into()));
    }
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE day_plans SET state='approved', approved_at=COALESCE(approved_at, $2), updated_at=$2 WHERE day=$1 AND state='draft'")
        .bind(day).bind(now).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO fact_entries (id,day,plan_block_id,title,actual_start,status,note,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,'running',$6,$5,$5)")
        .bind(Uuid::new_v4()).bind(day).bind(plan_block_id).bind(title.trim()).bind(now).bind(note.filter(|v| !v.trim().is_empty())).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn finish_fact(pool: &PgPool, fact: &FactEntry, note: Option<String>) -> Result<()> {
    changed(sqlx::query("UPDATE fact_entries SET status='completed', actual_end=GREATEST(clock_timestamp(), actual_start), note=COALESCE($1,note), updated_at=clock_timestamp() WHERE id=$2 AND updated_at=$3 AND status='running'")
        .bind(note.filter(|v| !v.trim().is_empty())).bind(fact.id).bind(fact.updated_at).execute(pool).await?.rows_affected())
}

pub async fn edit_fact(pool: &PgPool, fact: &FactEntry, draft: FactDraft) -> Result<()> {
    let (from, to) =
        maglife_domain::day_bounds(fact.day).map_err(|e| StorageError::Validation(e.into()))?;
    draft
        .validate(from, to, Utc::now())
        .map_err(|e| StorageError::Validation(e.into()))?;
    changed(sqlx::query("UPDATE fact_entries SET status=$1,actual_start=$2,actual_end=$3,title=$4,note=$5,updated_at=clock_timestamp() WHERE id=$6 AND updated_at=$7")
        .bind(draft.status.as_str()).bind(draft.actual_start).bind(draft.actual_end).bind(draft.title.trim()).bind(draft.note.filter(|v| !v.trim().is_empty())).bind(fact.id).bind(fact.updated_at).execute(pool).await?.rows_affected())
}

pub async fn delete_fact(pool: &PgPool, fact: &FactEntry) -> Result<()> {
    changed(
        sqlx::query("DELETE FROM fact_entries WHERE id=$1 AND updated_at=$2")
            .bind(fact.id)
            .bind(fact.updated_at)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

pub async fn skip_plan_block(
    pool: &PgPool,
    day: NaiveDate,
    block_id: Uuid,
    note: Option<String>,
) -> Result<()> {
    let block = load_plan_blocks(pool, day)
        .await?
        .into_iter()
        .find(|b| b.id == block_id)
        .ok_or(StorageError::Conflict)?;
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE day_plans SET state='approved', approved_at=COALESCE(approved_at, $2), updated_at=$2 WHERE day=$1 AND state='draft'")
        .bind(day).bind(now).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO fact_entries (id,day,plan_block_id,title,status,note,created_at,updated_at) VALUES ($1,$2,$3,$4,'skipped',$5,$6,$6)")
        .bind(Uuid::new_v4()).bind(day).bind(block.id).bind(block.title).bind(note.filter(|v| !v.trim().is_empty())).bind(now).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list_templates(pool: &PgPool) -> Result<Vec<DayTemplate>> {
    Ok(sqlx::query_as::<_, TemplateRow>("SELECT id,name,is_default,created_at,updated_at FROM day_templates ORDER BY is_default DESC, name").fetch_all(pool).await?.into_iter().map(Into::into).collect())
}

pub async fn create_template_from_day(
    pool: &PgPool,
    day: NaiveDate,
    draft: TemplateDraft,
) -> Result<()> {
    if draft.name.trim().is_empty() {
        return Err(StorageError::Validation("Введите название шаблона.".into()));
    }
    let blocks = load_plan_blocks(pool, day).await?;
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let id = Uuid::new_v4();
    if draft.is_default {
        sqlx::query("UPDATE day_templates SET is_default=FALSE, updated_at=clock_timestamp() WHERE is_default").execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO day_templates (id,name,is_default,created_at,updated_at) VALUES ($1,$2,$3,$4,$4)").bind(id).bind(draft.name.trim()).bind(draft.is_default).bind(now).execute(&mut *tx).await?;
    for block in blocks {
        sqlx::query("INSERT INTO template_blocks (id,template_id,title,category,starts_at,ends_at,ends_next_day) VALUES ($1,$2,$3,$4,$5,$6,$7)").bind(Uuid::new_v4()).bind(id).bind(block.title).bind(block.category).bind(block.planned_start.with_timezone(&Local).time()).bind(block.planned_end.with_timezone(&Local).time()).bind(block.planned_end.date_naive() != block.planned_start.date_naive()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn reset_database(pool: &PgPool) -> Result<()> {
    sqlx::query("TRUNCATE fact_entries, plan_blocks, day_plans, template_blocks, day_templates RESTART IDENTITY CASCADE").execute(pool).await?;
    Ok(())
}

fn changed(rows: u64) -> Result<()> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StorageError::Conflict)
    }
}
