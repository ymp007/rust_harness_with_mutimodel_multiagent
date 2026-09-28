"""
Empirical Benchmark 3: Windows EcoQoS & Power Throttling Stress Test
Tests SetProcessInformation / SetThreadInformation with PROCESS_POWER_THROTTLING_EXECUTION_SPEED
and quantifies the performance impact on compute/parsing tasks.
"""
import ctypes
from ctypes import wintypes
import time
import math
import re

# Win32 Constants & Structs
# PROCESS_INFORMATION_CLASS enum
ProcessPowerThrottling = 4  # ProcessPowerThrottling in Win32
ThreadPowerThrottling = 1   # ThreadPowerThrottling in Win32

# Flags for PROCESS_POWER_THROTTLING_STATE
PROCESS_POWER_THROTTLING_CURRENT_VERSION = 1
PROCESS_POWER_THROTTLING_EXECUTION_SPEED = 0x1
PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION = 0x4

class PROCESS_POWER_THROTTLING_STATE(ctypes.Structure):
    _fields_ = [
        ("Version", wintypes.ULONG),
        ("ControlMask", wintypes.ULONG),
        ("StateMask", wintypes.ULONG),
    ]

class THREAD_POWER_THROTTLING_STATE(ctypes.Structure):
    _fields_ = [
        ("Version", wintypes.ULONG),
        ("ControlMask", wintypes.ULONG),
        ("StateMask", wintypes.ULONG),
    ]

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
GetCurrentProcess = kernel32.GetCurrentProcess
GetCurrentThread = kernel32.GetCurrentThread

SetProcessInformation = getattr(kernel32, "SetProcessInformation", None)
SetThreadInformation = getattr(kernel32, "SetThreadInformation", None)

print(f"Windows API availability: SetProcessInformation={SetProcessInformation is not None}, SetThreadInformation={SetThreadInformation is not None}")

def set_eco_qos(enable=True):
    if not SetProcessInformation:
        print("SetProcessInformation not available on this OS.")
        return False
    
    throttle_state = PROCESS_POWER_THROTTLING_STATE()
    throttle_state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION
    throttle_state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED
    throttle_state.StateMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED if enable else 0
    
    res = SetProcessInformation(
        GetCurrentProcess(),
        ProcessPowerThrottling,
        ctypes.byref(throttle_state),
        ctypes.sizeof(throttle_state)
    )
    if not res:
        err = ctypes.get_last_error()
        print(f"Failed to set EcoQoS (enable={enable}): WinError {err}")
        return False
    return True

# Workload: Heavy AST/Regex & CPU processing
def run_benchmark_workload():
    t0 = time.perf_counter()
    # 1. Regex scanning over 10MB of code text
    code_text = ("function parseAST(node) {\n  let x = Math.sin(node.val) * 3.14159;\n  return { type: 'AST', val: x };\n}\n" * 50000)
    matches = len(re.findall(r"function\s+(\w+)\((.*?)\)\s*\{", code_text))
    
    # 2. Math & hashing compute
    acc = 0.0
    for i in range(1, 1000000):
        acc += math.sin(i) * math.cos(i)
        
    duration = time.perf_counter() - t0
    return duration, matches, acc

print("--- Testing Baseline (Standard QoS / High Performance) ---")
set_eco_qos(enable=False)
baseline_times = [run_benchmark_workload()[0] for _ in range(5)]
avg_baseline = sum(baseline_times) / len(baseline_times)
print(f"Standard QoS Avg Run Time: {avg_baseline*1000.0:.2f} ms (Min: {min(baseline_times)*1000.0:.2f} ms)")

print("--- Testing Windows EcoQoS (PROCESS_POWER_THROTTLING_EXECUTION_SPEED) ---")
success = set_eco_qos(enable=True)
if success:
    eco_times = [run_benchmark_workload()[0] for _ in range(5)]
    avg_eco = sum(eco_times) / len(eco_times)
    print(f"EcoQoS Enabled Avg Run Time: {avg_eco*1000.0:.2f} ms (Min: {min(eco_times)*1000.0:.2f} ms)")
    slowdown = (avg_eco / avg_baseline - 1.0) * 100.0
    print(f"Relative Throughput Penalty under EcoQoS: {slowdown:+.1f}%")
else:
    print("EcoQoS could not be enabled.")

# Revert to standard
set_eco_qos(enable=False)
