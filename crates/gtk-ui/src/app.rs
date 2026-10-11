use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, CssProvider, Entry, Label,
    ListBox, ListBoxRow, Orientation, ScrolledWindow, Stack, StackSwitcher, gdk, glib, prelude::*,
};
use maglife_domain::{ActivityBlock, BlockDraft, BlockStatus, HistoryEdit};
use maglife_storage::{
    StorageError, connect, create_block, delete_block, edit_block, edit_history, finish_block,
    load_day, load_now, migrate, skip_block, start_block,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::runtime::Runtime;
use uuid::Uuid;

const APP_ID: &str = "dev.maglife.Maglife";
const DATE_FMT: &str = "%Y-%m-%d %H:%M";

#[derive(Clone)]
struct UiState {
    pool: sqlx::PgPool,
    runtime: Arc<Runtime>,
    day: Rc<RefCell<NaiveDate>>,
    selected: Rc<RefCell<Option<Uuid>>>,
}

fn local_now() -> DateTime<Local> {
    Local::now()
}
fn local_to_utc(value: NaiveDateTime) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&value)
        .single()
        .map(|x| x.with_timezone(&Utc))
}
fn parse_local(text: &str) -> Result<DateTime<Utc>, String> {
    NaiveDateTime::parse_from_str(text.trim(), DATE_FMT)
        .ok()
        .and_then(local_to_utc)
        .ok_or_else(|| format!("Введите дату и время в формате {DATE_FMT}."))
}
fn show_time(time: Option<DateTime<Utc>>) -> String {
    time.map(|x| x.with_timezone(&Local).format(DATE_FMT).to_string())
        .unwrap_or_default()
}
fn day_bounds(day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = Local
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .unwrap()
        .with_timezone(&Utc);
    (start, start + Duration::days(1))
}

