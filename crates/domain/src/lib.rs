use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type ItemId = Uuid;
pub type ItemTypeId = String;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionPolicy {
    Protected,
    Confirmed,
    Adaptive,
    Opportunistic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanningMetadata {
    pub priority: i32,
    pub interest: f32,
    pub momentum: f32,
    pub min_session_minutes: u32,
    pub target_minutes_per_week: u32,
    pub policy: PermissionPolicy,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemType {
    pub id: ItemTypeId,
    pub label: String,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldDefinition {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Directive {
    pub id: Uuid,
    pub subject: Option<ItemId>,
    pub effect: String,
    pub text: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionStatus {
    Planned,
    Active,
    Completed,
    Skipped,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub id: Uuid,
    pub item_id: ItemId,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub actual_start: Option<DateTime<Utc>>,
    pub actual_end: Option<DateTime<Utc>>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Constraint {
    pub id: Uuid,
    pub label: String,
    pub kind: String,
    pub hard: bool,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanEntry {
    pub id: Uuid,
    pub item_id: ItemId,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub score: f32,
    pub explanation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanVersion {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub reason: String,
    pub state: String,
    pub entries: Vec<PlanEntry>,
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
