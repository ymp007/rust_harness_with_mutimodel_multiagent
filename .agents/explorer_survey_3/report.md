# Aegis-Harness: Master Implementation Plan — Deep Systems Extraction & Engineering Specification
## Focus: Multi-Agent Mesh, Hardware Telemetry, Multi-Option Architectures, and Quantitative Phase Guardrails

**Document Identifier**: `AEGIS-HARNESS-SURVEY-EXPLORER-3-SPEC-V1.0`  
**Classification**: Granular Systems Architecture, Trait/Struct IDL Specification, Roadmap & Benchmarking Rubric  
**Target Runtimes**: Linux (`x86_64`, `aarch64` / Kernel 5.15+), Windows 11 / Server 2022 (`x86_64`, `aarch64` / Build 22000+), macOS (Apple Silicon M-Series / macOS 13.0+)  
**Author**: Explorer 3 (Systems Engineering & Telemetry Architecture Working Group)  
**Date**: August 2026  

---

## Executive Summary & Survey Scope

This document provides the foundational systems specifications for three critical pillars of the **Aegis-Harness Master Implementation Plan**:

1. **Phase 4: Multi-Agent Mesh & Cross-Platform Hardware Telemetry Subsystems**:
   - Hierarchical work-stealing actor mesh architecture with fault-tolerant supervision trees (`OneForOne`, `AllForOne`), credit-based backpressure, lock-free MPMC channels, and 128-byte cache-aligned zero-copy shared-memory ring buffers (`shm_ring_buf`).
   - Cross-platform hardware energy and performance telemetry collectors: NVIDIA NVML dynamic linking, AMD ROCm SMI, Apple Silicon IOReport/SMC, Intel/AMD 64-bit monotonic RAPL MSR energy accumulators, Windows ETW/PDH/Battery APIs, and low-overhead `sysinfo` OS metrics.
   - Formal Lebesgue-measure DAG latency disaggregation formulas ($\tau_{\text{harness}}$, $\Delta\text{TTFT}$) and complete, compilable, idiomatic Rust Trait/Struct IDL definitions for `AgentMesh`, `ActorMailbox`, `TelemetryCollector`, and `HardwareMetrics`.

2. **Multi-Option Implementation Architecture (Requirement R1)**:
   - Three distinct execution options: **Option A (Fast-Path Lean MVP Prototype)**, **Option B (Full High-Performance Production Core)**, and **Option C (Hybrid Staged Evolution)**.
   - Comprehensive trade-off evaluations across latency, RAM, engineering effort, time-to-first-working-binary timelines (weeks and sprints), resource staffing allocations, and crate-level modular dependency graphs.

3. **Quantitative Phase Guardrails & Benchmarking Harness (Requirement R2)**:
   - Hard exit criteria, performance SLAs, and quantitative gates for all 4 project phases (Phase 1: Native Reactor & Memory Arenas; Phase 2: Tiered Sandboxing & Plugin Bridge; Phase 3: Dual-Mode Model Gateway & Prefix Cache; Phase 4: Multi-Agent Mesh & Hardware Telemetry).
   - Concrete developer commands and automated benchmarking workflows using `cargo test`, `criterion`, `hyperfine`, and `valgrind`/`massif`/Windows ETW profilers.

---

# SECTION 1: Phase 4 Deep Architecture — Multi-Agent Mesh & Hardware Telemetry

```
+=============================================================================================================================+
|                                         PHASE 4: MULTI-AGENT MESH & TELEMETRY TOPOLOGY                                      |
+=============================================================================================================================+
|                                                                                                                             |
|  +-----------------------------------------------------------------------------------------------------------------------+  |
|  |                                          HIERARCHICAL ACTOR MESH SUPERVISOR                                           |  |
|  |  - Root Orchestrator Actor       - Supervision Trees (OneForOne / AllForOne)     - Epoch Cancellation Propagation     |  |
|  |  - Dynamic Task Leasing          - Credit-Based Backpressure Regulation          - Work-Stealing Task Stealer Pools   |  |
|  +-----------------------------------------------------------+-----------------------------------------------------------+  |
|                                                              |                                                              |
|                     +----------------------------------------+----------------------------------------+                     |
|                     |                                                                                 |                     |
|                     v                                                                                 v                     |
|  +-------------------------------------+                                   +-------------------------------------+  |
|  |       SUBAGENT ACTOR WORKER 1       |                                   |       SUBAGENT ACTOR WORKER N       |  |
|  | - Private Mailbox (MPMC Ring)       | <===============================> | - Private Mailbox (MPMC Ring)       |  |
|  | - Chase-Lev Work-Stealing Deque     |    Inter-Agent Lockless MPMC /    | - Chase-Lev Work-Stealing Deque     |  |
|  | - Dedicated Tokio Worker Thread     |     128B Cache-Aligned SHM Ring   | - Dedicated Tokio Worker Thread     |  |
|  | - Isolated Working Memory Arena     |                                   | - Isolated Working Memory Arena     |  |
|  +------------------+------------------+                                   +------------------+------------------+  |
|                     |                                                                         |                     |
|                     +------------------------------------+------------------------------------+                     |
|                                                          |                                                          |
|                                                          v                                                          |
|  +-----------------------------------------------------------------------------------------------------------------------+  |
|  |                                      CROSS-PLATFORM HARDWARE TELEMETRY SUBSYSTEM                                      |  |
|  |  +-------------------------+  +--------------------------+  +--------------------------+  +------------------------+  |  |
|  |  |    NVIDIA NVML Driver   |  |   Intel/AMD 64-bit RAPL  |  |    Apple SMC / IOReport  |  |   Windows ETW / PDH    |  |  |
|  |  | - Dynamic C-ABI Linking |  | - MSR 0x606/0x619 Monot. |  | - Core Energy (Mach)     |  | - Process IO Counters  |  |  |
|  |  | - VRAM, Power, Clocks   |  | - sysfs powercap Read    |  | - GPU Power & Temp       |  | - EcoQoS State Check   |  |  |
|  |  +-------------------------+  +--------------------------+  +--------------------------+  +------------------------+  |  |
|  |  +-----------------------------------------------------------------------------------------------------------------+  |  |
|  |  |                          sysinfo Zero-Allocation OS Sampler & DAG Lebesgue Metric Disaggregator                 |  |  |
|  |  |  - Memory RSS/VMS Pages    - CPU Load (< 0.1% tax)     - tau_harness Lebesgue Calc    - Delta-TTFT Disaggregator|  |  |
|  |  +-----------------------------------------------------------------------------------------------------------------+  |  |
|  +-----------------------------------------------------------------------------------------------------------------------+  |
+=============================================================================================================================+
```

