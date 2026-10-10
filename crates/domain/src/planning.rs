use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
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
    pub item_id: Uuid,
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
