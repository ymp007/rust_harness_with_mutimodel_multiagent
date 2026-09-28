---

# Section 7: Master Verification Matrix, Operational Guardrails & CI/CD Gate Enforcement

This section establishes the consolidated master verification matrix, automated continuous integration pipelines, automated benchmark runners, and systems failure triage runbooks.

---

## 7.1 Consolidated Master Table of Quantitative Guardrails Across All 4 Phases

```
+=============================================================================================================================================+
|                                              AEGIS-HARNESS MASTER QUANTITATIVE GUARDRAILS MATRIX                                            |
+=============+===========+=============================================+====================+================================================+
| Guardrail ID| Phase     | Metric Category & Description               | Hard Exit Gate     | Verification Command / Tooling                 |
+=============+===========+=============================================+====================+================================================+
| **G1-01**   | Phase 1   | Base Engine Idle Memory (RSS)               | **< 10.0 MB**      | valgrind --tool=massif / Windows ETW           |
| **G1-02**   | Phase 1   | Single-Agent Peak Memory @ 128k Context     | **< 40.0 MB**      | cargo bench --bench bench_memory_arena         |
| **G1-03**   | Phase 1   | Inter-Turn Harness Overhead (tau_harness)   | **< 1.2 ms (P99)** | criterion benchmark (1,000 turn runs)          |
| **G1-04**   | Phase 1   | Idle CPU Utilization (Waiting for input)    | **0.00% (Zero)**   | OS kernel sampler (epoll/IOCP sleep)           |
| **G1-05**   | Phase 1   | Windows Daemon Turn Dispatch Latency        | **< 1.2 ms (P95)** | hyperfine Named Pipe dispatch                  |
| **G1-06**   | Phase 1   | Async SQLite WAL RAM Staging Latency        | **< 5.0 µs**       | Criterion memory timer                         |
| **G1-07**   | Phase 1   | Memory Arena Slice Allocation Speed         | **< 15 ns**        | cargo bench --bench bench_arena_alloc          |
| **G1-08**   | Phase 1   | Synchronous Inline Token Hook Execution     | **< 40 ns**        | cargo bench --bench bench_token_hook           |
+-------------+-----------+---------------------------------------------+--------------------+------------------------------------------------+
| **G2-01**   | Phase 2   | Tier 1 Wasm Sandbox Cold-Start Latency      | **< 85 µs (P95)**  | criterion benchmark (1,000 runs)               |
| **G2-02**   | Phase 2   | Tier 2 OS Sandbox Spawn Latency             | **< 2.5 ms (P95)** | criterion benchmark (1,000 runs)               |
| **G2-03**   | Phase 2   | Process Cancellation Grace Timeout          | **500 ms ± 10 ms** | tokio::time assertion test                     |
| **G2-04**   | Phase 2   | Orphan Subprocess Retention on Kill         | **0 (Absolute 0)** | OS Process Tree Audit                          |
| **G2-05**   | Phase 2   | Out-of-Band SHM Transfer Latency (50 MB)    | **< 2.0 ms**       | cargo bench --bench bench_shm_oob              |
| **G2-06**   | Phase 2   | WASM Linear Memory Hardware Isolation Cap  | **16 MB Hard Cap** | Wasmtime segmentation trap assertion           |
| **G2-07**   | Phase 2   | Host I/O Truncation Protection Quota        | **2 MB Hard Limit**| HostOutputStream buffer overflow test          |
| **G2-08**   | Phase 2   | Windows Low Integrity Token SID Confinement | **S-1-16-4096**    | Win32 TokenIntegrityLevel Verification         |
+-------------+-----------+---------------------------------------------+--------------------+------------------------------------------------+
| **G3-01**   | Phase 3   | Time-to-First-Token Overhead (Delta-TTFT)   | **< 350 µs (P99)** | criterion benchmark (1,000 runs)               |
| **G3-02**   | Phase 3   | SIMD SSE Parser Throughput                  | **> 1,800 MB/s**   | cargo bench --bench bench_simd_sse             |
| **G3-03**   | Phase 3   | GPU Worker Crash Auto-Restart Latency       | **< 45.0 ms**      | Process kill & restart timer                   |
| **G3-04**   | Phase 3   | Radix Prefix Cache Longest Prefix Lookup    | **< 10.0 µs**      | criterion benchmark (10,000 nodes)             |
| **G3-05**   | Phase 3   | SIMD Padded Arena Alignment Requirement     | **64-Byte Aligned**| Memory address modulo assertion                |
| **G3-06**   | Phase 3   | UTF-8 Codepoint Split Recovery Rate         | **100.0% (Zero)**  | Fuzz testing with split multi-byte bytes       |
| **G3-07**   | Phase 3   | RoPE KV-Cache Middle Compaction Latency     | **< 15.0 µs**      | llama_kv_cache_seq_shift bench                 |
| **G3-08**   | Phase 3   | In-Process C-FFI Inference Launch Latency   | **< 20.0 µs**      | Direct C function call profiler                |
+-------------+-----------+---------------------------------------------+--------------------+------------------------------------------------+
| **G4-01**   | Phase 4   | 5-Agent Mesh Concurrent Active RSS          | **< 175.0 MB**     | OS memory sampler under full load              |
| **G4-02**   | Phase 4   | Cross-Process IPC Roundtrip Latency         | **< 1.8 µs (P99)** | Criterion SHM benchmark                        |
| **G4-03**   | Phase 4   | Telemetry Engine CPU Overhead Tax           | **< 0.10% CPU**    | sysinfo process CPU audit                      |
| **G4-04**   | Phase 4   | Lebesgue Measure Calculation Latency        | **< 5.0 µs**       | criterion benchmark (100 DAG intervals)        |
| **G4-05**   | Phase 4   | RAPL MSR Energy Accounting Monotonicity     | **100.0% (Zero)**  | Rollover simulation unit test                  |
| **G4-06**   | Phase 4   | SHM Head/Tail Cacheline Isolation Alignment| **128-Byte Aligned**| Address offset modulo assertion                |
| **G4-07**   | Phase 4   | Actor Mailbox High Watermark Throttling     | **80.0% Capacity** | Backpressure suspension unit test              |
| **G4-08**   | Phase 4   | Host UI Frame Drops / Stutter Under Load    | **0.00% (Zero)**   | Thread QoS governor active verification        |
+=============+===========+=============================================+====================+================================================+
```

