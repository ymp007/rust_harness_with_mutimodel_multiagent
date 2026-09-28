use std::sync::atomic::{AtomicU64, Ordering};
use tracing::debug;
use y_types::AgentError;

pub const DEFAULT_WASM_FUEL_LIMIT: u64 = 1_000_000_000; // 1 Billion instructions
pub const DEFAULT_MAX_MEMORY_PAGES: u32 = 256; // 16 MB (64 KB per Wasm page)
pub const MAX_WASM_IO_QUOTA_BYTES: usize = 2 * 1024 * 1024; // 2 MB I/O Quota

pub struct Tier1WasmSandbox {
    fuel_remaining: AtomicU64,
    fuel_limit: u64,
    #[allow(dead_code)]
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

    /// Simulates execution of an in-process Wasm guest component with fuel metering.
    /// In full production, this binds to Wasmtime Component Model runtime.
    pub fn execute_guest(&self, fuel_cost: u64, input_payload: &[u8]) -> Result<Vec<u8>, AgentError> {
        if input_payload.len() > MAX_WASM_IO_QUOTA_BYTES {
            return Err(AgentError::SecurityViolation(format!(
                "Input payload {} bytes exceeds 2 MB Wasm I/O Quota",
                input_payload.len()
            )));
        }

        let current_fuel = self.fuel_remaining.load(Ordering::Acquire);
        if current_fuel < fuel_cost {
            return Err(AgentError::ResourceExhausted(
                "Wasm guest exhausted CPU fuel quota".to_string(),
            ));
        }

        self.fuel_remaining.fetch_sub(fuel_cost, Ordering::AcqRel);
        debug!("Wasm guest executed with {} fuel cost. Remaining: {}", fuel_cost, current_fuel - fuel_cost);

        // Echo response or mock AST/diff execution
        Ok(input_payload.to_vec())
    }

    pub fn reset_fuel(&self) {
        self.fuel_remaining.store(self.fuel_limit, Ordering::Release);
    }

    pub fn fuel_remaining(&self) -> u64 {
        self.fuel_remaining.load(Ordering::Acquire)
    }
}
