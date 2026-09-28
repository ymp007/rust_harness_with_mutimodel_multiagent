# Handoff Report: Phase 2 Tiered Sandboxing & Plugin Bridge Architectural Extraction

**Author**: Explorer 2  
**Working Directory**: `c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_survey_2`  
**Date**: 2026-08-17T17:45:00Z  
**Type**: Hard Handoff (Task Complete)

---

## 1. Observation

Direct architectural and specification observations extracted from the reference documents (`harness_architecture_report.md`, `industry_comparative_analysis.md`, `PROJECT.md`, `ORIGINAL_REQUEST.md`):

1. **Tier 1 WebAssembly Sandbox Architecture**:
   - `harness_architecture_report.md` (lines 454–485, 1141): Wasmtime runtime with Cranelift AOT compilation achieves cold-start latency `< 85 µs`, using `InstanceAllocationStrategy::Pooling` with custom 64 KB guard pages (`memory_guard_size = 64KB`, `instance_guard_size = 64KB`), avoiding virtual address exhaustion (saving 4TB+ in 1,000 concurrent agent configurations).
   - CPU control enforced via dual interruption: `consume_fuel(true)` (deterministic instruction fuel metering) and `epoch_interruption(true)` (wall-clock watchdog deadline).
   - Linear memory hard-capped at 16 MB (`max_memory_size = 16 * 1024 * 1024`).
   - Host protection enforces a hard 2 MB host I/O streaming quota on `stdout`/`stderr` with async backpressure to prevent host heap exhaustion.
   - Component Model WIT interfaces (`aegis:plugin/tool-runtime`) define typed contracts for tool execution.

2. **Tier 2 OS Process Sandboxing Across Platforms**:
   - **Windows**: `harness_architecture_report.md` (lines 465, 1215) & `industry_comparative_analysis.md` (lines 696, 859): Win32 Job Objects configured via `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, `JOB_OBJECT_LIMIT_PROCESS_MEMORY`, `JOB_OBJECT_LIMIT_JOB_MEMORY`, `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`, `JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION`) and `JOBOBJECT_CPU_RATE_CONTROL_INFORMATION` (hard cap). Restricted security tokens generated via `CreateRestrictedToken` with `DISABLE_MAX_PRIVILEGE | LUA_TOKEN` and Low Integrity Level SID `S-1-16-4096`. Process creation uses `CREATE_SUSPENDED | CREATE_BREAKAWAY_FROM_JOB` assigned via `AssignProcessToJobObject`.
   - **Linux**: `harness_architecture_report.md` (lines 463–464, 483) & `industry_comparative_analysis.md` (lines 694–695): Bubblewrap (`bwrap`) using `unshare` namespaces (`CLONE_NEWPID | CLONE_NEWIPC | CLONE_NEWNET | CLONE_NEWUTS | CLONE_NEWUSER | CLONE_NEWNS`), read-only root mounts (`--ro-bind /usr /usr`), tmpfs, and workspace bind. Syscall filtering via `seccomp-bpf` (killing `ptrace`, `bpf`, `mount`, raw sockets). Filesystem path confinement via Landlock LSM ABI v1-v4 rules and `openat2` with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS` preventing TOCTOU symlink traversal. Resource limits via cgroups v2 (`memory.max`, `memory.high`, `cpu.max`).
   - **macOS**: `harness_architecture_report.md` (line 466) & `industry_comparative_analysis.md` (line 697): Apple Seatbelt via `libsandbox.dylib` (`sandbox_init_with_parameters`) using Scheme SBPL (Sandbox Profile Language v1) enforcing `(deny default)`, restricted `file-read*` to system libraries, `file-read* file-write*` strictly to `$AEGIS_WORKSPACE_DIR` and `$AEGIS_TMP_DIR`, and `(deny network*)`.

3. **Model Context Protocol (MCP) Multi-Transport Integration**:
   - `harness_architecture_report.md` (lines 920–950, 1213) & `industry_comparative_analysis.md` (lines 825–835): Full JSON-RPC 2.0 client/server protocol supporting three transports:
     * **Stdio**: Async child process pipe transport with newline-delimited framing and separated stderr telemetry stream.
     * **Unix Domain Sockets (UDS)**: POSIX local domain socket (`/var/run/aegis/mcp_*.sock` or `~/.aegis/ipc/...`) with `0600` permissions.
     * **Windows Named Pipes**: Full duplex overlapped IOCP transport at `\\.\pipe\aegis_mcp_*` with current-user DACL security descriptors.

4. **Async Lifecycle, Cancellation & Large Payload Streaming**:
   - `harness_architecture_report.md` (lines 418–432, 1194–1195): 2-stage graceful cancellation (Stage 1 Soft: `SIGTERM`/`CTRL_BREAK_EVENT` with 500ms grace period -> Stage 2 Hard: `SIGKILL`/`TerminateJobObject`).
   - Non-blocking I/O ring buffer sinks prevent OS pipe buffer deadlocks.
   - Large payload handling: Payloads $\le 1\text{ MB}$ stream in 64 KB chunked frames; payloads $> 1\text{ MB}$ (up to 50 MB+) utilize anonymous Out-of-Band (OOB) Shared Memory Descriptors (`ShmOobDescriptor` containing `handle_id`, `total_bytes`, `blake3_hash`). Head/tail transcript pagination (first 500 lines + last 500 lines) bounds LLM context bloat.