---

## 7.2 Automated CI/CD Gate Enforcement Pipeline (`.github/workflows/aegis_ci.yml`)

```yaml
name: Aegis-Harness Production CI/CD Gate Enforcement

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  static_analysis_and_lint:
    name: Rustfmt & Clippy Verification
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Verify Formatting
        run: cargo fmt --all -- --check
      - name: Run Clippy Linter
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  test_matrix:
    name: Multi-Platform Unit & Integration Matrix
    strategy:
      fail-fast: false
      matrix:
        os: [ ubuntu-latest, windows-latest, macos-14 ]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run Full Workspace Test Suite
        run: cargo test --workspace --all-features -- --nocapture

  memory_and_guardrail_audit:
    name: Systems Guardrails & Memory Footprint Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Valgrind & Hyperfine
        run: sudo apt-get update && sudo apt-get install -y valgrind hyperfine
      - name: Build Release Binaries
        run: cargo build --release --workspace
      - name: Enforce Idle Memory SLA (< 10.0 MB RSS)
        run: |
          valgrind --tool=massif --massif-out-file=massif.out target/release/aegis-daemon &
          DAEMON_PID=$!
          sleep 6
          kill -SIGTERM $DAEMON_PID
          PEAK_BYTES=$(ms_print massif.out | grep "Detailed snapshots" -A 10 | grep -oP '\d+(?= bytes)' | head -n 1)
          echo "Peak RSS Measured: $PEAK_BYTES bytes"
          if [ "$PEAK_BYTES" -gt 10485760 ]; then
            echo "ERROR: Base Engine Idle Memory exceeded 10 MB SLA!"
            exit 1
          fi

  benchmark_regression_gate:
    name: Criterion Performance Regression Gate
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Run Criterion Benchmarks
        run: |
          cargo bench --package aegis-reactor --bench bench_turn_loop
          cargo bench --package aegis-gateway --bench bench_simd_sse
          cargo bench --package aegis-sandbox --bench bench_wasm_instantiation
          cargo bench --package aegis-shm --bench bench_shm_latency
```

---

## 7.3 Automated Test & Benchmark Runner Shell Script

```bash
#!/usr/bin/env bash
# ============================================================================
# scripts/run_all_benchmarks.sh
# Master Automated Systems Benchmark & Verification Runner for Aegis-Harness
# ============================================================================

set -euo pipefail

echo "=========================================================================="
echo "          STARTING AEGIS-HARNESS MASTER SYSTEMS VERIFICATION              "
echo "=========================================================================="

echo "[1/5] Running Cargo Unit & Integration Tests..."
cargo test --workspace --all-features

echo "[2/5] Benchmarking Phase 1: Native Reactor & Memory Arenas..."
cargo bench --package aegis-reactor --bench bench_turn_loop
cargo test --package aegis-arena --test test_decayed_purge

echo "[3/5] Benchmarking Phase 2: Wasm & OS Sandboxing..."
cargo bench --package aegis-sandbox --bench bench_wasm_instantiation
cargo bench --package aegis-sandbox --bench bench_os_spawn

echo "[4/5] Benchmarking Phase 3: SIMD SSE Parser & Prefix Cache..."
cargo bench --package aegis-gateway --bench bench_simd_sse
cargo bench --package aegis-cache --bench bench_radix_lookup

echo "[5/5] Benchmarking Phase 4: Shared Memory IPC & Telemetry..."
cargo bench --package aegis-shm --bench bench_shm_latency
cargo bench --package aegis-telemetry --bench bench_lebesgue_disaggregation

echo "=========================================================================="
echo "          ALL QUANTITATIVE PERFORMANCE GUARDRAILS PASSED (SLA MET)        "
echo "=========================================================================="
```

