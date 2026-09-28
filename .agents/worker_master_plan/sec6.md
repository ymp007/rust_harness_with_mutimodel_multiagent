---

# Section 6: Phase 4: Multi-Agent Mesh & Hardware Telemetry

Phase 4 implements the hierarchical work-stealing multi-agent actor mesh, lock-free shared memory channels, cross-platform hardware telemetry engine, thread-level OS power governance, and mathematical DAG latency disaggregation formulas.

```
+===================================================================================================+
|                                    PHASE 4 ARCHITECTURAL TOPOLOGY                                 |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                            Hierarchical Multi-Agent Actor Mesh                              |  |
|  |  [ Root Supervisor ] ---> [ Task Planner Actor ] ---> [ Specialist Worker Actors 1..N ]     |  |
|  |  (OneForOne / AllForOne Supervision Trees | Chase-Lev Work-Stealing Task Deques)             |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |     LOCKLESS SHM WIRE SYSTEM     |                   |    CROSS-PLATFORM TELEMETRY      |      |
|  | - 128B Cacheline-Isolated Rings  |                   | - Dynamic NVML (libloading)      |      |
|  | - Double Virtual-Address Mapping |                   | - 64b Monotonic RAPL MSR Rollover|      |
|  | - rkyv Zero-Copy Deserialization |                   | - Apple SMC & Windows ETW/PDH    |      |
|  | - < 1.8 µs Roundtrip IPC Latency |                   | - Thread QoS (EcoQoS / nice +19) |      |
|  +----------------------------------+                   +----------------------------------+      |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |              LEBESGUE MEASURE ASYNCHRONOUS DAG LATENCY & ENERGY DISAGGREGATION               |  |
|  |       tau_harness = mu(bigcup I_turn) - sum (tau_model_stream + tau_tool_execution)          |  |
|  +---------------------------------------------------------------------------------------------+  |
+===================================================================================================+
```

---

## 6.1 Hierarchical Work-Stealing Actor Mesh Architecture

Aegis-Harness coordinates multi-agent topologies using an asynchronous actor mesh:
1. **Supervision Trees**: Actors are structured in hierarchical supervision trees supporting `OneForOne` (restarting only the failed actor) and `AllForOne` (restarting the entire agent group on catastrophic failure) restart strategies.
2. **Chase-Lev Work-Stealing Deques (`crossbeam_deque`)**: Each worker thread maintains a private double-ended queue. Worker actors push/pop tasks locally ($LIFO$ for cache locality); idle worker threads steal tasks from the tail of victim queues ($FIFO$), eliminating cross-thread contention.
3. **Credit-Based Backpressure**: Mailboxes enforce high/low watermarks ($W_{\text{high}} = 80\%$, $W_{\text{low}} = 20\%$). When mailbox capacity reaches $80\%$, the sender is asynchronously suspended until the consumer drains below $20\%$, preventing unbounded memory growth.

---

## 6.2 Lock-Free MPMC Channels & Zero-Copy Inter-Agent Shared Memory

- **128-Byte Cacheline Isolation**: Head and Tail atomic pointers reside on separate 128-byte cache lines, completely eliminating false sharing on dual-core CPU architectures.
- **Double Virtual Address Mapping**: Ring buffers map the underlying physical memory pages consecutively twice in virtual address space. Writes wrapping around the buffer boundary require no split-copy logic.
- **Zero-Copy Memory Casts (`rkyv`)**: Inter-agent messages are cast directly from SHM pointers, achieving **< 1.8 µs (P99)** roundtrip IPC latency.

---

## 6.3 Cross-Platform Hardware Telemetry & Energy Profiling Subsystem

