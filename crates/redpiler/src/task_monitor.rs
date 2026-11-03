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
    /// Non-fatal problems worth telling the player about. Recorded for empty
    /// containers, whose comparator output is treated as 0 instead of aborting
    /// the compile. Every distinct position is kept (BACKLOG.md).
    warnings: Mutex<Vec<OverrideError>>,
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

    /// Records a non-fatal problem. Unlike [`Self::set_error`], every distinct
    /// position is kept so one compile can report all of them at once
    /// (BACKLOG.md). Duplicates of the same position are collapsed.
    pub fn set_warning(&self, warning: OverrideError) {
        let mut warnings = self.warnings.lock().unwrap();
        let pos = match warning {
            OverrideError::EmptyContainer { pos, .. } => pos,
            OverrideError::Unsupported { pos, .. } => pos,
        };
        let already_known = warnings.iter().any(|known| match known {
            OverrideError::EmptyContainer { pos: known, .. } => *known == pos,
            OverrideError::Unsupported { pos: known, .. } => *known == pos,
        });
        if !already_known {
            warnings.push(warning);
        }
    }

    /// Every recorded non-fatal problem, in the order they were found.
    pub fn warnings(&self) -> Vec<OverrideError> {
        self.warnings.lock().unwrap().clone()
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