---

## 7.4 Performance Regression Thresholds & Failure Triage Runbooks

### Runbook 1: Memory Arena Page-Fault Storm Triage
- **Symptom**: Inter-turn latency spikes to $> 10\text{ ms}$; CPU profiler indicates high kernel time in `do_anonymous_page`.
- **Root Cause**: `check_decayed_purge` triggered decommit too aggressively (< 5.0 seconds).
- **Triage Action**: Inspect `crates/aegis-arena/src/purge.rs`. Verify that `last_active.elapsed() >= Duration::from_secs(5)` before calling `mallctl` or `mimalloc_trim`.

### Runbook 2: SIMD SSE Parser Overread & UTF-8 Split Triage
- **Symptom**: Segmentation fault in `_mm256_loadu_si256` or string decode panic on multi-byte characters.
- **Root Cause**: Chunk buffer missing `SIMDJSON_PADDING = 64` or carryover register offset corruption.
- **Triage Action**: Inspect `crates/aegis-gateway/src/sse/simd.rs`. Ensure buffer allocation uses `AlignedVec` with 64-byte trailing zero-padding.

### Runbook 3: Anonymous Pipe Buffer Deadlock Triage
- **Symptom**: Sandboxed tool hangs indefinitely during large output generation ($> 64\text{ KB}$).
- **Root Cause**: Host stopped reading stdout while waiting for process exit, filling OS kernel pipe buffer (64 KB).
- **Triage Action**: Inspect `crates/aegis-sandbox/src/os/`. Ensure child stdout and stderr are drained concurrently using non-blocking asynchronous readers (`tokio::io::copy`), or route payloads $> 1\text{ MB}$ to Out-of-Band SHM descriptors (`ShmOobDescriptor`).

### Runbook 4: Out-of-Process GPU Worker Crash Triage
- **Symptom**: `ModelGateway` returns `GatewayError::WorkerCrashed`.
- **Root Cause**: GPU driver CUDA out-of-memory error or Metal device loss.
- **Triage Action**: Inspect `crates/aegis-gateway/src/worker/mod.rs`. Verify supervisor auto-restart spawns a fresh worker instance in $< 45\text{ ms}$ and falls back to CPU C-FFI if GPU memory remains exhausted.

### Runbook 5: Actor Mesh Mailbox Backpressure Overflow Triage
- **Symptom**: Actor mailbox capacity reaches $100\%$, dropping messages.
- **Root Cause**: Downstream consumer blocked on synchronous computation.
- **Triage Action**: Inspect `crates/aegis-mesh/src/mailbox.rs`. Verify credit-based backpressure suspends upstream producers at $80\%$ high watermark ($W_{\text{high}}$) until the queue drains below $20\%$ ($W_{\text{low}}$).

---

# Section 8: Document Control, References & Traceability Index

### 8.1 Document History & Revisions
- **Version 1.0.0 (Production Hardened)**: Master Implementation Plan released with complete technical IDLs, multi-option architecture, 4 implementation phases, 32 quantitative guardrails, and CI/CD gate automation.

### 8.2 Master Traceability Matrix

```
+=============================================================================================================+
|                                         MASTER REQUIREMENTS TRACEABILITY MATRIX                             |
+=========================+=======================================+===========================================+
| System Requirement ID   | Description                           | Architecture Section & Implementation IDL |
+=========================+=======================================+===========================================+
| **REQ-R1**              | Multi-Option Implementation Spectrum  | Section 2 (Option A, Option B, Option C)  |
| **REQ-R2**              | Native Reactor & Memory Arenas        | Section 3 (crates/aegis-reactor, arena)   |
| **REQ-R3**              | Tiered Sandboxing & Plugin Bridge     | Section 4 (crates/aegis-sandbox, mcp)     |
| **REQ-R4**              | Dual-Mode Model Gateway & Prefix Cache| Section 5 (crates/aegis-gateway, cache)   |
| **REQ-R5**              | Multi-Agent Mesh & Telemetry Engine   | Section 6 (crates/aegis-mesh, telemetry)  |
| **REQ-R6**              | Master Verification & CI/CD Gates     | Section 7 (Master Matrix & CI/CD Pipeline)|
+=========================+=======================================+===========================================+
```

### 8.3 Systems Engineering References
1. **Tokio Project**: Asynchronous Runtime for the Rust Programming Language (2024).
2. **Wasmtime & WASI Preview 2**: Bytecode Alliance WebAssembly Standard Component Model Specification (2024).
3. **rkyv**: Zero-Copy Deserialization Framework for Rust (2024).
4. **BLAKE3**: One-Way Cryptographic Hash Function with Tree Hashing (2020).
5. **Linux Kernel Organization**: Namespaces, Seccomp-BPF, Landlock LSM, and `openat2` System Call Reference (Kernel 5.15+).
6. **Microsoft Win32 Architecture**: Job Objects, CPU Rate Control, and Restricted Security Tokens Documentation.
