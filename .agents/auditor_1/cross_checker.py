import os
import re

def cross_check_data():
    print("=== Cross-Checking Data Consistency Across Deliverables ===")
    
    with open('harness_architecture_report.md', 'r', encoding='utf-8') as f:
        master = f.read()
    with open('.agents/survey_explorer_1/survey_landscape_report.md', 'r', encoding='utf-8') as f:
        e1 = f.read()
    with open('.agents/survey_explorer_2/architecture_options_report.md', 'r', encoding='utf-8') as f:
        e2 = f.read()
    with open('.agents/survey_explorer_3/blueprint_and_roadmap_report.md', 'r', encoding='utf-8') as f:
        e3 = f.read()

    # Verify quantitative figures consistency
    key_metrics = [
        ("Aegis-Harness Idle RSS (< 10 MB)", "7.8 MB"),
        ("Active RSS @ 4k Context", "14.2 MB"),
        ("Peak RSS @ 128k Context", "38.5 MB"),
        ("Cold Startup Latency", "1.8 ms"),
        ("Harness Inter-Turn Latency Overhead", "1.2 ms"),
        ("TTFT Overhead", "350"),
        ("Wasm Cold Start Latency", "85"),
        ("SHM Ring Buffer Latency", "1.8"),
        ("SSE Parser Throughput", "1,800 MB/s")
    ]

    print("\n--- Key Quantitative Metrics Verification ---")
    for metric_name, value in key_metrics:
        in_master = value in master
        in_upstream = (value in e1) or (value in e2) or (value in e3)
        print(f"Metric '{metric_name}' ({value}): Master={in_master}, Upstream={in_upstream}")

    # Check framework names across M1 and Master
    frameworks = [
        "AutoGen 0.2", "AutoGen 0.4", "CrewAI", "LangGraph", 
        "OpenHands", "Smolagents", "Semantic Kernel", "Aider", "PydanticAI"
    ]
    print("\n--- Framework Name Verification ---")
    for fw in frameworks:
        in_master = fw in master
        in_e1 = fw in e1
        print(f"Framework '{fw}': Master={in_master}, E1={in_e1}")

    # Check architectural configurations across M2 and Master
    configs = [
        "Native Core + Async Worker Pool",
        "Lightweight Process Pool",
        "WebAssembly / Micro-Sandbox Worker",
        "Hybrid Actor"
    ]
    print("\n--- Architecture Configurations Verification ---")
    for cfg in configs:
        in_master = cfg.lower() in master.lower()
        in_e2 = cfg.lower() in e2.lower()
        print(f"Config '{cfg}': Master={in_master}, E2={in_e2}")

    # Check roadmap phases and timeline across M3 and Master
    phases = [
        "Phase 1: MVP Scaffold & Minimal Runtime Core",
        "Phase 2: High-Performance Engine & Provider Integrations",
        "Phase 3: Ultra-Low-Overhead Sandboxing & Tool Runtime",
        "Phase 4: Production Hardening, Context Optimization & Extensibility"
    ]
    print("\n--- Phased Roadmap Verification ---")
    for p in phases:
        in_master = p.lower() in master.lower()
        in_e3 = p.lower() in e3.lower()
        print(f"Phase '{p}': Master={in_master}, E3={in_e3}")

if __name__ == '__main__':
    cross_check_data()
