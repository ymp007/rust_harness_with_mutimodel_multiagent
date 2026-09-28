# Orchestration Plan: AI Agent Harness Research & Architecture

## Objective
Research and architect an ultra-low-overhead, high-performance evaluation and execution harness for AI agents (supporting both local on-device models and remote APIs) optimized for minimal host device resource consumption.

## Phase Breakdown

### Phase 0: Survey & Scope Mapping
- Dispatch 3 parallel Explorers:
  1. Explorer 1 (`survey_explorer_1`): Research current state-of-the-art agent harness frameworks (AutoGen, CrewAI, LangGraph, OpenHands/SWE-bench, Smolagents, Semantic Kernel, Aider) focusing on execution runtime architectures, process models, and quantitative resource footprints.
  2. Explorer 2 (`survey_explorer_2`): Research low-overhead runtime technologies, IPC mechanisms, sandboxing strategies (Wasm/WASI, seccomp/bwrap, microVMs/Firecracker, gVisor, lightweight processes), zero-copy streaming, and local/remote model execution backends (llama.cpp, vLLM, Aphrodite, ONNX Runtime, Metal/Vulkan, Apple MLX, cloud API pooling).
  3. Explorer 3 (`survey_explorer_3`): Research benchmarking methodologies, agent evaluation harnesses, profiling metrics (P50/P95/P99 latency, RSS, CPU per step, token throughput, sandbox cold-start), and actionable architectural roadmap structures.
- Synthesize findings into `PROJECT.md` at project root with full Feature Inventory and Milestone Decomposition.

### Phase 1: Milestone 1 — Landscape & Performance Profiling Research (R1)
- Deep-dive technical research and quantitative comparison matrix across leading harnesses.
- Bottleneck analysis: context serialization, streaming latency, tool call execution, process isolation, memory bloat.

### Phase 2: Milestone 2 — Low-Overhead Harness Architecture & Design (R2)
- Design universal, model-agnostic agent harness architecture.
- Evaluate at least 3 distinct architectural configurations with explicit device overhead pros/cons.
- Concrete technology stack recommendations (Rust/Zig/Go vs Python, IPC mechanisms, concurrency models, sandboxing).
- Local model execution vs cloud API optimization (zero-copy, ring buffers, context caching, prompt delta compression, resource throttling).

### Phase 3: Milestone 3 — Detailed Blueprint, Interface Specs & Evaluation Framework (R3)
- Modular system architecture, ASCII / Mermaid diagrams, component breakdowns, data flows.
- Concrete interface specs: APIs, IPC protocols, plugin/tool schemas, lifecycle hooks.
- Reproducible performance evaluation methodology and benchmark metrics.
- Prioritized, phased implementation roadmap (MVP scaffold, engine core, sandboxing, benchmarking suite).

### Phase 4: Final Deliverable Synthesis
- Assemble and polish `harness_architecture_report.md` in `c:/Users/patel/OneDrive/Documents/harness project`.

### Phase 5: Verification & Multi-Agent Audit Gating
- Dispatch 2 independent Reviewers (`teamwork_preview_reviewer`) to review completeness, technical depth, and consistency.
- Dispatch 2 Challengers (`teamwork_preview_challenger`) to stress-test architectural claims, feasibility, and edge cases.
- Dispatch 1 Forensic Auditor (`teamwork_preview_auditor`) for integrity verification.
- Gate evaluation in `GATE_STATUS.md`.

### Phase 6: Sentinel Handoff
- Send final completion message and victory claim to Sentinel (`b7c3a878-7db5-4311-9a89-6fa77a048212`).