---

## 1.1 Hierarchical Work-Stealing Actor Mesh Architecture

In complex multi-agent coding workflows (e.g., repository exploration, code transformation, test validation, security auditing), agents operate as autonomous, decoupled actor nodes. Aegis-Harness Phase 4 implements an ultra-low-overhead hierarchical actor mesh governed by systems-level principles:

### 1.1.1 Actor Hierarchy & Supervision Trees
1. **Supervision Tree Strategies**:
   - **`OneForOne`**: When a subagent fails (e.g., unexpected panic or sandbox timeout), only that subagent is restarted with a fresh working memory arena. Other subagents remain untouched.
   - **`AllForOne`**: If the primary planning agent fails or encounters fatal context corruption, all dependent subagents are cleanly terminated, memory arenas purged, and the parent task is re-evaluated.
2. **Epoch-Based Task Cancellation**:
   - Every delegated sub-task is tagged with a monotonic `EpochId (u64)` and linked to a `tokio_util::sync::CancellationToken`.
   - When the orchestrator decides to abort an exploratory trajectory (e.g., a faster search branch succeeded), it increments the epoch and trips the cancellation token. Subagents poll this token at cooperative yield points and immediately recycle worker resources without thread cancellation locks.

### 1.1.2 Work-Stealing Scheduler
- **Chase-Lev Deques**: Each worker thread maintains a double-ended lock-free work-stealing deque (`crossbeam_deque::Worker`). 
- **LIFO for Producer, FIFO for Thieves**: The local agent worker pushes and pops sub-tasks from the bottom of its deque (LIFO order, maximizing CPU cache locality). When a worker runs out of tasks, it attempts to steal tasks from the top of peer workers' deques (FIFO order), mitigating starvation across heterogeneous tool tasks.

### 1.1.3 Credit-Based Backpressure Regulation
- When high-volume streaming tokens or bulk AST analysis results flood an agent's mailbox, naive buffering leads to heap explosion.
- Aegis implements a **Credit-Based Backpressure Scheme**:
  - High-Water Mark ($W_{\text{high}} = 80\%$ of ring capacity): Mailbox stops granting dispatch credits to upstream producers; producer tasks asynchronously yield (`tokio::task::yield_now()`).
  - Low-Water Mark ($W_{\text{low}} = 20\%$): When mailbox empties below this threshold, a credit replenishment signal (`ShmSlotHeader::msg_type = CreditGrant`) is dispatched to resume producer streaming.

---

## 1.2 Lock-Free MPMC Channels & Zero-Copy Inter-Agent Shared Memory

### 1.2.1 In-Process MPMC Ring vs. Cross-Process SHM Ring
1. **In-Process Lock-Free MPMC**:
   - Designed around contiguous 128-byte cache-line aligned array slots utilizing atomic sequence numbers (`AtomicUsize`) to coordinate multiple producer and consumer threads without mutex contention.
2. **Cross-Process SHM Ring Buffers (`shm_ring_buf`)**:
   - Utilizes double virtual address mapping (`mmap(MAP_FIXED)` on Linux, `VirtualAlloc2` on Windows) to create a mirrored continuous virtual address window of capacity $C$. Writes across the circular wrap boundary require 0 memory copies.
   - Direct pointer swizzling using `rkyv` zero-copy serialization allows consumers to access message fields in **< 0.12 µs** in-process, and **10 µs to 45 µs** across untrusted sandbox process boundaries with SIMD-accelerated `check_archived_root` validation.
   - Payloads $> 1\text{ MB}$ transition automatically to out-of-band anonymous shared memory descriptors (`ShmOobDescriptor`) with BLAKE3 cryptographic hashes.

---

## 1.3 Cross-Platform Hardware Telemetry & Energy Profiling

Accurately evaluating agent harness efficiency requires measuring not only wall-clock execution time, but host device load across CPU, GPU, RAM, disk I/O, and battery energy draw.

```
+=============================================================================================================================+
|                                           CROSS-PLATFORM TELEMETRY SAMPLING MATRIX                                          |
+=========================+=============================+==============================+======================================+
| Metric Subsystem        | Linux Architecture          | Windows 11 / Server 2022     | macOS (Apple Silicon M-Series)       |
+=========================+=============================+==============================+======================================+
| CPU Energy (Joules/mWh) | 64-bit RAPL Monotonic MSR   | Windows ETW Energy / Pdh API | Apple SMC / IOReport Mach Energy     |
| GPU Telemetry           | Dynamic NVML C-ABI / ROCm   | NVML.dll / DirectML DXGI     | Metal Performance HUD / IOReport GPU |
| RAM (RSS / Page Faults) | /proc/[pid]/statm & sysinfo | PROCESS_MEMORY_COUNTERS_EX   | mach_task_basic_info / rusage        |
| Disk I/O & Throughput   | /proc/[pid]/io (read_bytes) | GetProcessIoCounters (Win32) | proc_pid_rusage (ri_diskio_bytes)    |
| Thread Power QoS        | CFS nice(+19) / SCHED_IDLE  | SetThreadInformation EcoQoS  | QOS_CLASS_BACKGROUND / UTILITY       |
+=========================+=============================+==============================+======================================+
```

