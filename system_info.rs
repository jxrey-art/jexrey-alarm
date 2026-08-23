//! Real local-system telemetry (never fabricated — see brief §13/§14).
//!
//! Getting an accurate CPU percentage out of `sysinfo` requires refreshing
//! twice with a short sleep in between, which would visibly stall the UI
//! thread if done inside `App::update`. So this is the one piece of the
//! app that *does* get its own background thread: it owns the `sysinfo`
//! handle, refreshes every couple of seconds, and publishes the latest
//! snapshot into a small `Arc<Mutex<..>>` the UI reads from without ever
//! blocking on it.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    pub os_name: String,
    pub os_version: String,
    pub host_name: String,
    pub cpu_count: usize,
    pub cpu_usage_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
}

impl Default for SystemSnapshot {
    fn default() -> Self {
        Self {
            os_name: "Unknown".to_string(),
            os_version: String::new(),
            host_name: String::new(),
            cpu_count: 0,
            cpu_usage_percent: 0.0,
            memory_used_mb: 0,
            memory_total_mb: 0,
        }
    }
}

pub struct SystemMonitor {
    snapshot: Arc<Mutex<SystemSnapshot>>,
}

impl SystemMonitor {
    /// Spawns the background collector thread and returns a handle whose
    /// `latest()` can be polled cheaply from the UI thread every frame.
    pub fn spawn() -> Self {
        let snapshot = Arc::new(Mutex::new(SystemSnapshot::default()));
        let snapshot_writer = Arc::clone(&snapshot);

        thread::Builder::new()
            .name("jexrey-telemetry".into())
            .spawn(move || {
                let refresh = RefreshKind::new()
                    .with_cpu(CpuRefreshKind::everything())
                    .with_memory(MemoryRefreshKind::everything());
                let mut sys = System::new_with_specifics(refresh);

                let os_name = System::name().unwrap_or_else(|| "Windows".to_string());
                let os_version = System::os_version().unwrap_or_default();
                let host_name = System::host_name().unwrap_or_default();

                loop {
                    sys.refresh_cpu_usage();
                    thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
                    sys.refresh_cpu_usage();
                    sys.refresh_memory();

                    let cpu_usage_percent = sys.global_cpu_info().cpu_usage();
                    let cpu_count = sys.cpus().len();
                    let memory_used_mb = sys.used_memory() / 1024 / 1024;
                    let memory_total_mb = sys.total_memory() / 1024 / 1024;

                    if let Ok(mut guard) = snapshot_writer.lock() {
                        *guard = SystemSnapshot {
                            os_name: os_name.clone(),
                            os_version: os_version.clone(),
                            host_name: host_name.clone(),
                            cpu_count,
                            cpu_usage_percent,
                            memory_used_mb,
                            memory_total_mb,
                        };
                    }

                    thread::sleep(Duration::from_millis(1800));
                }
            })
            .expect("failed to spawn telemetry thread");

        Self { snapshot }
    }

    pub fn latest(&self) -> SystemSnapshot {
        self.snapshot
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }
}
