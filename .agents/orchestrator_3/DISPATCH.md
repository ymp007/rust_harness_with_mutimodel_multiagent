# Dispatch Record

## 2026-08-17T17:36:58Z
You are the Project Orchestrator for the Aegis-Harness Master Implementation Plan.

Your working directory is: `c:/Users/patel/OneDrive/Documents/harness project/.agents/orchestrator_3`
The project root is: `c:/Users/patel/OneDrive/Documents/harness project`

# Mission
Synthesize the existing research and architectural specifications into an exhaustive, multi-phase, multi-option master implementation plan for building the Aegis-Harness, featuring granular phase-by-phase instructions, deliverables, guardrails, verification gates, and a specialized plugin subsystem with Tier 2 OS sandboxing.
Target deliverable: `c:/Users/patel/OneDrive/Documents/harness project/master_implementation_plan.md`.

# Reference Documents in Project Root
1. `harness_architecture_report.md` — Master Architecture Report (Aegis-Harness V2.0 specification)
2. `industry_comparative_analysis.md` — Master Comparative Systems Report (benchmark matrices, deconstruction, competitive advantages)
3. `PROJECT.md` — Project Feature Matrix & Synthesis
4. `.agents/ORIGINAL_REQUEST.md` — Verbatim user requests (specifically section `## 2026-08-17T17:36:06Z`)

# Core Requirements & Acceptance Criteria
- **R1: Multi-Phase, Multi-Option Implementation Architecture**
  - 4 distinct phases:
    1. Phase 1: Native Reactor & Memory Arenas
    2. Phase 2: Tiered Sandboxing & Plugin Bridge
    3. Phase 3: Dual-Mode Model Gateway & Prefix Cache
    4. Phase 4: Multi-Agent Mesh & Hardware Telemetry
  - Distinct execution options (e.g., Option A: Fast-Path MVP Prototype vs. Option B: Full High-Performance Production Core) with trade-offs, resource allocations, time-to-first-working-binary timelines, and modular dependency graphs.
- **R2: Granular Execution Instructions & Quantitative Phase Guardrails**
  - Each phase must provide:
    1. Concrete Deliverables with file-level targets (module structures, crate layouts, file paths).
    2. Step-by-step developer instructions.
    3. Automated test commands and benchmarking scripts (cargo test, criterion, hyperfine, valgrind/massif).
    4. Quantitative Guardrails (hard exit gates): Idle RSS < 10 MB, Single-agent peak RSS < 40 MB, Inter-turn overhead < 1.2 ms, Wasm cold start < 85 µs, OS sandbox spawn < 2.5 ms, 0.00% idle CPU utilization, test pass rates.
- **R3: Plugin Integration Architecture with Tier 2 OS Sandboxing**
  - Complete plugin connection architecture for Model Context Protocol (MCP) clients and servers over Stdio, Unix domain sockets, and Windows Named Pipes.
  - Tier 2 OS isolation mechanics across platforms:
    - Windows Job Objects (memory/CPU limits, restricted security tokens, kill-on-job-close).
    - Linux Bubblewrap (`unshare` namespaces, `seccomp-bpf` syscall filters, landlock / `RESOLVE_BENEATH` path confinement).
    - macOS Seatbelt / `sandbox-exec` profiles.
  - Async signal handling, process cancellation, non-blocking I/O ring buffering, and large payload streaming (preventing memory blowup and truncation).
- **R4: Master Implementation Plan Deliverable**
  - Comprehensive, publication-grade document `master_implementation_plan.md` with complete architectural ASCII/Mermaid diagrams, data-flow schemas, Rust trait & struct IDL definitions, configuration examples, and explicit traceability to research specifications.
