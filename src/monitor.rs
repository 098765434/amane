// a screen the compositor shows windows on, handed to views made once per monitor
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,

    // dimensions in widget units, with the compositor and App scale factors already accounted for
    pub width: u32,
    pub height: u32,
}

thread_local! {
    // the monitor the pointer was last over, by name
    static POINTER_MONITOR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/*
 * the monitor the pointer was last over, by name: a panel opened from a bar
 * shows on that bar's monitor with LayerWindow::output(pointer_monitor())
 */
pub fn pointer_monitor() -> Option<String> {
    POINTER_MONITOR.with_borrow(Clone::clone)
}

pub(crate) fn set_pointer_monitor(name: &str) {
    POINTER_MONITOR.with_borrow_mut(|monitor| {
        if monitor.as_deref() != Some(name) {
            *monitor = Some(name.to_string());
        }
    });
}
