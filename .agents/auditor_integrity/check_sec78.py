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
        for l in lines[:40]:
            print(l)
        if len(lines) > 40:
            print(f"... [{len(lines)-40} more lines omitted]")
    else:
        print(f"[NOT FOUND with patterns: {start_pattern} -> {end_pattern}]")

print_section("7.2 CI/CD Pipeline & 7.3 Benchmark Runner", r"## 7\.2 Automated CI/CD Gate Enforcement Pipeline", r"## 7\.4 Performance Regression")
print_section("7.4 Triage Runbooks", r"## 7\.4 Performance Regression Thresholds & Failure Triage Runbooks", r"# Section 8: Document Control")
print_section("Section 8 Traceability & References", r"# Section 8: Document Control", r"$")
