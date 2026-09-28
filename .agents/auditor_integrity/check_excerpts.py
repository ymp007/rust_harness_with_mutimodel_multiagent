import re
import sys

sys.stdout.reconfigure(encoding='utf-8')

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

def print_section(title, start_pattern, end_pattern):
    print(f"\n=======================================================")
    print(f"EXCERPT: {title}")
    print(f"=======================================================")
    m = re.search(f"{start_pattern}(.*?){end_pattern}", text, re.DOTALL)
    if m:
        content = m.group(1).strip()
        lines = content.splitlines()
        print(f"Total lines: {len(lines)}")
        for l in lines[:30]:
            print(l)
        if len(lines) > 30:
            print(f"... [{len(lines)-30} more lines omitted]")
    else:
        print(f"[NOT FOUND with patterns: {start_pattern} -> {end_pattern}]")

print_section("3.4 Decayed Hysteresis Page Purging", r"## 3\.4 Decayed Hysteresis Page Purging Mechanics", r"## 3\.5 Async SQLite WAL")
print_section("4.2 OS Sandboxing Subsystem", r"## 4\.2 Tier 2: Multi-Platform OS Process Sandboxing Subsystem", r"## 4\.3 Model Context Protocol")
print_section("4.4 Large Payload OOB Streaming", r"## 4\.4 Async Process Lifecycle, Cancellation, Buffering & Large Payload Streaming", r"## 4\.5 Rust Trait")
print_section("5.3 Resilient SIMD SSE Parser", r"## 5\.3 Resilient SIMD SSE Stream Parser with UTF-8 Carryover", r"## 5\.4 Hierarchical Radix Tree")
print_section("6.3 & 6.4 Telemetry & Lebesgue Disaggregation", r"## 6\.3 Cross-Platform Hardware Telemetry", r"## 6\.5 Rust Trait")
print_section("7.1 Master Table of Quantitative Guardrails", r"## 7\.1 Consolidated Master Table of Quantitative Guardrails", r"## 7\.2 Automated CI/CD")
