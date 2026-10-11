use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc};
use gtk4::{
    AlertDialog, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, DropDown,
    Entry, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow, Stack, StackSwitcher, gdk,
    glib, prelude::*,
};
use maglife_domain::{DayPlanState, FactEntry, FactStatus, PlanBlock, PlanDraft};
use maglife_storage::{
    StorageError, approve_day, connect, create_plan_block, create_template_from_day, delete_fact,
    delete_plan_block, edit_fact, edit_plan_block, ensure_day, finish_fact, load_facts,
    load_plan_blocks, migrate, reset_database, skip_plan_block, start_fact,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::runtime::Runtime;
use uuid::Uuid;

const APP_ID: &str = "dev.maglife.Maglife";
const DATE_FMT: &str = "%Y-%m-%d %H:%M";

thread_local! { static REFRESH_BUS: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None); }

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
fn parse_local(text: &str) -> Result<DateTime<Utc>, String> {
    NaiveDateTime::parse_from_str(text.trim(), DATE_FMT)
        .ok()
        .and_then(|v| Local.from_local_datetime(&v).single())
        .map(|v| v.with_timezone(&Utc))
        .ok_or_else(|| format!("Введите дату и время в формате {DATE_FMT}."))
}
fn show_time(value: Option<DateTime<Utc>>) -> String {
    value
        .map(|v| v.with_timezone(&Local).format(DATE_FMT).to_string())
        .unwrap_or_default()
}
fn clear(list: &ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
}
fn error_label() -> Label {
    let label = Label::new(None);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.add_css_class("muted");
    label
}
fn message(label: &Label, value: impl Into<String>) {
    label.set_text(&value.into());
}
fn refresh_all(local: &Rc<dyn Fn()>) {
    REFRESH_BUS.with(|bus| {
        if let Some(refresh) = bus.borrow().as_ref() {
            refresh();
        } else {
            local();
        }
    });
}
fn plan_text(block: &PlanBlock) -> String {
    format!(
        "{} — {}\n{}",
        block.planned_start.with_timezone(&Local).format("%H:%M"),
        block.planned_end.with_timezone(&Local).format("%H:%M"),
        block.title
    )
}
fn fact_text(fact: &FactEntry) -> String {
    format!(
        "{} — {}\n{} · {}",
        fact.actual_start
            .map(|v| v.with_timezone(&Local).format("%H:%M").to_string())
            .unwrap_or_else(|| "—".into()),
        fact.actual_end
            .map(|v| v.with_timezone(&Local).format("%H:%M").to_string())
            .unwrap_or_else(|| "сейчас".into()),
        fact.title,
        fact.status.label()
    )
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_data(r#"
      window { background: #f4f6fb; }
      .page { padding: 22px; }
      .title { font-size: 28px; font-weight: 800; color: #182238; }
      .section { font-size: 17px; font-weight: 750; color: #182238; }
      .subtitle, .muted { color: #707b91; font-size: 14px; }
      .card, .timeline { background: #fff; border: 1px solid #e1e6f0; border-radius: 16px; padding: 16px; }
      .block-row { padding: 12px 14px; border-bottom: 1px solid #edf0f5; }
      .block-row:hover { background: #f7f8fc; }
      .block-title { color: #182238; font-size: 15px; font-weight: 700; }
      .time { color: #65708a; font-size: 13px; min-width: 118px; }
      .pill { border-radius: 999px; padding: 4px 9px; font-size: 12px; }
      .draft { background: #fff2d9; color: #8a5a12; }
      .approved { background: #e4f6ed; color: #21764b; }
      .running { background: #e7edff; color: #3b55b5; }
      .match { background: #e9f8ef; }
      .shift { background: #fff6db; }
      .missed { background: #ffe9eb; }
      .extra { background: #eee9ff; }
      .primary { background: #5268d9; color: #fff; font-weight: 700; }
      button { min-height: 34px; border-radius: 9px; }
      button:hover { background: #e9edff; }
      button:active { background: #cbd5ff; }
      button.primary:hover { background: #4056c3; }
      button.primary:active { background: #3045a9; }
      button:disabled { opacity: .45; }
      .timeline row:selected { background-color: #edf1ff; color: #182238; }
      .timeline row:selected:hover { background-color: #e4eaff; }
      entry, combobox { min-height: 36px; }
    "#);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    install_css();
    let runtime = Arc::new(Runtime::new().expect("tokio runtime"));
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgresql:///maglife".into());
    let pool = match runtime.block_on(async {
        let p = connect(&url).await?;
        migrate(&p).await?;
        Ok::<_, StorageError>(p)
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
    let (now, now_refresh) = build_now_page(state.clone());
    stack.add_titled(&now, Some("now"), "Сейчас");
    let (history, history_refresh) = build_history_page(state.clone());
    stack.add_titled(&history, Some("history"), "История");
    let all: Rc<dyn Fn()> = {
        let a = plan_refresh.clone();
        let b = now_refresh.clone();
        let c = history_refresh.clone();
        Rc::new(move || {
            a();
            b();
            c();
        })
    };
    REFRESH_BUS.with(|bus| *bus.borrow_mut() = Some(all.clone()));
    all();
    let tick = all.clone();
    glib::timeout_add_seconds_local(30, move || {
        tick();
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
        .default_width(1120)
        .default_height(780)
        .child(&root)
        .build()
        .present();
}

fn toolbar(day: &Rc<RefCell<NaiveDate>>, refresh: &Rc<dyn Fn()>, label: &Label) -> GtkBox {
    let bar = GtkBox::new(Orientation::Horizontal, 8);
    let prev = Button::with_label("‹");
    let next = Button::with_label("›");
    let today = Button::with_label("Сегодня");
    bar.append(&prev);
    bar.append(label);
    bar.append(&next);
    bar.append(&today);
    {
        let day = day.clone();
        let refresh = refresh.clone();
        prev.connect_clicked(move |_| {
            *day.borrow_mut() -= Duration::days(1);
            refresh_all(&refresh);
        });
    }
    {
        let day = day.clone();
        let refresh = refresh.clone();
        next.connect_clicked(move |_| {
            *day.borrow_mut() += Duration::days(1);
            refresh_all(&refresh);
        });
    }
    {
        let day = day.clone();
        let refresh = refresh.clone();
        today.connect_clicked(move |_| {
            *day.borrow_mut() = local_now().date_naive();
            refresh_all(&refresh);
        });
    }
    bar
}

fn plan_row(block: &PlanBlock) -> ListBoxRow {
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
    let category = Label::new(Some(block.category.as_deref().unwrap_or("Без категории")));
    category.set_xalign(0.0);
    category.add_css_class("muted");
    text.append(&category);
    line.append(&text);
    let source = Label::new(Some(
        if matches!(block.source, maglife_domain::PlanSource::Template) {
            "шаблон"
        } else {
            "ручной"
        },
    ));
    source.add_css_class("pill");
    line.append(&source);
    row.set_child(Some(&line));
    row
}

fn build_plan_page(state: UiState) -> (GtkBox, Rc<dyn Fn()>) {
    let page = GtkBox::new(Orientation::Vertical, 14);
    page.set_margin_top(18);
    let heading = Label::new(None);
    heading.add_css_class("title");
    heading.set_xalign(0.0);
    let plan_refresh: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let heading = heading.clone();
        move || {
            let day = *state.day.borrow();
            heading.set_text(&format!("План · {}", day.format("%A, %d %B %Y")));
        }
    });
    page.append(&toolbar(&state.day, &plan_refresh, &heading));
    let status = Label::new(None);
    status.add_css_class("pill");
    let approve = Button::with_label("Утвердить план");
    approve.add_css_class("primary");
    let template_name = Entry::builder()
        .placeholder_text("Название шаблона")
        .build();
    let save_template = Button::with_label("Сохранить день как шаблон");
    let reset = Button::with_label("Очистить тестовую базу");
    let actions = GtkBox::new(Orientation::Horizontal, 8);
    actions.append(&status);
    actions.append(&approve);
    actions.append(&template_name);
    actions.append(&save_template);
    actions.append(&reset);
    page.append(&actions);
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
    editor.set_width_request(315);
    editor.add_css_class("card");
    let editor_title = Label::new(Some("Новый плановый блок"));
    editor_title.add_css_class("section");
    editor_title.set_xalign(0.0);
    editor.append(&editor_title);
    let title = Entry::builder().placeholder_text("Название").build();
    let category = Entry::builder()
        .placeholder_text("Категория (необязательно)")
        .build();
    let start = Entry::builder().placeholder_text(DATE_FMT).build();
    let end = Entry::builder().placeholder_text(DATE_FMT).build();
    for (caption, widget) in [
        ("Название", title.clone().upcast::<gtk4::Widget>()),
        ("Категория", category.clone().upcast()),
        ("Начало", start.clone().upcast()),
        ("Конец", end.clone().upcast()),
    ] {
        let label = Label::new(Some(caption));
        label.set_xalign(0.0);
        label.add_css_class("muted");
        editor.append(&label);
        editor.append(&widget);
    }
    let save = Button::with_label("Сохранить плановый блок");
    save.add_css_class("primary");
    let remove = Button::with_label("Удалить блок");
    let info = error_label();
    editor.append(&save);
    editor.append(&remove);
    editor.append(&info);
    columns.append(&editor);
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let heading = heading.clone();
        let status = status.clone();
        let approve = approve.clone();
        let save = save.clone();
        let remove = remove.clone();
        Rc::new(move || {
            let day = *state.day.borrow();
            heading.set_text(&format!("План · {}", day.format("%A, %d %B %Y")));
            clear(&list);
            match state.runtime.block_on(ensure_day(&state.pool, day)) {
                Ok(plan) => {
                    status.set_text(plan.state.label());
                    status.remove_css_class("draft");
                    status.remove_css_class("approved");
                    status.add_css_class(if plan.state == DayPlanState::Draft {
                        "draft"
                    } else {
                        "approved"
                    });
                    let editable = plan.state == DayPlanState::Draft;
                    approve.set_sensitive(editable);
                    save.set_sensitive(editable);
                    remove.set_sensitive(editable);
                    if let Ok(blocks) = state.runtime.block_on(load_plan_blocks(&state.pool, day)) {
                        for block in blocks {
                            list.append(&plan_row(&block));
                        }
                    }
                }
                Err(error) => message(&status, error.user_message()),
            }
        })
    };
    let reset_form = {
        let state = state.clone();
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        let editor_title = editor_title.clone();
        move || {
            *state.selected.borrow_mut() = None;
            editor_title.set_text("Новый плановый блок");
            title.set_text("");
            category.set_text("");
            let now = local_now();
            let base = Local
                .from_local_datetime(
                    &state
                        .day
                        .borrow()
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
        let state = state.clone();
        let list = list.clone();
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        let editor_title = editor_title.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let Some(id) = row.widget_name().as_str().parse::<Uuid>().ok() else {
                return;
            };
            *state.selected.borrow_mut() = Some(id);
            let day = *state.day.borrow();
            if let Ok(blocks) = state.runtime.block_on(load_plan_blocks(&state.pool, day)) {
                if let Some(block) = blocks.into_iter().find(|b| b.id == id) {
                    editor_title.set_text("Изменить плановый блок");
                    title.set_text(&block.title);
                    category.set_text(block.category.as_deref().unwrap_or(""));
                    start.set_text(&show_time(Some(block.planned_start)));
                    end.set_text(&show_time(Some(block.planned_end)));
                }
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let info = info.clone();
        approve.connect_clicked(move |_| {
            match state
                .runtime
                .block_on(approve_day(&state.pool, *state.day.borrow()))
            {
                Ok(()) => {
                    message(&info, "План утверждён и зафиксирован.");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let reset = reset_form.clone();
        let title = title.clone();
        let category = category.clone();
        let start = start.clone();
        let end = end.clone();
        let info = info.clone();
        save.connect_clicked(move |_| {
            let draft = match (parse_local(&start.text()), parse_local(&end.text())) {
                (Ok(s), Ok(e)) => PlanDraft {
                    title: title.text().to_string(),
                    category: Some(category.text().to_string()),
                    planned_start: s,
                    planned_end: e,
                },
                (Err(e), _) | (_, Err(e)) => {
                    message(&info, e);
                    return;
                }
            };
            let day = *state.day.borrow();
            let result = if let Some(id) = *state.selected.borrow() {
                state
                    .runtime
                    .block_on(load_plan_blocks(&state.pool, day))
                    .ok()
                    .and_then(|v| v.into_iter().find(|b| b.id == id))
                    .map_or_else(
                        || Err(StorageError::Conflict),
                        |b| {
                            state
                                .runtime
                                .block_on(edit_plan_block(&state.pool, &b, draft))
                        },
                    )
            } else {
                state
                    .runtime
                    .block_on(create_plan_block(&state.pool, day, draft))
            };
            match result {
                Ok(()) => {
                    reset();
                    message(&info, "Сохранено");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let info = info.clone();
        remove.connect_clicked(move |_| {
            let day = *state.day.borrow();
            let result = state
                .runtime
                .block_on(load_plan_blocks(&state.pool, day))
                .ok()
                .and_then(|v| {
                    v.into_iter()
                        .find(|b| Some(b.id) == *state.selected.borrow())
                })
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |b| state.runtime.block_on(delete_plan_block(&state.pool, &b)),
                );
            match result {
                Ok(()) => {
                    *state.selected.borrow_mut() = None;
                    message(&info, "Удалено");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let template_name = template_name.clone();
        let info = info.clone();
        save_template.connect_clicked(move |_| {
            let name = template_name.text().trim().to_string();
            match state.runtime.block_on(create_template_from_day(
                &state.pool,
                *state.day.borrow(),
                maglife_domain::TemplateDraft {
                    name,
                    is_default: true,
                },
            )) {
                Ok(()) => message(&info, "Шаблон сохранён и выбран по умолчанию."),
                Err(error) => message(&info, error.user_message()),
            };
            refresh_all(&refresh);
        });
    }
    {
        let state = state.clone();
        let info = info.clone();
        let refresh = refresh.clone();
        reset.connect_clicked(move |_| {
            let dialog = AlertDialog::builder()
                .modal(true)
                .message("Очистить тестовую базу?")
                .detail("Все планы, факты и шаблоны будут удалены без возможности восстановления.")
                .buttons(["Отмена", "Очистить"])
                .cancel_button(0)
                .default_button(1)
                .build();
            let state = state.clone();
            let info = info.clone();
            let refresh = refresh.clone();
            dialog.choose(
                None::<&gtk4::Window>,
                None::<&gtk4::gio::Cancellable>,
                move |result| {
                    if matches!(result, Ok(1)) {
                        match state.runtime.block_on(reset_database(&state.pool)) {
                            Ok(()) => {
                                message(&info, "Тестовая база очищена.");
                                refresh_all(&refresh);
                            }
                            Err(error) => message(&info, error.user_message()),
                        }
                    }
                },
            );
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
        "План фиксирован, а эта вкладка записывает то, что происходит на самом деле.",
    ));
    intro.add_css_class("subtitle");
    intro.set_xalign(0.0);
    page.append(&intro);
    let current = GtkBox::new(Orientation::Vertical, 8);
    current.add_css_class("card");
    let current_label = Label::new(Some("Текущий плановый блок"));
    current_label.add_css_class("section");
    current_label.set_xalign(0.0);
    let current_text = Label::new(Some("Загрузка…"));
    current_text.set_xalign(0.0);
    current_text.set_wrap(true);
    current.append(&current_label);
    current.append(&current_text);
    let note = Entry::builder()
        .placeholder_text("Причина или заметка (необязательно)")
        .build();
    current.append(&note);
    let actions = GtkBox::new(Orientation::Horizontal, 8);
    let start = Button::with_label("Начать");
    start.add_css_class("primary");
    let finish = Button::with_label("Завершить");
    let skip = Button::with_label("Пропустить");
    let start_other = Button::with_label("Начать другое");
    actions.append(&start);
    actions.append(&finish);
    actions.append(&skip);
    actions.append(&start_other);
    current.append(&actions);
    page.append(&current);
    let other_title = Entry::builder()
        .placeholder_text("Что делаешь вне плана?")
        .build();
    page.append(&other_title);
    let next = GtkBox::new(Orientation::Vertical, 8);
    next.add_css_class("card");
    let next_label = Label::new(Some("Следующий плановый блок"));
    next_label.add_css_class("section");
    next_label.set_xalign(0.0);
    let next_text = Label::new(Some("—"));
    next_text.set_xalign(0.0);
    next.append(&next_label);
    next.append(&next_text);
    page.append(&next);
    let info = error_label();
    page.append(&info);
    let running_id = Rc::new(RefCell::new(None));
    let current_plan_id = Rc::new(RefCell::new(None));
    let start_other_for_refresh = start_other.clone();
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let clock = clock.clone();
        let current_text = current_text.clone();
        let next_text = next_text.clone();
        let running_id = running_id.clone();
        let current_plan_id = current_plan_id.clone();
        let start = start.clone();
        let finish = finish.clone();
        let skip = skip.clone();
        Rc::new(move || {
            let now = Utc::now();
            clock.set_text(&format!(
                "{}  ·  {}",
                now.with_timezone(&Local).format("%H:%M:%S"),
                now.with_timezone(&Local).format("%A, %d %B")
            ));
            let day = local_now().date_naive();
            let plans = state
                .runtime
                .block_on(load_plan_blocks(&state.pool, day))
                .unwrap_or_default();
            let facts = state
                .runtime
                .block_on(load_facts(&state.pool, day))
                .unwrap_or_default();
            let running = facts.iter().find(|f| f.is_running());
            *running_id.borrow_mut() = running.map(|f| f.id);
            let active_plan = plans.iter().find(|b| {
                b.planned_start <= now
                    && now < b.planned_end
                    && facts.iter().all(|f| f.plan_block_id != Some(b.id))
            });
            *current_plan_id.borrow_mut() = active_plan.map(|b| b.id);
            if let Some(fact) = running {
                current_text.set_text(&format!("Факт: {}\n{}", fact.title, fact_text(fact)));
            } else if let Some(block) = active_plan {
                current_text.set_text(&format!("{}\nПлан: {}", block.title, plan_text(block)));
            } else {
                current_text.set_text("Сейчас нет незаписанного планового блока.");
            }
            start.set_sensitive(running.is_none() && active_plan.is_some());
            finish.set_sensitive(running.is_some());
            skip.set_sensitive(running.is_none() && active_plan.is_some());
            start_other_for_refresh.set_sensitive(running.is_none());
            let upcoming = plans
                .iter()
                .filter(|b| {
                    b.planned_start > now && facts.iter().all(|f| f.plan_block_id != Some(b.id))
                })
                .min_by_key(|b| b.planned_start);
            next_text.set_text(
                &upcoming
                    .map(plan_text)
                    .unwrap_or_else(|| "Больше ничего не запланировано.".into()),
            );
        })
    };
    {
        let clock = clock.clone();
        glib::timeout_add_seconds_local(1, move || {
            let now = Utc::now();
            clock.set_text(&format!(
                "{}  ·  {}",
                now.with_timezone(&Local).format("%H:%M:%S"),
                now.with_timezone(&Local).format("%A, %d %B")
            ));
            glib::ControlFlow::Continue
        });
    }
    for (button, action) in [
        (start.clone(), "start"),
        (finish.clone(), "finish"),
        (skip.clone(), "skip"),
    ] {
        let state = state.clone();
        let refresh = refresh.clone();
        let note = note.clone();
        let info = info.clone();
        let running_id = running_id.clone();
        let current_plan_id = current_plan_id.clone();
        button.connect_clicked(move |_| {
            let day = local_now().date_naive();
            let result = if action == "start" {
                let id = *current_plan_id.borrow();
                let title = state
                    .runtime
                    .block_on(load_plan_blocks(&state.pool, day))
                    .ok()
                    .and_then(|v| v.into_iter().find(|b| Some(b.id) == id))
                    .map(|b| b.title);
                title.map_or_else(
                    || Err(StorageError::Conflict),
                    |title| {
                        state.runtime.block_on(start_fact(
                            &state.pool,
                            day,
                            id,
                            title,
                            Some(note.text().to_string()),
                        ))
                    },
                )
            } else if action == "finish" {
                let id = *running_id.borrow();
                state
                    .runtime
                    .block_on(load_facts(&state.pool, day))
                    .ok()
                    .and_then(|v| v.into_iter().find(|f| Some(f.id) == id))
                    .map_or_else(
                        || Err(StorageError::Conflict),
                        |fact| {
                            state.runtime.block_on(finish_fact(
                                &state.pool,
                                &fact,
                                Some(note.text().to_string()),
                            ))
                        },
                    )
            } else {
                current_plan_id.borrow().map_or_else(
                    || Err(StorageError::Conflict),
                    |id| {
                        state.runtime.block_on(skip_plan_block(
                            &state.pool,
                            day,
                            id,
                            Some(note.text().to_string()),
                        ))
                    },
                )
            };
            match result {
                Ok(()) => {
                    note.set_text("");
                    message(&info, "Факт сохранён");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh.clone();
        let title = other_title.clone();
        let note = note.clone();
        let info = info.clone();
        start_other.connect_clicked(move |_| {
            let value = title.text().trim().to_string();
            if value.is_empty() {
                message(&info, "Введите, что происходит вне плана.");
                return;
            }
            match state.runtime.block_on(start_fact(
                &state.pool,
                local_now().date_naive(),
                None,
                value,
                Some(note.text().to_string()),
            )) {
                Ok(()) => {
                    title.set_text("");
                    note.set_text("");
                    message(&info, "Факт вне плана начат");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    (page, refresh)
}

fn diff_row(plan: Option<&PlanBlock>, fact: Option<&FactEntry>, now: DateTime<Utc>) -> ListBoxRow {
    let row = ListBoxRow::new();
    row.add_css_class("block-row");
    let line = GtkBox::new(Orientation::Horizontal, 12);
    let left = Label::new(Some(
        &plan
            .map(plan_text)
            .unwrap_or_else(|| "— вне плана —".into()),
    ));
    left.set_xalign(0.0);
    left.set_hexpand(true);
    let arrow = Label::new(Some("→"));
    arrow.add_css_class("muted");
    let right = Label::new(Some(
        &fact
            .map(fact_text)
            .unwrap_or_else(|| "— нет факта —".into()),
    ));
    right.set_xalign(0.0);
    right.set_hexpand(true);
    line.append(&left);
    line.append(&arrow);
    line.append(&right);
    if plan.is_none() {
        row.add_css_class("extra");
    } else if fact.is_none() {
        if plan.is_some_and(|b| b.planned_end <= now) {
            row.add_css_class("missed");
        }
    } else if let (Some(p), Some(f)) = (plan, fact) {
        if f.status == FactStatus::Skipped {
            row.add_css_class("missed");
        } else if f
            .actual_start
            .is_some_and(|actual| (actual - p.planned_start).num_minutes().abs() <= 5)
            && f.actual_end
                .is_some_and(|end| (end - p.planned_end).num_minutes().abs() <= 5)
        {
            row.add_css_class("match");
        } else {
            row.add_css_class("shift");
        }
    }
    row
}

fn build_history_page(state: UiState) -> (GtkBox, Rc<dyn Fn()>) {
    let page = GtkBox::new(Orientation::Vertical, 14);
    page.set_margin_top(18);
    let heading = Label::new(None);
    heading.add_css_class("title");
    heading.set_xalign(0.0);
    let refresh: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let heading = heading.clone();
        move || {
            heading.set_text(&format!(
                "История · {}",
                state.day.borrow().format("%A, %d %B %Y")
            ))
        }
    });
    page.append(&toolbar(&state.day, &refresh, &heading));
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
        "История сравнивает утверждённый план с фактом. Факт можно исправить вручную.",
    ));
    help.set_wrap(true);
    help.set_xalign(0.0);
    help.add_css_class("subtitle");
    editor.append(&help);
    let status = DropDown::from_strings(&["В процессе", "Завершено", "Пропущено"]);
    let fact_title = Entry::builder()
        .placeholder_text("Что было фактически")
        .build();
    let actual_start = Entry::builder().placeholder_text(DATE_FMT).build();
    let actual_end = Entry::builder().placeholder_text(DATE_FMT).build();
    let note = Entry::builder()
        .placeholder_text("Причина / заметка")
        .build();
    for (caption, widget) in [
        ("Статус", status.clone().upcast::<gtk4::Widget>()),
        ("Факт", fact_title.clone().upcast()),
        ("Фактическое начало", actual_start.clone().upcast()),
        ("Фактический конец", actual_end.clone().upcast()),
        ("Заметка", note.clone().upcast()),
    ] {
        let label = Label::new(Some(caption));
        label.add_css_class("muted");
        label.set_xalign(0.0);
        editor.append(&label);
        editor.append(&widget);
    }
    let save = Button::with_label("Сохранить исправление");
    save.add_css_class("primary");
    let remove = Button::with_label("Удалить ошибочный факт");
    let info = error_label();
    editor.append(&save);
    editor.append(&remove);
    editor.append(&info);
    columns.append(&editor);
    let refresh_list: Rc<dyn Fn()> = {
        let state = state.clone();
        let list = list.clone();
        let heading = heading.clone();
        Rc::new(move || {
            let day = *state.day.borrow();
            heading.set_text(&format!("История · {}", day.format("%A, %d %B %Y")));
            clear(&list);
            let plans = state
                .runtime
                .block_on(load_plan_blocks(&state.pool, day))
                .unwrap_or_default();
            let facts = state
                .runtime
                .block_on(load_facts(&state.pool, day))
                .unwrap_or_default();
            let now = Utc::now();
            for plan in &plans {
                let fact = facts.iter().find(|f| f.plan_block_id == Some(plan.id));
                let row = diff_row(Some(plan), fact, now);
                row.set_widget_name(&fact.map(|f| f.id).unwrap_or(plan.id).to_string());
                list.append(&row);
            }
            for fact in facts.iter().filter(|f| f.plan_block_id.is_none()) {
                let row = diff_row(None, Some(fact), now);
                row.set_widget_name(&fact.id.to_string());
                list.append(&row);
            }
        })
    };
    {
        let state = state.clone();
        let status = status.clone();
        let fact_title = fact_title.clone();
        let actual_start = actual_start.clone();
        let actual_end = actual_end.clone();
        let note = note.clone();
        list.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let Some(id) = row.widget_name().as_str().parse::<Uuid>().ok() else {
                return;
            };
            let facts = state
                .runtime
                .block_on(load_facts(&state.pool, *state.day.borrow()))
                .unwrap_or_default();
            if let Some(fact) = facts.into_iter().find(|f| f.id == id) {
                *state.selected.borrow_mut() = Some(fact.id);
                status.set_selected(match fact.status {
                    FactStatus::Running => 0,
                    FactStatus::Completed => 1,
                    FactStatus::Skipped => 2,
                });
                fact_title.set_text(&fact.title);
                actual_start.set_text(&show_time(fact.actual_start));
                actual_end.set_text(&show_time(fact.actual_end));
                note.set_text(fact.note.as_deref().unwrap_or(""));
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh_list.clone();
        let status = status.clone();
        let fact_title = fact_title.clone();
        let actual_start = actual_start.clone();
        let actual_end = actual_end.clone();
        let note = note.clone();
        let info = info.clone();
        save.connect_clicked(move |_| {
            let Some(id) = *state.selected.borrow() else {
                message(&info, "Выберите факт справа от плана.");
                return;
            };
            let day = *state.day.borrow();
            let result = state
                .runtime
                .block_on(load_facts(&state.pool, day))
                .ok()
                .and_then(|v| v.into_iter().find(|f| f.id == id))
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |fact| {
                        let fact_status = match status.selected() {
                            0 => FactStatus::Running,
                            2 => FactStatus::Skipped,
                            _ => FactStatus::Completed,
                        };
                        let (start, end) = if fact_status == FactStatus::Skipped {
                            (None, None)
                        } else {
                            let start = match parse_local(&actual_start.text()) {
                                Ok(v) => v,
                                Err(e) => return Err(StorageError::Validation(e)),
                            };
                            let end = if fact_status == FactStatus::Running {
                                None
                            } else {
                                match parse_local(&actual_end.text()) {
                                    Ok(v) => Some(v),
                                    Err(e) => return Err(StorageError::Validation(e)),
                                }
                            };
                            (Some(start), end)
                        };
                        let draft = maglife_domain::FactDraft {
                            title: fact_title.text().to_string(),
                            plan_block_id: fact.plan_block_id,
                            actual_start: start,
                            actual_end: end,
                            status: fact_status,
                            note: Some(note.text().to_string()),
                        };
                        state.runtime.block_on(edit_fact(&state.pool, &fact, draft))
                    },
                );
            match result {
                Ok(()) => {
                    message(&info, "Факт исправлен");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    {
        let state = state.clone();
        let refresh = refresh_list.clone();
        let info = info.clone();
        remove.connect_clicked(move |_| {
            let Some(id) = *state.selected.borrow() else {
                message(&info, "Выберите факт.");
                return;
            };
            let day = *state.day.borrow();
            let result = state
                .runtime
                .block_on(load_facts(&state.pool, day))
                .ok()
                .and_then(|v| v.into_iter().find(|f| f.id == id))
                .map_or_else(
                    || Err(StorageError::Conflict),
                    |fact| state.runtime.block_on(delete_fact(&state.pool, &fact)),
                );
            match result {
                Ok(()) => {
                    *state.selected.borrow_mut() = None;
                    message(&info, "Факт удалён");
                    refresh_all(&refresh);
                }
                Err(error) => message(&info, error.user_message()),
            }
        });
    }
    (page, refresh_list)
}