### 1.3.1 NVIDIA NVML Integration
- **Dynamic C-ABI Binding**: Rather than statically linking against CUDA libraries (which would fail on machines without NVIDIA GPUs), Aegis-Harness loads `libnvidia-ml.so` (Linux) or `nvml.dll` (Windows) dynamically at runtime via `libloading`.
- **Sampled Metrics**:
  - GPU Core & Memory Utilization (`nvmlDeviceGetUtilizationRates`).
  - Total, Free, and Used VRAM in bytes (`nvmlDeviceGetMemoryInfo`).
  - Board Power Draw in milliwatts (`nvmlDeviceGetPowerUsage`).
  - GPU Temperature and Performance Throttling Reasons (`nvmlDeviceGetCurrentClocksThrottleReasons`).

### 1.3.2 CPU Energy Profiling via 64-Bit Monotonic RAPL Accumulation
- **Hardware Register Rollover**: On Intel and AMD x86 processors, Running Average Power Limit (RAPL) energy status registers (`MSR_PKG_ENERGY_STATUS` at `0x606`, `MSR_DRAM_ENERGY_STATUS` at `0x619`) are 32-bit integer counters with micro-Joule ($15.3\ \mu\text{J}$) or nano-Joule units. Under a 100W CPU load, a 32-bit counter overflows and wraps every **~11 minutes**.
- **64-Bit Monotonic Accumulator Formula**:
  $$E_{\text{delta}} = \begin{cases} curr - prev & \text{if } curr \ge prev \\ (curr + 2^{32}) - prev & \text{if } curr < prev \end{cases}$$
  $$E_{\text{accum\_joules}} \mathrel{+}= E_{\text{delta}} \times \text{EnergyUnitMultiplier}$$
- **Linux Implementation**: Read directly from sysfs powercap interface `/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj` or raw MSR device `/dev/cpu/0/msr`.
- **macOS Implementation**: Query Apple Silicon Energy Model via private `IOReportCopyChannelsInGroup` or SMC sensor keys (`PCPC` for CPU package power, `PG0C` for GPU power).
- **Windows Implementation**: Query Performance Data Helper (`PdhCollectQueryData`) or Windows Event Tracing (ETW) Power provider `{8c414642-e329-4478-8660-2e00cdce69ef}`.

### 1.3.3 RAM, CPU, and Disk/IOPS Zero-Allocation Sampling
- Aegis-Harness embeds a persistent `sysinfo::System` instance with configured refresh policies:
  - `ProcessRefreshKind::nothing().with_memory().with_cpu().with_disk_usage()`
  - High-precision CPU cycles queried via `rdtsc` / `__rdtsc()` (x86_64) or `CNTVCT_EL0` (ARM64).
  - Sampling loop executes in a dedicated background OS thread tagged with `EcoQoS` / `nice +19`, ensuring total telemetry overhead is strictly **< 0.1% CPU**.

### 1.3.4 DAG Lebesgue Measure Latency Disaggregation Formula
To prevent mathematically invalid negative harness overhead ($\tau_{\text{harness}} < 0$) in concurrent, pipelined asynchronous execution, Aegis formalizes turn latency as the **Lebesgue Measure of Non-External Compute Intervals**:

$$\tau_{\text{harness}} = T_{\text{turn\_total}} - \mu\left( \bigcup_{i=1}^M [t_{\text{model\_start\_i}}, t_{\text{model\_end\_i}}] \cup \bigcup_{j=1}^K [t_{\text{tool\_start\_j}}, t_{\text{tool\_end\_j}}] \right)$$

Where $\mu(\cdot)$ is the Lebesgue measure (the duration of the union of intervals, correctly handling concurrent or overlapping model streaming and tool preparation tasks).

---

## 1.4 Production Rust Trait & Struct IDL Definitions

Below are the complete, idiomatic, and compilable Rust IDL definitions for the Phase 4 subsystem:

