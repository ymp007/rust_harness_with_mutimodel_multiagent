---

# Section 4: Phase 2: Tiered Sandboxing & Plugin Bridge Subsystem (Requirement R3 Focus)

Phase 2 engineers the multi-tier micro-sandboxing, cross-platform OS process confinement, Model Context Protocol (MCP) transport engine, and zero-copy large-payload IPC subsystem.

```
+===================================================================================================+
|                                    PHASE 2 ARCHITECTURAL TOPOLOGY                                 |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                            Aegis Core Dispatcher / Plugin Bridge                            |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                                                           |                   |
|                   v                                                           v                   |
|  +----------------------------------+                   +----------------------------------+      |
|  |             TIER 1               |                   |             TIER 2               |      |
|  |     Wasmtime WASI Preview 2      |                   |    OS Process Sandbox Engine     |      |
|  | - In-Process Cranelift AOT       |                   | - Windows Job Objects + Low Integ|      |
|  | - Pooling Allocator (< 85 µs)    |                   | - Linux Bubblewrap + Landlock    |      |
|  | - 64 KB Guard Pages & Fuel Meter |                   | - macOS Seatbelt SBPL Profile    |      |
|  | - 2 MB Hard Host I/O Quota       |                   | - 2-Stage Cancellation (500ms)   |      |
|  +----------------------------------+                   +----------------------------------+      |
|                   |                                                           |                   |
|                   +-----------------------------+-----------------------------+                   |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                    MODEL CONTEXT PROTOCOL (MCP) MULTI-TRANSPORT CLIENT                      |  |
|  |  [ Stdio (Separated Stderr) ]   [ Unix Socket (0600) ]   [ Windows Named Pipe (DACL+IOCP) ] |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |             128-BYTE ALIGNED SHARED MEMORY (SHM) OUT-OF-BAND DESCRIPTORS (> 1MB)            |  |
|  |  - Double Virtual-Mapped Ring Buffer (< 1.8 µs RTT)                                         |  |
|  |  - BLAKE3 256-bit Integrity Checksums & Head/Tail Pagination (> 50 MB Payloads)             |  |
|  +---------------------------------------------------------------------------------------------+  |
+===================================================================================================+
```

---

## 4.1 Tier 1: In-Process WebAssembly Micro-Sandbox (Wasmtime WASI Preview 2)

### Architectural Rationale & Performance Advantage
Executing untrusted agent plugins via ephemeral containers (Docker) incurs 800 ms to 15,000 ms of cold-start latency. Tier 1 embeds **Wasmtime 24.0+** using the WebAssembly Component Model (`WASI Preview 2`), achieving cold-start instantiation latencies of **< 85 µs (P95)**.

```
+===================================================================================================+
|                                TIER 1 WASMTIME PREVIEW 2 ARCHITECTURE                             |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------+    +---------------------------+    +---------------------------+  |
|  |     Cranelift AOT Pre-    |    |  Instance Pooling Alloc.  |    |    64 KB Guard Pages &    |  |
|  |    Compilation Pipeline   |    |  (Pre-Allocated Mem Slots)|    |    Strict Fuel Metering   |  |
|  |  (OptLevel::SpeedAndSize) |    |  (Zero Runtime malloc)    |    |  (Deterministic Timeout)  |  |
|  +-------------+-------------+    +-------------+-------------+    +-------------+-------------+  |
|                |                                |                                |                |
|                +--------------------------------+--------------------------------+                |
|                                                 |                                                 |
|                                                 v                                                 |
|  +---------------------------------------------------------------------------------------------+  |
|  |                           WASI PREVIEW 2 HOST I/O CONFINEMENT                               |  |
|  |  - Standard Virtual Filesystem (WasiCtxBuilder with Virtual Sandboxed Directory)            |  |
|  |  - Hard Host I/O Quota: 2 MB Max Stdout Buffer (HostOutputStream Truncation Protection)    |  |
|  |  - Epoch Watchdog Timer: Background Task Ticks Epoch Every 10ms -> Traps Runaway Execution  |  |
|  +---------------------------------------------------------------------------------------------+  |
+===================================================================================================+
```

