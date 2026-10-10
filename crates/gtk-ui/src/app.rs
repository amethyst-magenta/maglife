use chrono::Utc;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, Entry, Label, ListBox,
    Notebook, Orientation, ScrolledWindow, gdk, prelude::*,
};
use maglife_domain::{Directive, Event, Item, Session, SessionStatus};
use maglife_storage::{
    add_availability, connect, finish_latest_session, insert_directive, insert_item,
    insert_session, list_events, list_items, load_items, migrate, update_item_status,
};
use std::sync::Arc;

const APP_ID: &str = "dev.maglife.PersonalActivityOS";
pub fn run() {
    install_css();
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_data(r#"
        .app-shell { background: #f6f7fb; }
        .top-title { font-size: 22px; font-weight: 700; color: #172033; padding: 8px 0; }
        .section-title { font-size: 18px; font-weight: 700; color: #172033; margin-bottom: 4px; }
        .muted { color: #697386; }
        .card { background: #ffffff; border-radius: 14px; padding: 18px; border: 1px solid #e4e8f0; }
        .primary { background: #5865f2; color: white; border-radius: 9px; padding: 10px 16px; font-weight: 700; }
        .input { background: #ffffff; border-radius: 10px; padding: 10px; border: 1px solid #d8deea; }
    "#);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
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
        .placeholder_text("Что добавить? Идея, проект, занятие или заметка...")
        .hexpand(true)
        .build();
    let capture_button = Button::with_label("Добавить");
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
    now.add_css_class("card");
    let now_title = Label::new(Some("Сейчас"));
    now_title.add_css_class("section-title");
    now.append(&now_title);
    now.append(&Label::new(Some("Нет активного занятия")));
    let start_button = Button::with_label("Начать предложенное занятие");
    start_button.add_css_class("primary");
    now.append(&start_button);
    let finish_button = Button::with_label("Завершить занятие");
    now.append(&finish_button);
    now.append(&Label::new(Some(
        "Система выберет подходящее занятие с учётом свободного времени и твоих предпочтений.",
    )));
    let plan = GtkBox::new(Orientation::Vertical, 10);
    plan.add_css_class("card");
    plan.append(&Label::new(Some("ПЛАН НА СЕГОДНЯ")));
    let plan_rows = ListBox::new();
    if let Ok(items) = runtime.block_on(load_items(&pool)) {
        let generated = maglife_planner::build_plan(&items, Utc::now(), 480);
        for entry in &generated.entries {
            plan_rows.append(&Label::new(Some(&format!(
                "{}–{}  score {:.1}  {}",
                entry.start.format("%H:%M"),
                entry.end.format("%H:%M"),
                entry.score,
                entry.explanation.join(" · ")
            ))));
        }
        if let Err(e) = runtime.block_on(maglife_storage::save_plan(&pool, &generated)) {
            eprintln!("plan save: {e}");
        }
    }
    plan.append(&plan_rows);
    let availability = Entry::builder()
        .placeholder_text("Свободное время сегодня в минутах, например 240")
        .build();
    let availability_button = Button::with_label("Сохранить свободное время");
    {
        let entry = availability.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        availability_button.connect_clicked(move |_| {
            if let Ok(minutes) = entry.text().parse::<i64>() {
                let start = Utc::now();
                if let Err(e) = runtime.block_on(add_availability(
                    &pool,
                    start,
                    start + chrono::Duration::minutes(minutes),
                )) {
                    eprintln!("availability: {e}");
                }
            }
        });
    }
    plan.append(&availability);
    plan.append(&availability_button);
    plan.append(&Button::with_label("Перестроить план"));
    let world = GtkBox::new(Orientation::Vertical, 10);
    world.add_css_class("card");
    world.append(&capture_bar);
    world.append(
        &ScrolledWindow::builder()
            .vexpand(true)
            .child(&world_list)
            .build(),
    );
    let detail = GtkBox::new(Orientation::Vertical, 10);
    detail.add_css_class("card");
    detail.append(&Label::new(Some("КАРТОЧКА ОБЪЕКТА")));
    let detail_text = Label::new(Some(
        "Поля · контекст · занятия · решения · права изменений · заметки AI",
    ));
    detail_text.set_wrap(true);
    detail.append(&detail_text);
    detail.append(&Button::with_label("Открыть выбранный объект"));
    let status_actions = GtkBox::new(Orientation::Horizontal, 6);
    for status in ["active", "paused", "done", "archived"] {
        let button = Button::with_label(status);
        let status = status.to_string();
        let pool = pool.clone();
        let runtime = runtime.clone();
        let list = world_list.clone();
        let history = history_list.clone();
        button.connect_clicked(move |_| {
            if let Some(row) = list.selected_row() {
                if let Some(label) = row.child().and_then(|w| w.downcast::<Label>().ok()) {
                    let text = label.text().to_string();
                    if let Ok(items) = runtime.block_on(load_items(&pool)) {
                        if let Some(item) = items.iter().find(|item| text.contains(&item.title)) {
                            if let Err(e) =
                                runtime.block_on(update_item_status(&pool, item.id, &status))
                            {
                                eprintln!("status: {e}");
                            }
                        }
                    }
                }
            }
            let _ = &history;
        });
        status_actions.append(&button);
    }
    detail.append(&status_actions);
    {
        let detail_text = detail_text.clone();
        world_list.connect_row_activated(move |_, row| { if let Some(label) = row.child().and_then(|w| w.downcast::<Label>().ok()) { detail_text.set_text(&format!("Selected Item\n\n{}\n\nOverview · custom fields · context · sessions · decisions · permissions · AI notes", label.text())); } });
    }
    {
        let pool = pool.clone();
        let runtime = runtime.clone();
        let start_button = start_button.clone();
        start_button.connect_clicked(move |_| {
            if let Ok(items) = runtime.block_on(load_items(&pool)) {
                if let Some(item) = items.first() {
                    let now = Utc::now();
                    let session = Session {
                        id: uuid::Uuid::new_v4(),
                        item_id: item.id,
                        planned_start: Some(now),
                        planned_end: Some(now + chrono::Duration::minutes(60)),
                        actual_start: Some(now),
                        actual_end: None,
                        status: SessionStatus::Active,
                    };
                    if let Err(e) = runtime.block_on(insert_session(&pool, &session)) {
                        eprintln!("session: {e}");
                    }
                }
            }
        });
    }
    {
        let pool = pool.clone();
        let runtime = runtime.clone();
        finish_button.connect_clicked(move |_| {
            if let Err(e) = runtime.block_on(finish_latest_session(&pool, "completed")) {
                eprintln!("session finish: {e}");
            }
        });
    }
    let input = GtkBox::new(Orientation::Vertical, 10);
    input.append(&Label::new(Some("ВВОД")));
    input.append(&Label::new(Some(
        "Например: «идея: попробовать локальный LLM router» или «магдекс неделю не ставить»",
    )));
    let directive_entry = Entry::builder()
        .placeholder_text("Укажи правило или пожелание для планировщика...")
        .hexpand(true)
        .build();
    let directive_button = Button::with_label("Сохранить пожелание");
    {
        let entry = directive_entry.clone();
        let pool = pool.clone();
        let runtime = runtime.clone();
        directive_button.connect_clicked(move |_| {
            let text = entry.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            let directive = Directive {
                id: uuid::Uuid::new_v4(),
                subject: None,
                effect: "note".into(),
                text,
                valid_from: Utc::now(),
                valid_until: None,
            };
            if let Err(e) = runtime.block_on(insert_directive(&pool, &directive)) {
                eprintln!("directive: {e}");
            }
            entry.set_text("");
        });
    }
    input.append(&directive_entry);
    input.append(&directive_button);
    let retro = GtkBox::new(Orientation::Vertical, 10);
    retro.add_css_class("card");
    retro.append(&Label::new(Some("РАЗБОР АКТИВНОСТИ")));
    if let Ok(rows) = runtime.block_on(maglife_storage::retro_summary(&pool)) {
        for (status, total, minutes) in rows {
            retro.append(&Label::new(Some(&format!(
                "{status}: {total} sessions · {minutes:.0} minutes"
            ))));
        }
    }
    retro.append(&Label::new(Some(
        "История решений и событий находится во вкладке «История».",
    )));
    let notebook = Notebook::new();
    let history = ScrolledWindow::builder()
        .vexpand(true)
        .child(&history_list)
        .build();
    notebook.append_page(&now, Some(&Label::new(Some("Сейчас"))));
    notebook.append_page(&plan, Some(&Label::new(Some("План"))));
    notebook.append_page(&world, Some(&Label::new(Some("Мир"))));
    notebook.append_page(&detail, Some(&Label::new(Some("Карточка"))));
    notebook.append_page(&history, Some(&Label::new(Some("История"))));
    notebook.append_page(&input, Some(&Label::new(Some("Ввод"))));
    notebook.append_page(&retro, Some(&Label::new(Some("Разбор"))));
    let root = GtkBox::new(Orientation::Vertical, 10);
    root.add_css_class("app-shell");
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let app_title = Label::new(Some("Maglife"));
    app_title.add_css_class("top-title");
    root.append(&app_title);
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