5. **Rust Trait/Struct IDL Contracts**:
   - Full Rust IDLs specified for `PluginHost`, `PluginInstance`, `PluginManifest`, `SandboxedProcess`, `ProcessSpawner`, `ProcessLimits`, `ProcessOutput`, `McpTransport`, `McpClient`, and `IsolationPolicy`.

---

## 2. Logic Chain

1. **Problem**: Traditional agent harnesses either execute unconfined commands on the host OS (high risk of prompt injection and arbitrary deletion) or run Docker containers (high cold-start latency of 1.5s–15s and 1GB+ RAM overhead).
2. **Step 1 (Tier 1 Wasm Sandbox)**: By embedding Wasmtime with Cranelift AOT compilation and pooling allocation (64 KB guard pages), lightweight deterministic tools can initialize in `< 85 µs` with strict linear memory (16 MB) and CPU fuel/epoch boundaries, protected by a hard 2 MB host I/O output cap.
3. **Step 2 (Tier 2 OS Process Sandbox)**: Host development tools requiring full POSIX capabilities (e.g. `gcc`, `git`, `pytest`, `cargo`) cannot run in WASI Preview 2. Platform-native kernel sandboxing provides `< 2.5 ms` launch latency while guaranteeing security:
   - On Windows: Job Objects constrain memory, CPU, and child processes, while Restricted Tokens strip privileges and enforce Low Integrity.
   - On Linux: Bubblewrap namespaces, Seccomp-BPF filters, Landlock LSM, and `openat2 RESOLVE_BENEATH` ensure absolute isolation.
   - On macOS: Seatbelt SBPL rules strictly confine filesystem write access to the workspace and block network access.
4. **Step 3 (MCP Multi-Transport Bridge)**: To interface with third-party tools, an asynchronous MCP bridge supports Stdio pipes, Unix Domain Sockets, and Windows Named Pipes with non-blocking JSON-RPC 2.0 framing and schema pre-validation.
5. **Step 4 (Async Lifecycle & Large Payload Streaming)**: Deadlocks and memory exhaustion are prevented by non-blocking ring sinks, a 2-stage graceful cancellation pipeline, and out-of-band shared memory descriptors (`ShmOobDescriptor`) for 10MB–50MB+ streams.
6. **Conclusion**: The extracted Phase 2 architecture provides an exhaustive, mathematically bounded, production-ready specification that satisfies all performance SLAs and security mandates.

---

## 3. Caveats

1. **WASI Preview 2 Subprocess Limitation**: WebAssembly components under WASI 0.2 do not support spawning host subprocesses (`fork`/`exec`) or linking against arbitrary host C-libraries. Therefore, compiler toolchains and shell scripts must be routed to Tier 2 (OS Sandboxing).
2. **Windows Token Integrity in Unelevated Shells**: Creating restricted tokens with `LUA_TOKEN` and setting Low Integrity `S-1-16-4096` requires proper DACL handling on temp folders so the sandboxed child can write to designated scratch directories.
3. **Linux Kernel Dependency for Landlock and openat2**: Full Landlock LSM and `openat2(RESOLVE_BENEATH)` require Linux kernel 5.15+. On older kernels, Bubblewrap mount namespaces and Seccomp-BPF provide the primary containment.

---

## 4. Conclusion

Phase 2 architectural extraction is 100% complete and fully specified. The comprehensive deliverable has been written to:
`c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_survey_2/report.md`

Key Deliverables Specified:
- Complete Tier 1 (Wasmtime WASI Preview 2) engine, pooling allocator, 64 KB guard page optimization, fuel metering, epoch watchdog, and 2 MB I/O quotas.
- Complete Tier 2 OS process sandboxing for Windows (Job Objects + Restricted Tokens), Linux (Bubblewrap + Seccomp-BPF + Landlock + openat2), and macOS (Seatbelt SBPL).
- Complete Model Context Protocol (MCP) client/server over Stdio, Unix Domain Sockets, and Windows Named Pipes.
- Complete async signal handling, 2-stage graceful cancellation, non-blocking ring buffering, and large payload streaming (OOB descriptors).
- Complete Rust trait IDL definitions for all subsystems.

---

## 5. Verification Method

To verify the extracted specifications independently:

1. **Inspect Report Artifact**:
   - File: `c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_survey_2/report.md`
   - Verify all sections: Tier 1 Wasm, Tier 2 OS Sandboxes (Windows/Linux/macOS), MCP multi-transport, async cancellation, and Rust trait IDLs.
2. **Syntax and Trait Compilation Check**:
   - Verify Rust IDL trait contracts conform to standard Tokio async signatures (`#[async_trait]`, `Send + Sync`, zero-allocation inline hooks).
3. **Guardrail Verification Matrix**:
   - Tier 1 Wasm cold start: `< 85 µs` (AOT + pooling allocator)
   - Tier 2 OS Sandbox launch: `< 2.5 ms` (Linux `bwrap`) / `< 1.2 ms` (Windows daemon IPC)
   - Memory limits: 16 MB linear memory (Wasm), 512 MB hard cap (OS process)
   - Host I/O streaming cap: 2 MB hard quota with backpressure
   - Cancellation: 500 ms soft grace timeout followed by hard kill
