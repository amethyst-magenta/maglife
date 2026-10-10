use crate::{Item, ItemId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
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
