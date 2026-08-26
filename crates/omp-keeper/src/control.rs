use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A clonable, process-local stop door for one controlled keeper run.
#[derive(Clone, Debug, Default)]
pub struct StopControl {
    requested: Arc<AtomicBool>,
}

impl StopControl {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn request_stop(&self) {
        self.requested.store(true, Ordering::Release);
    }

    pub fn is_stop_requested(&self) -> bool {
        self.requested.load(Ordering::Acquire)
    }
}

/// Observes the actual OMP child, never the thread which owns it.
pub trait KeeperObserver: Send + Sync {
    fn child_started(&self, pid: u32);
    fn child_stopped(&self, pid: u32);
}

#[derive(Debug, Default)]
pub struct NoopObserver;

impl KeeperObserver for NoopObserver {
    fn child_started(&self, _pid: u32) {}

    fn child_stopped(&self, _pid: u32) {}
}
