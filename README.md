# Y Code (`y-code`)

**Y Code** is an ultra-low-overhead, high-performance execution and evaluation harness for local and remote AI agents, optimized for minimal host device resource consumption (RAM, CPU, and battery).

## System Architecture Highlights
- **Native Systems Runtime**: Pure Rust on Tokio's work-stealing reactor (Zero GC).
- **Resident Background Daemon (`y-daemon`)**: Bypasses Windows PE process spawn latency (< 1.2 ms turn dispatch over Named Pipes).
- **Deterministic Turn FSM**: 10-point `AgentLifecycleHook` trait with zero-allocation staging.
- **Memory Arenas (`y-arena`)**: 16 MB warm working pool with 5.0-second decayed hysteresis page purging (eliminates soft page-fault storms).
- **Staged Persistence (`y-storage`)**: Tier 0 RAM ring staging (< 5.0 µs) with asynchronous SQLite WAL worker (64 MB cap).

## Quickstart

### 1. Build Workspace
```bash
cargo build --release
```

### 2. Run Tests
```bash
cargo test --workspace
```

### 3. Launch Resident Daemon
```bash
cargo run --bin y-daemon
```

### 4. Dispatch Agent Turn via CLI
```bash
cargo run --bin y-cli -- "Explain the zero-copy architecture of Y Code"
```