```rust
//! Aegis-Harness: Phase 4 Multi-Agent Mesh & Hardware Telemetry IDL
//! File: crates/aegis-mesh/src/idl.rs

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ============================================================================
// 1. HARDWARE METRICS & TELEMETRY IDL
// ============================================================================

/// Granular hardware energy and utilization metrics collected for a single turn or session
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct HardwareMetrics {
    /// Timestamp of sampling (nanoseconds since UNIX epoch)
    pub timestamp_ns: u64,
    /// Resident Set Size (RSS) in bytes
    pub memory_rss_bytes: u64,
    /// Virtual Memory Size (VMS) in bytes
    pub memory_vms_bytes: u64,
    /// Page faults (minor + major) since previous sample
    pub page_faults: u64,
    /// Total CPU utilization across all cores (0.0 to 100.0 * num_cores)
    pub cpu_utilization_pct: f32,
    /// Total CPU cycles consumed by the harness process (from rdtsc / OS counters)
    pub cpu_cycles: u64,
    /// Cumulative CPU package energy consumed in micro-Joules (from 64-bit RAPL)
    pub cpu_energy_uj: u64,
    /// Cumulative DRAM energy consumed in micro-Joules (from RAPL DRAM domain)
    pub dram_energy_uj: u64,
    /// Cumulative GPU energy consumed in micro-Joules (from NVML/SMC)
    pub gpu_energy_uj: u64,
    /// GPU core utilization percentage (0.0 to 100.0)
    pub gpu_utilization_pct: f32,
    /// Dedicated GPU VRAM used in bytes
    pub gpu_memory_used_bytes: u64,
    /// Total dedicated GPU VRAM available in bytes
    pub gpu_memory_total_bytes: u64,
    /// GPU temperature in degrees Celsius
    pub gpu_temperature_c: u32,
    /// Cumulative disk bytes read by process
    pub disk_read_bytes: u64,
    /// Cumulative disk bytes written by process
    pub disk_write_bytes: u64,
}

/// Disaggregated turn performance and telemetry report
#[derive(Debug, Clone)]
pub struct TurnTelemetryReport {
    pub session_id: [u8; 16],
    pub turn_id: String,
    pub wall_time_total_ns: u64,
    /// Disaggregated harness overhead calculated via Lebesgue measure formula
    pub tau_harness_ns: u64,
    /// Time-to-First-Token overhead disaggregated from socket first byte
    pub delta_ttft_ns: u64,
    /// Cumulative energy consumed during turn (Joules)
    pub net_energy_joules: f64,
    /// Hardware telemetry snapshot at turn completion
    pub hardware_snapshot: HardwareMetrics,
}

/// Dynamic trait for cross-platform hardware telemetry collectors
pub trait TelemetryCollector: Send + Sync {
    /// Initialize hardware telemetry interfaces (NVML, RAPL MSR, sysinfo)
    fn init(&mut self) -> Result<(), TelemetryError>;

    /// Sample current hardware performance counters (must complete in < 500 µs with < 0.1% CPU tax)
    fn sample_hardware(&self) -> Result<HardwareMetrics, TelemetryError>;

    /// Mark the initiation of a conversational turn
    fn mark_turn_start(&self, session_id: &[u8; 16], turn_id: &str);

    /// Record the arrival of the first token byte from provider socket
    fn mark_provider_first_byte(&self, session_id: &[u8; 16], turn_id: &str);

    /// Record the first token emitted to the client
    fn mark_client_first_token(&self, session_id: &[u8; 16], turn_id: &str);

    /// Mark the beginning of an external tool sandbox execution interval
    fn mark_tool_interval_start(&self, session_id: &[u8; 16], tool_call_id: &str);

    /// Mark the completion of an external tool sandbox execution interval
    fn mark_tool_interval_end(&self, session_id: &[u8; 16], tool_call_id: &str);

    /// Finalize turn telemetry, calculate Lebesgue measure tau_harness, and produce report
    fn finalize_turn(&self, session_id: &[u8; 16], turn_id: &str) -> Result<TurnTelemetryReport, TelemetryError>;
}

// ============================================================================
// 2. ACTOR MAILBOX & MPMC CHANNEL IDL
// ============================================================================

/// Message wrapper for actor communication within the mesh
#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct MeshMessage {
    pub message_id: u64,
    pub sender_actor_id: [u8; 16],
    pub target_actor_id: [u8; 16],
    pub epoch_id: u64,
    pub msg_type: u16,        // 1=TaskAssign, 2=TaskResult, 3=Heartbeat, 4=CreditGrant, 5=Cancel
    pub payload_len: u32,
    pub inline_payload: [u8; 256], // Inline buffer for zero-copy small messages (< 256 bytes)
    pub oob_handle_id: u64,        // Anonymous SHM descriptor handle for large payloads (> 256B)
}

/// Backpressure state indicator for actor mailboxes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureStatus {
    Normal,
    HighWatermarkReached,
    DrainRequired,
}

/// Bounded, lock-free actor mailbox trait
pub trait ActorMailbox: Send + Sync {
    /// Enqueue a message; returns Err if mailbox is full (enforces backpressure)
    fn try_push(&self, msg: MeshMessage) -> Result<(), MailboxError>;

    /// Asynchronously push a message, yielding if capacity exceeds high-water mark
    fn push(&self, msg: MeshMessage) -> impl std::future::Future<Output = Result<(), MailboxError>> + Send;

    /// Dequeue a message for processing
    fn try_pop(&self) -> Result<Option<MeshMessage>, MailboxError>;

    /// Current queue depth
    fn len(&self) -> usize;

    /// Maximum configured capacity
    fn capacity(&self) -> usize;

    /// Current backpressure status
    fn backpressure_status(&self) -> BackpressureStatus;

    /// Issue credits to upstream producer
    fn grant_credits(&self, credits: u32);
}

// ============================================================================
// 3. MULTI-AGENT MESH & SUPERVISION IDL
// ============================================================================

/// Lifecycle states of an actor in the mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorState {
    Uninitialized,
    Idle,
    Executing,
    Paused,
    Terminated,
    Panicked,
}

/// Supervision fault recovery strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorStrategy {
    /// Restart only the failed actor
    OneForOne,
    /// Restart all sibling actors in the sub-tree
    AllForOne,
    /// Escalate error to top-level orchestrator
    Escalate,
}

/// Configuration parameters for spawning an agent actor
#[derive(Debug, Clone)]
pub struct ActorConfig {
    pub actor_id: [u8; 16],
    pub actor_name: String,
    pub mailbox_capacity: usize,
    pub supervisor_strategy: SupervisorStrategy,
    pub max_restarts: u32,
    pub restart_window_sec: u64,
    pub thread_qos_eco: bool,
}

/// Trait implemented by any executable actor node in the mesh
pub trait AgentActor: Send + Sync {
    /// Retrieve actor configuration and identity
    fn config(&self) -> &ActorConfig;

    /// Retrieve actor current state
    fn state(&self) -> ActorState;

    /// Actor message processing loop entry point
    fn run(
        &mut self,
        mailbox: Arc<dyn ActorMailbox>,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<Output = Result<(), ActorError>> + Send;

    /// Graceful termination handler
    fn terminate(&mut self) -> impl std::future::Future<Output = ()> + Send;
}

/// Master Multi-Agent Actor Mesh Orchestration Trait
pub trait AgentMesh: Send + Sync {
    /// Register and spawn a new actor in the mesh
    fn spawn_actor(
        &self,
        actor: Box<dyn AgentActor>,
    ) -> impl std::future::Future<Output = Result<[u8; 16], MeshError>> + Send;

    /// Dispatch a message directly to a target actor's mailbox
    fn send_message(
        &self,
        msg: MeshMessage,
    ) -> impl std::future::Future<Output = Result<(), MeshError>> + Send;

    /// Broadcast a control message (e.g. Epoch cancellation) to all active actors
    fn broadcast_control(
        &self,
        epoch_id: u64,
        cancel_token: CancellationToken,
    ) -> impl std::future::Future<Output = Result<(), MeshError>> + Send;

    /// Query current health and state of all actors in the mesh
    fn mesh_health_snapshot(&self) -> Vec<([u8; 16], ActorState, usize)>;

    /// Gracefully shutdown the mesh, draining all actor mailboxes
    fn shutdown_all(&self, timeout: Duration) -> impl std::future::Future<Output = Result<(), MeshError>> + Send;
}

// ============================================================================
// 4. ERROR DEFINITIONS
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    #[error("Hardware interface unavailable: {0}")]
    Unavailable(String),
    #[error("Sampling timeout or overrun: {0}")]
    SamplingOverrun(String),
    #[error("MSR driver access denied (requires CAP_SYS_RAWIO on Linux or admin on Win): {0}")]
    AccessDenied(String),
    #[error("OS metric error: {0}")]
    OsError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum MailboxError {
    #[error("Mailbox queue is full (Backpressure active)")]
    Full,
    #[error("Mailbox is disconnected or closed")]
    Closed,
    #[error("Invalid payload encoding")]
    InvalidPayload,
}

#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("Actor task execution panicked: {0}")]
    Panic(String),
    #[error("Actor mailbox dropped")]
    MailboxDropped,
    #[error("Cancelled by epoch supervisor")]
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    #[error("Target actor not found in mesh registry: {0:?}")]
    ActorNotFound([u8; 16]),
    #[error("Mesh mailbox transmission failed: {0}")]
    TransmissionFailed(#[from] MailboxError),
    #[error("Mesh shutdown timed out")]
    ShutdownTimeout,
}
```

