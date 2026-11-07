use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use amane::status_item::{
    Accepts, ActivityButton, ActivityContent, ActivityEvent, Dismiss, ItemEvent, ItemIcon,
    ItemSpec, MenuEntry, Tint, Trailing,
};
use amane::{LiveActivity, StatusItem};

/*
 * an item in the bar and a two-minute countdown activity, with no window
 * of its own; needs a bar serving org.amane.StatusBar1 (amane-shell does).
 * Click, right-click or drop files on the item, and press the activity's
 * buttons: what happens is printed. Restart the bar and both come back
 */
fn main() {
    let mut spec = ItemSpec::new("org.amane.example", ItemIcon::Glyph(String::from("tray")));

    spec.tooltip = String::from("amane's status item example");
    spec.accepts = Accepts {
        files: true,
        text: true,
    };
    spec.menu = vec![
        MenuEntry::new("hello", "Say Hello"),
        MenuEntry::separator(),
        MenuEntry::new("quit", "Quit"),
    ];

    let item = StatusItem::register(spec);

    item.on_event(|event| {
        println!("item: {event:?}");

        if event == ItemEvent::MenuSelected(String::from("quit")) {
            std::process::exit(0);
        }
    });

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("failed to read the clock")
        .as_millis() as i64;

    let mut content =
        ActivityContent::new("Example", ItemIcon::Glyph(String::from("timer")), "Tea");

    content.subtitle = String::from("Steeping");
    content.trailing = Trailing::Countdown(now + 120_000);
    content.tint = Some(Tint::Orange);
    content.buttons = vec![ActivityButton::new("stop", "Stop")];

    let activity = LiveActivity::start(content.clone());

    activity.on_event(move |event| {
        println!("activity: {event:?}");

        if event == ActivityEvent::Action(String::from("stop")) {
            activity.end(Dismiss::Now);
        }
    });

    // half way, an update that alerts
    thread::sleep(Duration::from_secs(60));

    content.subtitle = String::from("One minute left");

    activity.update(content, true);

    thread::sleep(Duration::from_secs(60));

    activity.end(Dismiss::After(5));

    // the item stays until the menu's Quit
    loop {
        thread::park();
    }
}
