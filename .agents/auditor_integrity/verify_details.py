import re

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

print("=================================================================")
print("CHECKLIST 1: AUTHENTICITY & COMPLETENESS VERIFICATION")
print("=================================================================")

# 1.1 Trait and Method Signatures
print("\n--- Rust Trait Inspection ---")
traits = re.findall(r'pub trait (\w+)[^{]*\{(.*?)\n\}', text, re.DOTALL)
print(f"Total Traits Found: {len(traits)}")
for tname, tbody in traits:
    methods = re.findall(r'(?:async )?fn (\w+)\s*\((.*?)\)(?:\s*->\s*([^{;]+))?;', tbody, re.DOTALL)
    print(f"\nTrait `{tname}`: {len(methods)} methods")
    for mname, margs, mret in methods:
        margs_clean = " ".join(margs.split())
        mret_clean = " ".join(mret.split()) if mret else "()"
        print(f"  - fn {mname}({margs_clean}) -> {mret_clean}")

# 1.2 Structs and Enums
print("\n--- Structs and Enums Inspection ---")
structs = re.findall(r'pub (struct|enum) (\w+)', text)
print(f"Total Public Structs & Enums Found: {len(structs)}")
for stype, sname in structs:
    print(f"  {stype} {sname}")

# 1.3 Check for ASCII/Mermaid Diagrams
print("\n--- Diagram Count and Content ---")
ascii_boxes = re.findall(r'\+[-=]+\+', text)
print(f"ASCII diagram headers (+--+ / +==+): {len(ascii_boxes)}")

# 1.4 Check CLI and Shell Scripts
print("\n--- CLI / Script Verification ---")
bash_blocks = re.findall(r'```(?:bash|sh)\n(.*?)```', text, re.DOTALL)
for i, b in enumerate(bash_blocks):
    print(f"\nScript Block {i+1} ({len(b.splitlines())} lines):")
    for line in b.strip().splitlines()[:5]:
        print(f"  {line}")
    if len(b.splitlines()) > 5:
        print(f"  ... ({len(b.splitlines()) - 5} more lines)")

print("\n=================================================================")
print("CHECKLIST 2: ANTI-CHEATING & TECHNICAL MECHANISMS VERIFICATION")
print("=================================================================")

mechanisms = {
    "Decayed Hysteresis Page Purging": [
        "hysteresis", "MADV_DONTNEED", "VirtualUnlock", "jemalloc", "page-fault", "5s"
    ],
    "SIMD SSE Carryover Register": [
        "SIMD", "_mm_loadu_si128", "carryover", "UTF-8", "overread", "1800"
    ],
    "Landlock / RESOLVE_BENEATH / Bubblewrap": [
        "Landlock", "RESOLVE_BENEATH", "bwrap", "unshare", "seccomp", "CLONE_NEWPID"
    ],
    "Windows Job Objects & Low-Integrity Tokens": [
        "JobObject", "CreateRestrictedToken", "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE", "EcoQoS", "LUID"
    ],
    "64-bit Monotonic RAPL MSR Accumulation": [
        "MSR_PKG_ENERGY_STATUS", "0x611", "0x606", "overflow", "rollover", "monotonic"
    ]
}

for name, keywords in mechanisms.items():
    print(f"\nChecking mechanism: {name}")
    found = {}
    for kw in keywords:
        matches = len(re.findall(re.escape(kw), text, re.IGNORECASE))
        found[kw] = matches
        print(f"  Keyword '{kw}': {matches} occurrences")
    if all(count > 0 for count in found.values()):
        print(f"  => PASS: All {len(keywords)} keywords verified present in detail.")
    else:
        missing = [k for k, v in found.items() if v == 0]
        print(f"  => FAIL: Missing keywords {missing}")

print("\n=================================================================")
print("CHECKLIST 3: TRACEABILITY VERIFICATION (R1-R4 & PHASES 1-4)")
print("=================================================================")

reqs = {
    "R1: Multi-Phase, Multi-Option Implementation Architecture": [
        "Option A", "Option B", "Option C", "Phase 1", "Phase 2", "Phase 3", "Phase 4"
    ],
    "R2: Granular Execution Instructions & Quantitative Phase Guardrails": [
        "< 10 MB", "< 40 MB", "< 1.2 ms", "< 85 µs", "< 2.5 ms", "0.00% idle CPU"
    ],
    "R3: Plugin Integration Architecture with Tier 2 OS Sandboxing": [
        "Tier 2", "Model Context Protocol", "MCP", "JobObject", "Bubblewrap", "Seatbelt", "Named Pipe"
    ],
    "R4: Master Implementation Plan Deliverable": [
        "master_implementation_plan.md", "harness_architecture_report.md", "industry_comparative_analysis.md"
    ]
}

for req, kws in reqs.items():
    print(f"\nChecking Requirement: {req}")
    for kw in kws:
        matches = len(re.findall(re.escape(kw), text, re.IGNORECASE))
        print(f"  '{kw}': {matches} matches")

phases = [
    "Phase 1: Native Reactor & Memory Arenas",
    "Phase 2: Tiered Sandboxing & Plugin Bridge",
    "Phase 3: Dual-Mode Model Gateway & Prefix Cache",
    "Phase 4: Multi-Agent Mesh & Hardware Telemetry"
]

for p in phases:
    matches = len(re.findall(re.escape(p), text, re.IGNORECASE))
    print(f"\nPhase: '{p}' -> {matches} mentions")