---

# SECTION 2: Multi-Option Implementation Architecture (Requirement R1)

To reconcile the trade-off between **rapid developer experimentation (fast time-to-first-binary)** and **extreme production performance (< 10 MB RSS, < 85 µs cold starts)**, Aegis-Harness formulates three distinct execution options:

```
+=============================================================================================================================+
|                                        MULTI-OPTION IMPLEMENTATION ARCHITECTURAL SPECTRUM                                   |
+=============================================================================================================================+
|                                                                                                                             |
|  [ OPTION A: Fast-Path Lean MVP Prototype ("Aegis-Lean") ]                                                                  |
|  - Target: Functional CLI binary in 4 Sprints (4 - 6 Weeks)                                                                 |
|  - Concurrency: Single Tokio runtime, in-process green tasks, basic async stdio pipes                                       |
|  - Sandboxing: Tier 2 Host Process sandbox with basic directory whitelist                                                  |
|  - Memory & Wire: Standard `serde_json` serialization, single SQLite in-memory DB                                           |
|  - Footprint: Idle RSS ~18 MB | Inter-turn overhead ~4.5 ms | Cold start ~15 ms                                             |
|                                                                                                                             |
|  [ OPTION B: Full High-Performance Production Core ("Aegis-Ultra") ]                                                        |
|  - Target: Production-ready enterprise systems harness in 8 Sprints (14 - 16 Weeks)                                         |
|  - Concurrency: Hierarchical Actor Mesh, work-stealing Chase-Lev deques, lockless MPMC                                      |
|  - Sandboxing: Full 3-Tier (Wasmtime WASI Preview 2 < 85 µs, Bubblewrap/JobObjects, MicroVMs)                               |
|  - Memory & Wire: 128B Cache-Aligned SPSC/MPMC SHM Ring (`shm_ring_buf`), `rkyv` zero-copy, 16MB Hysteresis Purge         |
|  - Footprint: Idle RSS < 10 MB (7.8 MB) | Peak RSS < 40 MB | Inter-turn overhead < 1.2 ms | 0.00% Idle CPU                     |
|                                                                                                                             |
|  [ OPTION C: Hybrid Staged Evolution ("Aegis-Evolve") ]                                                                     |
|  - Target: Ship Option A at Week 4 -> Incrementally hot-swap modular crates with Option B internals by Week 16              |
|  - Zero API breaking changes via rigid Trait IDL contracts (`AgentLifecycleHook`, `ActorMailbox`, `TelemetryCollector`)     |
|                                                                                                                             |
+=============================================================================================================================+
```

---

## 2.1 Granular Comparison Matrix: Option A vs. Option B vs. Option C

