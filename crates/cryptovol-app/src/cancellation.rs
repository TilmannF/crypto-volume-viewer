//! Cooperative cancellation primitive for long-running operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A cooperative cancellation flag shared across cloned handles.
///
/// Cloning a `CancellationToken` shares the same underlying flag, so
/// cancelling any clone is immediately visible through every other clone.
/// Long-running operations (e.g. extraction) poll [`CancellationToken::is_cancelled`]
/// at safe checkpoints; this type does not interrupt execution on its own.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Creates a new, uncancelled token.
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Marks this token (and all of its clones) as cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Returns `true` if this token (or any of its clones) has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}
