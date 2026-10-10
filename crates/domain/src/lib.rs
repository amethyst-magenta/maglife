use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type ItemId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ItemStatus {
    Captured,
    Active,
    Paused,
    Done,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub type_id: String,
    pub title: String,
    pub status: ItemStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<ItemId>,
    pub metadata: Value,
}

impl Item {
    pub fn capture(type_id: impl Into<String>, title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            type_id: type_id.into(),
            title: title.into(),
            status: ItemStatus::Captured,
            created_at: now,
            updated_at: now,
            parent_id: None,
            metadata: Value::Object(Default::default()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Actor {
    User,
    Scheduler,
    System,
    Ai { model: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub actor: Actor,
    pub kind: String,
    pub item_id: Option<ItemId>,
    pub payload: Value,
}

impl Event {
    pub fn item_created(item: &Item) -> Self {
        Self {
            id: Uuid::new_v4(),
            occurred_at: Utc::now(),
            actor: Actor::User,
            kind: "item.created".into(),
            item_id: Some(item.id),
            payload: serde_json::json!({"title": item.title, "type_id": item.type_id}),
        }
    }
}