### Key Technical Characteristics:
1. **Ahead-of-Time (AOT) Pre-Compilation**: WASM binaries are pre-compiled to native ELF/PE machine code during plugin installation via Cranelift (`OptLevel::SpeedAndSize`).
2. **Instance Pooling Allocator**: Pre-allocates fixed execution memory slots (`InstanceLimits { count: 100, memory_pages: 256 }`), reducing instance instantiation to an $O(1)$ pointer reset (< 85 µs).
3. **Hardware Guard Pages & Linear Memory Isolation**: Enforces a 16 MB maximum linear memory limit with **64 KB unmapped virtual memory guard pages** surrounding each guest instance, converting out-of-bounds reads/writes into hardware segmentation traps.
4. **Deterministic Fuel Metering & Epoch Watchdogs**: Fuel counters enforce strict compute instruction limits ($10^7$ instructions). An independent background epoch timer increments every 10 ms; if a guest loop exceeds the turn timeout, the epoch engine terminates the instance immediately without blocking host threads.
5. **WASI Component Model (`WIT`)**: Interfaces adhere to the standard `aegis:plugin@1.0.0` WIT contract:

```wit
package aegis:plugin@1.0.0;

interface execution {
    record tool-request {
        tool-name: string,
        arguments-json: string,
        timeout-ms: u32,
    }

    record tool-response {
        exit-code: s32,
        stdout-content: string,
        stderr-content: string,
        execution-time-ns: u64,
    }

    execute-tool: func(req: tool-request) -> result<tool-response, string>;
}

world plugin-world {
    import wasi:cli/environment@0.2.0;
    import wasi:clocks/monotonic-clock@0.2.0;
    import wasi:io/streams@0.2.0;
    export execution;
}
```

---

## 4.2 Tier 2: Multi-Platform OS Process Sandboxing Subsystem

When agent plugins require host tool compilation or CLI execution (e.g. `gcc`, `git`, `python`), Aegis routes execution to Tier 2 native OS kernel sandboxes, launching in **< 2.5 ms (P95)**.

### 4.2.1 Windows Subsystem: Win32 Job Objects & Restricted Security Tokens
On Windows 11 / Server 2022, unprivileged tool execution is confined via Win32 Job Objects combined with Restricted Security Tokens:

```
+===================================================================================================+
|                              WINDOWS WIN32 KERNEL CONFINEMENT PIPELINE                            |
+===================================================================================================+
|                                                                                                   |
|  [ 1. Create Job Object ]                                                                         |
|  - CreateJobObjectW(NULL, "Global\\AegisSandboxJob_*")                                             |
|  - SetInformationJobObject(JOBOBJECT_EXTENDED_LIMIT_INFORMATION)                                  |
|    * JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE (Guarantees zero orphan process leaks)                    |
|    * JOB_OBJECT_LIMIT_PROCESS_MEMORY (Max 512 MB per process)                                     |
|    * JOB_OBJECT_LIMIT_ACTIVE_PROCESS (Max 8 child subprocesses)                                   |
|  - SetInformationJobObject(JOBOBJECT_CPU_RATE_CONTROL_INFORMATION)                                 |
|    * JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP (Max 50% CPU)        |
|                                                 |                                                 |
|                                                 v                                                 |
|  [ 2. Create Low Integrity Restricted Token ]                                                     |
|  - OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE | TOKEN_ASSIGN_PRIMARY, &hToken)          |
|  - CreateRestrictedToken(hToken, DISABLE_MAX_PRIVILEGE | LUA_TOKEN, ... &hRestrictedToken)        |
|  - SetTokenInformation(hRestrictedToken, TokenIntegrityLevel, "S-1-16-4096" [Low Integrity])      |
|                                                 |                                                 |
|                                                 v                                                 |
|  [ 3. Spawn Suspended Process & Assign ]                                                          |
|  - CreateProcessAsUserW(hRestrictedToken, ..., CREATE_SUSPENDED | CREATE_BREAKAWAY_FROM_JOB, ...) |
|  - AssignProcessToJobObject(hJob, pi.hProcess)                                                    |
|  - ResumeThread(pi.hThread)                                                                       |
|                                                                                                   |
+===================================================================================================+
```

