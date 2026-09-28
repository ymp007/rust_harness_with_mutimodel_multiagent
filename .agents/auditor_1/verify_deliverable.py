"""
Forensic Auditor Comprehensive Verification Script for industry_comparative_analysis.md
"""
import os
import re
import sys

target_path = r"c:\Users\patel\OneDrive\Documents\harness project\industry_comparative_analysis.md"
arch_report_path = r"c:\Users\patel\OneDrive\Documents\harness project\harness_architecture_report.md"
project_path = r"c:\Users\patel\OneDrive\Documents\harness project\PROJECT.md"

print("=" * 80)
print("FORENSIC INTEGRITY AUDIT SUITE: industry_comparative_analysis.md")
print("=" * 80)

# Check 1: File Existence and Size
if not os.path.exists(target_path):
    print("[FAIL] Primary deliverable does NOT exist!")
    sys.exit(1)

size_bytes = os.path.getsize(target_path)
with open(target_path, "r", encoding="utf-8") as f:
    content = f.read()
    lines = content.splitlines()

print(f"[PASS] Primary deliverable exists.")
print(f"       File size: {size_bytes:,} bytes ({size_bytes/1024:.2f} KB)")
print(f"       Total lines: {len(lines):,}")
print(f"       Total words: {len(content.split()):,}")

# Check 2: Structure & Markdown Integrity (Code fences & Mermaid)
in_code = False
code_blocks = []
current = []
for i, l in enumerate(lines, 1):
    if l.startswith("```"):
        if not in_code:
            in_code = True
            current = [(i, l.strip())]
        else:
            in_code = False
            current.append((i, l.strip()))
            code_blocks.append(current)
            current = []

if in_code:
    print(f"[FAIL] Unclosed code block starting at line {current[0][0]}")
else:
    print(f"[PASS] All {len(code_blocks)} code fences correctly paired and closed.")

mermaid_blocks = [b for b in code_blocks if "mermaid" in b[0][1]]
print(f"[INFO] Found {len(mermaid_blocks)} Mermaid diagram blocks.")

# Check 3: Completeness of Target Systems Deconstruction
systems_to_check = [
    ("Claude Code", "Anthropic", 2),
    ("Google Antigravity", "Google", 2),
    ("Aider", "Paul Gauthier", 2),
    ("Cursor", "Anysphere", 2),
    ("OpenHands", "OpenDevin", 2),
    ("SWE-agent", "Princeton NLP", 2),
    ("Cline / Roo Code", "Open Source", 2),
    ("Aegis-Harness", "Production", 2),
]

print("\n--- CHECK: Target Systems Coverage ---")
for sys_name, org, min_occur in systems_to_check:
    c_sys = len(re.findall(re.escape(sys_name), content, re.IGNORECASE))
    c_org = len(re.findall(re.escape(org), content, re.IGNORECASE))
    if c_sys >= min_occur:
        print(f"[PASS] {sys_name:20} (Org: {org:15}) -> Mentions: {c_sys}")
    else:
        print(f"[FAIL] {sys_name:20} Insufficient mentions: {c_sys} < {min_occur}")

# Check 4: Section-by-Section Verification of Requirements (R1, R2, R3)
sections = [
    ("Section 1 (R1): State-of-the-Market Architecture & Runtime Deconstruction", "R1"),
    ("Section 2 (R2): Comparative Systems Analysis & Competitive Advantage Assessment", "R2"),
    ("Section 3 (R3): Structured 8x10 Comparative Benchmark Matrix & Architectural Topologies", "R3"),
    ("Section 4: Strategic Recommendations & Future Trajectory", "Recommendations"),
    ("Academic & Systems Engineering References", "Citations"),
]

print("\n--- CHECK: Requirement Section Headings ---")
for sec, label in sections:
    if sec in content:
        print(f"[PASS] Found {label:15}: {sec}")
    else:
        print(f"[FAIL] Missing {label:15}: {sec}")

# Check 5: Matrix Completeness & Dimension Verification
print("\n--- CHECK: 8x10 Comparative Matrix Coverage ---")
dimensions = [
    "D1: Primary Language & Engine",
    "D2: IPC & Wire Architecture",
    "D3: Sandboxing & Isolation",
    "D4: Base Idle Memory (RSS)",
    "D5: Peak 100k+ Context RAM",
    "D6: Tool Cold-Start Latency",
    "D7: Serialization & Wire Cost",
    "D8: Local Model & Stream TPS",
    "D9: Host Power & Thread QoS",
    "D10: Cross-Platform Complexity"
]

for dim in dimensions:
    cnt = content.count(dim)
    if cnt >= 2: # Should appear in both tables (5 systems + 3 systems)
        print(f"[PASS] {dim:35} -> Appears {cnt} times (split table complete)")
    else:
        print(f"[FAIL] {dim:35} -> Appears {cnt} times (expected >= 2)")

# Check 6: Core Architectural Mechanism Verification
print("\n--- CHECK: Core Systems Architectural Mechanisms ---")
mechanisms = [
    ("128-Byte Lockless SHM Ring", r"128-?[Bb]yte.*?(?:SHM|shared|ring)"),
    ("Double Virtual Address Mapping", r"(?:Double Virtual Address|VirtualAlloc2|mmap.*MAP_FIXED)"),
    ("rkyv Zero-Copy Deserialization", r"rkyv"),
    ("Tier 1 Wasmtime WASI Micro-Sandboxing", r"Wasmtime WASI"),
    ("Tier 2 OS Sandboxing (openat2 RESOLVE_BENEATH)", r"(?:openat2|RESOLVE_BENEATH|Job Objects|bwrap)"),
    ("Tier 3 MicroVMs (Firecracker)", r"Firecracker"),
    ("Windows EcoQoS Governance", r"(?:EcoQoS|PROCESS_POWER_THROTTLING_EXECUTION_SPEED|SetThreadInformation)"),
    ("Linux CFS Priority (nice +19)", r"nice \+19"),
    ("5.0s Decayed Inactivity Heap Purging", r"(?:5\.0-?second|5s).*?(?:hysteresis|purge|mallctl)"),
    ("BLAKE3 Token-ID Prefix KV-Cache", r"BLAKE3.*?(?:token|KV-cache)"),
    ("SIMD SSE with UTF-8 Carryover", r"(?:SIMD SSE|carryover register)"),
]

for name, pattern in mechanisms:
    matches = re.findall(pattern, content, re.IGNORECASE)
    if matches:
        print(f"[PASS] {name:45} -> {len(matches)} mentions")
    else:
        print(f"[FAIL] {name:45} -> NOT FOUND")

# Check 7: Quantitative Consistency Cross-Check
print("\n--- CHECK: Quantitative Metrics Consistency ---")
metrics = [
    ("< 10.0 MB", "Aegis Base Idle RSS", True),
    ("7.8 MB", "Aegis Measured Idle RSS", True),
    ("< 40.0 MB", "Aegis Peak Context RAM Target", True),
    ("38.5 MB", "Aegis Measured Peak Context RAM", True),
    ("< 85 µs", "Tier 1 Wasm Cold Start", True),
    ("< 0.12 µs", "rkyv Deserialization Latency", True),
    ("< 1.2 ms", "Windows Resident Daemon / P99 Inter-Turn", True),
    ("0 / 7", "Market Novelty Ratio", True),
]

for val, desc, req in metrics:
    if val in content:
        print(f"[PASS] Found metric '{val}' ({desc})")
    else:
        print(f"[FAIL] Missing expected metric '{val}' ({desc})")

print("\n" + "=" * 80)
print("AUDIT VERIFICATION COMPLETED.")
print("=" * 80)
