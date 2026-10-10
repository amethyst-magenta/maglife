use maglife_domain::{Event, Item, ItemId};
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
