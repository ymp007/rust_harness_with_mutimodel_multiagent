"""
Cross-Document Consistency Verification Script
"""
import re

files = {
    "comparative_analysis": r"c:\Users\patel\OneDrive\Documents\harness project\industry_comparative_analysis.md",
    "architecture_report": r"c:\Users\patel\OneDrive\Documents\harness project\harness_architecture_report.md",
    "project_scope": r"c:\Users\patel\OneDrive\Documents\harness project\PROJECT.md"
}

contents = {}
for k, path in files.items():
    with open(path, "r", encoding="utf-8") as f:
        contents[k] = f.read()

print("=" * 80)
print("CROSS-DOCUMENT FACT & METRIC VERIFICATION")
print("=" * 80)

facts = [
    ("7.8 MB Idle RSS", r"7\.8\s*MB"),
    ("38.5 MB Peak Context RSS", r"38\.5\s*MB"),
    ("< 85 µs Wasm Cold Start", r"<\s*85\s*(?:µs|us)"),
    ("128B Cache-Line Alignment", r"128-?[Bb]yte|128B"),
    ("rkyv Zero-Copy Deserialization", r"rkyv"),
    ("5.0s Idle Hysteresis", r"5(?:\.0)?\s*s(?:econd)?\s*(?:idle\s*)?hysteresis"),
    ("2 MB Host I/O Quota", r"2\s*MB\s*(?:host\s*)?I/O"),
    ("Windows EcoQoS", r"EcoQoS"),
    ("Linux nice +19 / SCHED_IDLE", r"nice\s*\+19|SCHED_IDLE"),
    ("Double Virtual Address Mapping", r"Double\s+Virtual\s+Address|VirtualAlloc2"),
    ("BLAKE3 Token-ID Hashing", r"BLAKE3"),
    ("SIMD SSE Carryover Register", r"carryover"),
]

for fact_name, pattern in facts:
    matches = {}
    all_present = True
    for doc_key, text in contents.items():
        m = len(re.findall(pattern, text, re.IGNORECASE))
        matches[doc_key] = m
        if m == 0:
            all_present = False
    status = "[PASS]" if all_present else "[WARN]"
    print(f"{status} {fact_name:35} -> Comp: {matches['comparative_analysis']:2d} | Arch: {matches['architecture_report']:2d} | Proj: {matches['project_scope']:2d}")

print("\n" + "=" * 80)
print("CROSS-DOCUMENT VERIFICATION COMPLETE")
print("=" * 80)