fn install_css() {
    let css = CssProvider::new();
    css.load_from_data(
        r#"
      window { background: #f4f6fb; }
      .page { padding: 22px; }
      .title { font-size: 28px; font-weight: 800; color: #182238; }
      .subtitle { color: #6b7487; font-size: 14px; }
      .card { background: #ffffff; border: 1px solid #e3e7f0; border-radius: 16px; padding: 16px; }
      .timeline { background: #ffffff; border: 1px solid #e3e7f0; border-radius: 16px; }
      .block-row { padding: 12px 14px; border-bottom: 1px solid #edf0f5; }
      .block-row:hover { background: #f7f8fc; }
      .time { color: #65708a; font-size: 13px; min-width: 118px; }
      .block-title { color: #182238; font-size: 15px; font-weight: 700; }
      .muted { color: #7a8498; }
      .pill { border-radius: 999px; padding: 4px 9px; font-size: 12px; }
      .active { background: #e7edff; color: #3b55b5; }
      .completed { background: #e4f6ed; color: #21764b; }
      .skipped { background: #fff0e8; color: #a14d26; }
      .missed { background: #ffe9eb; color: #a63843; }
      .primary { background: #5268d9; color: white; font-weight: 700; }
      entry, combobox { min-height: 36px; }
      button { min-height: 34px; }
    "#,
    );
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn clear(list: &ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
}
fn status_class(block: &ActivityBlock, now: DateTime<Utc>) -> &'static str {
    if block.is_missed(now) {
        "missed"
    } else {
        match block.status {
            BlockStatus::Active => "active",
            BlockStatus::Completed => "completed",
            BlockStatus::Skipped => "skipped",
            BlockStatus::Planned => "",
        }
    }
}
fn status_label(block: &ActivityBlock, now: DateTime<Utc>) -> &'static str {
    if block.is_missed(now) {
        "Не начато"
    } else {
        block.status.label()
    }
}
fn block_row(block: &ActivityBlock, now: DateTime<Utc>, history: bool) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.set_widget_name(&block.id.to_string());
    row.add_css_class("block-row");
    let line = GtkBox::new(Orientation::Horizontal, 12);
    let time = Label::new(Some(&format!(
        "{} — {}",
        block.planned_start.with_timezone(&Local).format("%H:%M"),
        block.planned_end.with_timezone(&Local).format("%H:%M")
    )));
    time.add_css_class("time");
    line.append(&time);
    let text = GtkBox::new(Orientation::Vertical, 3);
    let title = Label::new(Some(&block.title));
    title.set_xalign(0.0);
    title.add_css_class("block-title");
    text.append(&title);
    let details = if history {
        format!(
            "План  {} — {}   ·   Факт  {} — {}",
            show_time(Some(block.planned_start)),
            show_time(Some(block.planned_end)),
            show_time(block.actual_start),
            show_time(block.actual_end)
        )
    } else {
        block
            .category
            .clone()
            .unwrap_or_else(|| "Без категории".into())
    };
    let detail = Label::new(Some(&details));
    detail.set_xalign(0.0);
    detail.add_css_class("muted");
    text.append(&detail);
    line.append(&text);
    let badge = Label::new(Some(&status_label(block, now)));
    badge.add_css_class("pill");
    let class = status_class(block, now);
    if !class.is_empty() {
        badge.add_css_class(class);
    }
    line.append(&badge);
    row.set_child(Some(&line));
    row
}

fn error_label() -> Label {
    let label = Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("muted");
    label
}
fn set_error(label: &Label, message: impl Into<String>) {
    label.set_text(&message.into());
}

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    install_css();
    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///maglife".into());
    let pool = match runtime.block_on(async {
        let pool = connect(&database_url).await?;
        migrate(&pool).await?;
        Ok::<_, StorageError>(pool)
    }) {
        Ok(pool) => pool,
        Err(error) => {
            let label = Label::new(Some(&format!(
                "Не удалось открыть Maglife\n\n{}",
                error.user_message()
            )));
            label.set_margin_top(40);
            label.set_margin_start(40);
            ApplicationWindow::builder()
                .application(app)
                .title("Maglife")
                .default_width(560)
                .default_height(260)
                .child(&label)
                .build()
                .present();
            return;
        }
    };
    let state = UiState {
        pool,
        runtime,
        day: Rc::new(RefCell::new(local_now().date_naive())),
        selected: Rc::new(RefCell::new(None)),
    };
    let stack = Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .transition_type(gtk4::StackTransitionType::SlideLeftRight)
        .build();
    let switcher = StackSwitcher::builder()
        .stack(&stack)
        .halign(gtk4::Align::Start)
        .build();
    let (plan, plan_refresh) = build_plan_page(state.clone());
    stack.add_titled(&plan, Some("plan"), "План");
    let (now_page, now_refresh) = build_now_page(state.clone());
    stack.add_titled(&now_page, Some("now"), "Сейчас");
    let (history, history_refresh) = build_history_page(state.clone());
    stack.add_titled(&history, Some("history"), "История");
    let refresh_all = {
        let a = plan_refresh.clone();
        let b = now_refresh.clone();
        let c = history_refresh.clone();
        Rc::new(move || {
            a();
            b();
            c();
        })
    };
    refresh_all();
    let refresh_tick = refresh_all.clone();
    glib::timeout_add_seconds_local(30, move || {
        refresh_tick();
        glib::ControlFlow::Continue
    });
    let header = GtkBox::new(Orientation::Vertical, 5);
    let title = Label::new(Some("Maglife"));
    title.set_xalign(0.0);
    title.add_css_class("title");
    header.append(&title);
    header.append(&switcher);
    let root = GtkBox::new(Orientation::Vertical, 0);
    root.add_css_class("page");
    root.append(&header);
    root.append(&stack);
    ApplicationWindow::builder()
        .application(app)
        .title("Maglife — день в твоих руках")
        .default_width(1060)
        .default_height(760)
        .child(&root)
        .build()
        .present();
}

fn build_plan_page(state: UiState) -> (GtkBox, Rc<dyn Fn()>) {
    let page = GtkBox::new(Orientation::Vertical, 14);
    page.set_margin_top(18);
    let toolbar = GtkBox::new(Orientation::Horizontal, 8);
    let previous = Button::with_label("‹");
    let next = Button::with_label("›");
    let today = Button::with_label("Сегодня");
    let date = Label::new(None);
    date.add_css_class("subtitle");
    toolbar.append(&previous);
    toolbar.append(&date);
    toolbar.append(&next);
    toolbar.append(&today);
    let add = Button::with_label("＋ Новый блок");
    add.add_css_class("primary");
    toolbar.append(&add);
    page.append(&toolbar);
    let columns = GtkBox::new(Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    page.append(&columns);
    let list = ListBox::new();
    list.add_css_class("timeline");
    list.set_vexpand(true);
    list.set_selection_mode(gtk4::SelectionMode::Single);
    let scroll = ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hexpand(true)
        .build();
    columns.append(&scroll);
    let editor = GtkBox::new(Orientation::Vertical, 10);
    editor.set_width_request(300);
    editor.add_css_class("card");
    let editor_title = Label::new(Some("Новый блок"));
    editor_title.set_xalign(0.0);
    editor_title.add_css_class("block-title");
    editor.append(&editor_title);
    let title = Entry::builder()
        .placeholder_text("Название: сон, рутина, работа…")
        .build();
    let category = Entry::builder()
        .placeholder_text("Категория (необязательно)")
        .build();
    let start = Entry::builder().placeholder_text(DATE_FMT).build();
    let end = Entry::builder().placeholder_text(DATE_FMT).build();
    for (name, entry) in [
        ("Название", &title),
        ("Категория", &category),
        ("Начало", &start),
        ("Конец", &end),
    ] {
        let l = Label::new(Some(name));
        l.set_xalign(0.0);
        l.add_css_class("muted");
        editor.append(&l);
        editor.append(entry);
    }
    let save = Button::with_label("Сохранить блок");
    save.add_css_class("primary");
    let remove = Button::with_label("Удалить выбранный");
    let message = error_label();
    editor.append(&save);
    editor.append(&remove);
    editor.append(&message);
    columns.append(&editor);
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let date = date.clone();
        Rc::new(move || {
            let day = *state.day.borrow();
            date.set_text(&day.format("%A, %d %B %Y").to_string());
            clear(&list);
            let (from, to) = day_bounds(day);
            match state.runtime.block_on(load_day(&state.pool, from, to)) {
                Ok(blocks) => {
                    let now = Utc::now();
                    for block in blocks {
                        list.append(&block_row(&block, now, false));
                    }
                }
                Err(error) => {
                    let row = ListBoxRow::new();
                    row.set_child(Some(&Label::new(Some(&error.user_message()))));
                    list.append(&row);
                }
            }
        })
    };
    let reset_form = {
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        let editor_title = editor_title.clone();
        let selected = state.selected.clone();
        let day = state.day.clone();
        move || {
            *selected.borrow_mut() = None;
            editor_title.set_text("Новый блок");
            title.set_text("");
            category.set_text("");
            let now = local_now();
            let base = Local
                .from_local_datetime(
                    &day.borrow()
                        .and_hms_opt(now.hour(), now.minute(), 0)
                        .unwrap(),
                )
                .single()
                .unwrap();
            start.set_text(&base.format(DATE_FMT).to_string());
            end.set_text(&(base + Duration::hours(1)).format(DATE_FMT).to_string());
        }
    };
    reset_form();
    {
        let refresh = refresh.clone();
        let reset = reset_form.clone();
        add.connect_clicked(move |_| {
            reset();
            refresh();
        });
    }
    {
        let state = state.clone();
        let list = list.clone();
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        let editor_title = editor_title.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let id = row.widget_name().as_str().parse::<Uuid>().ok();
            *state.selected.borrow_mut() = id;
            if let Some(id) = id {
                let (from, to) = day_bounds(*state.day.borrow());
                if let Ok(blocks) = state.runtime.block_on(load_day(&state.pool, from, to)) {
                    if let Some(block) = blocks.into_iter().find(|b| b.id == id) {
                        editor_title.set_text("Изменить блок");
                        title.set_text(&block.title);
                        category.set_text(block.category.as_deref().unwrap_or(""));
                        start.set_text(&show_time(Some(block.planned_start)));
                        end.set_text(&show_time(Some(block.planned_end)));
                    }
                }
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let message = message.clone();
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        save.connect_clicked(move |_| {
            let draft = match (parse_local(&start.text()), parse_local(&end.text())) {
                (Ok(s), Ok(e)) => BlockDraft {
                    title: title.text().to_string(),
                    category: Some(category.text().to_string()),
                    planned_start: s,
                    planned_end: e,
                },
                (Err(e), _) | (_, Err(e)) => {
                    set_error(&message, e);
                    return;
                }
            };
            let result = if let Some(id) = *state.selected.borrow() {
                let (from, to) = day_bounds(*state.day.borrow());
                state
                    .runtime
                    .block_on(load_day(&state.pool, from, to))
                    .ok()
                    .and_then(|blocks| blocks.into_iter().find(|b| b.id == id))
                    .map_or_else(
                        || Err(StorageError::Conflict),
                        |block| {
                            state
                                .runtime
                                .block_on(edit_block(&state.pool, &block, draft))
                        },
                    )
            } else {
                state.runtime.block_on(create_block(&state.pool, draft))
            };
            match result {
                Ok(()) => {
                    set_error(&message, "Сохранено");
                    refresh();
                }
                Err(error) => set_error(&message, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let message = message.clone();
        remove.connect_clicked(move |_| {
            let Some(id) = *state.selected.borrow() else {
                set_error(&message, "Выберите блок в списке.");
                return;
            };
            let (from, to) = day_bounds(*state.day.borrow());
            let result = state
                .runtime
                .block_on(load_day(&state.pool, from, to))
                .ok()
                .and_then(|blocks| blocks.into_iter().find(|b| b.id == id))
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |block| state.runtime.block_on(delete_block(&state.pool, &block)),
                );
            match result {
                Ok(()) => {
                    *state.selected.borrow_mut() = None;
                    refresh();
                }
                Err(error) => set_error(&message, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        previous.connect_clicked(move |_| {
            *state.day.borrow_mut() -= Duration::days(1);
            refresh();
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        next.connect_clicked(move |_| {
            *state.day.borrow_mut() += Duration::days(1);
            refresh();
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        today.connect_clicked(move |_| {
            *state.day.borrow_mut() = local_now().date_naive();
            refresh();
        });
    }
    (page, refresh)
}

fn build_now_page(state: UiState) -> (GtkBox, Rc<dyn Fn()>) {
    let page = GtkBox::new(Orientation::Vertical, 14);
    page.set_margin_top(18);
    let clock = Label::new(None);
    clock.add_css_class("title");
    page.append(&clock);
    let intro = Label::new(Some(
        "Ручной контроль текущего момента. План ничего не запускает сам.",
    ));
    intro.add_css_class("subtitle");
    intro.set_xalign(0.0);
    page.append(&intro);
    let current = GtkBox::new(Orientation::Vertical, 7);
    current.add_css_class("card");
    let current_title = Label::new(Some("Сейчас"));
    current_title.add_css_class("block-title");
    current_title.set_xalign(0.0);
    let current_text = Label::new(Some("Загрузка…"));
    current_text.set_xalign(0.0);
    current_text.set_wrap(true);
    current.append(&current_title);
    current.append(&current_text);
    let actions = GtkBox::new(Orientation::Horizontal, 8);
    let start = Button::with_label("Начать");
    start.add_css_class("primary");
    let finish = Button::with_label("Завершить");
    let skip = Button::with_label("Пропустить");
    actions.append(&start);
    actions.append(&finish);
    actions.append(&skip);
    current.append(&actions);
    page.append(&current);
    let next_card = GtkBox::new(Orientation::Vertical, 7);
    next_card.add_css_class("card");
    let next_title = Label::new(Some("Следующий блок"));
    next_title.add_css_class("block-title");
    next_title.set_xalign(0.0);
    let next_text = Label::new(Some("—"));
    next_text.set_xalign(0.0);
    next_card.append(&next_title);
    next_card.append(&next_text);
    page.append(&next_card);
    let message = error_label();
    page.append(&message);
    let current_id = Rc::new(RefCell::new(None));
    let refresh_message = message.clone();
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let clock = clock.clone();
        let current_text = current_text.clone();
        let next_text = next_text.clone();
        let current_id = current_id.clone();
        Rc::new(move || {
            let now = Utc::now();
            clock.set_text(&format!(
                "{}  ·  {}",
                now.with_timezone(&Local).format("%H:%M:%S"),
                now.with_timezone(&Local).format("%A, %d %B")
            ));
            match state.runtime.block_on(load_now(&state.pool, now)) {
                Ok(blocks) => {
                    let active = blocks
                        .iter()
                        .find(|b| b.status == BlockStatus::Active)
                        .or_else(|| blocks.iter().find(|b| b.is_scheduled_now(now)));
                    *current_id.borrow_mut() = active.map(|b| b.id);
                    current_text.set_text(active.map(|b| format!("{}\nПлан: {} — {}\nСтатус: {}", b.title, show_time(Some(b.planned_start)), show_time(Some(b.planned_end)), status_label(b, now))).unwrap_or_else(|| "Сейчас нет блока по плану. Можно открыть План и добавить его вручную.".into()).as_str());
                    let next = blocks
                        .iter()
                        .filter(|b| b.status == BlockStatus::Planned && b.planned_start > now)
                        .min_by_key(|b| b.planned_start);
                    let next_label = next
                        .map(|b| {
                            format!(
                                "{}\n{} — {}",
                                b.title,
                                show_time(Some(b.planned_start)),
                                show_time(Some(b.planned_end))
                            )
                        })
                        .unwrap_or_else(|| "На сегодня больше ничего не запланировано.".to_owned());
                    next_text.set_text(&next_label);
                }
                Err(error) => set_error(&refresh_message, error.user_message()),
            }
        })
    };
    for (button, action) in [
        (start.clone(), "start"),
        (finish.clone(), "finish"),
        (skip.clone(), "skip"),
    ] {
        let state = state.clone();
        let refresh = refresh.clone();
        let message = message.clone();
        let current_id = current_id.clone();
        button.connect_clicked(move |_| {
            let Some(id) = *current_id.borrow() else {
                set_error(&message, "Сейчас нет выбранного блока.");
                return;
            };
            let blocks = load_now(&state.pool, Utc::now());
            let result = state
                .runtime
                .block_on(blocks)
                .ok()
                .and_then(|items| items.into_iter().find(|b| b.id == id))
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |block| match action {
                        "start" => state.runtime.block_on(start_block(&state.pool, &block)),
                        "finish" => state.runtime.block_on(finish_block(&state.pool, &block)),
                        _ => state.runtime.block_on(skip_block(&state.pool, &block)),
                    },
                );
            match result {
                Ok(()) => {
                    set_error(&message, "Сохранено");
                    refresh();
                }
                Err(error) => set_error(&message, error.user_message()),
            }
        });
    }
    (page, refresh)
}

fn build_history_page(state: UiState) -> (GtkBox, Rc<dyn Fn()>) {
    let page = GtkBox::new(Orientation::Vertical, 14);
    page.set_margin_top(18);
    let heading = Label::new(Some("План против факта"));
    heading.set_xalign(0.0);
    heading.add_css_class("title");
    let history_toolbar = GtkBox::new(Orientation::Horizontal, 8);
    let previous = Button::with_label("‹");
    let next = Button::with_label("›");
    let today = Button::with_label("Сегодня");
    history_toolbar.append(&previous);
    history_toolbar.append(&heading);
    history_toolbar.append(&next);
    history_toolbar.append(&today);
    page.append(&history_toolbar);
    let columns = GtkBox::new(Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    page.append(&columns);
    let list = ListBox::new();
    list.add_css_class("timeline");
    list.set_selection_mode(gtk4::SelectionMode::Single);
    let scroll = ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hexpand(true)
        .build();
    columns.append(&scroll);
    let editor = GtkBox::new(Orientation::Vertical, 9);
    editor.set_width_request(320);
    editor.add_css_class("card");
    let help = Label::new(Some(
        "Выберите блок и при необходимости исправьте фактическое время.",
    ));
    help.set_wrap(true);
    help.set_xalign(0.0);
    help.add_css_class("subtitle");
    editor.append(&help);
    let status = ComboBoxText::new();
    for (value, label) in [
        ("planned", "Не начато"),
        ("active", "В процессе"),
        ("completed", "Завершено"),
        ("skipped", "Пропущено"),
    ] {
        status.append(Some(value), label);
    }
    let actual_start = Entry::builder().placeholder_text(DATE_FMT).build();
    let actual_end = Entry::builder().placeholder_text(DATE_FMT).build();
    for (name, widget) in [
        ("Результат", status.clone().upcast::<gtk4::Widget>()),
        ("Фактическое начало", actual_start.clone().upcast()),
        ("Фактический конец", actual_end.clone().upcast()),
    ] {
        let l = Label::new(Some(name));
        l.set_xalign(0.0);
        l.add_css_class("muted");
        editor.append(&l);
        editor.append(&widget);
    }
    let save = Button::with_label("Сохранить историю");
    save.add_css_class("primary");
    let message = error_label();
    editor.append(&save);
    editor.append(&message);
    columns.append(&editor);
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let heading = heading.clone();
        Rc::new(move || {
            let day = *state.day.borrow();
            heading.set_text(&format!("План против факта · {}", day.format("%d.%m.%Y")));
            clear(&list);
            let (from, to) = day_bounds(day);
            match state.runtime.block_on(load_day(&state.pool, from, to)) {
                Ok(blocks) => {
                    let now = Utc::now();
                    for block in blocks {
                        list.append(&block_row(&block, now, true));
                    }
                }
                Err(error) => {
                    let row = ListBoxRow::new();
                    row.set_child(Some(&Label::new(Some(&error.user_message()))));
                    list.append(&row);
                }
            }
        })
    };
    {
        let state = state.clone();
        let refresh = refresh.clone();
        previous.connect_clicked(move |_| {
            *state.day.borrow_mut() -= Duration::days(1);
            refresh();
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        next.connect_clicked(move |_| {
            *state.day.borrow_mut() += Duration::days(1);
            refresh();
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        today.connect_clicked(move |_| {
            *state.day.borrow_mut() = local_now().date_naive();
            refresh();
        });
    }
    {
        let state = state.clone();
        let status = status.clone();
        let actual_start = actual_start.clone();
        let actual_end = actual_end.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let Some(id) = row.widget_name().as_str().parse::<Uuid>().ok() else {
                return;
            };
            *state.selected.borrow_mut() = Some(id);
            let (from, to) = day_bounds(*state.day.borrow());
            if let Ok(blocks) = state.runtime.block_on(load_day(&state.pool, from, to)) {
                if let Some(b) = blocks.into_iter().find(|b| b.id == id) {
                    status.set_active_id(Some(b.status.as_str()));
                    actual_start.set_text(&show_time(b.actual_start));
                    actual_end.set_text(&show_time(b.actual_end));
                }
            }
        });
    }
    {
        let state = state.clone();
        let status = status.clone();
        let actual_start = actual_start.clone();
        let actual_end = actual_end.clone();
        let message = message.clone();
        let refresh = refresh.clone();
        save.connect_clicked(move |_| {
            let Some(id) = *state.selected.borrow() else {
                set_error(&message, "Выберите блок в истории.");
                return;
            };
            let edit = match status.active_id().as_deref() {
                Some("planned") => HistoryEdit {
                    status: BlockStatus::Planned,
                    actual_start: None,
                    actual_end: None,
                },
                Some("active") => match parse_local(&actual_start.text()) {
                    Ok(start) => HistoryEdit {
                        status: BlockStatus::Active,
                        actual_start: Some(start),
                        actual_end: None,
                    },
                    Err(e) => {
                        set_error(&message, e);
                        return;
                    }
                },
                Some("completed") => match (
                    parse_local(&actual_start.text()),
                    parse_local(&actual_end.text()),
                ) {
                    (Ok(start), Ok(end)) => HistoryEdit {
                        status: BlockStatus::Completed,
                        actual_start: Some(start),
                        actual_end: Some(end),
                    },
                    (Err(e), _) | (_, Err(e)) => {
                        set_error(&message, e);
                        return;
                    }
                },
                _ => HistoryEdit {
                    status: BlockStatus::Skipped,
                    actual_start: None,
                    actual_end: None,
                },
            };
            let (from, to) = day_bounds(*state.day.borrow());
            let result = state
                .runtime
                .block_on(load_day(&state.pool, from, to))
                .ok()
                .and_then(|blocks| blocks.into_iter().find(|b| b.id == id))
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |block| {
                        state
                            .runtime
                            .block_on(edit_history(&state.pool, &block, edit))
                    },
                );
            match result {
                Ok(()) => {
                    set_error(&message, "Сохранено");
                    refresh();
                }
                Err(error) => set_error(&message, error.user_message()),
            }
        });
    }
    (page, refresh)
}