### 4.2.2 Linux Subsystem: Bubblewrap, Seccomp-BPF & Landlock LSM
On Linux (Kernel 5.15+), Aegis-Harness combines three distinct kernel isolation layers:

```
+===================================================================================================+
|                                LINUX KERNEL DEFENSE-IN-DEPTH STACK                                |
+===================================================================================================+
|                                                                                                   |
|  [ LAYER 1: Namespace Isolation (Bubblewrap / bwrap) ]                                            |
|  - unshare: PID, IPC, NET, UTS, USER, CGROUP namespaces                                           |
|  - File Mounts: Read-only system (/usr, /lib, /bin), Read-write tmpfs (/tmp, 64MB),              |
|    Strict bind mount to target workspace root directory only.                                     |
|                                                 |                                                 |
|                                                 v                                                 |
|  [ LAYER 2: Filesystem LSM Confinement (Landlock LSM ABI v3) ]                                    |
|  - Landlock Ruleset: LANDLOCK_ACCESS_FS_READ | LANDLOCK_ACCESS_FS_WRITE                           |
|  - Landlock Rule: Restrict all path access strictly beneath /workspace                            |
|  - Path Resolution Pinning: openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_MAGICLINKS)  |
|    * Eliminates Time-of-Check to Time-of-Use (TOCTOU) symlink jailbreaks completely.              |
|                                                 |                                                 |
|                                                 v                                                 |
|  [ LAYER 3: Syscall Filtering (Seccomp-BPF) ]                                                     |
|  - BPF Whitelist: read, write, openat2, close, fstat, lseek, mmap, mprotect, munmap, brk,        |
|    rt_sigaction, rt_sigprocmask, clone3, execve, exit_group.                                      |
|  - Hard Trap / Kill: ptrace, process_vm_writev, bpf, kexec_load, mount, umount2, setns.          |
|                                                 |                                                 |
|                                                 v                                                 |
|  [ LAYER 4: Resource Cgroups v2 ]                                                                 |
|  - /sys/fs/cgroup/aegis/sandbox_$PID/memory.max = 536870912 (512 MB)                              |
|  - /sys/fs/cgroup/aegis/sandbox_$PID/cpu.max = "50000 100000" (50% Core Cap)                     |
|                                                                                                   |
+===================================================================================================+
```

### 4.2.3 macOS Subsystem: Apple Seatbelt Scheme/SBPL Confinement
On macOS (Apple Silicon M-Series / macOS 13.0+), processes are confined via Apple Seatbelt Sandbox profiles:
- Profiles compiled via `sandbox_init_with_parameters` in C-FFI.
- Ruleset:
  ```scheme
  (version 1)
  (deny default)
  (allow process-exec (literal "/bin/sh") (literal "/usr/bin/git"))
  (allow file-read* (subpath "/Applications/Xcode.app") (subpath "/Library/Developer"))
  (allow file-read* file-write* (subpath "/private/tmp/aegis_workspace"))
  (deny network*)
  (deny sysctl-write)
  ```

---

## 4.3 Model Context Protocol (MCP) Multi-Transport Client & Server Subsystem

Aegis-Harness implements the complete **Model Context Protocol (MCP)** specification, enabling plug-and-play interoperability with external tool servers across three distinct transport mediums:

```
+===================================================================================================+
|                                  MCP MULTI-TRANSPORT ARCHITECTURE                                 |
+===================================================================================================+
|                                                                                                   |
|  +---------------------------------------------------------------------------------------------+  |
|  |                       Aegis MCP Client & Tool Schema Registry Manager                       |  |
|  +----------------------------------------------+----------------------------------------------+  |
|                                                 |                                                 |
|                   +-----------------------------+-----------------------------+                   |
|                   |                             |                             |                   |
|                   v                             v                             v                   |
|  +----------------------------------+  +-------------------+  +--------------------------------+  |
|  |           STDIO PIPE             |  | UNIX DOMAIN SOCKET|  |      WINDOWS NAMED PIPE        |  |
|  | - Line-delimited JSON-RPC 2.0     |  | - /run/aegis/*.sock|  | - \\.\pipe\aegis_mcp_*         |  |
|  | - STRICT SEPARATION:             |  | - Mode: 0600      |  | - Custom User DACL Confinement |  |
|  |   Stdout = JSON-RPC Messages     |  | - Non-blocking    |  | - Windows IOCP Overlapped Async|  |
|  |   Stderr = Host Diagnostic Logs  |  |   Async Tokio UDS |  | - Zero Cross-Session Sniffing  |  |
|  +----------------------------------+  +-------------------+  +--------------------------------+  |
|                                                                                                   |
+===================================================================================================+
```

