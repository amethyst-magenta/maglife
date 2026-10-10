use maglife_domain::{Directive, Event, Item, ItemId, ItemStatus, PlanVersion, Session};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

pub type Result<T> = std::result::Result<T, sqlx::Error>;

pub async fn connect(database_url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn migrate(pool: &PgPool) -> std::result::Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations")
        .run(pool)
        .await
        .map(|_| ())
}

pub async fn insert_item(pool: &PgPool, item: &Item, event: &Event) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO items (id, type_id, title, status, created_at, updated_at, parent_id, metadata) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(item.id).bind(&item.type_id).bind(&item.title).bind(format!("{:?}", item.status).to_lowercase())
        .bind(item.created_at).bind(item.updated_at).bind(item.parent_id).bind(&item.metadata).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO events (id, occurred_at, actor, kind, item_id, payload) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(event.id).bind(event.occurred_at).bind(serde_json::to_value(&event.actor).unwrap()).bind(&event.kind).bind(event.item_id).bind(&event.payload).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn list_items(pool: &PgPool) -> Result<Vec<(ItemId, String, String)>> {
    sqlx::query("SELECT id, title, status FROM items ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|r| (r.get("id"), r.get("title"), r.get("status")))
                .collect()
        })
}

pub async fn load_items(pool: &PgPool) -> Result<Vec<Item>> {
    sqlx::query("SELECT id, type_id, title, status, created_at, updated_at, parent_id, metadata FROM items ORDER BY created_at DESC")
        .fetch_all(pool).await.map(|rows| rows.into_iter().map(|r| Item { id: r.get("id"), type_id: r.get("type_id"), title: r.get("title"), status: match r.get::<String, _>("status").as_str() { "active" => ItemStatus::Active, "paused" => ItemStatus::Paused, "done" => ItemStatus::Done, "archived" => ItemStatus::Archived, _ => ItemStatus::Captured }, created_at: r.get("created_at"), updated_at: r.get("updated_at"), parent_id: r.get("parent_id"), metadata: r.get("metadata") }).collect())
}

pub async fn list_events(
    pool: &PgPool,
) -> Result<Vec<(chrono::DateTime<chrono::Utc>, String, String)>> {
    sqlx::query("SELECT occurred_at, kind, payload::text FROM events ORDER BY occurred_at DESC")
        .fetch_all(pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|r| (r.get("occurred_at"), r.get("kind"), r.get("payload")))
                .collect()
        })
}

pub async fn insert_directive(pool: &PgPool, directive: &Directive) -> Result<()> {
    sqlx::query("INSERT INTO directives (id, subject, effect, text, valid_from, valid_until) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(directive.id).bind(directive.subject).bind(&directive.effect).bind(&directive.text).bind(directive.valid_from).bind(directive.valid_until).execute(pool).await?;
    Ok(())
}

pub async fn insert_session(pool: &PgPool, session: &Session) -> Result<()> {
    sqlx::query("INSERT INTO sessions (id, item_id, planned_start, planned_end, actual_start, actual_end, status) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(session.id).bind(session.item_id).bind(session.planned_start).bind(session.planned_end).bind(session.actual_start).bind(session.actual_end).bind(format!("{:?}", session.status).to_lowercase()).execute(pool).await?;
    Ok(())
}

pub async fn list_sessions(pool: &PgPool) -> Result<Vec<(uuid::Uuid, String, String)>> {
    sqlx::query("SELECT id, status, actual_start::text FROM sessions ORDER BY COALESCE(actual_start, planned_start) DESC NULLS LAST")
        .fetch_all(pool).await.map(|rows| rows.into_iter().map(|r| (r.get("id"), r.get("status"), r.get("actual_start"))).collect())
}

pub async fn finish_latest_session(pool: &PgPool, status: &str) -> Result<bool> {
    let result = sqlx::query("UPDATE sessions SET actual_end = NOW(), status = $1 WHERE id = (SELECT id FROM sessions WHERE status = 'active' ORDER BY actual_start DESC LIMIT 1)")
        .bind(status).execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_item_status(pool: &PgPool, id: uuid::Uuid, status: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE items SET status = $1, updated_at = NOW() WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO events (id, occurred_at, actor, kind, item_id, payload) VALUES ($1,NOW(),'\"User\"'::jsonb,'status.changed',$2,$3)").bind(uuid::Uuid::new_v4()).bind(id).bind(serde_json::json!({"status": status})).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn add_availability(
    pool: &PgPool,
    starts: chrono::DateTime<chrono::Utc>,
    ends: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    sqlx::query("INSERT INTO availability_windows (id, starts_at, ends_at) VALUES ($1,$2,$3)")
        .bind(uuid::Uuid::new_v4())
        .bind(starts)
        .bind(ends)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn save_plan(pool: &PgPool, plan: &PlanVersion) -> Result<()> {
    sqlx::query("INSERT INTO plan_versions (id, created_at, reason, state, entries) VALUES ($1,$2,$3,$4,$5)")
        .bind(plan.id).bind(plan.created_at).bind(&plan.reason).bind(&plan.state)
        .bind(serde_json::to_value(&plan.entries).unwrap()).execute(pool).await?;
    Ok(())
}