```
+===================================================================================================+
|                                HARDWARE TELEMETRY SAMPLING ENGINE                                 |
+===================================================================================================+
|                                                                                                   |
|  [ Linux Telemetry: Monotonic RAPL MSR Energy Accounting ]                                        |
|  - Reads /sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj                                    |
|  - 32-Bit Microjoule Rollover Compensation Formula:                                               |
|    Delta_E = (E_curr >= E_prev) ? (E_curr - E_prev) : (E_curr + (2^32 - E_prev))                  |
|  - 64-Bit Monotonic Energy Accumulator: E_total_uj += Delta_E                                     |
|                                                                                                   |
|  [ NVIDIA GPU Telemetry: Dynamic NVML Driver Linking ]                                            |
|  - Dynamically loads libnvidia-ml.so / nvml.dll via libloading at runtime (Zero hard link error)  |
|  - Samples nvmlDeviceGetPowerUsage, nvmlDeviceGetMemoryInfo, nvmlDeviceGetUtilizationRates        |
|                                                                                                   |
|  [ Apple Silicon Telemetry: Apple SMC & IOReport ]                                                |
|  - Queries Apple SMC keys (PSTR, PC0C) and IORegistry IOReport for SoC Package Energy (mWh).      |
|                                                                                                   |
|  [ Windows Telemetry: Windows ETW & PDH Counters ]                                                |
|  - Queries PdhOpenQuery / \\Processor Information(_Total)\\% Processor Energy                    |
|                                                                                                   |
|  [ Thread-Level Power & Priority Governance ]                                                     |
|  - Windows: SetThreadInformation(ThreadPowerThrottling, EcoQoS)                                   |
|  - Linux: pthread_setschedparam(SCHED_IDLE) or nice(+19)                                           |
|  - macOS: pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND)                                     |
|                                                                                                   |
+===================================================================================================+
```

---

## 6.4 Formal Lebesgue Measure DAG Latency & Energy Disaggregation Formulas

In an asynchronous multi-agent execution DAG, turns, model token streams, and sandboxed tool executions run concurrently across overlapping timelines. Naive arithmetic summation yields mathematically invalid results exceeding total wall-clock time.

```
Timeline:
Turn:        [====================================================================] (T_start to T_end)
Model Gen:         [====================]
Tool Exec:                     [=============================]
Harness Overh: [==]                 [==]                     [===================]
```

### 1. Inter-Turn Harness Overhead ($\tau_{\text{harness}}$):
Let $I_{\text{turn}} = [t_{\text{start}}, t_{\text{end}}]$ be the total turn wall-clock duration interval.  
Let $I_{\text{model}, i} = [t_{\text{m\_start}, i}, t_{\text{m\_end}, i}]$ be the model streaming interval for subtask $i$.  
Let $I_{\text{tool}, j} = [t_{\text{t\_start}, j}, t_{\text{t\_end}, j}]$ be the tool execution interval for subtask $j$.  
Let $\mu(\cdot)$ represent the standard 1-dimensional Lebesgue measure on $\mathbb{R}$.

$$\tau_{\text{harness}} = \mu(I_{\text{turn}}) - \mu\left(\left(\bigcup_{i} I_{\text{model}, i}\right) \cup \left(\bigcup_{j} I_{\text{tool}, j}\right)\right)$$

### 2. Time-to-First-Token Overhead ($\Delta\text{TTFT}$):
Let $t_{\text{prompt\_dispatch}}$ be the timestamp when the prompt leaves the turn FSM.  
Let $t_{\text{wire\_first\_byte}}$ be the timestamp when the first raw TCP/SSE byte arrives on the network socket.  
Let $t_{\text{first\_token\_hook}}$ be the timestamp when the `on_token_chunk` hook is invoked on the host.

$$\Delta\text{TTFT} = t_{\text{first\_token\_hook}} - t_{\text{wire\_first\_byte}}$$

### 3. Net Energy Consumption ($E_{\text{agent}}$):
Let $P_{\text{total}}(t)$ be the instantaneous total host power (Watts) sampled at time $t$.  
Let $P_{\text{idle\_baseline}}$ be the pre-measured idle baseline power of the host machine.

