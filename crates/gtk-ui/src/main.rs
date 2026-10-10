use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, Label, ListBox, Notebook,
    Orientation, prelude::*,
};
use maglife_domain::{Event, Item};
use maglife_storage::{connect, insert_item, list_items, migrate};
use std::sync::Arc;

const APP_ID: &str = "dev.maglife.PersonalActivityOS";

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let runtime = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));
    let pool = match runtime.block_on(async {
        let pool = connect(
            &std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///maglife".into()),
        )
        .await?;
        migrate(&pool).await.map_err(|e| e.to_string())?;
        Ok::<_, Box<dyn std::error::Error>>(pool)
    }) {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("database: {e}");
            return;
        }
    };
    let list = ListBox::new();
    let history = ListBox::new();
    let refresh = {
        let list = list.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        move || {
            while let Some(row) = list.row_at_index(0) {
                list.remove(&row);
            }
            if let Ok(items) = runtime.block_on(list_items(&pool)) {
                for (_, title, status) in items {
                    list.append(&Label::new(Some(&format!("{status}  {title}"))));
                }
            }
        }
    };
    refresh();
    if let Ok(events) = runtime.block_on(maglife_storage::list_events(&pool)) {
        for (time, kind, payload) in events {
            history.append(&Label::new(Some(&format!(
                "{}  {kind}  {payload}",
                time.format("%Y-%m-%d %H:%M")
            ))));
        }
    }
    let entry = Entry::builder()
        .placeholder_text("Capture idea or anything else...")
        .hexpand(true)
        .build();
    let add = Button::with_label("Capture");
    {
        let entry = entry.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        let refresh = refresh.clone();
        add.connect_clicked(move |_| {
            let title = entry.text().to_string();
            if title.trim().is_empty() {
                return;
            }
            let item = Item::capture("idea", title);
            if let Err(e) = runtime.block_on(insert_item(&pool, &item, &Event::item_created(&item)))
            {
                eprintln!("capture: {e}");
            }
            entry.set_text("");
            refresh();
        });
    }
    let input = GtkBox::new(Orientation::Horizontal, 8);
    input.append(&entry);
    input.append(&add);
    let world = GtkBox::new(Orientation::Vertical, 12);
    world.append(&input);
    world.append(&list);
    let notebook = Notebook::new();
    notebook.append_page(&world, Some(&Label::new(Some("WORLD"))));
    notebook.append_page(&history, Some(&Label::new(Some("HISTORY"))));
    let root = GtkBox::new(Orientation::Vertical, 12);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.append(&Label::new(Some("NOW / WORLD — Personal Activity OS")));
    root.append(&notebook);
    ApplicationWindow::builder()
        .application(app)
        .title("Maglife")
        .default_width(720)
        .default_height(520)
        .child(&root)
        .build()
        .present();
}