### MCP Feature Inventory:
1. **Dynamic Tool Discovery**: Automatic execution of `tools/list` on connection initiation, caching tool definitions in an in-memory trie.
2. **Strict Stdio Separation**: Subprocess stdout is routed exclusively to the JSON-RPC framing parser; stderr is captured independently by a non-blocking diagnostic ring sink, preventing logging pollution from corrupting protocol frames.
3. **JSON-RPC 2.0 Framing**: Zero-copy token scanning for `Content-Length` headers and line delimiters.

---

## 4.4 Async Process Lifecycle, Cancellation, Buffering & Large Payload Streaming

```
+===================================================================================================+
|                              2-STAGE PROCESS CANCELLATION ENGINE                                  |
+===================================================================================================+
|                                                                                                   |
|  [ Stage 1: Soft Cancellation (SIGTERM / GenerateConsoleCtrlEvent) ]                             |
|  - Turn timeout expires or user aborts turn -> Emit Soft Cancel signal to sandbox.                |
|  - Initiate 500 ms Grace Period Watchdog Timer.                                                   |
|                                                 |                                                 |
|                                                 +---> Process Exits within 500ms? -> Clean Teardown|
|                                                 |                                                 |
|                                                 v (500ms Timeout Expires)                         |
|  [ Stage 2: Hard Immediate Termination (SIGKILL / TerminateJobObject) ]                           |
|  - Linux/macOS: Send SIGKILL to process group (-$PGID).                                           |
|  - Windows: TerminateJobObject(hJob, 1) -> Atomic kernel-level teardown of all subprocesses.       |
|                                                                                                   |
+===================================================================================================+
```

### Large Payload Streaming via Out-of-Band (OOB) Shared Memory Descriptors:
When tool outputs exceed **1 MB** (e.g. 50 MB build logs, AST dumps, large test outputs), passing payloads across anonymous JSON pipes induces severe serialization churn. Aegis-Harness activates **Out-of-Band Shared Memory Descriptors (`ShmOobDescriptor`)**:

```
+===================================================================================================+
|                         OUT-OF-BAND SHM STREAMING ARCHITECTURE (> 1MB)                            |
+===================================================================================================+
|                                                                                                   |
|  [ Tool Sandbox Process ]                                                                         |
|  - Output exceeds 1 MB -> Memory-maps shared ring buffer (/dev/shm/aegis_oob_* / Win32 Local\\*)   |
|  - Writes raw binary output into SHM segment (4 KB page-aligned).                                 |
|  - Computes 256-bit BLAKE3 payload checksum.                                                      |
|  - Emits lightweight JSON-RPC pointer notification over Stdio pipe (< 128 bytes):                 |
|    {"jsonrpc":"2.0","result":{"__oob_shm_id":"aegis_oob_42","size":52428800,"blake3":"..."}}      |
|                                                 |                                                 |
|                                                 v (Pipe latency: < 5 µs)                          |
|  [ Aegis Host Core ]                                                                              |
|  - Maps SHM segment directly -> Casts rkyv data structure with zero memory copy.                 |
|  - Verifies BLAKE3 checksum -> Extracts Head/Tail summary for model context window (100 KB).      |
|                                                                                                   |
+===================================================================================================+
```

---

## 4.5 Rust Trait & Struct IDL Definitions

