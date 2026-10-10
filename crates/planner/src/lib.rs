use chrono::{DateTime, Duration, Utc};
use maglife_domain::{Item, PlanEntry, PlanVersion};
use uuid::Uuid;

pub fn score(item: &Item) -> (f32, Vec<String>) {
    let priority = item
        .metadata
        .get("priority")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    let interest = item
        .metadata
        .get("interest")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    let momentum = item
        .metadata
        .get("momentum")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    (
        priority + interest * 3.0 + momentum * 2.0,
        vec![
            format!("interest: {interest:.1}"),
            format!("momentum: {momentum:.1}"),
            format!("priority: {priority:.1}"),
        ],
    )
}

pub fn build_plan(items: &[Item], start: DateTime<Utc>, minutes: i64) -> PlanVersion {
    let mut ranked: Vec<_> = items.iter().map(|item| (score(item), item)).collect();
    ranked.sort_by(|a, b| {
        b.0.0
            .partial_cmp(&a.0.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut cursor = start;
    let mut entries = Vec::new();
    for ((value, mut explanation), item) in ranked {
        if cursor >= start + Duration::minutes(minutes) {
            break;
        }
        let end = cursor + Duration::minutes(60);
        explanation.insert(0, format!("selected score: {value:.1}"));
        entries.push(PlanEntry {
            id: Uuid::new_v4(),
            item_id: item.id,
            start: cursor,
            end,
            score: value,
            explanation,
        });
        cursor = end + Duration::minutes(15);
    }
    PlanVersion {
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        reason: "initial plan generated from current item scores".into(),
        state: "proposed".into(),
        entries,
    }
}
