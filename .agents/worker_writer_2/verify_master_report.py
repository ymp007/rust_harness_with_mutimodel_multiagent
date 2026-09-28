import os
import re
import sys
import json
import yaml

REPORT_PATH = r"c:\Users\patel\OneDrive\Documents\harness project\harness_architecture_report.md"

def test_placeholders(content):
    pattern = re.compile(r'\b(TODO|FIXME|TBD|XXX|lorem ipsum|placeholder)\b', re.IGNORECASE)
    matches = pattern.findall(content)
    # Exclude any intentional markdown discussions if any
    return len(matches), matches

def test_code_blocks(content):
    code_blocks = re.findall(r'```([a-zA-Z0-9_-]*)\n(.*?)```', content, re.DOTALL)
    results = []
    for idx, (lang, block) in enumerate(code_blocks, 1):
        lang = lang.strip().lower()
        status = "OK"
        detail = ""
        if lang in ["yaml", "yml"]:
            try:
                yaml.safe_load(block)
                status = "VALID_YAML"
            except Exception as e:
                status = "INVALID_YAML"
                detail = str(e)
        elif lang == "json":
            try:
                json.loads(block)
                status = "VALID_JSON"
            except Exception as e:
                status = "INVALID_JSON"
                detail = str(e)
        elif lang == "protobuf":
            open_b = block.count('{')
            close_b = block.count('}')
            if open_b == close_b and "syntax = \"proto3\";" in block:
                status = "VALID_PROTO3"
            else:
                status = "INVALID_PROTO3"
                detail = f"Brace mismatch {open_b} vs {close_b}"
        elif lang == "rust":
            open_b = block.count('{')
            close_b = block.count('}')
            if open_b == close_b:
                status = "VALID_RUST"
            else:
                status = "INVALID_RUST"
                detail = f"Brace mismatch {open_b} vs {close_b}"
        results.append((idx, lang, status, detail))
    return results

def test_remediations(content):
    checks = {
        "1. Windows Cold Startup & Daemon": [
            "aegis-daemon", "Named Pipe", "19", "41", "CreateProcessW"
        ],
        "2. 128-Byte Cache-Line Alignment": [
            "128-Byte", "Apple Silicon", "Head Pointer", "Tail Pointer", "Sector"
        ],
        "3. SPSC Buffer Overflow & Large Payload": [
            "Double Virtual Address Mapping", "ShmOobDescriptor", "is_last_chunk"
        ],
        "4. Thread-Level OS Resource Governance": [
            "SetThreadInformation", "ThreadPowerThrottling", "E-core", "P-core"
        ],
        "5. Adaptive Lazy Page Purging & Warm Arena": [
            "16 MB", "Hysteresis", "5.0"
        ],
        "6. Asynchronous Non-Blocking WAL Commits": [
            "stage_state_in_memory_arena", "BgWriter", "drain_staged_delta"
        ],
        "7. Zero-Allocation Synchronous Token Callback": [
            "fn on_token_chunk(&self, _chunk: &TokenChunk) -> ChunkAction",
            "ChunkAction::Forward"
        ],
        "8. GPU Process Crash Domain Isolation": [
            "aegis-inference-worker", "CUDA", "crash", "worker process"
        ],
        "9. Token ID-Based Prefix Hashing": [
            "&[TokenId]", "&[u32]", "llama_kv_cache_seq_shift", "BPE"
        ],
        "10. SIMD SSE Resilient Parser": [
            "carryover register", "SIMDJSON_PADDING", "AlignedVec"
        ],
        "11. WASI Host Memory Protection": [
            "2 MB", "guard_size = 64KB", "openat2", "RESOLVE_BENEATH", "AppContainer"
        ],
        "12. Mathematically Sound Latency Formula": [
            r"\tau_{\text{harness}}", r"\bigcup", r"\mu(", r"\text{CriticalPath}"
        ],
        "13. Benchmark Controls & Hardware Profiler Rigor": [
            "cpupower frequency-set -g performance", "N_{\\text{warmup}}", "64-bit monotonic"
        ]
    }
    remediation_status = {}
    for name, keywords in checks.items():
        found = [k for k in keywords if k.lower() in content.lower()]
        remediation_status[name] = {
            "total_keywords": len(keywords),
            "found_keywords": len(found),
            "all_passed": len(keywords) == len(found),
            "missing": [k for k in keywords if k.lower() not in content.lower()]
        }
    return remediation_status

def run_all():
    with open(REPORT_PATH, "r", encoding="utf-8") as f:
        content = f.read()

    print(f"Total Lines: {len(content.splitlines())}")
    print(f"Total Bytes: {len(content.encode('utf-8'))}")

    placeholders_count, matches = test_placeholders(content)
    print(f"Placeholders Count: {placeholders_count} {matches}")

    code_results = test_code_blocks(content)
    print(f"Code Blocks Verified: {len(code_results)}")
    for idx, lang, status, detail in code_results:
        print(f"  Block {idx} ({lang}): {status} {detail}")

    remediation_status = test_remediations(content)
    print("\n--- Remediation Verification ---")
    all_ok = True
    for name, stat in remediation_status.items():
        status_str = "PASS" if stat["all_passed"] else f"FAIL (missing {stat['missing']})"
        if not stat["all_passed"]:
            all_ok = False
        print(f"  [{status_str}] {name} ({stat['found_keywords']}/{stat['total_keywords']})")
    
    print(f"\nFinal Status: {'ALL PASSED' if all_ok and placeholders_count == 0 else 'ISSUES FOUND'}")

if __name__ == "__main__":
    run_all()
