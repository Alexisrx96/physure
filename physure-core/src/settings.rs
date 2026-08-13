//! Process-wide settings read from `physure.conf`.

use std::sync::atomic::{AtomicUsize, Ordering};

const DEFAULT_PARALLEL_THRESHOLD: usize = 10_000;
static PARALLEL_THRESHOLD: AtomicUsize = AtomicUsize::new(DEFAULT_PARALLEL_THRESHOLD);

/// Minimum element count at which a `for`-expression switches from sequential
/// evaluation to `rayon`-parallel evaluation. Set from `physure.conf`'s
/// `[Settings] parallel_threshold`; `parallel_map` ignores this and is always parallel.
pub fn parallel_threshold() -> usize {
    PARALLEL_THRESHOLD.load(Ordering::Relaxed)
}

/// Sets the process-wide threshold, returning the one it replaced. This is what reading
/// a `physure.conf` calls; tests use it directly to force sequential (`usize::MAX`) or
/// parallel (`0`) execution deterministically.
pub fn set_parallel_threshold(n: usize) -> usize {
    PARALLEL_THRESHOLD.swap(n, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_parallel_threshold_returns_previous_and_updates() {
        let original = set_parallel_threshold(42);
        assert_eq!(parallel_threshold(), 42);
        let previous = set_parallel_threshold(original);
        assert_eq!(previous, 42);
    }
}
