"""
Empirical Benchmark 4: Tool IPC Overhead vs Real-World Tool Execution
Quantifies stdio pipe JSON-RPC roundtrip time versus realistic tool execution workloads.
"""
import time
import json
import subprocess
import os

print("--- STARTING TOOL IPC & EXECUTION LATENCY BENCHMARK ---")

# 1. Measure pure JSON serialization + in-memory roundtrip
tool_call_payload = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/call",
    "params": {
        "name": "read_file",
        "arguments": {
            "path": "c:/Users/patel/OneDrive/Documents/harness project/industry_comparative_analysis.md",
            "start_line": 1,
            "end_line": 100
        }
    }
}

t0 = time.perf_counter()
N = 10000
for _ in range(N):
    raw = json.dumps(tool_call_payload)
    parsed = json.loads(raw)
json_rtt_us = ((time.perf_counter() - t0) / N) * 1e6
print(f"In-Memory JSON Serialize+Parse RTT: {json_rtt_us:.2f} µs")

# 2. Measure actual file reading & line slicing on host OS
test_file = "industry_comparative_analysis.md"
t0 = time.perf_counter()
N_file = 500
for _ in range(N_file):
    with open(test_file, "r", encoding="utf-8") as f:
        lines = [next(f) for _ in range(100)]
file_read_ms = ((time.perf_counter() - t0) / N_file) * 1000.0
print(f"Real File Read (100 lines) Latency: {file_read_ms:.3f} ms ({file_read_ms*1000:.1f} µs)")

# 3. Measure Git status subprocess execution (Tier 2 OS tool execution)
t0 = time.perf_counter()
N_git = 20
for _ in range(N_git):
    res = subprocess.run(["git", "status", "--porcelain"], capture_output=True, text=True)
git_status_ms = ((time.perf_counter() - t0) / N_git) * 1000.0
print(f"Subprocess git status execution: {git_status_ms:.2f} ms")

# 4. IPC percentage calculation
ipc_overhead_ms = (json_rtt_us / 1000.0)
pct_file = (ipc_overhead_ms / (file_read_ms + ipc_overhead_ms)) * 100.0
pct_git = (ipc_overhead_ms / (git_status_ms + ipc_overhead_ms)) * 100.0

print(f"\n--- IPC Overhead Significance Breakdown ---")
print(f"For File I/O Tool: IPC is {ipc_overhead_ms:.3f} ms vs Tool {file_read_ms:.3f} ms -> IPC is {pct_file:.2f}% of turnaround")
print(f"For Git Subprocess Tool: IPC is {ipc_overhead_ms:.3f} ms vs Tool {git_status_ms:.2f} ms -> IPC is {pct_git:.4f}% of turnaround")
