## 2026-08-17T16:40:20Z
<USER_REQUEST>
You are explorer_2_survey, an Explorer subagent in the comparative systems study team.
Your working directory is: c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_2_survey

Mandatory inputs to read:
- c:/Users/patel/OneDrive/Documents/harness project/.agents/ORIGINAL_REQUEST.md
- c:/Users/patel/OneDrive/Documents/harness project/harness_architecture_report.md
- c:/Users/patel/OneDrive/Documents/harness project/PROJECT.md

Your Assigned Mission:
Deeply investigate, analyze, and document the runtime stacks and architectural mechanics of the following 4 agentic harnesses:
1. Cursor: C++ / TypeScript / VS Code fork architecture, native shadow workspace / daemon, custom language server protocol / IPC, local indexing / retrieval engine, speculative edits / token streaming engine, memory profile and host resource usage.
2. OpenHands (formerly OpenDevin): Python backend (FastAPI/asyncio), Docker container sandboxing, REST / WebSocket IPC, Jupyter / bash environment execution, multi-agent state manager, memory footprint (Python runtime + Docker daemon + container), cold-start latency.
3. SWE-agent: Python runtime, SWE-bench environment, Docker execution harness, Agent-Computer Interface (ACI) design, bash-centric tool execution, context window management and file viewing pagination, isolation model.
4. Cline / Roo Code: TypeScript / VS Code extension runtime, Webview / Node.js extension host IPC, diff / edit streaming pipeline, direct host tool execution, permission guardrails, memory footprint within VS Code extension host.

For each system, provide rigorous technical specifics:
- Runtime Stack (Node/Python/C++/Rust, runtime engine V8/CPython, GC mechanics and pause behavior)
- IPC / Protocol Transport (stdio JSON-RPC, Unix domain socket, REST, WebSocket, SHM)
- Process Isolation / Sandboxing Model (Bare subprocess, Docker container, custom sandbox, permission prompts)
- Memory Footprints (Idle RAM vs Peak Context Expansion RAM during 100k+ token sessions)
- Context Window Serialization Pipeline (JSON stringification/parsing overhead, AST transformations)
- Token Streaming & Tool Dispatch Latency (Round-trip tool call latency, streaming pipeline)

Deliverables:
- Write your comprehensive investigation report to c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_2_survey/analysis.md
- Write a structured handoff report to c:/Users/patel/OneDrive/Documents/harness project/.agents/explorer_2_survey/handoff.md
- Notify parent via send_message when complete.
</USER_REQUEST>
