use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, Label, ListBox, Notebook,
    Orientation, ScrolledWindow, prelude::*,
};
use maglife_domain::{Event, Item};
use maglife_storage::{connect, insert_item, list_events, list_items, migrate};
use std::sync::Arc;

const APP_ID: &str = "dev.maglife.PersonalActivityOS";
fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}
fn clear(list: &ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
}
fn build_ui(app: &Application) {
    let runtime = Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime"));
    let pool = match runtime.block_on(async {
        let p = connect(
            &std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///maglife".into()),
        )
        .await?;
        migrate(&p).await.map_err(|e| e.to_string())?;
        Ok::<_, Box<dyn std::error::Error>>(p)
    }) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("database: {e}");
            return;
        }
    };
    let world_list = ListBox::new();
    let history_list = ListBox::new();
    let refresh = {
        let list = world_list.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        move || {
            clear(&list);
            if let Ok(items) = runtime.block_on(list_items(&pool)) {
                for (_, title, status) in items {
                    list.append(&Label::new(Some(&format!("{status:10}  {title}"))));
                }
            }
        }
    };
    let refresh_history = {
        let list = history_list.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        move || {
            clear(&list);
            if let Ok(events) = runtime.block_on(list_events(&pool)) {
                for (time, kind, payload) in events {
                    list.append(&Label::new(Some(&format!(
                        "{}  {kind}  {payload}",
                        time.format("%m-%d %H:%M")
                    ))));
                }
            }
        }
    };
    refresh();
    refresh_history();
    let capture = Entry::builder()
        .placeholder_text("Capture: idea, project, directive...")
        .hexpand(true)
        .build();
    let capture_button = Button::with_label("Capture");
    {
        let entry = capture.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        let refresh = refresh.clone();
        let history = refresh_history.clone();
        capture_button.connect_clicked(move |_| {
            let title = entry.text().trim().to_string();
            if title.is_empty() {
                return;
            }
            let item = Item::capture("idea", title);
            if let Err(e) = runtime.block_on(insert_item(&pool, &item, &Event::item_created(&item)))
            {
                eprintln!("capture: {e}");
            }
            entry.set_text("");
            refresh();
            history();
        });
    }
    let capture_bar = GtkBox::new(Orientation::Horizontal, 8);
    capture_bar.append(&capture);
    capture_bar.append(&capture_button);
    let now = GtkBox::new(Orientation::Vertical, 14);
    now.append(&Label::new(Some("CURRENT ACTION")));
    now.append(&Label::new(Some("No active session")));
    now.append(&Button::with_label("Start next suggested session"));
    now.append(&Label::new(Some(
        "Planner chooses from eligible Items and available time.",
    )));
    let plan = GtkBox::new(Orientation::Vertical, 10);
    plan.append(&Label::new(Some("TODAY · PLAN")));
    for text in [
        "09:00  Deep work · 90 min",
        "11:00  Available window",
        "14:00  Review / context",
        "19:00  Personal projects",
    ] {
        plan.append(&Label::new(Some(text)));
    }
    plan.append(&Button::with_label("Rebuild plan"));
    let world = GtkBox::new(Orientation::Vertical, 10);
    world.append(&capture_bar);
    world.append(
        &ScrolledWindow::builder()
            .vexpand(true)
            .child(&world_list)
            .build(),
    );
    let detail = GtkBox::new(Orientation::Vertical, 10);
    detail.append(&Label::new(Some("ITEM DETAIL")));
    detail.append(&Label::new(Some(
        "Fields · context · sessions · decisions · permissions · AI notes",
    )));
    detail.append(&Button::with_label("Open selected Item"));
    let input = GtkBox::new(Orientation::Vertical, 10);
    input.append(&Label::new(Some("COMMAND BAR")));
    input.append(&Label::new(Some(
        "idea: build local indexer  ·  magdex неделю не ставь  ·  почему сейчас Artix?",
    )));
    let notebook = Notebook::new();
    let history = ScrolledWindow::builder()
        .vexpand(true)
        .child(&history_list)
        .build();
    notebook.append_page(&now, Some(&Label::new(Some("NOW"))));
    notebook.append_page(&plan, Some(&Label::new(Some("PLAN"))));
    notebook.append_page(&world, Some(&Label::new(Some("WORLD"))));
    notebook.append_page(&detail, Some(&Label::new(Some("ITEM DETAIL"))));
    notebook.append_page(&history, Some(&Label::new(Some("HISTORY"))));
    notebook.append_page(&input, Some(&Label::new(Some("INPUT"))));
    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.append(&Label::new(Some("MAGLIFE · Personal Activity OS")));
    root.append(&notebook);
    ApplicationWindow::builder()
        .application(app)
        .title("Maglife")
        .default_width(900)
        .default_height(640)
        .child(&root)
        .build()
        .present();
}
