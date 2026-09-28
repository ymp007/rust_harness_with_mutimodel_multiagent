"""
Empirical Benchmark 5: Memory Bandwidth Contention in Local Model Execution
Tests the impact of concurrent agent runtime background work (AST indexing, vector search, memory copies)
on token generation throughput during local LLM execution.
"""
import time
import threading
import multiprocessing
import array
import os

print("--- STARTING MEMORY BANDWIDTH CONTENTION BENCHMARK ---")

# Allocate a simulated 256MB model weight buffer (e.g. Small Language Model layer chunks)
BUFFER_SIZE = 64 * 1024 * 1024  # 64M floats = 256MB
weight_buffer = array.array('f', [1.0] * BUFFER_SIZE)

def simulate_token_generation_step(num_passes=4):
    """Simulates autoregressive token generation reading weights and computing dot products."""
    t0 = time.perf_counter()
    total_sum = 0.0
    view = memoryview(weight_buffer)
    # Read across the weight buffer simulating memory-bandwidth-bound matrix-vector multiply
    step = 1024
    for _ in range(num_passes):
        for i in range(0, len(view), step):
            total_sum += view[i]
    elapsed = time.perf_counter() - t0
    return elapsed, total_sum

# Measure baseline token generation latency (isolated)
print("1. Measuring Isolated Token Generation Latency (No Background Contention)...")
baseline_latencies = []
for _ in range(20):
    lat, _ = simulate_token_generation_step()
    baseline_latencies.append(lat)

avg_baseline = sum(baseline_latencies) / len(baseline_latencies)
simulated_tps_baseline = 1.0 / avg_baseline
print(f"Isolated Baseline Latency: {avg_baseline*1000.0:.2f} ms ({simulated_tps_baseline:.1f} simulated tokens/sec)")

# Simulate concurrent memory bandwidth contention (e.g. concurrent AST indexing / vector search / IPC memcpy)
stop_contention = threading.Event()

def contention_worker():
    scratch = bytearray(16 * 1024 * 1024) # 16MB buffer
    while not stop_contention.is_set():
        # High bandwidth memory copy / scan
        scratch[:] = b'A' * len(scratch)
        _ = sum(scratch[::4096])

print("\n2. Measuring Token Generation Latency under Concurrent Memory Contention (4 threads)...")
threads = [threading.Thread(target=contention_worker) for _ in range(4)]
for t in threads:
    t.start()

time.sleep(0.1) # Let contention threads spin up

contentious_latencies = []
for _ in range(20):
    lat, _ = simulate_token_generation_step()
    contentious_latencies.append(lat)

stop_contention.set()
for t in threads:
    t.join()

avg_contentious = sum(contentious_latencies) / len(contentious_latencies)
simulated_tps_contentious = 1.0 / avg_contentious
degradation = ((avg_contentious / avg_baseline) - 1.0) * 100.0

print(f"Contention Latency: {avg_contentious*1000.0:.2f} ms ({simulated_tps_contentious:.1f} simulated tokens/sec)")
print(f"Throughput Degradation due to Memory Bus Contention: -{degradation:.1f}%")
