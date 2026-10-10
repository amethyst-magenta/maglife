use maglife_domain::{Event, Item};
use maglife_storage::{connect, insert_item, list_items, migrate};

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
        _ => {
            for (id, title, status) in list_items(&pool).await? {
                println!("{status:10} {id} {title}");
            }
        }
    }
    Ok(())
}
