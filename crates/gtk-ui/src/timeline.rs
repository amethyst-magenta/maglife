use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};
use gtk4::{
    Box as GtkBox, Button, DrawingArea, Fixed, Label, Orientation, Overlay, ScrolledWindow, cairo,
    pango, prelude::*,
};
use maglife_domain::{FactEntry, FactStatus, PlanBlock};
use std::{cell::RefCell, rc::Rc};
use uuid::Uuid;

const MINUTES: f64 = 1440.0;
const SCALE: f64 = 1.5;
const TOP: f64 = 18.0;
const AXIS: f64 = 66.0;
const GUTTER: f64 = 64.0;
const INSET: f64 = 8.0;
const HEIGHT: i32 = (TOP * 2.0 + MINUTES * SCALE) as i32;

type Selection = Rc<RefCell<Option<Box<dyn Fn(Option<Uuid>)>>>>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TimelineMode {
    Plan,
    Comparison,
}

#[derive(Clone, Copy)]
enum Tone {
    Planned,
    Match,
    Shift,
    Missed,
    Extra,
}

impl Tone {
    fn class(self) -> &'static str {
        match self {
            Self::Planned => "event-planned",
            Self::Match => "event-match",
            Self::Shift => "event-shift",
            Self::Missed => "event-missed",
            Self::Extra => "event-extra",
        }
    }

    fn rgb(self) -> (f64, f64, f64) {
        match self {
            Self::Planned => (0.32, 0.41, 0.85),
            Self::Match => (0.22, 0.63, 0.44),
            Self::Shift => (0.80, 0.58, 0.18),
            Self::Missed => (0.82, 0.36, 0.43),
            Self::Extra => (0.53, 0.40, 0.79),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Span {
    start: f64,
    end: f64,
}

impl Span {
    fn new(day: NaiveDate, start: DateTime<Utc>, end: DateTime<Utc>) -> Option<Self> {
        let minute = |value: DateTime<Utc>| {
            let local = value.with_timezone(&Local);
            let days = (local.date_naive() - day).num_days() as f64;
            days * MINUTES + local.num_seconds_from_midnight() as f64 / 60.0
        };
        Self::clipped(minute(start), minute(end))
    }

    fn clipped(start: f64, end: f64) -> Option<Self> {
        if end < 0.0 || start >= MINUTES || end < start {
            return None;
        }
        Some(Self {
            start: start.max(0.0),
            end: end.min(MINUTES),
        })
    }

    fn y(self) -> f64 {
        TOP + self.start * SCALE
    }

    fn height(self) -> f64 {
        ((self.end - self.start) * SCALE).max(3.0)
    }
}

struct Event {
    id: Uuid,
    span: Span,
    side: usize,
    lane: usize,
    lanes: usize,
    button: Button,
}

struct Link {
    plan: Span,
    fact: Span,
    tone: Tone,
    id: Uuid,
}

struct Model {
    day: Option<NaiveDate>,
    events: Vec<Event>,
    links: Vec<Link>,
    selected: Option<Uuid>,
}

#[derive(Clone)]
pub struct DayTimeline {
    pub widget: GtkBox,
    mode: TimelineMode,
    area: DrawingArea,
    fixed: Fixed,
    scroll: ScrolledWindow,
    model: Rc<RefCell<Model>>,
    selection: Selection,
}

impl DayTimeline {
    pub fn new(mode: TimelineMode) -> Self {
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.add_css_class("day-timeline");
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        let header = GtkBox::new(Orientation::Horizontal, 0);
        header.add_css_class("timeline-header");
        let axis = Label::new(Some("Время"));
        axis.set_width_request(AXIS as i32);
        header.append(&axis);
        let plan = Label::new(Some("ПЛАН"));
        plan.set_hexpand(true);
        header.append(&plan);
        if mode == TimelineMode::Comparison {
            let gap = Label::new(Some("↔"));
            gap.set_width_request(GUTTER as i32);
            header.append(&gap);
            let fact = Label::new(Some("ФАКТ"));
            fact.set_hexpand(true);
            header.append(&fact);
            // Keep both headings aligned to equal-width tracks.
            header.set_homogeneous(false);
            let group = gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal);
            group.add_widget(&plan);
            group.add_widget(&fact);
        }
        widget.append(&header);
        let area = DrawingArea::builder()
            .content_height(HEIGHT)
            .content_width(if mode == TimelineMode::Plan { 300 } else { 480 })
            .hexpand(true)
            .build();
        let fixed = Fixed::new();
        let overlay = Overlay::builder().child(&area).build();
        overlay.add_overlay(&fixed);
        overlay.set_measure_overlay(&fixed, false);
        let scroll = ScrolledWindow::builder()
            .child(&overlay)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .hexpand(true)
            .vexpand(true)
            .build();
        widget.append(&scroll);
        let model = Rc::new(RefCell::new(Model {
            day: None,
            events: vec![],
            links: vec![],
            selected: None,
        }));
        let weak = Rc::downgrade(&model);
        area.set_draw_func(move |_, cr, width, _| {
            if let Some(model) = weak.upgrade() {
                draw_grid(cr, width as f64, mode, &model.borrow());
            }
        });
        let weak = Rc::downgrade(&model);
        let fixed_weak = fixed.downgrade();
        area.connect_resize(move |_, width, _| {
            if let (Some(model), Some(fixed)) = (weak.upgrade(), fixed_weak.upgrade()) {
                layout_events(&fixed, &model.borrow().events, width as f64, mode);
            }
        });
        let selection: Selection = Rc::new(RefCell::new(None));
        widget.set_focusable(true);
        let key = gtk4::EventControllerKey::new();
        let weak = Rc::downgrade(&model);
        let selection_for_key = selection.clone();
        let area_weak = area.downgrade();
        key.connect_key_pressed(move |_, key, _, _| {
            if key != gtk4::gdk::Key::Escape {
                return gtk4::glib::Propagation::Proceed;
            }
            if let Some(model) = weak.upgrade() {
                let mut model = model.borrow_mut();
                model.selected = None;
                for event in &model.events {
                    event.button.remove_css_class("event-selected");
                }
            }
            if let Some(area) = area_weak.upgrade() {
                area.queue_draw();
            }
            if let Some(callback) = selection_for_key.borrow().as_ref() {
                callback(None);
            }
            gtk4::glib::Propagation::Stop
        });
        widget.add_controller(key);
        // Clicking an empty part of the timeline clears the active block.
        let click = gtk4::GestureClick::new();
        let weak = Rc::downgrade(&model);
        let selection_for_click = selection.clone();
        let area_weak = area.downgrade();
        click.connect_released(move |_, _, _, _| {
            if let Some(model) = weak.upgrade() {
                let mut model = model.borrow_mut();
                model.selected = None;
                for event in &model.events {
                    event.button.remove_css_class("event-selected");
                }
            }
            if let Some(area) = area_weak.upgrade() {
                area.queue_draw();
            }
            if let Some(callback) = selection_for_click.borrow().as_ref() {
                callback(None);
            }
        });
        area.add_controller(click);
        // The fixed overlay owns empty space, so handle deselection there. The
        // target phase keeps this gesture away from event buttons underneath it.
        let click = gtk4::GestureClick::new();
        click.set_propagation_phase(gtk4::PropagationPhase::Target);
        let weak = Rc::downgrade(&model);
        let selection_for_fixed = selection.clone();
        let area_weak = area.downgrade();
        click.connect_released(move |_, _, _, _| {
            if let Some(model) = weak.upgrade() {
                let mut model = model.borrow_mut();
                model.selected = None;
                for event in &model.events {
                    event.button.remove_css_class("event-selected");
                }
            }
            if let Some(area) = area_weak.upgrade() {
                area.queue_draw();
            }
            if let Some(callback) = selection_for_fixed.borrow().as_ref() {
                callback(None);
            }
        });
        fixed.add_controller(click);
        Self {
            widget,
            mode,
            area,
            fixed,
            scroll,
            model,
            selection,
        }
    }

    pub fn connect_selected(&self, callback: impl Fn(Option<Uuid>) + 'static) {
        *self.selection.borrow_mut() = Some(Box::new(callback));
    }

    pub fn show_day(&self, day: NaiveDate, plans: &[PlanBlock], facts: &[FactEntry]) {
        let old = self.model.borrow();
        let changed_day = old.day != Some(day);
        let selected = if changed_day { None } else { old.selected };
        drop(old);
        while let Some(child) = self.fixed.first_child() {
            self.fixed.remove(&child);
        }
        let mut events = vec![];
        let mut links = vec![];
        let now = Utc::now();
        for plan in plans {
            let Some(span) = Span::new(day, plan.planned_start, plan.planned_end) else {
                continue;
            };
            let fact = facts
                .iter()
                .find(|fact| fact.plan_block_id == Some(plan.id));
            let (tone, annotation) = if self.mode == TimelineMode::Plan {
                (Tone::Planned, plan.category.as_deref().unwrap_or(""))
            } else {
                match fact {
                    Some(fact) if fact.status == FactStatus::Skipped => (Tone::Missed, "Пропущено"),
                    Some(fact) if matches_plan(plan, fact) => (Tone::Match, "По плану"),
                    Some(_) => (Tone::Shift, "Расхождение"),
                    None if plan.planned_end <= now => (Tone::Missed, "Нет факта"),
                    None => (Tone::Planned, "Ожидается"),
                }
            };
            let id = if self.mode == TimelineMode::Plan {
                plan.id
            } else {
                fact.map(|f| f.id).unwrap_or(plan.id)
            };
            let event = self.event(
                id,
                span,
                0,
                &plan.title,
                &interval_text(plan.planned_start, Some(plan.planned_end)),
                annotation,
                tone,
            );
            events.push(event);
            if self.mode == TimelineMode::Comparison {
                if let Some(fact) = fact {
                    if let Some(actual_start) = fact.actual_start {
                        if let Some(actual) =
                            Span::new(day, actual_start, fact.actual_end.unwrap_or(now))
                        {
                            links.push(Link {
                                plan: span,
                                fact: actual,
                                tone,
                                id: fact.id,
                            });
                        }
                    }
                }
            }
        }
        if self.mode == TimelineMode::Comparison {
            for fact in facts {
                // A skip has no actual duration. Its reason is attached to the planned card.
                let Some(start) = fact.actual_start else {
                    continue;
                };
                let Some(span) = Span::new(day, start, fact.actual_end.unwrap_or(now)) else {
                    continue;
                };
                let tone = plans
                    .iter()
                    .find(|p| Some(p.id) == fact.plan_block_id)
                    .map(|plan| {
                        if matches_plan(plan, fact) {
                            Tone::Match
                        } else {
                            Tone::Shift
                        }
                    })
                    .unwrap_or(Tone::Extra);
                let detail = fact
                    .note
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or(fact.status.label());
                events.push(self.event(
                    fact.id,
                    span,
                    1,
                    &fact.title,
                    &interval_text(start, fact.actual_end),
                    detail,
                    tone,
                ));
            }
        }
        assign_lanes(&mut events);
        layout_events(&self.fixed, &events, self.area.width() as f64, self.mode);
        for event in &events {
            if Some(event.id) == selected {
                event.button.add_css_class("event-selected");
            }
        }
        let first = events.iter().map(|e| e.span.start).min_by(f64::total_cmp);
        *self.model.borrow_mut() = Model {
            day: Some(day),
            events,
            links,
            selected,
        };
        self.area.queue_draw();
        if changed_day {
            let minute = first.unwrap_or_else(|| {
                if day == Local::now().date_naive() {
                    Local::now().num_seconds_from_midnight() as f64 / 60.0
                } else {
                    8.0 * 60.0
                }
            });
            let scroll = self.scroll.downgrade();
            gtk4::glib::idle_add_local_once(move || {
                if let Some(scroll) = scroll.upgrade() {
                    let adjustment = scroll.vadjustment();
                    adjustment.set_value(
                        (TOP + (minute - 45.0).max(0.0) * SCALE)
                            .min((adjustment.upper() - adjustment.page_size()).max(0.0)),
                    );
                }
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn event(
        &self,
        id: Uuid,
        span: Span,
        side: usize,
        title: &str,
        time: &str,
        detail: &str,
        tone: Tone,
    ) -> Event {
        let button = Button::new();
        button.add_css_class("day-event");
        button.add_css_class(tone.class());
        let lines = if span.height() >= 72.0 {
            format!("{title}\n{time}\n{detail}")
        } else if span.height() >= 40.0 {
            format!("{title}\n{time}")
        } else {
            title.to_string()
        };
        let label = Label::new(Some(&lines));
        label.set_xalign(0.0);
        label.set_yalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_visible(span.height() >= 20.0);
        button.set_child(Some(&label));
        button.set_valign(gtk4::Align::Start);
        button.set_tooltip_text(Some(&format!("{title}\n{time}\n{detail}")));
        let weak = Rc::downgrade(&self.model);
        let selection = self.selection.clone();
        let area = self.area.downgrade();
        button.connect_clicked(move |_| {
            if let Some(model) = weak.upgrade() {
                let mut model = model.borrow_mut();
                model.selected = Some(id);
                for event in &model.events {
                    if event.id == id {
                        event.button.add_css_class("event-selected");
                    } else {
                        event.button.remove_css_class("event-selected");
                    }
                }
            }
            if let Some(area) = area.upgrade() {
                area.queue_draw();
            }
            if let Some(callback) = selection.borrow().as_ref() {
                callback(Some(id));
            }
        });
        self.fixed.put(&button, 0.0, span.y());
        Event {
            id,
            span,
            side,
            lane: 0,
            lanes: 1,
            button,
        }
    }
}

fn matches_plan(plan: &PlanBlock, fact: &FactEntry) -> bool {
    fact.status == FactStatus::Completed
        && fact.title == plan.title
        && fact
            .actual_start
            .is_some_and(|t| (t - plan.planned_start).num_seconds().abs() <= 300)
        && fact
            .actual_end
            .is_some_and(|t| (t - plan.planned_end).num_seconds().abs() <= 300)
}

fn interval_text(start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> String {
    let start_text = start.with_timezone(&Local).format("%H:%M").to_string();
    let end_text = end
        .map(|t| t.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "сейчас".into());
    let minutes = (end.unwrap_or_else(Utc::now) - start).num_minutes().max(0);
    let duration = if minutes >= 60 {
        format!("{} ч {} мин", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} мин")
    };
    format!("{start_text} — {end_text} · {duration}")
}

fn tracks(width: f64, mode: TimelineMode) -> (f64, f64) {
    if mode == TimelineMode::Comparison {
        let track = ((width - AXIS - GUTTER) / 2.0).max(1.0);
        (track, AXIS + track + GUTTER)
    } else {
        ((width - AXIS).max(1.0), width)
    }
}

fn layout_events(fixed: &Fixed, events: &[Event], width: f64, mode: TimelineMode) {
    let (track, right) = tracks(width, mode);
    for event in events {
        let origin = if event.side == 0 { AXIS } else { right };
        let lane_width = (track - INSET * 2.0) / event.lanes as f64;
        let x = origin + INSET + event.lane as f64 * lane_width;
        event.button.set_size_request(
            (lane_width - 3.0).max(1.0) as i32,
            event.span.height() as i32,
        );
        fixed.move_(&event.button, x, event.span.y());
    }
}

// Actual intervals may overlap after manual corrections. Place them beside one another,
// keeping their vertical positions tied to the shared clock.
fn assign_lanes(events: &mut [Event]) {
    for side in 0..=1 {
        let indices: Vec<_> = (0..events.len())
            .filter(|&i| events[i].side == side)
            .collect();
        let spans: Vec<_> = indices.iter().map(|&i| events[i].span).collect();
        for (index, (lane, lanes)) in indices.into_iter().zip(lane_layout(&spans)) {
            events[index].lane = lane;
            events[index].lanes = lanes;
        }
    }
}

fn lane_layout(spans: &[Span]) -> Vec<(usize, usize)> {
    let mut result = vec![(0, 1); spans.len()];
    let mut indices: Vec<_> = (0..spans.len()).collect();
    indices.sort_by(|&a, &b| spans[a].start.total_cmp(&spans[b].start));
    let mut group: Vec<usize> = vec![];
    let mut ends: Vec<f64> = vec![];
    let mut group_end: f64 = -1.0;
    for index in indices {
        let span = spans[index];
        if span.start >= group_end {
            for &i in &group {
                result[i].1 = ends.len();
            }
            group.clear();
            ends.clear();
        }
        let lane = ends
            .iter()
            .position(|&end| end <= span.start)
            .unwrap_or(ends.len());
        if lane == ends.len() {
            ends.push(span.end);
        } else {
            ends[lane] = span.end;
        }
        result[index].0 = lane;
        group_end = if group.is_empty() {
            span.end
        } else {
            group_end.max(span.end)
        };
        group.push(index);
    }
    for i in group {
        result[i].1 = ends.len();
    }
    result
}

fn draw_grid(cr: &cairo::Context, width: f64, mode: TimelineMode, model: &Model) {
    cr.set_source_rgb(0.09, 0.11, 0.15);
    let _ = cr.paint();
    let (track, right) = tracks(width, mode);
    if mode == TimelineMode::Comparison {
        cr.set_source_rgb(0.07, 0.09, 0.13);
        cr.rectangle(AXIS + track, 0.0, GUTTER, HEIGHT as f64);
        let _ = cr.fill();
    }
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(12.0);
    for half_hour in 0..=48 {
        let y = TOP + half_hour as f64 * 30.0 * SCALE;
        let hour = half_hour % 2 == 0;
        cr.set_source_rgb(
            if hour { 0.18 } else { 0.13 },
            if hour { 0.21 } else { 0.16 },
            if hour { 0.28 } else { 0.21 },
        );
        cr.set_line_width(1.0);
        cr.move_to(AXIS, y + 0.5);
        cr.line_to(width, y + 0.5);
        let _ = cr.stroke();
        if hour {
            cr.set_source_rgb(0.53, 0.58, 0.68);
            cr.move_to(13.0, y + 4.0);
            let _ = cr.show_text(&format!("{:02}:00", half_hour / 2));
        }
    }
    for link in &model.links {
        let x1 = AXIS + track;
        let x2 = right;
        let mid = (x1 + x2) / 2.0;
        let y1 = link.plan.y();
        let y2 = link.fact.y();
        let bottom1 = y1 + link.plan.height();
        let bottom2 = y2 + link.fact.height();
        let (r, g, b) = link.tone.rgb();
        let selected = model.selected == Some(link.id);
        cr.move_to(x1, y1);
        cr.curve_to(mid, y1, mid, y2, x2, y2);
        cr.line_to(x2, bottom2);
        cr.curve_to(mid, bottom2, mid, bottom1, x1, bottom1);
        cr.close_path();
        cr.set_source_rgba(r, g, b, if selected { 0.30 } else { 0.14 });
        let _ = cr.fill_preserve();
        cr.set_source_rgba(r, g, b, if selected { 0.8 } else { 0.35 });
        cr.set_line_width(1.0);
        let _ = cr.stroke();
    }
    if model.day == Some(Local::now().date_naive()) {
        let minute = Local::now().num_seconds_from_midnight() as f64 / 60.0;
        let y = TOP + minute * SCALE;
        cr.set_source_rgb(0.96, 0.39, 0.49);
        cr.set_line_width(1.5);
        cr.move_to(AXIS - 4.0, y);
        cr.line_to(width, y);
        let _ = cr.stroke();
        cr.arc(AXIS - 4.0, y, 3.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_to_day_without_shifting_intervals() {
        let span = Span::clipped(-60.0, 90.0).unwrap();
        assert_eq!(span.start, 0.0);
        assert_eq!(span.end, 90.0);
        assert_eq!(span.height(), 90.0 * SCALE);
        let span = Span::clipped(1380.0, 1500.0).unwrap();
        assert_eq!(span.end, MINUTES);
        assert!(Span::clipped(1440.0, 1500.0).is_none());
        assert!(Span::clipped(100.0, 50.0).is_none());
    }

    #[test]
    fn overlapping_facts_share_lanes_until_the_group_ends() {
        let spans = [
            Span::clipped(480.0, 540.0).unwrap(),
            Span::clipped(510.0, 570.0).unwrap(),
            Span::clipped(540.0, 600.0).unwrap(),
            Span::clipped(600.0, 660.0).unwrap(),
        ];
        assert_eq!(lane_layout(&spans), vec![(0, 2), (1, 2), (0, 2), (0, 1)]);
    }
}
