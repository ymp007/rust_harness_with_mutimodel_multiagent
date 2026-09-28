# Aegis-Harness Architectural & Specification Extraction Report: Phase 2
## Tiered Sandboxing, Multi-Platform OS Process Confinement & Model Context Protocol (MCP) Plugin Bridge

**Document Identifier**: `AEGIS-SPEC-PHASE2-TIERED-SANDBOX-PLUGIN-BRIDGE-V1.0`  
**Classification**: Granular Systems Architectural Specification & Implementation Reference  
**Author**: Systems Architecture & Runtime Engineering Working Group (Explorer Survey 2)  
**Target Runtimes**: 
- Windows 11 / Server 2022 (x86_64, aarch64 / Build 22000+)
- Linux (x86_64, aarch64 / Kernel 5.15+ with Landlock & cgroups v2)
- macOS 13.0+ (Apple Silicon M-Series / Intel x86_64)
**Scope**: Phase 2 Tiered Sandboxing (Tier 1 Wasmtime WASI, Tier 2 OS Native Sandboxes), Model Context Protocol (MCP) Multi-Transport Subsystem, Async Process Lifecycle Supervision, and Rust IDL Trait Contracts.

---

## Table of Contents
1. [Executive Summary & Phase 2 System Architecture](#1-executive-summary--phase-2-system-architecture)
2. [Tier 1: In-Process WebAssembly Micro-Sandbox (Wasmtime WASI Preview 2)](#2-tier-1-in-process-webassembly-micro-sandbox-wasmtime-wasi-preview-2)
   - 2.1 Runtime Engine Configuration & Cranelift AOT Compilation
   - 2.2 Instance Pooling Allocator & 64 KB Virtual Guard Page Optimization
   - 2.3 Dual Interruption Model: CPU Fuel Metering vs. Epoch-Based Interruption Timers
   - 2.4 Linear Memory Limits & 2 MB Hard Host I/O Quota Backpressure
   - 2.5 Component Model & WIT Interface Definitions
3. [Tier 2: Multi-Platform OS Process Sandboxing Subsystem](#3-tier-2-multi-platform-os-process-sandboxing-subsystem)
   - 3.1 Windows Subsystem: Win32 Job Objects & Restricted Security Tokens
     - 3.1.1 Job Object Creation & Limit Configuration
     - 3.1.2 CPU Rate Control & Power Governance
     - 3.1.3 Restricted Security Token Generation & Privilege Stripping
     - 3.1.4 Process Spawning & Job Association Pipeline
   - 3.2 Linux Subsystem: Bubblewrap, Namespaces, Seccomp-BPF & Landlock LSM
     - 3.2.1 Unprivileged User Namespaces & Mount Topology
     - 3.2.2 Seccomp-BPF Syscall Filter Construction
     - 3.2.3 Landlock LSM & Path Confinement (`openat2 RESOLVE_BENEATH`)
     - 3.2.4 cgroups v2 Resource Enforcement (`memory.max`, `memory.high`, `cpu.max`)
   - 3.3 macOS Subsystem: Apple Seatbelt & Scheme SBPL Profiles
     - 3.3.1 Sandbox Profile Language (SBPL) Ruleset Architecture
     - 3.3.2 C-FFI `sandbox_init_with_parameters` Confinement Pipeline
4. [Model Context Protocol (MCP) Multi-Transport Client & Server Subsystem](#4-model-context-protocol-mcp-multi-transport-client--server-subsystem)
   - 4.1 JSON-RPC 2.0 Protocol State Machine & Message Framing
   - 4.2 Standard I/O (Stdio) Transport Architecture
   - 4.3 Unix Domain Socket (UDS) Transport Architecture
   - 4.4 Windows Named Pipes Transport (`\\.\pipe\...`) Architecture
   - 4.5 Dynamic Tool Discovery, Schema Registration & Security Validation
5. [Async Process Lifecycle, Cancellation, Ring Buffering & Large Payload Streaming](#5-async-process-lifecycle-cancellation-ring-buffering--large-payload-streaming)
   - 5.1 Asynchronous Signal Handling & 2-Stage Graceful Cancellation Pipeline
   - 5.2 Timeout Supervisor & Orphan Process Reclamation
   - 5.3 Non-Blocking I/O Ring Buffering & Deadlock Defense
   - 5.4 High-Capacity Payload Streaming (> 10 MB - 50 MB+) & Out-of-Band (OOB) SHM Descriptors
6. [Complete Rust Trait & Struct IDL Specifications](#6-complete-rust-trait--struct-idl-specifications)
   - 6.1 `PluginHost`, `PluginInstance`, and `PluginManifest` Traits
   - 6.2 `SandboxedProcess`, `ProcessSpawner`, and `ProcessLimits` Traits
   - 6.3 `McpTransport`, `McpClient`, and `McpFraming` Traits
   - 6.4 `IsolationPolicy`, `PlatformSandboxConfig`, and `CapabilityPermission` Types
7. [Phase 2 Implementation Roadmap, Concrete Deliverables & Quantitative Guardrails](#7-phase-2-implementation-roadmap-concrete-deliverables--quantitative-guardrails)
   - 7.1 Component File Hierarchy
   - 7.2 Step-by-Step Developer Implementation Instructions
   - 7.3 Automated Test Matrix & Verification Commands
   - 7.4 Quantitative Exit Criteria & System Guardrails

---

## 1. Executive Summary & Phase 2 System Architecture

Phase 2 of the Aegis-Harness project bridges the native asynchronous reactor core to external and embedded tool execution environments. In existing agentic coding frameworks (such as Claude Code, Cursor, Cline, OpenHands, and SWE-agent), tool invocation is characterized by either **zero security sandboxing** (exposing the host operating system to prompt injection, environment exfiltration, and arbitrary file deletion) or **catastrophically slow containerization** (incurring 1.5s to 15s Docker provisioning overhead).

Aegis-Harness resolves this trade-off by establishing a **Tiered Sandboxing & Multi-Transport Plugin Bridge**:
1. **Tier 1 (In-Process Wasm Sandbox)**: Leverages `Wasmtime` with WASI Preview 2 to run high-frequency, in-process deterministic utilities, diff patchers, AST analyzers, and custom plugins with **< 85 µs cold-start latency**, strict linear memory limits (16 MB), CPU fuel metering, epoch timers, and hard 2 MB host I/O streaming quotas.
2. **Tier 2 (Native OS Process Sandboxing)**: Provides isolated host execution for full POSIX development tools (`git`, `gcc`, `rustc`, `pytest`, `cargo`, and standard MCP servers) with **< 2.5 ms launch overhead** using platform-native kernel confinement:
   - **Windows**: Win32 Job Objects (`JOBOBJECT_EXTENDED_LIMIT_INFORMATION`, CPU rate control, `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) paired with Restricted Security Tokens (`CreateRestrictedToken` stripping privileges and enforcing Low Integrity `S-1-16-4096`).
   - **Linux**: Bubblewrap (`bwrap`) / `libbubblewrap` unshare namespaces (PID, IPC, NET, UTS, USER, Mount), `seccomp-bpf` syscall filters, Landlock LSM filesystem access rules, `openat2(RESOLVE_BENEATH)` path pinning, and cgroups v2 hard memory caps.
   - **macOS**: Apple Seatbelt (`libsandbox.dylib` / `sandbox-exec`) using fine-grained Scheme/SBPL isolation profiles.
3. **Model Context Protocol (MCP) Plugin Bridge**: Implements high-throughput, non-blocking MCP Client and Server runtimes across three production transports:
   - **Stdio**: Async stream-framed pipes with separated stderr telemetry.
   - **Unix Domain Sockets (UDS)**: Local socket path abstraction (`/var/run/aegis/...` or `~/.aegis/ipc/...`).
   - **Windows Named Pipes**: Full duplex overlapped I/O integrated with Windows IOCP (`\\.\pipe\aegis_mcp_*`).
4. **Resilient Async Lifecycle & Large Payload Streaming**: 2-stage graceful process cancellation, timeout watchdog supervision, ring buffer backpressure, and Out-of-Band (OOB) shared memory descriptors (`ShmOobDescriptor`) enabling 10 MB+ to 50 MB+ payload transfers without host RAM exhaustion or pipe deadlocks.

```
+===================================================================================================+
|                              AEGIS-HARNESS PHASE 2 ARCHITECTURAL TOPOLOGY                         |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                             AEGIS HARNESS ASYNC REACTOR (RUST / TOKIO)                      |  |
|  |   [ Turn State FSM ] <---> [ Tool Dispatch Engine ] <---> [ 10-Point Lifecycle Hook Bus ]   |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |    TIER 1: WASM MICRO-SANDBOX    |                   |    TIER 2: OS PROCESS SANDBOX    |      |
|  | - Wasmtime WASI Preview 2 Runtime|                   | - SandboxedProcess Spawner Trait |      |
|  | - Cranelift AOT Compilation      |                   | - Multi-Platform Isolation Guard |      |
|  | - Pooling Instance Allocator     |                   +-----------------+----------------+      |
|  | - 64 KB Virtual Guard Pages      |                                     |                       |
|  | - CPU Fuel + Epoch Interruption  |             +-----------------------+---------------------+ |
|  | - 16 MB Linear Memory Hard Cap   |             |                       |                     | |
|  | - 2 MB Host I/O Stream Quota     |             v                       v                     v |
|  | - WASI Component Model & WIT ABI |     +---------------+       +---------------+     +-------+ |
|  | - Cold Start: < 85 µs            |     | Windows Subsys|       | Linux Subsys  |     | macOS | |
|  +----------------------------------+     | - Job Objects |       | - Bubblewrap  |     |Subsys | |
|                                           | - CPU Rate Ctrl|       | - Namespaces  |     |-Seat- | |
|                                           | - Restr. Token|       | - Seccomp-BPF |     |  belt | |
|                                           | - Low Integ.  |       | - Landlock LSM|     |-SBPL  | |
|                                           +---------------+       | - openat2 Bnth|     |Profile| |
|                                                                   +---------------+     +-------+ |
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                           MODEL CONTEXT PROTOCOL (MCP) PLUGIN BRIDGE                        |  |
|  |   [ MCP Client Trait ] <=======> [ McpTransport Trait (Frame Serializer / Router) ]         |  |
|  |                                  |                |                  |                      |  |
|  |                    +-------------+                +-------------+    +-------------+        |  |
|  |                    v                              v             v                  |        |  |
|  |           [ Stdio Transport ]           [ UDS Transport ]  [ Named Pipes Transport]         |  |
|  |           (Async Child Pipes)           (POSIX Domain Sock) (Windows Overlapped IOCP)       |  |
|  +---------------------------------------------------------------------------------------------+  |
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                      ASYNC PROCESS LIFECYCLE, STREAMING & BUFFERING SUBSYSTEM               |  |
|  | - 2-Stage Graceful Cancellation: Stage 1 Soft (SIGTERM/Ctrl-C) -> Stage 2 Hard (Kill / Job)  |  |
|  | - Non-Blocking I/O Ring Buffer: Circular Chunk Sinks with Backpressure Deadlock Defense     |  |
|  | - Large Payload Handling: In-Band Chunking (<1MB) vs Out-of-Band SHM Descriptors (>1MB)     |  |
|  +---------------------------------------------------------------------------------------------+  |
+===================================================================================================+
```

---

## 2. Tier 1: In-Process WebAssembly Micro-Sandbox (Wasmtime WASI Preview 2)

Tier 1 provides a high-density, capability-confined execution environment designed for high-frequency deterministic tools: AST parsing, code formatting, search/replace diff validation, regex filtering, and compiled plugin components.

### 2.1 Runtime Engine Configuration & Cranelift AOT Compilation
The Tier 1 host embeds `wasmtime` (v24.0+), configured for deterministic Ahead-Of-Time (AOT) compilation via the Cranelift code generator.

```rust
use wasmtime::{Config, Engine, InstanceAllocationStrategy, OptLevel, PoolingAllocationConfig};

pub fn create_wasmtime_engine() -> Result<Engine, wasmtime::Error> {
    let mut config = Config::new();
    
    // Cranelift optimization: Speed and size optimizations
    config.cranelift_opt_level(OptLevel::SpeedAndSize);
    config.cranelift_nan_canonicalization(true); // Deterministic float semantics
    
    // Asynchronous integration with Tokio work-stealing reactor
    config.async_support(true);
    
    // Interruption mechanisms: Fuel metering + Epoch timer interruption
    config.consume_fuel(true);
    config.epoch_interruption(true);
    
    // Configure WASI Component Model & WebAssembly standards
    config.wasm_component_model(true);
    config.wasm_memory64(false); // 32-bit linear addresses for strict 4GB bound
    config.wasm_multi_memory(true);
    config.wasm_bulk_memory(true);
    
    // Memory pooling allocator with optimized virtual guard pages
    let mut pooling = PoolingAllocationConfig::default();
    pooling.max_instances(1000);
    pooling.max_memories_per_instance(2);
    pooling.max_memory_size(16 * 1024 * 1024); // 16 MB max linear memory
    pooling.table_elements(10_000);
    
    // Virtual Memory Guard Size Optimization (64 KB vs default 4GB/8GB)
    // Prevents virtual memory exhaustion across 1000+ concurrent agent instances
    pooling.memory_guard_size(64 * 1024); // 64 KB
    pooling.instance_guard_size(64 * 1024);
    
    config.allocation_strategy(InstanceAllocationStrategy::Pooling(pooling));
    
    Engine::new(&config)
}
```

### 2.2 Instance Pooling Allocator & 64 KB Virtual Guard Page Optimization
In default Wasmtime configurations, every allocated instance maps 4 GB to 8 GB of virtual address space with `PROT_NONE` guard pages to trap out-of-bounds linear memory offsets. In concurrent multi-agent meshes running 500 to 1,000 subagents, this default strategy consumes **4 TB to 8 TB of virtual memory**, exhausting OS page tables and causing kernel allocation panics.

Aegis-Harness resolves this via `InstanceAllocationStrategy::Pooling` with **64 KB guard pages**:
- **Mechanism**: The pooling allocator pre-allocates a contiguous memory slab. Instantiating a new guest sandbox does not execute `mmap` or page table allocations; it simply resets linear memory indices and zeroes the active 16 MB working window.
- **Cold-Start Latency**: Resetting instance state executes in **< 85 microseconds**.
- **Bound Checking**: Cranelift generates explicit hardware-bound checks for memory accesses, maintaining absolute hardware memory safety without requiring massive virtual address guard reservations.

### 2.3 Dual Interruption Model: CPU Fuel Metering vs. Epoch-Based Interruption Timers
To guarantee deterministic execution and prevent rogue plugins from hanging the Tokio reactor, Aegis-Harness implements a **Dual Interruption Model**:

```
+===================================================================================================+
|                                    DUAL INTERRUPTION ARCHITECTURE                                 |
+===================================================================================================+
|                                                                                                   |
|  1. CPU Fuel Metering (`consume_fuel`)            2. Epoch-Based Interruption Timers              |
|  - Deterministic instruction counting.            - Real-time wall-clock timeout enforcement.     |
|  - Injected into basic blocks by Cranelift.       - Background engine thread increments epoch.    |
|  - Prevents algorithmic infinite loops.           - Traps guest asynchronously during yield.      |
|  - Fuel limit: e.g. 10,000,000 units.             - Wall-clock budget: e.g. 2,000 ms.             |
|                                                                                                   |
|  Execution Flow:                                                                                  |
|  [ Guest Tool Execution ] ----> Out of Fuel? ------------> Trap: `Trap::OutOfFuel`                |
|           |                                                                                       |
|           +-------------------> Wall-clock Epoch Expired? -> Trap: `Trap::Interrupt`              |
|                                                                                                   |
+===================================================================================================+
```

1. **Deterministic Fuel Metering**:
   - Each operation (arithmetic, branch, memory load/store) consumes calibrated fuel units.
   - The host initializes the store with a fixed fuel quota (e.g., `store.set_fuel(10_000_000)`).
   - If the guest exceeds the quota, Wasmtime triggers `wasmtime::Trap::OutOfFuel`, immediately terminating guest execution without crashing the harness.
2. **Epoch-Based Interruption Watchdog**:
   - A dedicated Tokio timer task periodically increments the engine's epoch counter (`engine.increment_epoch()`) at a fixed interval (e.g., every 10 ms).
   - The guest store is configured with a deadline: `store.set_epoch_deadline(200)` (representing a 2,000 ms real-time ceiling).
   - When the deadline expires, the guest exits with `wasmtime::Trap::Interrupt`, shielding against unmetered host I/O stalls or blocking host function calls.

### 2.4 Linear Memory Limits & 2 MB Hard Host I/O Quota Backpressure
Sandboxed code could theoretically exhaust host memory by emitting unbounded output streams to stdout/stderr. Aegis-Harness enforces a 2-tier memory defense:
1. **Guest Linear Memory Cap**: Hard-capped at **16 MB** (`max_memory_size = 16 * 1024 * 1024`). Attempts to grow linear memory beyond 256 Wasm pages (64 KB each) fail with `memory.grow` returning `-1`.
2. **Host-Side I/O Stream Hard Quota**:
   - The host implements custom WASI output sinks: `HostOutputStream::new(max_bytes = 2 * 1024 * 1024)`.
   - The host maintains an atomic byte counter. As the guest writes data via `fd_write` / `wasi:cli/stdout`, the counter increments.
   - If the cumulative output exceeds **2 MB**, the sink rejects further writes with `wasi::io::streams::StreamError`, logs a truncation event, and terminates the tool with `ToolError::OutputLimitExceeded`.

### 2.5 Component Model & WIT Interface Definitions
Tier 1 plugins adhere to the WebAssembly Component Model using WebAssembly Interface Type (WIT) declarations.

```wit
package aegis:plugin@1.0.0;

/// Core interface for sandboxed tool execution in Tier 1
interface tool-runtime {
    enum tool-error-kind {
        invalid-input,
        execution-failed,
        timeout,
        quota-exceeded,
        internal-error,
    }

    record tool-error {
        kind: tool-error-kind,
        message: string,
    }

    record tool-call-request {
        tool-name: string,
        arguments-json: string,
        timeout-ms: u32,
    }

    record tool-call-response {
        exit-code: s32,
        stdout: string,
        stderr: string,
        fuel-consumed: u64,
        duration-us: u64,
    }

    /// Invokes a registered tool within the sandboxed component
    execute: func(req: tool-call-request) -> result<tool-call-response, tool-error>;
    
    /// Returns the JSON Schema description and capabilities of this plugin
    describe: func() -> string;
}

world aegis-plugin {
    import wasi:clocks/monotonic-clock@0.2.0;
    import wasi:filesystem/types@0.2.0;
    import wasi:filesystem/preopens@0.2.0;
    
    export tool-runtime;
}
```

---

## 3. Tier 2: Multi-Platform OS Process Sandboxing Subsystem

Tier 2 provides strict kernel-level sandboxing for native host processes requiring standard POSIX/Win32 APIs: Git, compilers (`gcc`, `clang`, `rustc`), build engines (`cargo`, `npm`, `make`), and test runners (`pytest`).

```
+===================================================================================================+
|                              TIER 2 PLATFORM ISOLATION MECHANISM MATRIX                           |
+=========================+================================+================================+=======+
| Security Feature        | Windows Subsystem              | Linux Subsystem                | macOS |
+=========================+================================+================================+=======+
| Primary Confinement API | Win32 Job Objects              | Bubblewrap (`bwrap` / unshare) | Seat- |
| Memory Quota Hard Cap   | JobObjectExtendedLimitInfo     | cgroups v2 `memory.max`        | belt  |
| CPU Quota Enforcement   | JobObjectCpuRateControlInfo    | cgroups v2 `cpu.max` (quota/per| (SBPL)|
| Process Subtree Cleanup | JOB_OBJECT_LIMIT_KILL_ON_JOB   | PR_SET_PDEATHSIG (SIGKILL)     | POSIX |
| Privilege Reduction     | Restricted Token (Low/Untrusted| User Namespace (uid=1000/root) | deny  |
| Filesystem Containment  | ACLs & Path Normalization      | Landlock LSM + openat2 BENEATH | Scheme|
| Syscall Filtering       | AppContainer / Token SID strips| seccomp-bpf system call filter | rules |
| Cold-Start Launch Time  | < 2.5 ms (Warm Daemon / Job)   | 2.1 ms – 3.8 ms (bwrap spawn)  | ~3 ms |
+=========================+================================+================================+=======+
```

---

### 3.1 Windows Subsystem: Win32 Job Objects & Restricted Security Tokens

On Windows 11 and Windows Server 2022, process isolation is enforced by pairing Win32 **Job Objects** with **Restricted Security Tokens** and explicit **Process Mitigation Policies**.

#### 3.1.1 Job Object Creation & Limit Configuration
The harness creates an anonymous Job Object for each sandboxed tool session, configuring hard memory, CPU, and child process bounds:

```rust
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    CreateJobObjectW, SetInformationJobObject, AssignProcessToJobObject,
    JobObjectExtendedLimitInformation, JobObjectCpuRateControlInformation,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_CPU_RATE_CONTROL_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
};
#[cfg(windows)]
use windows::Win32::Foundation::{HANDLE, CloseHandle, BOOL};

#[cfg(windows)]
pub struct WindowsJobSandbox {
    job_handle: HANDLE,
}

#[cfg(windows)]
impl WindowsJobSandbox {
    pub fn new(max_memory_bytes: usize, max_processes: u32, cpu_rate_percent: u32) -> Result<Self, windows::core::Error> {
        unsafe {
            let job_handle = CreateJobObjectW(None, None)?;
            
            // 1. Configure Extended Limits (Memory, Child Process Count, Cleanup on Close)
            let mut ext_info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            ext_info.BasicLimitInformation.LimitFlags = 
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE |
                JOB_OBJECT_LIMIT_PROCESS_MEMORY |
                JOB_OBJECT_LIMIT_JOB_MEMORY |
                JOB_OBJECT_LIMIT_ACTIVE_PROCESS |
                JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
            
            ext_info.ProcessMemoryLimit = max_memory_bytes;
            ext_info.JobMemoryLimit = max_memory_bytes;
            ext_info.BasicLimitInformation.ActiveProcessLimit = max_processes; // e.g. 16
            
            SetInformationJobObject(
                job_handle,
                JobObjectExtendedLimitInformation,
                &ext_info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            
            // 2. Configure CPU Rate Control (Hard Cap, e.g. 50% = 5000 basis points)
            let mut cpu_info = JOBOBJECT_CPU_RATE_CONTROL_INFORMATION::default();
            cpu_info.ControlFlags = JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            cpu_info.Anonymous.CpuRate = (cpu_rate_percent * 100).min(10000); // 100% = 10000
            
            SetInformationJobObject(
                job_handle,
                JobObjectCpuRateControlInformation,
                &cpu_info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
            )?;
            
            Ok(Self { job_handle })
        }
    }

    pub fn assign_process(&self, process_handle: HANDLE) -> Result<(), windows::core::Error> {
        unsafe {
            AssignProcessToJobObject(self.job_handle, process_handle)?;
            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJobSandbox {
    fn drop(&mut self) {
        unsafe {
            if !self.job_handle.is_invalid() {
                // Closing handle automatically terminates all processes in Job Object
                let _ = CloseHandle(self.job_handle);
            }
        }
    }
}
```

#### 3.1.2 Restricted Security Token Generation & Privilege Stripping
To prevent malicious processes from accessing system credentials, modifying registry hives, or communicating across unapproved IPC pipes, the host creates a **Restricted Security Token**:
1. **Disable Administrative SIDs**: Sets `DISABLE_MAX_PRIVILEGE`.
2. **Strip All Privileges**: Strips `SeDebugPrivilege`, `SeImpersonatePrivilege`, `SeCreateGlobalPrivilege`, `SeTakeOwnershipPrivilege`, `SeBackupPrivilege`, and `SeRestorePrivilege`.
3. **Enforce Low Integrity Level**: Applies Mandatory Integrity Control (MIC) SID `S-1-16-4096` (`SECURITY_MANDATORY_LOW_RID`) or `S-1-16-1024` (`SECURITY_MANDATORY_UNTRUSTED_RID`).
4. **AppContainer Confinement**: Optionally generates an ephemeral AppContainer SID with `INTERNET_CLIENT` network capability disabled.

```rust
#[cfg(windows)]
use windows::Win32::Security::{
    CreateRestrictedToken, OpenProcessToken, TOKEN_DUPLICATE, TOKEN_QUERY,
    TOKEN_ASSIGN_PRIMARY, LUA_TOKEN, DISABLE_MAX_PRIVILEGE,
    SetTokenInformation, TokenIntegrityLevel, TOKEN_MANDATORY_LABEL,
    SE_GROUP_INTEGRITY, SID_AND_ATTRIBUTES, ConvertStringSidToSidW,
};
#[cfg(windows)]
use windows::Win32::System::Threading::GetCurrentProcess;

#[cfg(windows)]
pub fn create_low_integrity_restricted_token() -> Result<HANDLE, windows::core::Error> {
    unsafe {
        let mut process_token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY,
            &mut process_token,
        )?;

        let mut restricted_token = HANDLE::default();
        // Create restricted token: strip privileges and disable admin SIDs
        CreateRestrictedToken(
            process_token,
            DISABLE_MAX_PRIVILEGE | LUA_TOKEN,
            None,
            None,
            None,
            &mut restricted_token,
        )?;
        let _ = CloseHandle(process_token);

        // Apply Low Integrity Level SID (S-1-16-4096)
        let low_integrity_sid_str = windows::core::w!("S-1-16-4096");
        let mut p_sid = windows::Win32::Foundation::PSID::default();
        ConvertStringSidToSidW(low_integrity_sid_str, &mut p_sid)?;

        let mut tml = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: p_sid,
                Attributes: SE_GROUP_INTEGRITY,
            },
        };

        SetTokenInformation(
            restricted_token,
            TokenIntegrityLevel,
            &mut tml as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32,
        )?;

        Ok(restricted_token)
    }
}
```

#### 3.1.3 Process Spawning & Job Association Pipeline
To eliminate race conditions where a spawned process spawns unmonitored children or executes code before being assigned to the Job Object:
1. `CreateProcessAsUserW` (or `CreateProcessW`) is invoked with `CREATE_SUSPENDED | CREATE_BREAKAWAY_FROM_JOB`.
2. The main thread immediately assigns the primary process handle to the Job Object via `AssignProcessToJobObject`.
3. The main thread resumes the suspended primary thread via `ResumeThread`.

---

### 3.2 Linux Subsystem: Bubblewrap, Namespaces, Seccomp-BPF & Landlock LSM

On Linux kernel 5.15+, Tier 2 isolation combines **unprivileged user namespaces**, **Bubblewrap (`bwrap`)**, **`seccomp-bpf` syscall filters**, **Landlock LSM**, and **cgroups v2**.

```
+===================================================================================================+
|                                    LINUX ISOLATION ARCHITECTURE                                   |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                             Bubblewrap Invocation Layer (`bwrap`)                           |  |
|  |  --unshare-all  (CLONE_NEWPID | CLONE_NEWIPC | CLONE_NEWNET | CLONE_NEWUTS | CLONE_NEWUSER)|  |
|  |  --ro-bind /usr /usr      --ro-bind /lib /lib      --ro-bind /lib64 /lib64                 |  |
|  |  --ro-bind /bin /bin      --ro-bind /etc /etc      --proc /proc      --dev /dev            |  |
|  |  --tmpfs /tmp             --bind $WORKSPACE $WORKSPACE     --die-with-parent                |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                                Seccomp-BPF Syscall Filter                                   |  |
|  |  - ALLOW: read, write, openat, openat2, close, poll, epoll_wait, mmap, mprotect, clone3, etc. |  |
|  |  - DENY / KILL: ptrace, bpf, mount, pivot_root, reboot, kexec_load, init_module, raw socket  |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                                Landlock LSM & openat2 Confinement                           |  |
|  |  - Landlock ABI v1-v4: Restrict filesystem hierarchy to workspace directory tree           |  |
|  |  - openat2(dirfd, path, { flags: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS })                    |  |
|  |  - Absolute mathematical immunity against TOCTOU symlink breakout attacks                   |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                                cgroups v2 Resource Governor                                 |  |
|  |  - memory.max = 512 MB (Hard OOM kill)        - memory.high = 400 MB (Reclaim throttle)     |  |
|  |  - cpu.max = 100000 100000 (100% of 1 core)   - pids.max = 64 (Fork-bomb defense)           |  |
|  +---------------------------------------------------------------------------------------------+  |
+===================================================================================================+
```

#### 3.2.1 Unprivileged User Namespaces & Mount Topology
Bubblewrap constructs an isolated mount tree using unprivileged user namespaces (`CLONE_NEWUSER`):
```bash
bwrap \
  --unshare-all \
  --die-with-parent \
  --new-session \
  --proc /proc \
  --dev /dev \
  --tmpfs /tmp \
  --ro-bind /usr /usr \
  --ro-bind /lib /lib \
  --ro-bind /lib64 /lib64 \
  --ro-bind /bin /bin \
  --ro-bind /sbin /sbin \
  --ro-bind /etc /etc \
  --bind /path/to/workspace /workspace \
  --chdir /workspace \
  --setenv PATH /usr/local/bin:/usr/bin:/bin \
  --setenv HOME /tmp \
  --seccomp 9 \
  -- /usr/bin/git status
```

#### 3.2.2 Seccomp-BPF Syscall Filter Construction
To prevent kernel exploitation, namespace escaping, or device manipulation, a compiled BPF program is loaded via `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, ...)`:

```rust
#[cfg(target_os = "linux")]
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, SeccompRule, TargetArch};
#[cfg(target_os = "linux")]
use std::collections::BTreeMap;

#[cfg(target_os = "linux")]
pub fn build_strict_seccomp_filter() -> Result<BpfProgram, Box<dyn std::error::Error>> {
    let mut rules: BTreeMap<String, Vec<SeccompRule>> = BTreeMap::new();
    
    // Explicitly allowed safe system calls
    let allowed_syscalls = [
        "read", "write", "readv", "writev", "openat", "openat2", "close", "stat", "fstat",
        "lstat", "lseek", "mmap", "mprotect", "munmap", "brk", "rt_sigaction", "rt_sigprocmask",
        "rt_sigreturn", "ioctl", "pread64", "pwrite64", "access", "pipe2", "select", "poll",
        "epoll_create1", "epoll_ctl", "epoll_wait", "epoll_pwait", "getpid", "getuid", "getgid",
        "geteuid", "getegid", "sched_yield", "nanosleep", "clock_gettime", "clock_nanosleep",
        "exit", "exit_group", "futex", "clone", "clone3", "execve", "wait4", "fcntl", "getcwd",
        "chdir", "mkdirat", "unlinkat", "renameat2", "readlinkat", "newfstatat", "getdents64"
    ];
    
    for syscall in allowed_syscalls {
        rules.insert(syscall.to_string(), vec![]);
    }
    
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::KillProcess, // Default action on unapproved syscall: terminate process immediately
        SeccompAction::Allow,
        TargetArch::x86_64,
    )?;
    
    let bpf_prog: BpfProgram = filter.try_into()?;
    Ok(bpf_prog)
}
```

#### 3.2.3 Landlock LSM & Path Confinement (`openat2 RESOLVE_BENEATH`)
To protect against Time-of-Check to Time-of-Use (TOCTOU) symlink path traversal attacks:
1. **`openat2` Syscall**:
   ```c
   struct open_how how = {
       .flags = O_RDWR | O_CLOEXEC,
       .mode = 0644,
       .resolve = RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS
   };
   int fd = syscall(SYS_openat2, workspace_dirfd, "relative/path/to/file.rs", &how, sizeof(how));
   ```
   If a symlink attempts to resolve to an inode outside `workspace_dirfd` (e.g. `/etc/passwd`), the Linux kernel rejects the operation with `-EXDEV`.
2. **Landlock LSM Ruleset**:
   Before executing the target tool, the launcher configures Landlock ABI rules granting `LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_MAKE_REG` strictly to the workspace descriptor.

---

### 3.3 macOS Subsystem: Apple Seatbelt & Scheme SBPL Profiles

On macOS (Darwin / Apple Silicon), Tier 2 isolation utilizes Apple's kernel sandbox infrastructure (**Seatbelt**) via `sandbox_init_with_parameters` in `libsandbox.dylib`.

#### 3.3.1 Sandbox Profile Language (SBPL) Ruleset Architecture
The sandbox profile is authored in Scheme/SBPL (Sandbox Profile Language v1):

```scheme
;; aegis_sandbox_profile.sb
(version 1)
(deny default)

;; 1. Inherit core Mach IPC and system process operations
(allow process-exec (literal "/bin/sh") (literal "/usr/bin/git") (literal "/usr/bin/make"))
(allow process-fork)
(allow sysctl-read)

;; 2. Read-Only access to Apple Frameworks, BSD toolchain, and dynamic linkers
(allow file-read*
    (subpath "/usr/lib")
    (subpath "/usr/share")
    (subpath "/System/Library")
    (subpath "/Library/Developer/CommandLineTools")
    (subpath "/usr/bin")
    (subpath "/bin")
    (literal "/dev/null")
    (literal "/dev/urandom")
    (literal "/dev/dtracehelper"))

;; 3. Full read-write confinement strictly to the Workspace & ephemeral temporary directories
(allow file-read* file-write*
    (subpath (param "AEGIS_WORKSPACE_DIR"))
    (subpath (param "AEGIS_TMP_DIR")))

;; 4. Complete Network Denial (No inbound or outbound sockets, except local logging)
(deny network*)
(allow network-outbound (literal "/private/var/run/syslog"))

;; 5. Deny mach lookup to sensitive Apple services
(deny mach-lookup
    (global-name "com.apple.coreservices.launchservicesd")
    (global-name "com.apple.SecurityServer"))
```

#### 3.3.2 C-FFI `sandbox_init_with_parameters` Confinement Pipeline
```rust
#[cfg(target_os = "macos")]
use std::ffi::{CString, CStr};
#[cfg(target_os = "macos")]
use std::os::raw::{c_char, c_int};

#[cfg(target_os = "macos")]
extern "C" {
    fn sandbox_init_with_parameters(
        profile: *const c_char,
        flags: u64,
        parameters: *const *const c_char,
        errorbuf: *mut *mut c_char,
    ) -> c_int;
    fn sandbox_free_error(errorbuf: *mut c_char);
}

#[cfg(target_os = "macos")]
pub fn apply_seatbelt_profile(profile_sbpl: &str, workspace_dir: &str, tmp_dir: &str) -> Result<(), String> {
    let c_profile = CString::new(profile_sbpl).map_err(|e| e.to_string())?;
    let k1 = CString::new("AEGIS_WORKSPACE_DIR").unwrap();
    let v1 = CString::new(workspace_dir).unwrap();
    let k2 = CString::new("AEGIS_TMP_DIR").unwrap();
    let v2 = CString::new(tmp_dir).unwrap();
    
    // Null-terminated key-value parameter array
    let params: [*const c_char; 5] = [
        k1.as_ptr(), v1.as_ptr(),
        k2.as_ptr(), v2.as_ptr(),
        std::ptr::null(),
    ];
    
    let mut errorbuf: *mut c_char = std::ptr::null_mut();
    
    let ret = unsafe {
        sandbox_init_with_parameters(
            c_profile.as_ptr(),
            0, // flags
            params.as_ptr(),
            &mut errorbuf,
        )
    };
    
    if ret != 0 {
        let err_msg = if !errorbuf.is_null() {
            let msg = unsafe { CStr::from_ptr(errorbuf).to_string_lossy().into_owned() };
            unsafe { sandbox_free_error(errorbuf) };
            msg
        } else {
            format!("Seatbelt sandbox_init_with_parameters failed with code {}", ret)
        };
        return Err(err_msg);
    }
    
    Ok(())
}
```

---

## 4. Model Context Protocol (MCP) Multi-Transport Client & Server Subsystem

The Model Context Protocol (MCP) provides universal interoperability between the Aegis-Harness native core and external tool servers.

```
+===================================================================================================+
|                                    MCP MULTI-TRANSPORT ARCHITECTURE                                |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                                  Aegis MCP Client Orchestrator                              |  |
|  |  - JSON-RPC 2.0 Message Multiplexer (`initialize`, `tools/list`, `tools/call`, `ping`)      |  |
|  |  - Capability Token & Schema Validator      - Request-Response Correlator (Atomic ID Map)   |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |       TRANSPORT A: STDIO         |                   |        TRANSPORT B: UDS          |      |
|  | - Line-delimited JSON-RPC (\n)   |                   | - Length-prefixed binary framing |      |
|  | - Tokio AsyncProcess (Pipe IO)   |                   | - Unix Domain Socket path        |      |
|  | - Direct stderr routing to trace |                   | - Local POSIX IPC connection     |      |
|  +----------------------------------+                   +----------------------------------+      |
|                                                                       |                           |
|                                                                       v                           |
|                                                         +----------------------------------+      |
|                                                         |    TRANSPORT C: NAMED PIPES      |      |
|                                                         | - \\.\pipe\aegis_mcp_<session>   |      |
|                                                         | - Overlapped Windows IOCP IO     |      |
|                                                         | - Current-User Restricted DACL   |      |
|                                                         +----------------------------------+      |
+===================================================================================================+
```

### 4.1 JSON-RPC 2.0 Protocol State Machine & Message Framing
All MCP messages conform to JSON-RPC 2.0:
- **Requests**: `{"jsonrpc": "2.0", "id": 42, "method": "tools/call", "params": {...}}`
- **Responses**: `{"jsonrpc": "2.0", "id": 42, "result": {...}}` or `{"jsonrpc": "2.0", "id": 42, "error": {...}}`
- **Notifications**: `{"jsonrpc": "2.0", "method": "notifications/initialized"}`

### 4.2 Standard I/O (Stdio) Transport Architecture
For external CLI tool processes, Aegis-Harness launches the sandboxed process with redirected stdin, stdout, and stderr.
- **Framing**: Newline-delimited JSON (`\n`) or standard `Content-Length: ...\r\n\r\n` header framing.
- **Stderr Separation**: Guest stderr is read into a dedicated non-blocking stream buffer and routed to telemetry tracing, ensuring diagnostic messages never corrupt the JSON-RPC parsing pipeline on stdout.

### 4.3 Unix Domain Socket (UDS) Transport Architecture
On POSIX platforms (Linux / macOS), background persistent MCP tool services communicate over local Unix Domain Sockets (`/var/run/aegis/mcp_<tool_name>.sock` or `~/.aegis/ipc/<tool_name>.sock`):
- Socket permissions are strictly set to `0600` (`S_IRUSR | S_IWUSR`), restricting access exclusively to the current user ID.
- Supports length-prefixed binary framing (4-byte big-endian payload length + JSON payload).

### 4.4 Windows Named Pipes Transport (`\\.\pipe\...`) Architecture
On Windows, persistent MCP tool servers interface over Win32 Named Pipes:
- **Pipe Path**: `\\.\pipe\aegis_mcp_<session_id>_<tool_name>`
- **Security Descriptor**: A custom Discretionary Access Control List (DACL) restricting connection privileges strictly to the current user's Security Identifier (`D:P(A;;GA;;;user_sid)`).
- **Asynchronous IOCP**: Integrated directly into Tokio's Windows event loop via `tokio::net::windows::named_pipe::NamedPipeClient`.

### 4.5 Dynamic Tool Discovery, Schema Registration & Security Validation
Upon transport connection, the MCP Client performs a 3-step handshake:
1. **Initialize Phase**: Sends `initialize` request with client capabilities; verifies server protocol version.
2. **Tools List Phase**: Invokes `tools/list` to retrieve all registered tool declarations and JSON Schemas.
3. **Schema Sanitization**: Compiles tool schemas with `jsonschema` crate to pre-validate arguments before dispatch.

---

## 5. Async Process Lifecycle, Cancellation, Ring Buffering & Large Payload Streaming

### 5.1 Asynchronous Signal Handling & 2-Stage Graceful Cancellation Pipeline
To handle user interrupts (`Ctrl-C`), turn timeouts, or budget aborts without leaving orphaned background processes, Aegis-Harness enforces a deterministic **2-Stage Graceful Cancellation Pipeline**:

```
+===================================================================================================+
|                                2-STAGE GRACEFUL CANCELLATION PIPELINE                             |
+===================================================================================================+
|                                                                                                   |
|  [ Cancellation Trigger: User Interrupt / Timeout / Policy Breach ]                               |
|            |                                                                                      |
|            v                                                                                      |
|  STAGE 1: Soft Cancellation (Graceful Termination)                                                |
|  - POSIX (Linux/macOS): Send `SIGTERM` / `SIGINT` to child process group.                         |
|  - Windows: Send `CTRL_BREAK_EVENT` or invoke soft cancellation IPC endpoint.                    |
|  - Start 500 ms Wall-Clock Grace Period Timer (`tokio::time::sleep(500ms)`).                      |
|            |                                                                                      |
|            +---> Child Exits within 500ms?                                                        |
|            |     |                                                                                |
|            |     v (YES)                                                                          |
|            |     [ Collect Exit Status & Reclaim Resources ]                                      |
|            |                                                                                      |
|            v (NO: 500ms Grace Period Expired)                                                     |
|  STAGE 2: Hard Kill (Immediate Hardware Preemption)                                               |
|  - POSIX: Send `SIGKILL` (-9) to child process group (`killpg(pgid, SIGKILL)`).                   |
|  - Windows: Call `TerminateJobObject(job_handle, 1)` or `TerminateProcess(hProcess, 1)`.          |
|  - Force close standard I/O pipe handles to break blocking kernel wait queues.                    |
|                                                                                                   |
+===================================================================================================+
```

### 5.2 Timeout Supervisor & Orphan Process Reclamation
Every tool invocation is wrapped in a Tokio timeout future:
```rust
let timeout_duration = Duration::from_millis(limits.timeout_ms);
match tokio::time::timeout(timeout_duration, tool_future).await {
    Ok(result) => result,
    Err(_) => {
        // Timeout triggered: execute 2-stage cancellation
        process.cancel().await?;
        Err(ToolError::ExecutionTimeout(limits.timeout_ms))
    }
}
```

### 5.3 Non-Blocking I/O Ring Buffering & Deadlock Defense
A classic failure mode in subprocess execution occurs when a child process fills the OS anonymous pipe buffer (typically 64 KB on Linux, 4 KB on Windows) with stderr output while the parent blocks reading stdout, creating a permanent bi-directional deadlock.

Aegis-Harness eliminates this via **Non-Blocking Asynchronous Ring Buffer Sinks**:
- Child `stdout` and `stderr` are read concurrently using independent `tokio::io::AsyncReadExt` loops into partitioned, non-blocking ring sinks.
- Stream chunks are processed in 64 KB frames without blocking the parent reactor thread.

### 5.4 High-Capacity Payload Streaming (> 10 MB - 50 MB+) & Out-of-Band (OOB) SHM Descriptors
When tools produce massive output streams (e.g. 50 MB build logs or test outputs):

```
+===================================================================================================+
|                                  LARGE PAYLOAD STREAMING TOPOLOGY                                 |
+===================================================================================================+
|                                                                                                   |
|  [ Tool Execution Emits Output Stream ]                                                           |
|            |                                                                                      |
|            +---> Payload <= 1 MB ?                                                                |
|            |     |                                                                                |
|            |     v                                                                                |
|            |  [ In-Band Ring Buffer Slices ]                                                      |
|            |  - Chunked into 64 KB frames with `is_last_chunk` flag                               |
|            |  - Direct zero-copy ring consumption                                                 |
|            |                                                                                      |
|            +---> Payload > 1 MB (e.g. 10 MB - 50 MB+) ?                                           |
|                  |                                                                                |
|                  v                                                                                |
|               [ Out-of-Band (OOB) Shared Memory Descriptors ]                                     |
|               - Worker creates anonymous shared memory (`memfd_create` / `CreateFileMappingW`)   |
|               - Direct streaming write to anonymous SHM object                                    |
|               - Emits 32-byte `ShmOobDescriptor` through ring buffer:                             |
|                 * `handle_id`: 64-bit OS descriptor handle                                        |
|                 * `total_bytes`: Exact payload size (e.g. 52,428,800 bytes)                       |
|                 * `blake3_hash`: 128-bit cryptographic payload checksum                          |
|               - Host maps memory segment on-demand, verifies BLAKE3 hash, and archives to disk    |
|                                                                                                   |
+===================================================================================================+
```

To prevent host memory bloat during LLM prompt construction, the context manager applies **Head-and-Tail Pagination**:
- The full 50 MB log is streamed to disk / cold WAL archive.
- The in-memory message transcript retains only the first 500 lines and last 500 lines (e.g. 100 KB total), inserting a truncation notice: `[... 48,200 lines truncated; full 50MB log archived at session://artifacts/log_42.txt ...]`.

---

## 6. Complete Rust Trait & Struct IDL Specifications

Below are the complete, production-ready Rust trait and struct definitions establishing the Phase 2 subsystem interfaces.

```rust
//! Aegis-Harness Phase 2 Interface Definitions (IDL)
//! Crate: aegis_sandbox

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

// ============================================================================
// 1. Common Error Types & Enums
// ============================================================================

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum SandboxError {
    #[error("Initialization failed: {0}")]
    InitFailed(String),

    #[error("Process launch failed: {0}")]
    LaunchFailed(String),

    #[error("Execution timed out after {0} ms")]
    Timeout(u64),

    #[error("Memory quota exceeded (limit: {limit_bytes} bytes, consumed: {consumed_bytes} bytes)")]
    MemoryQuotaExceeded {
        limit_bytes: u64,
        consumed_bytes: u64,
    },

    #[error("Output stream quota exceeded (limit: {limit_bytes} bytes)")]
    OutputQuotaExceeded {
        limit_bytes: u64,
    },

    #[error("CPU fuel exhausted (fuel limit: {fuel_limit})")]
    FuelExhausted {
        fuel_limit: u64,
    },

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("IPC communication error: {0}")]
    IpcError(String),

    #[error("Platform-specific error (OS code {code}): {message}")]
    PlatformError {
        code: i32,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxTier {
    Tier1Wasm,
    Tier2OsProcess,
    Tier3MicroVm,
}

// ============================================================================
// 2. Resource Limits & Isolation Policies
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessLimits {
    pub max_memory_bytes: u64,
    pub max_cpu_percent: u32,
    pub max_processes: u32,
    pub timeout_ms: u64,
    pub max_output_bytes: u64,
    pub fuel_limit: Option<u64>,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 512 * 1024 * 1024, // 512 MB
            max_cpu_percent: 100,               // 100% of 1 core
            max_processes: 16,
            timeout_ms: 30_000,                 // 30 seconds
            max_output_bytes: 2 * 1024 * 1024,  // 2 MB
            fuel_limit: Some(100_000_000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityPermission {
    pub allow_network_egress: bool,
    pub allowed_hosts: Vec<String>,
    pub readonly_paths: Vec<PathBuf>,
    pub readwrite_paths: Vec<PathBuf>,
    pub environment_variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationPolicy {
    pub tier: SandboxTier,
    pub limits: ProcessLimits,
    pub capabilities: CapabilityPermission,
    pub workspace_root: PathBuf,
    pub temporary_directory: PathBuf,
}

// ============================================================================
// 3. Sandboxed Process Abstraction
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub wall_time: Duration,
    pub peak_memory_bytes: u64,
    pub fuel_consumed: Option<u64>,
    pub truncated: bool,
}

#[async_trait]
pub trait SandboxedProcess: Send + Sync {
    /// Returns the OS process ID (or virtual instance ID)
    fn pid(&self) -> u32;

    /// Checks whether the sandboxed process is still actively running
    async fn is_running(&mut self) -> Result<bool, SandboxError>;

    /// Waits for process completion, collecting exit status and buffered output
    async fn wait(&mut self) -> Result<ProcessOutput, SandboxError>;

    /// Initiates 2-stage graceful cancellation (Stage 1 Soft -> Stage 2 Hard)
    async fn cancel(&mut self) -> Result<(), SandboxError>;
}

#[async_trait]
pub trait ProcessSpawner: Send + Sync {
    /// Spawns a sandboxed process under the configured IsolationPolicy
    async fn spawn(
        &self,
        command: &str,
        args: &[String],
        policy: &IsolationPolicy,
    ) -> Result<Box<dyn SandboxedProcess>, SandboxError>;
}

// ============================================================================
// 4. Plugin Host & Manifest
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema_json: String,
    pub required_tier: SandboxTier,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin_id: String,
    pub version: String,
    pub description: String,
    pub tier: SandboxTier,
    pub tools: Vec<ToolDefinition>,
}

#[async_trait]
pub trait PluginInstance: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    
    async fn call_tool(
        &self,
        tool_name: &str,
        arguments_json: &str,
        policy: &IsolationPolicy,
    ) -> Result<ProcessOutput, SandboxError>;

    async fn shutdown(&self) -> Result<(), SandboxError>;
}

#[async_trait]
pub trait PluginHost: Send + Sync {
    async fn load_wasm_plugin(
        &self,
        wasm_bytes: &[u8],
        policy: &IsolationPolicy,
    ) -> Result<Box<dyn PluginInstance>, SandboxError>;

    async fn load_os_plugin(
        &self,
        binary_path: &Path,
        policy: &IsolationPolicy,
    ) -> Result<Box<dyn PluginInstance>, SandboxError>;
}

// ============================================================================
// 5. Model Context Protocol (MCP) Interfaces
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpMessage {
    Request(JsonRpcRequest),
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Sends an MCP message over the active wire transport
    async fn send_message(&mut self, message: &McpMessage) -> Result<(), SandboxError>;

    /// Receives the next incoming MCP message from the wire
    async fn receive_message(&mut self) -> Result<McpMessage, SandboxError>;

    /// Closes the transport connection gracefully
    async fn close(&mut self) -> Result<(), SandboxError>;
}

#[async_trait]
pub trait McpClient: Send + Sync {
    /// Performs initialize handshake with server
    async fn initialize(&mut self) -> Result<(), SandboxError>;

    /// Discovers list of tools exposed by MCP server
    async fn list_tools(&mut self) -> Result<Vec<ToolDefinition>, SandboxError>;

    /// Invokes an MCP tool with JSON arguments
    async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, SandboxError>;
}
```

---

## 7. Phase 2 Implementation Roadmap, Concrete Deliverables & Quantitative Guardrails

### 7.1 Component File Hierarchy
The Phase 2 deliverables will be organized within the `crates/aegis-sandbox` and `crates/aegis-mcp` module hierarchy:

```
crates/
├── aegis-sandbox/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                     # Module root & public re-exports
│   │   ├── error.rs                   # SandboxError and platform error mappers
│   │   ├── policy.rs                  # IsolationPolicy, ProcessLimits, CapabilitySet
│   │   ├── wasm/
│   │   │   ├── mod.rs                 # Tier 1 Wasmtime WASI engine supervisor
│   │   │   ├── engine.rs              # Cranelift AOT & pooling allocator setup
│   │   │   ├── interrupt.rs           # Fuel metering & epoch watchdog
│   │   │   ├── streams.rs             # 2MB host I/O quotas & backpressure sinks
│   │   │   └── host_bindings.rs       # WASI Preview 2 WIT bindings
│   │   ├── os/
│   │   │   ├── mod.rs                 # Tier 2 SandboxedProcess trait router
│   │   │   ├── windows.rs             # Win32 Job Objects, CPU Rate & Restricted Token
│   │   │   ├── linux.rs               # Bubblewrap, Seccomp-BPF & Landlock LSM
│   │   │   └── macos.rs               # Apple Seatbelt Scheme/SBPL confinement
│   │   └── lifecycle/
│   │       ├── mod.rs                 # 2-Stage graceful cancellation supervisor
│   │       ├── ring_sink.rs           # Non-blocking circular I/O sinks
│   │       └── oob_shm.rs             # Out-of-band shared memory descriptors (>1MB)
│   └── tests/
│       ├── wasm_microbenchmark.rs     # Cold-start & fuel exhaustion tests
│       ├── os_isolation_tests.rs      # Job Objects, bwrap & Seatbelt tests
│       └── cancellation_tests.rs      # 2-stage graceful kill verification
└── aegis-mcp/
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                     # MCP client/server public interfaces
    │   ├── protocol.rs                # JSON-RPC 2.0 types & state machine
    │   ├── client.rs                  # McpClient trait implementation
    │   ├── server.rs                  # McpServer listener and dispatcher
    │   ├── registry.rs                # Dynamic tool discovery & schema cache
    │   └── transport/
    │       ├── mod.rs                 # McpTransport trait definition
    │       ├── stdio.rs               # Async pipe transport with stderr separation
    │       ├── uds.rs                 # POSIX Unix Domain Socket transport
    │       └── named_pipe.rs          # Windows Overlapped IOCP Named Pipe transport
    └── tests/
        ├── mcp_stdio_tests.rs         # Stdio transport roundtrip test
        ├── mcp_uds_tests.rs           # UDS transport benchmark
        └── mcp_named_pipe_tests.rs    # Windows Named Pipe benchmark
```

### 7.2 Step-by-Step Developer Implementation Instructions

```
+===================================================================================================+
|                                STEP-BY-STEP IMPLEMENTATION ROADMAP                                |
+===================================================================================================+
|                                                                                                   |
|  STEP 1: Implement Wasmtime WASI Preview 2 Runtime Core (`crates/aegis-sandbox/src/wasm/`)        |
|  - Configure `Config` with `InstanceAllocationStrategy::Pooling` and 64 KB guard pages.           |
|  - Implement `epoch_interruption` watchdog task and fuel consumption accounting.                  |
|  - Implement `HostOutputStream` enforcing hard 2 MB byte caps with async backpressure.             |
|                                                                                                   |
|  STEP 2: Implement Platform-Specific Tier 2 OS Sandboxes (`crates/aegis-sandbox/src/os/`)        |
|  - Windows: Construct `WindowsJobSandbox` with `JobObjectExtendedLimitInformation` and           |
|    `CreateRestrictedToken` enforcing Low Integrity SID `S-1-16-4096`.                             |
|  - Linux: Construct `bwrap` command generator with namespaces, Landlock LSM, and Seccomp-BPF.    |
|  - macOS: Construct `apply_seatbelt_profile` C-FFI wrapper with Scheme SBPL isolation.          |
|                                                                                                   |
|  STEP 3: Implement Async Lifecycle, Cancellation & OOB Descriptors (`src/lifecycle/`)            |
|  - Implement 2-stage cancellation: Stage 1 soft signal (500ms) -> Stage 2 hard kill.              |
|  - Implement non-blocking stdout/stderr ring sinks preventing pipe buffer deadlocks.              |
|  - Implement `ShmOobDescriptor` memory-mapped file creation for payloads > 1 MB.                  |
|                                                                                                   |
|  STEP 4: Implement Model Context Protocol (MCP) Subsystem (`crates/aegis-mcp/`)                  |
|  - Implement JSON-RPC 2.0 serializer/deserializer and request-response correlation router.        |
|  - Implement Stdio, UDS, and Windows Named Pipes transport drivers.                               |
|  - Implement `McpClient` tool discovery, schema registration, and sandboxed execution bridge.     |
|                                                                                                   |
|  STEP 5: Automated Verification, Benchmarking & Hard Guardrail Audit                              |
|  - Execute test matrix across Linux, Windows, and macOS target environments.                      |
|  - Verify all quantitative guardrails (Wasm cold start < 85 µs, OS sandbox < 2.5 ms, 0% CPU).     |
|                                                                                                   |
+===================================================================================================+
```

### 7.3 Automated Test Matrix & Verification Commands

```bash
# 1. Run all unit and integration tests across aegis-sandbox and aegis-mcp
cargo test --package aegis-sandbox --package aegis-mcp --all-targets

# 2. Run Tier 1 Wasmtime microbenchmarks (validates < 85 µs cold start)
cargo bench --bench wasm_cold_start

# 3. Run Tier 2 OS Sandbox isolation & symlink breakout tests
cargo test --test os_isolation_tests -- --nocapture

# 4. Run 2-Stage Process Cancellation and Timeout Watchdog tests
cargo test --test cancellation_tests -- --nocapture

# 5. Run Model Context Protocol (MCP) Multi-Transport compliance suite
cargo test --test mcp_stdio_tests --test mcp_uds_tests --test mcp_named_pipe_tests

# 6. Verify zero memory leaks and clean valgrind / ASan execution
RUSTFLAGS="-Zsanitizer=address" cargo test --target x86_64-unknown-linux-gnu
```

### 7.4 Quantitative Exit Criteria & System Guardrails

| Verification Metric | Target SLA / Guardrail | Verification Method | Enforcement Mechanism |
| :--- | :--- | :--- | :--- |
| **Tier 1 Wasm Cold Start** | **< 85 µs (P99)** | `cargo bench --bench wasm_cold_start` | Pre-compiled AOT + Pooling Allocator with 64KB guards |
| **Tier 2 OS Sandbox Cold Start** | **< 2.5 ms (Linux)**<br>**< 1.2 ms (Win Daemon)** | `cargo test --test os_isolation_tests` | `bwrap` namespace fork / Windows resident job assignment |
| **Tier 1 Linear Memory Cap** | **<= 16 MB** | Guest allocation stress test | Hard linear memory bounds in Wasmtime Store |
| **Host I/O Stream Cap** | **<= 2 MB** | 50 MB stdout flood test | `HostOutputStream` atomic counter + backpressure kill |
| **Stage 1 Soft Grace Timeout** | **<= 500 ms** | Graceful cancellation test | Tokio timer watchdog before Stage 2 hard kill |
| **Large Payload Handling** | **50 MB log streaming** | OOB SHM transfer test | `ShmOobDescriptor` memory-mapped anonymous handles |
| **Base Idle Engine RSS** | **< 10 MB (7.8 MB)** | Host OS telemetry sampling | Native Rust RAII + decayed hysteresis page purging |
| **Idle CPU Utilization** | **0.00% (Zero)** | OS perf / ETW sampling | Non-spinning kernel waits (`epoll`, `IOCP`, `kqueue`) |

---

## 8. Conclusion

The Phase 2 specification establishes a complete systems engineering blueprint for Tiered Sandboxing and MCP Plugin Integration within the Aegis-Harness runtime. By combining microsecond AOT WebAssembly execution (< 85 µs), robust platform-native OS sandboxing across Windows, Linux, and macOS (< 2.5 ms), multi-transport MCP client/server communication (Stdio, UDS, Named Pipes), and resilient async lifecycle control with large payload streaming, Aegis-Harness eliminates the historic dilemma between execution latency and security confinement.
