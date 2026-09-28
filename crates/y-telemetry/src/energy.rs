use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use parking_lot::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct EnergySnapshot {
    pub cpu_usage_percent: f32,
    pub used_memory_mb: f64,
    pub total_memory_mb: f64,
    pub timestamp_ns: u64,
}

pub struct HardwareEnergyMeter {
    sys: Mutex<System>,
}

impl HardwareEnergyMeter {
    pub fn new() -> Self {
        let refresh_kind = RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_memory(MemoryRefreshKind::new().with_ram());

        let mut sys = System::new_with_specifics(refresh_kind);
        sys.refresh_specifics(refresh_kind);

        Self {
            sys: Mutex::new(sys),
        }
    }

    /// Captures a point-in-time snapshot of system hardware metrics in < 100 µs.
    pub fn capture_snapshot(&self) -> EnergySnapshot {
        let mut sys = self.sys.lock();
        sys.refresh_cpu_usage();
        sys.refresh_memory();

        let global_cpu = sys.global_cpu_usage();
        let used_mem = sys.used_memory() as f64 / (1024.0 * 1024.0);
        let total_mem = sys.total_memory() as f64 / (1024.0 * 1024.0);
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        EnergySnapshot {
            cpu_usage_percent: global_cpu,
            used_memory_mb: used_mem,
            total_memory_mb: total_mem,
            timestamp_ns: now_ns,
        }
    }
}

impl Default for HardwareEnergyMeter {
    fn default() -> Self {
        Self::new()
    }
}
