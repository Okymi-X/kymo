use std::sync::Arc;

use crate::tokens::now;

/// A source of unix seconds. Injectable so tests can fix the time.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system() -> Clock {
    Arc::new(now)
}
