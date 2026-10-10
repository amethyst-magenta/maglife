use chrono::Utc;
use maglife_domain::{Directive, Event, Item};
use maglife_storage::{connect, insert_directive, insert_item, list_items, migrate};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///maglife".into());
    let pool = connect(&database_url).await?;
    migrate(&pool).await?;
    match std::env::args().nth(1).as_deref() {
        Some("capture") => {
            let title = std::env::args().skip(2).collect::<Vec<_>>().join(" ");
            if title.trim().is_empty() {
                return Err("usage: maglife capture <title>".into());
            }
            let item = Item::capture("idea", title);
            insert_item(&pool, &item, &Event::item_created(&item)).await?;
            println!("Captured {} ({})", item.title, item.id);
        }
        Some("directive") => {
            let text = std::env::args().skip(2).collect::<Vec<_>>().join(" ");
            if text.trim().is_empty() {
                return Err("usage: maglife directive <text>".into());
            }
            let directive = Directive {
                id: Uuid::new_v4(),
                subject: None,
                effect: "note".into(),
                text,
                valid_from: Utc::now(),
                valid_until: None,
            };
            insert_directive(&pool, &directive).await?;
            println!("Directive recorded: {}", directive.text);
        }
        _ => {
            for (id, title, status) in list_items(&pool).await? {
                println!("{status:10} {id} {title}");
            }
        }
    }
    Ok(())
}
