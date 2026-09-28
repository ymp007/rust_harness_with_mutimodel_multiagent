import ctypes
from ctypes import wintypes
import time
import math
import re
import os

kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

ProcessPowerThrottling = 4
ThreadPowerThrottling = 1

PROCESS_POWER_THROTTLING_CURRENT_VERSION = 1
PROCESS_POWER_THROTTLING_EXECUTION_SPEED = 0x1

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

# Set argtypes and restype
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenProcess.restype = wintypes.HANDLE

kernel32.SetProcessInformation.argtypes = [
    wintypes.HANDLE,
    ctypes.c_int,
    wintypes.LPVOID,
    wintypes.DWORD
]
kernel32.SetProcessInformation.restype = wintypes.BOOL

kernel32.SetThreadInformation.argtypes = [
    wintypes.HANDLE,
    ctypes.c_int,
    wintypes.LPVOID,
    wintypes.DWORD
]
kernel32.SetThreadInformation.restype = wintypes.BOOL

PROCESS_SET_INFORMATION = 0x0200
THREAD_SET_INFORMATION = 0x0020

pid = os.getpid()
hProcess = kernel32.OpenProcess(PROCESS_SET_INFORMATION, False, pid)
print(f"OpenProcess handle: {hProcess}")

def set_eco_qos_proc(enable=True):
    throttle_state = PROCESS_POWER_THROTTLING_STATE()
    throttle_state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION
    throttle_state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED
    throttle_state.StateMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED if enable else 0
    
    res = kernel32.SetProcessInformation(
        hProcess,
        ProcessPowerThrottling,
        ctypes.byref(throttle_state),
        ctypes.sizeof(throttle_state)
    )
    if not res:
        err = ctypes.get_last_error()
        print(f"SetProcessInformation failed (enable={enable}): WinError {err}")
        return False
    return True

def set_eco_qos_thread(enable=True):
    hThread = kernel32.GetCurrentThread()
    throttle_state = THREAD_POWER_THROTTLING_STATE()
    throttle_state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION
    throttle_state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED
    throttle_state.StateMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED if enable else 0
    
    res = kernel32.SetThreadInformation(
        hThread,
        ThreadPowerThrottling,
        ctypes.byref(throttle_state),
        ctypes.sizeof(throttle_state)
    )
    if not res:
        err = ctypes.get_last_error()
        print(f"SetThreadInformation failed (enable={enable}): WinError {err}")
        return False
    return True

print("Testing SetProcessInformation:")
p_res = set_eco_qos_proc(True)
print(f"SetProcessInformation(EcoQoS=True) result: {p_res}")

print("Testing SetThreadInformation:")
t_res = set_eco_qos_thread(True)
print(f"SetThreadInformation(EcoQoS=True) result: {t_res}")

# Benchmark
def run_workload():
    t0 = time.perf_counter()
    code_text = ("function parseAST(node) {\n  let x = Math.sin(node.val) * 3.14159;\n  return { type: 'AST', val: x };\n}\n" * 40000)
    matches = len(re.findall(r"function\s+(\w+)\((.*?)\)\s*\{", code_text))
    acc = 0.0
    for i in range(1, 800000):
        acc += math.sin(i) * math.cos(i)
    duration = time.perf_counter() - t0
    return duration

print("\n--- Running with EcoQoS Enabled ---")
set_eco_qos_thread(True)
eco_times = [run_workload() for _ in range(5)]
avg_eco = sum(eco_times) / len(eco_times)
print(f"EcoQoS Avg Time: {avg_eco*1000.0:.2f} ms")

print("\n--- Running with Standard QoS (EcoQoS Disabled) ---")
set_eco_qos_thread(False)
std_times = [run_workload() for _ in range(5)]
avg_std = sum(std_times) / len(std_times)
print(f"Standard QoS Avg Time: {avg_std*1000.0:.2f} ms")

penalty = (avg_eco / avg_std - 1.0) * 100.0
print(f"\nExecution Time Penalty under EcoQoS: {penalty:+.2f}%")