```rust
// ============================================================================
// CRATE: aegis-sandbox
// Module: Multi-Tier Sandboxing & Process Confinement Traits
// ============================================================================

use std::path::PathBuf;
use std::time::Duration;
use bytes::Bytes;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxTier {
    Tier1Wasm,
    Tier2OsProcess,
    Tier3MicroVm,
}

#[derive(Debug, Clone)]
pub struct ProcessLimits {
    pub max_memory_bytes: usize,
    pub max_cpu_time: Duration,
    pub wall_clock_timeout: Duration,
    pub max_output_bytes: usize,
    pub max_subprocesses: u32,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 512 * 1024 * 1024, // 512 MB
            max_cpu_time: Duration::from_secs(30),
            wall_clock_timeout: Duration::from_secs(60),
            max_output_bytes: 2 * 1024 * 1024, // 2 MB
            max_subprocesses: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub execution_time_ns: u64,
    pub memory_peak_bytes: usize,
    pub oob_shm_descriptor: Option<ShmOobDescriptor>,
}

#[derive(Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ShmOobDescriptor {
    pub shm_identifier: String,
    pub total_bytes: u64,
    pub blake3_checksum: [u8; 32],
    pub head_preview_bytes: Vec<u8>,
    pub tail_preview_bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Sandbox instantiation failed: {0}")]
    InstantiationError(String),
    #[error("Process execution timed out after {0:?}")]
    Timeout(Duration),
    #[error("Memory limit exceeded: {0} bytes > {1} bytes")]
    MemoryExceeded(usize, usize),
    #[error("Output buffer quota exceeded: {0} bytes")]
    OutputQuotaExceeded(usize),
    #[error("Kernel sandbox isolation failure: {0}")]
    ConfinementFailure(String),
    #[error("WASM runtime trap: {0}")]
    WasmTrap(String),
}

/// Abstract Sandboxed Process Trait implemented across all platforms
#[async_trait::async_trait]
pub trait SandboxedProcess: Send + Sync {
    async fn wait(&mut self) -> Result<ProcessOutput, SandboxError>;
    async fn soft_cancel(&mut self) -> Result<(), SandboxError>;
    async fn hard_kill(&mut self) -> Result<(), SandboxError>;
}

/// Abstract Sandbox Factory Trait
pub trait ProcessSpawner: Send + Sync {
    fn spawn(
        &self,
        command: &str,
        args: &[String],
        working_dir: PathBuf,
        env_vars: Vec<(String, String)>,
        limits: ProcessLimits,
    ) -> Result<Box<dyn SandboxedProcess>, SandboxError>;
}

// ============================================================================
// CRATE: aegis-mcp
// Module: Model Context Protocol (MCP) Multi-Transport Client
// ============================================================================

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    async fn send_request(&self, req: JsonRpcRequest) -> Result<JsonRpcResponse, SandboxError>;
    async fn close(&self) -> Result<(), SandboxError>;
}
```

---

## 4.6 Phase 2 File Hierarchy, Developer Steps, Tests, and Guardrails

### File-Level Target Layout:
```
crates/
├── aegis-sandbox/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                     # Public sandbox exports
│       ├── limits.rs                  # ProcessLimits and resource quotas
│       ├── oob_shm.rs                 # ShmOobDescriptor & large payload memory streaming
│       ├── wasm/
│       │   ├── mod.rs                 # Wasmtime runtime engine
│       │   ├── pool.rs                # Cranelift instance pooling allocator (< 85 µs)
│       │   ├── fuel.rs                # Fuel metering and epoch watchdogs
│       │   └── io.rs                  # Virtual WASI Preview 2 HostOutputStream
│       └── os/
│           ├── mod.rs                 # OS sandbox spawner factory
│           ├── windows.rs             # Win32 Job Object & Low Integrity Token sandbox
│           ├── linux.rs               # Bubblewrap, Seccomp-BPF & Landlock LSM sandbox
│           └── macos.rs               # Apple Seatbelt / SBPL profile sandbox
└── aegis-mcp/
    ├── Cargo.toml
    └── src/
        ├── lib.rs                     # McpClient and McpTransport exports
        ├── protocol.rs                # JSON-RPC 2.0 framing parser
        ├── registry.rs                # Dynamic tool schema trie registry
        └── transport/
            ├── mod.rs                 # Transport factory
            ├── stdio.rs               # Non-blocking async Stdio transport
            ├── unix.rs                # Unix Domain Socket transport (0600)
            └── named_pipe.rs          # Windows Named Pipe transport (DACL+IOCP)
```