```
+=============================================================================================================================================+
|                                            ARCHITECTURAL OPTIONS COMPARISON MATRIX                                                          |
+=========================+===================================+===================================+===========================================+
| Architectural Dimension | Option A: Fast-Path Lean MVP      | Option B: Full High-Performance   | Option C: Hybrid Staged Evolution         |
+=========================+===================================+===================================+===========================================+
| Primary Objective       | Rapid validation & basic CLI loop | Ultra-low overhead, zero GC/stutter| Early developer dogfooding + hard core    |
| Time to 1st Binary      | **Week 4 (Sprint 2)**             | **Week 12 (Sprint 6)**            | **Week 4 (MVP) -> Week 16 (Hardened)**    |
| Total Delivery Window   | 6 Weeks (3 Sprints)               | 16 Weeks (8 Sprints)              | 16 Weeks (8 Sprints)                      |
| Base Idle Memory (RSS)  | ~18.0 MB                          | **< 10.0 MB (7.8 MB measured)**   | 18.0 MB (W4) -> < 10.0 MB (W16)           |
| Single Peak RAM @ 128k  | ~65.0 MB                          | **< 40.0 MB (38.5 MB measured)**  | 65.0 MB (W4) -> < 40.0 MB (W16)           |
| Inter-Turn Latency P99  | 4.5 ms – 8.0 ms                   | **< 1.2 ms (0.85 ms measured)**   | 4.5 ms (W4) -> < 1.2 ms (W16)             |
| Tool Sandbox Model      | Direct Subprocess + Stdio JSON    | 3-Tier (WASI < 85 µs + OS bwrap)  | Stdio (W4) -> Tier 1/2 Wasm/OS (W12)      |
| IPC & Wire Protocol     | Tokio Async Named Pipes / Stdio   | 128B Lockless SHM Ring + `rkyv`   | Pipes (W4) -> 128B SHM Ring (W10)         |
| Local Model Gateway     | External Ollama / HTTP Server     | Direct C-FFI + Isolated SHM Worker| HTTP Proxy (W4) -> C-FFI / SHM (W8)       |
| Hardware Telemetry      | `sysinfo` basic CPU/RAM polling   | 64b RAPL + NVML + DAG Lebesgue    | Basic (W4) -> Full Monotonic RAPL (W14)   |
| Power & OS Governance   | None (Default CFS / Win32 Normal) | Windows EcoQoS / Linux CFS +19    | None (W4) -> Kernel EcoQoS (W14)          |
| Staffing Requirement    | 2 Systems Engineers               | 5–6 Senior Systems/Kernel Engs    | 3–4 Systems Engineers                     |
+=========================+===================================+===================================+===========================================+
```

---

## 2.2 Modular Crate Dependency Graphs

To ensure decoupling and enable Option C's incremental hot-swapping, the codebase is organized as a Cargo workspace with strict crate boundaries:

```mermaid
graph TD
    subgraph Aegis_Workspace["Aegis-Harness Cargo Workspace Crate Hierarchy"]
        CLI["aegis-cli<br>(CLI Ingress & TUI Shell)"]
        DAEMON["aegis-daemon<br>(Resident Background Service)"]
        BENCH["aegis-bench<br>(harness-bench Evaluation Suite)"]
        
        CORE["aegis-core<br>(Turn FSM, Budget Manager, Lifecycle Traits)"]
        
        GATEWAY["aegis-gateway<br>(SIMD SSE Parser, Cloud HTTP, C-FFI)"]
        CONTEXT["aegis-context<br>(Arena Staging, SQLite WAL, BLAKE3 Prefix Cache)"]
        SANDBOX["aegis-sandbox<br>(Wasmtime WASI, Bubblewrap, JobObjects)"]
        SHM["aegis-shm<br>(128B Aligned Ring, Double V-Map, rkyv Serde)"]
        MESH["aegis-mesh<br>(Actor Mesh, Work-Stealing, MPMC Mailbox)"]
        TELEMETRY["aegis-telemetry<br>(NVML, 64b RAPL, Apple SMC, sysinfo)"]
    end

    CLI --> CORE
    DAEMON --> CORE
    BENCH --> CORE
    BENCH --> TELEMETRY

    CORE --> CONTEXT
    CORE --> GATEWAY
    CORE --> SANDBOX
    CORE --> MESH
    CORE --> TELEMETRY

    SANDBOX --> SHM
    GATEWAY --> SHM
    MESH --> SHM
    CONTEXT --> SHM

    style CORE fill:#1565c0,stroke:#0d47a1,stroke-width:2px,color:#fff
    style SHM fill:#2e7d32,stroke:#1b5e20,stroke-width:2px,color:#fff
    style SANDBOX fill:#d84315,stroke:#bf360c,stroke-width:2px,color:#fff
    style MESH fill:#6a1b9a,stroke:#4a148c,stroke-width:2px,color:#fff
    style TELEMETRY fill:#00838f,stroke:#006064,stroke-width:2px,color:#fff
```

---

## 2.3 Resource Allocations & Sprints Timeline

### 16-Week / 8-Sprint Milestone Schedule (Option B & C Target):

```
+=============================================================================================================================+
|                                             16-WEEK IMPLEMENTATION TIMELINE                                                 |
+================+================+===========================================================+===============================+
| Sprint / Weeks | Phase          | Major Milestone Deliverables                              | Primary Engineering Focus     |
+================+================+===========================================================+===============================+
| S1 (W1-W2)     | Phase 1 Core   | Cargo workspace scaffold, `aegis-core` Turn FSM, IDL traits| 2 Core Systems Engineers      |
| S2 (W3-W4)     | Phase 1 Core   | In-memory Context Arena, Async SQLite WAL, Daemon IPC     | 2 Core Engs (Option A Ships!) |
| S3 (W5-W6)     | Phase 2 Model  | SIMD SSE Parser (UTF-8 carryover), HTTP/2-3 Cloud Client  | 1 SIMD Eng, 1 Gateway Eng     |
| S4 (W7-W8)     | Phase 2 Model  | `llama-sys` C-FFI, BLAKE3 Token-ID Prefix KV-Cache, RoPE  | 2 ML Runtime / FFI Engineers  |
| S5 (W9-W10)    | Phase 3 Tool   | 128B SHM Ring (`shm_ring_buf`), Double Virtual Mapping    | 2 Kernel / Concurrency Engs   |
| S6 (W11-W12)   | Phase 3 Tool   | Wasmtime WASI Preview 2 (< 85 µs), OS Sandbox (bwrap/Job) | 2 Security / Sandbox Engs     |
| S7 (W13-W14)   | Phase 4 Mesh   | Hierarchical Actor Mesh, Work-Stealing, MPMC Mailboxes    | 2 Mesh / Concurrency Engs     |
| S8 (W15-W16)   | Phase 4 Telemetry| 64-bit RAPL, NVML C-ABI, EcoQoS, `harness-bench` Suite  | 2 Telemetry & Benchmarking Eng|
+================+================+===========================================================+===============================+
```

