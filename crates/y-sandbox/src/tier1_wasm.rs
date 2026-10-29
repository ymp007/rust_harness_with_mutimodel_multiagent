use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;
use y_types::AgentError;

pub const DEFAULT_WASM_FUEL_LIMIT: u64 = 1_000_000_000; // 1 Billion instructions
pub const DEFAULT_MAX_MEMORY_PAGES: u32 = 256; // 16 MB (64 KB per Wasm page)
pub const MAX_WASM_IO_QUOTA_BYTES: usize = 2 * 1024 * 1024; // 2 MB I/O Quota

/// Metering scaffold for in-process guest execution.
///
/// This type accounts for a caller-declared `fuel_cost` and enforces the input
/// byte quota. It does not bind to a Wasm engine: there is no guest
/// instruction counter here, so `fuel_cost` is an assertion by the caller, not
/// a measurement of work performed. Treat it as a budget ledger until a real
/// runtime is wired in.
pub struct Tier1WasmSandbox {
    fuel_remaining: AtomicU64,
    fuel_limit: u64,
    max_memory_pages: u32,
}

impl Tier1WasmSandbox {
    pub fn new(fuel_limit: u64, max_memory_pages: u32) -> Self {
        Self {
            fuel_remaining: AtomicU64::new(fuel_limit),
            fuel_limit,
            max_memory_pages,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_WASM_FUEL_LIMIT, DEFAULT_MAX_MEMORY_PAGES)
    }

    /// Charges `fuel_cost` against the quota and returns an echo of the payload.
    ///
    /// Fuel is deducted with a single atomic compare-and-swap so concurrent
    /// callers cannot both pass the check and then underflow the counter, which
    /// would wrap to `u64::MAX` and permanently disable the quota.
    pub fn execute_guest(
        &self,
        fuel_cost: u64,
        input_payload: &[u8],
    ) -> Result<Vec<u8>, AgentError> {
        if input_payload.len() > MAX_WASM_IO_QUOTA_BYTES {
            return Err(AgentError::SecurityViolation(format!(
                "Input payload {} bytes exceeds 2 MB Wasm I/O Quota",
                input_payload.len()
            )));
        }

        self.fuel_remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                current.checked_sub(fuel_cost)
            })
            .map_err(|_| {
                AgentError::ResourceExhausted("Wasm guest exhausted CPU fuel quota".to_string())
            })?;

        debug!(
            "Wasm guest charged {} fuel. Remaining: {}",
            fuel_cost,
            self.fuel_remaining()
        );

        // Echo response; no guest memory is retained between calls.
        Ok(input_payload.to_vec())
    }

    /// Restores the full quota. Requires exclusive access so one caller cannot
    /// reset a shared sandbox's quota while another turn is in flight.
    pub fn reset_fuel(&mut self) {
        self.fuel_remaining
            .store(self.fuel_limit, Ordering::Release);
    }

    pub fn fuel_remaining(&self) -> u64 {
        self.fuel_remaining.load(Ordering::Acquire)
    }

    /// Configured guest memory ceiling, in 64 KiB pages. Not enforced by
    /// `execute_guest`; no guest can grow memory because no engine is bound.
    pub fn max_memory_pages(&self) -> u32 {
        self.max_memory_pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_charges_cannot_wrap_the_counter() {
        let sandbox = Arc::new(Tier1WasmSandbox::new(1_000, 256));
        let barrier = Arc::new(Barrier::new(16));
        let mut handles = Vec::new();
        for _ in 0..16 {
            let sandbox = sandbox.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let _ = sandbox.execute_guest(1_000, b"x");
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // Exactly one charge of 1000 can succeed against a limit of 1000.
        assert_eq!(
            sandbox.fuel_remaining(),
            0,
            "counter wrapped past the quota"
        );
    }

    #[test]
    fn exhausted_quota_is_reported() {
        let sandbox = Tier1WasmSandbox::new(100, 256);
        sandbox.execute_guest(100, b"x").unwrap();
        let err = sandbox.execute_guest(1, b"x").unwrap_err();
        assert!(matches!(err, AgentError::ResourceExhausted(_)));
        assert_eq!(sandbox.fuel_remaining(), 0);
    }

    #[test]
    fn oversized_payload_is_rejected_before_charging_fuel() {
        let sandbox = Tier1WasmSandbox::new(100, 256);
        let big = vec![0u8; MAX_WASM_IO_QUOTA_BYTES + 1];
        assert!(sandbox.execute_guest(10, &big).is_err());
        assert_eq!(
            sandbox.fuel_remaining(),
            100,
            "rejected input must not be charged"
        );
    }
}
