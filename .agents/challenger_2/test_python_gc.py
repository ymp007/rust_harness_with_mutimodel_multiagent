"""
Empirical Benchmark 2: Python CPython GC & Object Graph Scaling
Tests the claims of "10 ms to 35 ms CPU pauses for cyclic GC sweeps" and memory scaling in Python agent harnesses.
"""
import time
import gc
import json
import tracemalloc

print("--- STARTING CPYTHON GC & MEMORY STRESS TEST ---")
tracemalloc.start()

gc.set_debug(gc.DEBUG_STATS if hasattr(gc, 'DEBUG_STATS') else 0)

# Simulate 100-turn agent session with complex cyclic object graphs (e.g. LangGraph / OpenHands EventStream)
class AgentTurn:
    def __init__(self, index, content):
        self.index = index
        self.content = content
        self.parent = None
        self.children = []
        self.metadata = {"timestamp": time.time(), "tokens": len(content)}
        self.raw_data = "X" * 2000

    def add_child(self, child):
        self.children.append(child)
        child.parent = self  # Creates cyclic reference

start_time = time.perf_counter()
session_history = []
turns = 100

for i in range(turns):
    turn = AgentTurn(i, f"Turn {i}: Search repository for authentication patterns")
    # Simulate tool calls and responses
    for sub in range(5):
        sub_turn = AgentTurn(f"{i}_{sub}", f"Subtask {sub} executing AST query")
        turn.add_child(sub_turn)
    session_history.append(turn)

# Measure full cyclic GC pause
gc_start = time.perf_counter()
unreachable = gc.collect(generation=2)
gc_duration_ms = (time.perf_counter() - gc_start) * 1000.0

current_mem, peak_mem = tracemalloc.get_traced_memory()
tracemalloc.stop()

print(f"Created {turns} turns with {turns*5} cyclic sub-nodes in {(time.perf_counter() - start_time)*1000.0:.2f} ms")
print(f"Gen-2 Full GC Collect Time: {gc_duration_ms:.3f} ms, Unreachable Objects Collected: {unreachable}")
print(f"Tracemalloc Memory - Current: {current_mem/1024/1024:.2f} MB, Peak: {peak_mem/1024/1024:.2f} MB")

# Run 10 consecutive full GC cycles to get distribution
gc_durations = []
for _ in range(10):
    t0 = time.perf_counter()
    gc.collect(generation=2)
    gc_durations.append((time.perf_counter() - t0) * 1000.0)

print(f"Average Full GC Time over 10 runs: {sum(gc_durations)/len(gc_durations):.3f} ms (Min: {min(gc_durations):.3f} ms, Max: {max(gc_durations):.3f} ms)")
