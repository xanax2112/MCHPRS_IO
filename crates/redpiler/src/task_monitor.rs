use mchprs_redstone::comparator::OverrideError;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct TaskMonitor {
    cancelled: AtomicBool,
    max_progress: AtomicUsize,
    progress: AtomicUsize,
    message: Mutex<Option<Arc<String>>>,
    error: Mutex<Option<OverrideError>>,
}

impl TaskMonitor {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Records that a block's comparator override could not be read.
    ///
    /// Only the first error is kept: the frontend pass keeps walking the plot
    /// after reporting one, and the earliest position is the useful one.
    pub fn set_error(&self, error: OverrideError) {
        let mut slot = self.error.lock().unwrap();
        if slot.is_none() {
            *slot = Some(error);
        }
    }

    pub fn error(&self) -> Option<OverrideError> {
        *self.error.lock().unwrap()
    }

    pub fn set_progress(&self, progress: usize) {
        self.progress.store(progress, Ordering::Relaxed);
    }

    pub fn inc_progress(&self) {
        self.progress.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_max_progress(&self, max_progress: usize) {
        self.max_progress.store(max_progress, Ordering::Relaxed);
    }

    pub fn progress(&self) -> usize {
        self.progress.load(Ordering::Relaxed)
    }

    pub fn max_progress(&self) -> usize {
        self.max_progress.load(Ordering::Relaxed)
    }

    pub fn set_message(&self, message: String) {
        *self.message.lock().unwrap() = Some(Arc::new(message));
    }

    pub fn message(&self) -> Option<Arc<String>> {
        self.message.lock().unwrap().clone()
    }
}
