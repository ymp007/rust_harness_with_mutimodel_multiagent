# Plan: Industry Comparative Analysis Research Study

## Mission Overview
Produce an authoritative, publication-grade comparative systems research report (`industry_comparative_analysis.md`) evaluating 7 state-of-the-art agentic harnesses (Claude Code, Google Antigravity, Aider, Cursor, OpenHands, SWE-agent, Cline/Roo Code) against the newly architected Aegis-Harness across 10 architectural and performance dimensions.

## System Dimensions to Cover (8x10 Matrix)
1. Runtime Stack (Node/Python/C++/Rust)
2. IPC & Transport Mechanics (stdio JSON-RPC, Unix domain sockets, REST/WS, lockless SHM ring buffers)
3. Process Isolation & Sandboxing Model (Bare subprocess, Docker containers, ACI, Tiered AOT Wasmtime WASI / OS namespaces / MicroVMs)
4. Memory Footprint (Idle RAM vs Peak Context Expansion RAM)
5. Context Window & Serialization Pipeline (JSON serialization/parsing vs zero-copy rkyv)
6. Token Streaming & Tool Dispatch Latency (Sub-millisecond vs 10-100ms multi-process hops)
7. Power & OS Thread Governance (EcoQoS / Thread Priority vs unthrottled Node/Python runtimes)
8. Extensibility & Tooling Ecosystem (MCP, VS Code extensions, native Wasm plugins)
9. Cold-Start Latency & Setup Overhead (Docker spawn vs Subprocess vs microsecond Wasm instantiations)
10. Cross-Platform Compilation & Deployment Complexity (npm/pip single-file vs native Rust/LLVM cross-compiles)

## Execution Milestones
1. **Milestone 1: Survey Phase (Parallel Explorers)**
   - Explorer 1 (`teamwork_preview_explorer`): Investigate Claude Code, Google Antigravity, Aider.
   - Explorer 2 (`teamwork_preview_explorer`): Investigate Cursor, OpenHands, SWE-agent, Cline/Roo Code.
   - Explorer 3 (`teamwork_preview_explorer`): Investigate Aegis-Harness architecture from existing workspace docs (`harness_architecture_report.md`, `PROJECT.md`), synthesize comparative metrics, trade-offs, and microbenchmarks.

2. **Milestone 2: Deliverable Authoring (Worker)**
   - Worker (`teamwork_preview_worker`): Author `industry_comparative_analysis.md` with:
     * Executive Summary & Architectural Taxonomy
     * Deep-dive R1: State-of-the-Market Architecture & Runtime Deconstruction for all 7 tools
     * Deep-dive R2: Comparative Systems Analysis & Competitive Advantage of Aegis-Harness
     * R3: 8x10 Structured Benchmark Matrix & Diagrams (Mermaid/ASCII)
     * Trade-off Evaluation (Ecosystem maturity, MCP vs Wasm plugins, dev velocity, cross-compilation)
     * Conclusion & Strategic Recommendations

3. **Milestone 3: Verification, Adversarial Challenge & Forensic Audit**
   - Reviewer 1 (`teamwork_preview_reviewer`): Technical depth, completeness against R1/R2/R3, and architectural accuracy.
   - Reviewer 2 (`teamwork_preview_reviewer`): Systems engineering rigor, IPC/sandboxing correctness, benchmark fairness.
   - Challenger 1 (`teamwork_preview_challenger`): Stress-test comparative claims, benchmark figures, and trade-off nuances.
   - Challenger 2 (`teamwork_preview_challenger`): Verify edge cases, local model efficiency, GC pause impacts, and power metrics.
   - Auditor (`teamwork_preview_auditor`): Forensic integrity verification of all analyses, ensuring zero hardcoding, zero fabrications, authentic synthesis.

4. **Milestone 4: Synthesis, Gate Clearance & Reporting**
   - Synthesize all review and audit reports in `GATE_STATUS.md`.
   - Update `BRIEFING.md` and `progress.md`.
   - Report final completion to parent.
