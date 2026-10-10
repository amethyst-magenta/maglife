use maglife_domain::{Event, Item};

fn main() {
    let item = Item::capture("idea", "Первый вертикальный срез Personal Activity OS");
    let event = Event::item_created(&item);
    println!("Captured item: {} ({})", item.title, item.id);
    println!("Event: {}", event.kind);
}