$$E_{\text{agent}} = \int_{t_{\text{start}}}^{t_{\text{end}}} \left( P_{\text{total}}(t) - P_{\text{idle\_baseline}} \right) \, dt$$

---

## 6.5 Rust Trait & Struct IDL Definitions

```rust
// ============================================================================
// CRATE: aegis-mesh
// Module: Hierarchical Work-Stealing Actor Mesh Traits
// ============================================================================

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorStrategy {
    OneForOne,
    AllForOne,
}

#[derive(Debug, Clone)]
pub struct ActorConfig {
    pub actor_id: String,
    pub max_mailbox_capacity: usize,
    pub supervisor_strategy: SupervisorStrategy,
    pub high_watermark_ratio: f32, // 0.80
    pub low_watermark_ratio: f32,  // 0.20
}

#[derive(Debug, Clone)]
pub struct MeshMessage {
    pub message_id: u64,
    pub sender_actor_id: String,
    pub recipient_actor_id: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum MeshError {
    #[error("Actor mailbox overflow: {0}")]
    MailboxFull(String),
    #[error("Actor panic: {0}")]
    ActorPanic(String),
    #[error("Shared memory ring buffer error: {0}")]
    ShmError(String),
}

#[async_trait::async_trait]
pub trait AgentActor: Send + Sync {
    async fn handle_message(&mut self, msg: MeshMessage) -> Result<(), MeshError>;
    async fn on_actor_start(&mut self) -> Result<(), MeshError>;
    async fn on_actor_stop(&mut self) -> Result<(), MeshError>;
}

// ============================================================================
// CRATE: aegis-telemetry
// Module: Hardware Telemetry & Lebesgue Energy Engine
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct HardwareMetrics {
    pub cpu_utilization_pct: f32,
    pub process_rss_bytes: u64,
    pub total_energy_microjoules: u64,
    pub gpu_utilization_pct: Option<f32>,
    pub gpu_memory_used_bytes: Option<u64>,
    pub gpu_energy_microjoules: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TurnTelemetryReport {
    pub wall_time_ns: u64,
    pub tau_harness_ns: u64,
    pub delta_ttft_ns: u64,
    pub net_energy_joules: f64,
    pub peak_rss_bytes: u64,
}

pub trait TelemetryCollector: Send + Sync {
    fn sample_hardware(&self) -> HardwareMetrics;
    fn calculate_turn_telemetry(
        &self,
        turn_start_ns: u64,
        turn_end_ns: u64,
        model_intervals: &[(u64, u64)],
        tool_intervals: &[(u64, u64)],
    ) -> TurnTelemetryReport;
}
```

---

## 6.6 Phase 4 File Hierarchy, Developer Steps, Tests, and Guardrails

### File-Level Target Layout:
```
crates/
├── aegis-mesh/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Mesh public exports
│       ├── actor.rs                   # AgentActor trait & lifecycle
│       ├── mailbox.rs                 # Credit-based backpressure mailbox
│       ├── deque.rs                   # Chase-Lev work-stealing task deque
│       └── supervisor.rs              # OneForOne & AllForOne supervision trees
├── aegis-shm/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Lockless SHM ring exports
│       ├── ring_buf.rs                # 128-byte cache-aligned SPSC/MPMC ring
│       └── double_map.rs              # Double virtual address mapping engine
└── aegis-telemetry/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                     # Telemetry public exports
        ├── lebesgue.rs                # 1D Lebesgue measure interval merger
        ├── rapl_linux.rs              # 64-bit monotonic Linux RAPL MSR collector
        ├── nvml_gpu.rs                # Dynamic NVIDIA NVML loader
        ├── apple_smc.rs               # Apple Silicon SMC & IOReport collector
        ├── windows_pdh.rs             # Windows PDH & ETW collector
        └── qos.rs                     # SetThreadInformation EcoQoS governor
```

