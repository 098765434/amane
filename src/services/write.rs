use std::any::{self, TypeId};
use std::ops::{Deref, DerefMut};
use std::sync::RwLockWriteGuard;

use crate::Service;
use crate::changes;

use super::subscribe;
use crate::timing;

pub struct Write<S: Service> {
    // taken in drop, so subscribers run after the lock is released
    pub(crate) guard: Option<RwLockWriteGuard<'static, S>>,

    // set when the write turned out to change nothing a window shows
    pub(crate) quiet: bool,
}

impl<S: Service> Write<S> {
    /*
     * finishes without waking any window, for a write that changed nothing
     * a window shows (a poll that found nothing new, a field only kept for
     * later)
     */
    pub fn quiet(&mut self) {
        self.quiet = true;
    }
}

impl<S: Service> Deref for Write<S> {
    type Target = S;

    fn deref(&self) -> &S {
        self.guard
            .as_ref()
            .expect("a write's guard is only taken in drop")
    }
}

impl<S: Service> DerefMut for Write<S> {
    fn deref_mut(&mut self) -> &mut S {
        self.guard
            .as_mut()
            .expect("a write's guard is only taken in drop")
    }
}

// a finished write is a change the windows that read the service have to show
impl<S: Service> Drop for Write<S> {
    fn drop(&mut self) {
        drop(self.guard.take());

        if self.quiet {
            return;
        }

        // with AMANE_FRAMES, the frame log also says which service woke the windows
        if timing::enabled() {
            eprintln!("change {}", any::type_name::<S>());
        }

        changes::mark(TypeId::of::<S>());

        subscribe::changed::<S>();
    }
}
