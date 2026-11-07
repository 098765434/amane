use std::panic::{self, AssertUnwindSafe};
use std::sync::LazyLock;
use std::sync::mpsc::{self, Sender};
use std::thread;

type Job = Box<dyn FnOnce() + Send>;

/*
 * one thread runs the control calls input handlers ask for, in the order
 * they were asked, so a slow bus or sound server never stalls drawing
 */
static JOBS: LazyLock<Sender<Job>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel::<Job>();

    thread::spawn(move || {
        for job in receiver {
            // a failed call only loses that call, the thread keeps taking the next ones
            let _ = panic::catch_unwind(AssertUnwindSafe(job));
        }
    });

    sender
});

/// Runs `job` on amane's one thread for control calls, after the ones handed over before it,
/// so an input handler never waits on a slow bus or sound server and the calls keep their order.
/// A job that panics loses only itself. For services of your own: `amane::run_in_order`.
pub fn run(job: impl FnOnce() + Send + 'static) {
    JOBS.send(Box::new(job))
        .expect("failed to hand a control call to the worker");
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn keeps_order_after_a_failed_call() {
        let (sender, receiver) = mpsc::channel();
        let first = sender.clone();

        run(move || first.send(1).expect("failed to report job"));
        run(|| panic!("a failed call"));
        run(move || sender.send(2).expect("failed to report job"));

        let order: Vec<i32> = receiver.iter().take(2).collect();

        assert_eq!(order, [1, 2]);
    }
}