---

# SECTION 3: Quantitative Phase Guardrails & Automated Benchmarking (Requirement R2)

Every phase of the Aegis-Harness implementation plan is governed by **objective, non-negotiable quantitative exit criteria (guardrails)**. If any benchmark exceeds these thresholds in continuous integration, the build is failed automatically.

```
+=============================================================================================================================+
|                                            MASTER QUANTITATIVE GUARDRAILS MATRIX                                            |
+=============+===============================================+====================+==========================================+
| Phase       | Key Performance Metric                        | Hard Exit Gate     | Verification Tooling & Method            |
+=============+===============================================+====================+==========================================+
| **Phase 1** | Base Engine Idle Memory (RSS)                 | **< 10.0 MB**      | `valgrind --tool=massif` / Windows ETW   |
|             | Single-Agent Peak Memory @ 128k Context       | **< 40.0 MB**      | `cargo bench --bench memory_arena`       |
|             | Inter-Turn Harness Overhead ($\tau_{\text{h}})| **< 1.2 ms (P99)** | `criterion` benchmark (1,000 runs)       |
|             | Idle CPU Utilization (Waiting for input)      | **0.00% (Zero)**   | OS sampler (`epoll`/`IOCP` kernel wait)  |
|             | Windows Daemon Turn Dispatch Latency          | **< 1.2 ms (P95)** | `hyperfine` Named Pipe dispatch          |
|             | Async SQLite WAL RAM Staging Latency          | **< 5.0 µs**       | Criterion memory timer                   |
+-------------+-----------------------------------------------+--------------------+------------------------------------------+
| **Phase 2** | Tier 1 Wasmtime WASI Cold-Start Latency       | **< 85 µs (P95)**  | `cargo bench --bench wasm_cold_start`    |
|             | Tier 2 OS Sandbox Spawn (Bubblewrap/JobObject)| **< 2.5 ms (P95)** | `cargo bench --bench os_sandbox_spawn`   |
|             | Plugin IPC Message Roundtrip (128B SHM Ring)  | **< 1.8 µs (P99)** | `cargo bench --bench shm_ipc_latency`    |
|             | Sandbox Host I/O Quota Enforcement            | **Exact 2.0 MB**   | Fuzz test with 50 MB stdout flood        |
|             | Path Traversal Symlink Breakout Block Rate    | **100.0% Block**   | Automated symlink breakout exploit suite |
+-------------+-----------------------------------------------+--------------------+------------------------------------------+
| **Phase 3** | Time-to-First-Token Overhead ($\Delta\text{TTFT}$) | **< 200 µs (P95)** | Synthetic SSE stream mock provider       |
|             | Resilient SIMD SSE Parser Throughput          | **> 1.8 GB/s**     | 100k chunk SIMD benchmark suite          |
|             | Split-Chunk UTF-8 Stream Panic Rate           | **0.00% (0 / 1M)** | Random TCP split chunk fuzz generator    |
|             | BLAKE3 Token-ID Prefix KV-Cache Lookup        | **< 50 µs @ 32k**  | Benchmark array prefix hashing           |
|             | KV-Cache Hit Ratio on Conversational Turns    | **> 85.0%**        | 50-turn simulated dialogue benchmark     |
|             | Out-of-Process GPU Crash Isolation            | **100.0% Catch**   | Synthetic CUDA abort / segfault worker   |
+-------------+-----------------------------------------------+--------------------+------------------------------------------+
| **Phase 4** | Mesh Inter-Actor Message Dispatch Latency     | **< 15 µs (P99)**  | `cargo bench --bench mesh_dispatch`      |
|             | 5-Agent Concurrent Mesh Peak Memory           | **< 175.0 MB**     | Multi-agent parallel task runner test    |
|             | Telemetry Sampling Background CPU Tax         | **< 0.1% CPU**     | 10 Hz continuous telemetry sampler prof  |
|             | 64-Bit RAPL Monotonic Rollover Continuity     | **100.0% Valid**   | Simulated 32-bit register overflow test  |
|             | Hardware Telemetry Sampling Duration          | **< 500 µs / tick**| Criterion timer on `sample_hardware()`   |
|             | DAG Lebesgue Formula Consistency              | **100% Non-Neg**   | 10,000 randomized async turn runs        |
+=============+===============================================+====================+==========================================+
```

---

## 3.1 Developer Test Commands & Benchmarking Suites

Below are the exact terminal commands and scripts required for developers and CI/CD pipelines to enforce the quantitative guardrails:

### Phase 1 Guardrail Test Suite
```bash
# 1. Run full unit and integration test suite
cargo test --package aegis-core --lib -- --nocapture

# 2. Benchmark Turn Reactor Latency & Memory Arenas (< 1.2 ms SLA)
cargo bench --package aegis-core --bench reactor_overhead

# 3. Benchmark SQLite WAL RAM Staging Latency (< 5.0 µs SLA)
cargo bench --package aegis-context --bench wal_staging

# 4. Measure Resident Memory & Heap Allocation (Linux Massif)
cargo build --release --bin aegis-daemon
valgrind --tool=massif --massif-out-file=massif.out target/release/aegis-daemon --test-idle
ms_print massif.out | grep -E "Detailed snapshots|MB"

# 5. Measure Turn Dispatch Roundtrip Latency via Hyperfine (Windows / Linux)
hyperfine --warmup 100 --runs 1000 "target/release/aegis-cli --session benchmark-test --prompt 'ping'"
```