### Granular Developer Step-by-Step Instructions:
1. **Implement 128B Cache-Aligned SHM Ring**: Construct `shm_ring_buf` with 128-byte head/tail cacheline separation and double virtual address mapping. Verify `< 1.8 µs` RTT.
2. **Implement Chase-Lev Work-Stealing Actor Mesh**: Integrate `crossbeam_deque` deques and `OneForOne` supervision trees.
3. **Implement Credit-Based Backpressure Mailboxes**: Implement $80\% / 20\%$ high/low watermark throttling.
4. **Implement 64-bit Monotonic RAPL MSR Energy Sampler**: Implement microjoule 32-bit rollover compensation on Linux.
5. **Implement Dynamic NVML GPU Loader**: Load `libnvidia-ml.so` / `nvml.dll` via `libloading` with zero static link panics.
6. **Implement Thread QoS Governor**: Implement `SetThreadInformation` on Windows and `nice +19` / `SCHED_IDLE` on Linux.
7. **Implement Lebesgue Measure Latency Disaggregation**: Construct $O(N \log N)$ interval union algorithm calculating $\tau_{\text{harness}}$ and $\Delta\text{TTFT}$.

### Automated Test & Benchmark Commands:
```bash
# 1. Run all mesh, SHM, and telemetry unit tests
cargo test --package aegis-mesh --package aegis-shm --package aegis-telemetry

# 2. Benchmark Cross-Process SHM Roundtrip Latency (< 1.8 µs P99)
cargo bench --package aegis-shm --bench bench_shm_latency

# 3. Benchmark 5-Agent Mesh Concurrent RSS (< 175 MB)
cargo test --package aegis-mesh --test test_5_agent_mesh_memory -- --nocapture

# 4. Benchmark Lebesgue Measure DAG Interval Calculation (< 5 µs)
cargo bench --package aegis-telemetry --bench bench_lebesgue_disaggregation

# 5. Verify 64-bit RAPL MSR Rollover Compensation Logic
cargo test --package aegis-telemetry --test test_rapl_rollover -- --nocapture

# 6. Verify Windows EcoQoS / Linux Nice QoS Settings
cargo test --package aegis-telemetry --test test_thread_qos -- --nocapture
```

### Quantitative Exit Criteria & System Guardrails for Phase 4:
```
+=============================================================================================================================+
|                                           PHASE 4 QUANTITATIVE EXIT CRITERIA & GUARDRAILS                                   |
+=============+===============================================+====================+==========================================+
| Guardrail   | Metric Description                            | Hard Exit Gate     | Verification Tooling & Method            |
+=============+===============================================+====================+==========================================+
| **G4-01**   | 5-Agent Mesh Concurrent Active RSS            | **< 175.0 MB**     | OS memory sampler under full load        |
| **G4-02**   | Cross-Process IPC Roundtrip Latency           | **< 1.8 µs (P99)** | Criterion SHM benchmark                  |
| **G4-03**   | Telemetry Engine CPU Overhead Tax             | **< 0.10% CPU**    | sysinfo process CPU audit                |
| **G4-04**   | Lebesgue Measure Calculation Latency          | **< 5.0 µs**       | criterion benchmark (100 DAG intervals)  |
| **G4-05**   | RAPL MSR Energy Accounting Monotonicity       | **100.0% (Zero)**  | Rollover simulation unit test            |
| **G4-06**   | SHM Head/Tail Cacheline Isolation Alignment  | **128-Byte Aligned**| Address offset modulo assertion          |
| **G4-07**   | Actor Mailbox High Watermark Throttling Point | **80.0% Capacity** | Backpressure suspension unit test        |
| **G4-08**   | Host UI Frame Drops / Stutter Under Full Load | **0.00% (Zero)**   | Thread QoS governor active verification  |
+=============+===============================================+====================+==========================================+
```
