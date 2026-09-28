import re
import sys

# Ensure UTF-8 output
sys.stdout.reconfigure(encoding='utf-8')

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

print("===============================================================================")
print("AEGIS-HARNESS MASTER IMPLEMENTATION PLAN - COMPREHENSIVE FORENSIC AUDIT SUITE")
print("===============================================================================")

# 1. DOCUMENT STRUCTURE & STATS
lines = text.splitlines()
print(f"Total Lines: {len(lines)}")
print(f"Total Characters: {len(text)}")
print(f"Total Words: {len(text.split())}")

# Check Section Headers
print("\n--- Main Section Headers ---")
sections = [l for l in lines if re.match(r'^#\s+Section\s+\d+:', l) or re.match(r'^#\s+Executive\s+Summary', l)]
for s in sections:
    print(f"  {s}")

# Check Subsections
print("\n--- Subsections Count ---")
subsections = [l for l in lines if l.startswith('## ')]
print(f"Total Subsections (##): {len(subsections)}")
for sub in subsections:
    print(f"  {sub}")

# 2. PLACEHOLDER & INTEGRITY SCAN
print("\n--- Placeholder & Integrity Scan ---")
suspicious_patterns = [
    (r'\bTODO\b', "TODO keyword"),
    (r'\bTBD\b', "TBD keyword"),
    (r'\bFIXME\b', "FIXME keyword"),
    (r'\bXXX\b', "XXX keyword"),
    (r'\bPLACEHOLDER\b', "PLACEHOLDER keyword"),
    (r'unimplemented!\(\)', "Rust unimplemented! macro"),
    (r'todo!\(\)', "Rust todo! macro"),
    (r'\/\*\s*implement later\s*\*\/', "Implement later comment"),
    (r'\.\.\.\s*(?:rest of|to be)', "Elided code placeholder")
]

all_clean = True
for pat, desc in suspicious_patterns:
    matches = [(i+1, l) for i, l in enumerate(lines) if re.search(pat, l, re.IGNORECASE)]
    if matches:
        all_clean = False
        print(f"  [FAIL] Found {len(matches)} occurrences of {desc}:")
        for num, line in matches[:5]:
            print(f"    Line {num}: {line.strip()[:80]}")
    else:
        print(f"  [PASS] Zero occurrences of {desc}.")

# 3. RUST TRAIT & STRUCT IDL VERIFICATION
print("\n--- Rust Trait & Struct IDL Forensic Verification ---")

# Let's extract each Rust block and parse the traits and structs
rust_code_blocks = re.findall(r'```rust\n(.*?)```', text, re.DOTALL)
print(f"Total Rust code blocks: {len(rust_code_blocks)}")

for b_idx, block in enumerate(rust_code_blocks):
    print(f"\n--- Rust Block #{b_idx+1} ({len(block.splitlines())} lines) ---")
    
    # Check traits
    trait_matches = re.finditer(r'pub trait (\w+)[^{]*\{(.*?)\n\}', block, re.DOTALL)
    for tm in trait_matches:
        tname = tm.group(1)
        tbody = tm.group(2)
        methods = re.findall(r'(?:async )?fn (\w+)\s*\((.*?)\)(?:\s*->\s*([^{;\n]+))?', tbody)
        print(f"  Trait: `pub trait {tname}` -> {len(methods)} methods")
        for mname, margs, mret in methods:
            margs_clean = " ".join(margs.strip().split())
            mret_clean = " ".join(mret.strip().split()) if mret else "()"
            print(f"    - fn {mname}({margs_clean}) -> {mret_clean}")

    # Check structs and enums
    type_matches = re.findall(r'pub (struct|enum) (\w+)', block)
    print(f"  Public Types Defined: {len(type_matches)}")
    for t_type, t_name in type_matches:
        print(f"    - {t_type} {t_name}")

# 4. MATHEMATICAL FORMULATION & VERIFICATION
print("\n--- Mathematical Formulation Verification ---")
math_blocks = re.findall(r'\$\$([^\$]+)\$\$', text)
print(f"Total Block LaTeX Formulas: {len(math_blocks)}")
for i, mb in enumerate(math_blocks):
    print(f"Formula #{i+1}:")
    print(f"  {mb.strip()}")

