use crate::common::{Clock, ConfigStore, StoredConfig};
use core::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub struct LinuxClock {
    start: Instant,
}

impl Default for LinuxClock {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Clock for LinuxClock {
    fn now_micros(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

pub struct MemoryConfigStore {
    config: Option<StoredConfig>,
}

impl Default for MemoryConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryConfigStore {
    pub fn new() -> Self {
        Self {
            config: StoredConfig::try_new("0000-0001", "worker-linux", "linux-sim", "password"),
        }
    }
}

impl ConfigStore for MemoryConfigStore {
    fn read(&mut self) -> Option<StoredConfig> {
        self.config.clone()
    }

    fn write(&mut self, config: &StoredConfig) -> Result<(), ()> {
        self.config = Some(config.clone());
        Ok(())
    }
}

pub struct ReadySignal {
    signaled: AtomicBool,
}

impl Default for ReadySignal {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadySignal {
    pub const fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    pub fn signal(&self) {
        self.signaled.store(true, Ordering::SeqCst);
    }

    pub fn wait(&self) {
        while !self.signaled.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
    }
}

pub static CORE1_READY_SIGNAL: ReadySignal = ReadySignal::new();