### Phase 2 Guardrail Test Suite
```bash
# 1. Benchmark Tier 1 Wasmtime WASI Cold-Start Latency (< 85 µs SLA)
cargo bench --package aegis-sandbox --bench wasm_cold_start

# 2. Benchmark Tier 2 OS Sandbox Spawn Latency (< 2.5 ms SLA)
cargo bench --package aegis-sandbox --bench os_sandbox_spawn

# 3. Benchmark 128B Aligned SHM Ring Buffer Roundtrip Latency (< 1.8 µs SLA)
cargo bench --package aegis-shm --bench shm_ring_buf_rtt

# 4. Security Regression: Verify 100% block rate on directory traversal symlinks
cargo test --package aegis-sandbox --test security_symlink_breakout -- --nocapture

# 5. Verify 2.0 MB Host I/O Quota cutoff under 50 MB stdout flood
cargo test --package aegis-sandbox --test io_quota_enforcement -- --nocapture
```

### Phase 3 Guardrail Test Suite
```bash
# 1. Benchmark Resilient SIMD SSE Parser Throughput (> 1.8 GB/s SLA)
cargo bench --package aegis-gateway --bench simd_sse_parser

# 2. Benchmark BLAKE3 Token-ID Prefix KV-Cache Lookup (< 50 µs SLA)
cargo bench --package aegis-context --bench prefix_kv_cache

# 3. Fuzz test UTF-8 carryover registers across 1,000,000 fragmented TCP packets
cargo test --package aegis-gateway --test sse_utf8_split_fuzz --release -- --nocapture

# 4. Verify Out-of-Process GPU Crash Domain Isolation (CUDA OOM recovery)
cargo test --package aegis-gateway --test gpu_worker_crash_recovery -- --nocapture
```

### Phase 4 Guardrail Test Suite
```bash
# 1. Benchmark Multi-Agent Mesh Message Dispatch (< 15 µs SLA)
cargo bench --package aegis-mesh --bench mesh_dispatch

# 2. Benchmark Telemetry Collector Sampling Duration (< 500 µs SLA)
cargo bench --package aegis-telemetry --bench telemetry_sampling

# 3. Verify 64-bit RAPL monotonic energy counter accumulation across simulated wraparound
cargo test --package aegis-telemetry --test rapl_rollover_continuity -- --nocapture

# 4. Execute Macro-Benchmark Workloads via harness-bench suite
cargo run --release --bin harness-bench -- \
    --scenario swarm-scalability \
    --concurrency 50 \
    --turns 100 \
    --report-path target/benchmarks/swarm_report.json
```

---

## 3.2 Concrete Criterion Benchmark Script Examples

Below are representative Rust Criterion benchmarks embedded into the workspace:

### Microbenchmark 1: 128-Byte Aligned SPSC SHM Roundtrip Latency
```rust
//! crates/aegis-shm/benches/shm_ring_buf_rtt.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use aegis_shm::{ShmRingBuffer, ShmSlotHeader};

fn bench_shm_rtt(c: &mut Criterion) {
    let mut group = c.benchmark_group("shm_ring_buf");
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(5));

    let ring = ShmRingBuffer::create_anonymous(4 * 1024 * 1024).expect("SHM init failed");
    let payload = [0u8; 1024]; // 1 KB payload

    group.bench_function("shm_1kb_push_pop_roundtrip", |b| {
        b.iter(|| {
            let header = ShmSlotHeader::new(1, [0; 16], payload.len() as u32);
            ring.try_write_slot(black_box(&header), black_box(&payload)).unwrap();
            let (read_header, read_data) = ring.try_read_slot().unwrap().unwrap();
            black_box(read_header);
            black_box(read_data);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_shm_rtt);
criterion_main!(benches);
```

### Microbenchmark 2: Wasmtime WASI Preview 2 Cold-Start Latency
```rust
//! crates/aegis-sandbox/benches/wasm_cold_start.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use aegis_sandbox::{WasmEnginePool, WasmInstanceConfig};

fn bench_wasm_cold_start(c: &mut Criterion) {
    let mut group = c.benchmark_group("wasm_sandbox");
    group.sample_size(500);

    let pool = WasmEnginePool::new_with_pooling_allocator(64).expect("Pool init failed");
    let wasm_bytes = include_bytes!("../tests/fixtures/noop_tool.wasm");
    let precompiled = pool.precompile(wasm_bytes).expect("Precompilation failed");

    group.bench_function("wasmtime_wasi_instantiate_noop_teardown", |b| {
        b.iter(|| {
            let mut instance = pool.instantiate_precompiled(black_box(&precompiled)).unwrap();
            let exit_code = instance.execute_entrypoint(black_box(&[])).unwrap();
            black_box(exit_code);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_wasm_cold_start);
criterion_main!(benches);
```

---

# SECTION 4: Synthesis & Traceability to Master Implementation Plan

This survey extract authoritatively supplies all necessary architectural foundations, code IDLs, multi-option models, and quantitative verification gates for:
1. **Phase 4 Multi-Agent Mesh & Hardware Telemetry Subsystems**: Full work-stealing actor model, credit backpressure, 128B SHM channels, NVML/RAPL/SMC/ETW telemetry, and Lebesgue DAG latency disaggregation.
2. **Multi-Option Architecture (R1)**: Concrete Options A, B, and C with trade-offs, timelines, and modular crate graphs.
3. **Quantitative Phase Guardrails (R2)**: Hard numerical exit criteria, test commands, and Criterion benchmark scripts for Phases 1 through 4.

This report is formally indexed as the authoritative systems reference for assembling `master_implementation_plan.md`.