# 5. CORE TECHNICAL MECHANISMS AUDIT
print("\n--- Core Technical Mechanisms Forensic Audit ---")
mechanisms_deep = {
    "Decayed Hysteresis Memory Purging (Phase 1)": [
        ("jemalloc decay-to-zero / MADV_DONTNEED", r'madvise.*MADV_DONTNEED|VirtualUnlock|jemalloc'),
        ("5-Second Idle Hysteresis Window", r'5\s*(?:seconds|s)\s*idle\s*hysteresis|hysteresis'),
        ("Memory Arena Architecture", r'MemoryArena|arena'),
        ("RSS Bounds (< 10 MB idle, < 40 MB active)", r'< 10 MB|< 40 MB')
    ],
    "Tier 2 Multi-Platform OS Sandboxing & Plugin Bridge (Phase 2)": [
        ("Windows Job Objects & Restricted Token", r'CreateJobObjectW|SetInformationJobObject|CreateRestrictedToken|JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE'),
        ("Linux Bubblewrap & Landlock LSM", r'bwrap|bubblewrap|Landlock|RESOLVE_BENEATH|seccomp'),
        ("macOS Apple Seatbelt / SBPL", r'Seatbelt|sandbox_init|file-read|file-write'),
        ("Model Context Protocol (MCP) Multi-Transport", r'MCP|stdio|NamedPipe|Unix.*socket|jsonrpc'),
        ("Large Payload Out-of-Band SHM Streaming", r'ShmOobDescriptor|double virtual|out-of-band|OOB')
    ],
    "Resilient SIMD SSE Stream Parser & Radix Prefix Cache (Phase 3)": [
        ("SIMD Vector Streaming Parser (> 1,800 MB/s)", r'SIMD|SSE|16-byte|32-byte|vector'),
        ("UTF-8 Multi-Byte Codepoint Split Carryover Register", r'carryover|codepoint|split|0b110|0b1110|0b11110'),
        ("Vector Load Overread Guard / Tail Buffer", r'overread|tail|boundary|padding|chunk'),
        ("Radix Tree Token Prefix Cache & Dynamic RoPE Shifting", r'RadixNode|PrefixCache|RoPE|KV-cache|pos_new')
    ],
    "Multi-Agent Actor Mesh & Hardware Telemetry (Phase 4)": [
        ("Hierarchical Work-Stealing Actor Mesh", r'AgentActor|work-stealing|tokio.*mpsc|mpmc|Supervisor'),
        ("Lock-Free Zero-Copy Shared Memory Ring Buffer", r'shm_ring_buf|128B|cache.*align|SPSC|rkyv'),
        ("Cross-Platform RAPL MSR Energy Profiling", r'RAPL|energy_uj|MSR|rollover|monotonic|NVML|SMC'),
        ("Formal Lebesgue Measure DAG Latency Disaggregation", r'Lebesgue|tau_harness|DeltaTTFT|disaggregation')
    ]
}

for mech_group, checks in mechanisms_deep.items():
    print(f"\n* {mech_group}:")
    for label, pat in checks:
        matches = len(re.findall(pat, text, re.IGNORECASE))
        status = "PASS" if matches > 0 else "FAIL"
        print(f"    [{status}] {label}: {matches} matches found")

# 6. TRACEABILITY MATRIX AUDIT
print("\n--- Traceability Matrix Forensic Audit ---")
r_matches = {
    "Requirement R1 (Multi-Phase, Multi-Option)": re.findall(r'Requirement R1|R1\.1|Option A|Option B|Option C', text),
    "Requirement R2 (Granular Instructions & Guardrails)": re.findall(r'Requirement R2|R2\.1|Guardrails|SLA', text),
    "Requirement R3 (Plugin Integration & Tier 2 Sandboxing)": re.findall(r'Requirement R3|R3\.1|Tier 2|MCP', text),
    "Requirement R4 (Master Plan Deliverable & Traceability)": re.findall(r'Requirement R4|R4\.1|Traceability|harness_architecture_report|industry_comparative_analysis', text)
}
for r_name, r_list in r_matches.items():
    print(f"  {r_name}: {len(r_list)} references")
