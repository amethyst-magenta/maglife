use chrono::{DateTime, Utc};
use maglife_domain::{ActivityBlock, BlockDraft, BlockStatus, DomainError, HistoryEdit};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("Блок уже изменён или удалён. Обновите страницу и повторите действие.")]
    Conflict,
    #[error("После начала блока плановое время сохраняется для сравнения с фактом.")]
    PlanLocked,
}

impl StorageError {
    pub fn user_message(&self) -> String {
        if let Self::Database(sqlx::Error::Database(e)) = self {
            match e.constraint() {
                Some("no_plan_overlap") => {
                    return "План пересекается с другим блоком. Выберите свободное время.".into();
                }
                Some("one_active_block") => {
                    return "Сначала завершите активный блок или исправьте его историю.".into();
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
struct BlockRow {
    id: Uuid,
    title: String,
    category: Option<String>,
    planned_start: DateTime<Utc>,
    planned_end: DateTime<Utc>,
    actual_start: Option<DateTime<Utc>>,
    actual_end: Option<DateTime<Utc>>,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<BlockRow> for ActivityBlock {
    type Error = StorageError;
    fn try_from(r: BlockRow) -> Result<Self> {
        let status = match r.status.as_str() {
            "planned" => BlockStatus::Planned,
            "active" => BlockStatus::Active,
            "completed" => BlockStatus::Completed,
            "skipped" => BlockStatus::Skipped,
            _ => return Err(sqlx::Error::Decode("Неизвестный статус блока".into()).into()),
        };
        Ok(Self {
            id: r.id,
            title: r.title,
            category: r.category,
            planned_start: r.planned_start,
            planned_end: r.planned_end,
            actual_start: r.actual_start,
            actual_end: r.actual_end,
            status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

pub async fn load_day(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<ActivityBlock>> {
    sqlx::query_as::<_, BlockRow>("SELECT * FROM activity_blocks WHERE (planned_start < $2 AND planned_end > $1) OR (actual_start < $2 AND COALESCE(actual_end, GREATEST(NOW(), actual_start)) >= $1) ORDER BY planned_start")
        .bind(start).bind(end).fetch_all(pool).await?.into_iter().map(ActivityBlock::try_from).collect()
}

pub async fn load_now(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<ActivityBlock>> {
    sqlx::query_as::<_, BlockRow>("SELECT * FROM activity_blocks WHERE status = 'active' OR (planned_start <= $1 AND planned_end > $1) OR id = (SELECT id FROM activity_blocks WHERE status = 'planned' AND planned_start > $1 ORDER BY planned_start LIMIT 1) ORDER BY planned_start")
        .bind(now).fetch_all(pool).await?.into_iter().map(ActivityBlock::try_from).collect()
}

pub async fn create_block(pool: &PgPool, draft: BlockDraft) -> Result<()> {
    let b = ActivityBlock::new(draft, Utc::now())?;
    sqlx::query("INSERT INTO activity_blocks (id,title,category,planned_start,planned_end,created_at,updated_at) VALUES ($1,$2,$3,$4,$5,$6,$6)").bind(b.id).bind(b.title).bind(b.category).bind(b.planned_start).bind(b.planned_end).bind(b.created_at).execute(pool).await?;
    Ok(())
}

pub async fn edit_block(pool: &PgPool, original: &ActivityBlock, draft: BlockDraft) -> Result<()> {
    draft.validate()?;
    if original.actual_start.is_some()
        && (original.planned_start != draft.planned_start
            || original.planned_end != draft.planned_end)
    {
        return Err(StorageError::PlanLocked);
    }
    let category = draft
        .category
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_owned());
    changed(sqlx::query("UPDATE activity_blocks SET title=$1,category=$2,planned_start=$3,planned_end=$4,updated_at=clock_timestamp() WHERE id=$5 AND updated_at=$6 AND (actual_start IS NULL OR (planned_start=$3 AND planned_end=$4))").bind(draft.title.trim()).bind(category).bind(draft.planned_start).bind(draft.planned_end).bind(original.id).bind(original.updated_at).execute(pool).await?.rows_affected())
}

pub async fn delete_block(pool: &PgPool, original: &ActivityBlock) -> Result<()> {
    changed(
        sqlx::query(
            "DELETE FROM activity_blocks WHERE id=$1 AND updated_at=$2 AND actual_start IS NULL",
        )
        .bind(original.id)
        .bind(original.updated_at)
        .execute(pool)
        .await?
        .rows_affected(),
    )
}
pub async fn start_block(pool: &PgPool, original: &ActivityBlock) -> Result<()> {
    changed(sqlx::query("UPDATE activity_blocks SET status='active',actual_start=clock_timestamp(),updated_at=clock_timestamp() WHERE id=$1 AND updated_at=$2 AND status='planned'").bind(original.id).bind(original.updated_at).execute(pool).await?.rows_affected())
}
pub async fn finish_block(pool: &PgPool, original: &ActivityBlock) -> Result<()> {
    changed(sqlx::query("UPDATE activity_blocks SET status='completed',actual_end=GREATEST(clock_timestamp(),actual_start),updated_at=clock_timestamp() WHERE id=$1 AND updated_at=$2 AND status='active'").bind(original.id).bind(original.updated_at).execute(pool).await?.rows_affected())
}
pub async fn skip_block(pool: &PgPool, original: &ActivityBlock) -> Result<()> {
    changed(sqlx::query("UPDATE activity_blocks SET status='skipped',updated_at=clock_timestamp() WHERE id=$1 AND updated_at=$2 AND status='planned'").bind(original.id).bind(original.updated_at).execute(pool).await?.rows_affected())
}
pub async fn edit_history(
    pool: &PgPool,
    original: &ActivityBlock,
    edit: HistoryEdit,
) -> Result<()> {
    edit.validate(Utc::now())?;
    changed(sqlx::query("UPDATE activity_blocks SET status=$1,actual_start=$2,actual_end=$3,updated_at=clock_timestamp() WHERE id=$4 AND updated_at=$5").bind(edit.status.as_str()).bind(edit.actual_start).bind(edit.actual_end).bind(original.id).bind(original.updated_at).execute(pool).await?.rows_affected())
}
fn changed(rows: u64) -> Result<()> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StorageError::Conflict)
    }
}