### Granular Developer Step-by-Step Instructions:
1. **Implement Wasmtime WASI Preview 2 Micro-Sandbox**: Configure Wasmtime engine with Cranelift AOT `OptLevel::SpeedAndSize`, 64 KB guard pages, and instance pooling allocator. Verify `< 85 µs` cold start.
2. **Implement Windows Win32 Job Object Sandbox**: Construct `WindowsJobSandbox` calling `CreateJobObjectW`, setting memory/CPU limits, creating Low Integrity restricted tokens via `CreateRestrictedToken`, and launching suspended child processes.
3. **Implement Linux Landlock, Seccomp & Bubblewrap Sandbox**: Construct `LinuxBubblewrapSandbox` generating `bwrap` command arguments, Landlock ABI v3 rules, and Seccomp-BPF filter tables with `openat2(RESOLVE_BENEATH)` path pinning.
4. **Implement macOS Seatbelt Sandbox**: Construct `MacosSeatbeltSandbox` utilizing C-FFI `sandbox_init_with_parameters` with Scheme profiles.
5. **Implement 2-Stage Cancellation Engine**: Integrate 500 ms soft cancellation timer emitting `SIGTERM` / `Ctrl+Break` before triggering hard `SIGKILL` / `TerminateJobObject`.
6. **Implement Out-of-Band SHM Descriptors**: Construct `ShmOobDescriptor` memory-mapping engine for outputs $> 1\text{ MB}$, generating BLAKE3 checksums and Head/Tail pagination.
7. **Implement MCP Multi-Transport Client**: Construct `McpClient` supporting Stdio (separated stderr), UDS (`0600`), and Windows Named Pipes (`\\.\pipe\aegis_mcp_*`).

### Automated Test & Benchmark Commands:
```bash
# 1. Run all sandbox and MCP unit tests
cargo test --package aegis-sandbox --package aegis-mcp

# 2. Benchmark Wasmtime WASI Preview 2 Cold-Start (< 85 µs P95)
cargo bench --package aegis-sandbox --bench bench_wasm_instantiation

# 3. Benchmark OS Sandbox Spawn Latency (< 2.5 ms P95)
cargo bench --package aegis-sandbox --bench bench_os_spawn

# 4. Verify 2-Stage Cancellation and Orphan Cleanliness
cargo test --package aegis-sandbox --test test_cancellation_orphan_kill -- --nocapture

# 5. Benchmark Out-of-Band SHM Large Payload Streaming (50 MB in < 2 ms)
cargo bench --package aegis-sandbox --bench bench_shm_oob_streaming
```

### Quantitative Exit Criteria & System Guardrails for Phase 2:
```
+=============================================================================================================================+
|                                           PHASE 2 QUANTITATIVE EXIT CRITERIA & GUARDRAILS                                   |
+=============+===============================================+====================+==========================================+
| Guardrail   | Metric Description                            | Hard Exit Gate     | Verification Tooling & Method            |
+=============+===============================================+====================+==========================================+
| **G2-01**   | Tier 1 Wasm Sandbox Cold-Start Latency        | **< 85 µs (P95)**  | criterion benchmark (1,000 runs)         |
| **G2-02**   | Tier 2 OS Sandbox Spawn Latency               | **< 2.5 ms (P95)** | criterion benchmark (1,000 runs)         |
| **G2-03**   | Process Cancellation Grace Timeout            | **500 ms ± 10 ms** | tokio::time assertion test               |
| **G2-04**   | Orphan Subprocess Retention on Kill           | **0 (Absolute 0)** | OS Process Tree Audit                    |
| **G2-05**   | Out-of-Band SHM Transfer Latency (50 MB)      | **< 2.0 ms**       | cargo bench --bench bench_shm_oob        |
| **G2-06**   | WASM Linear Memory Hardware Isolation Cap    | **16 MB Hard Cap** | Wasmtime segmentation trap assertion     |
| **G2-07**   | Host I/O Truncation Protection Quota          | **2 MB Hard Limit**| HostOutputStream buffer overflow test    |
| **G2-08**   | Windows Low Integrity Token SID Confinement   | **S-1-16-4096**    | Win32 TokenIntegrityLevel Verification   |
+=============+===============================================+====================+==========================================+
```
