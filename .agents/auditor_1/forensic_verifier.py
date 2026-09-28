import sys
import re
import json

def audit_file(filepath):
    print(f"=== Auditing File: {filepath} ===")
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
        lines = content.splitlines()

    total_lines = len(lines)
    total_bytes = len(content.encode('utf-8'))
    print(f"Total lines: {total_lines}, Total bytes: {total_bytes}")

    # Check 1: Placeholder detection
    placeholder_patterns = [
        r'\bTODO\b', r'\bFIXME\b', r'\bTBD\b', r'\bXXX\b',
        r'lorem ipsum', r'placeholder', r'insert here', r'asdf'
    ]
    placeholder_matches = []
    for idx, line in enumerate(lines, 1):
        for pat in placeholder_patterns:
            if re.search(pat, line, re.IGNORECASE):
                placeholder_matches.append((idx, pat, line.strip()))

    print(f"\n[Check 1: Placeholders] Found {len(placeholder_matches)} occurrences.")
    for idx, pat, l in placeholder_matches:
        print(f"  Line {idx} [{pat}]: {l}")

    # Check 2: Extract and validate code blocks
    code_blocks = re.findall(r'```([a-zA-Z0-9_-]*)\n(.*?)```', content, re.DOTALL)
    print(f"\n[Check 2: Code Blocks] Extracted {len(code_blocks)} code blocks.")

    for i, (lang, block) in enumerate(code_blocks, 1):
        lang = lang.strip().lower()
        print(f"\n--- Code Block #{i} (Language: '{lang}', Length: {len(block.splitlines())} lines) ---")
        
        # Test JSON
        if lang == 'json':
            try:
                parsed = json.loads(block)
                print(f"  JSON Validation: SUCCESS (Keys: {list(parsed.keys())})")
            except Exception as e:
                print(f"  JSON Validation: ERROR - {e}")
        
        # Test YAML
        elif lang in ('yaml', 'yml'):
            try:
                # Basic YAML syntax verification
                # Check indentation and basic key-values
                import yaml
                parsed = yaml.safe_load(block)
                print(f"  YAML Validation (PyYAML): SUCCESS (Top keys: {list(parsed.keys()) if isinstance(parsed, dict) else type(parsed)})")
            except ImportError:
                print("  PyYAML not installed, performing manual structural parse")
                # Basic indentation check
                indent_errors = []
                for l_idx, l in enumerate(block.splitlines(), 1):
                    if '\t' in l:
                        indent_errors.append(f"Tab character at line {l_idx}")
                if indent_errors:
                    print(f"  YAML Validation: FAILED - {indent_errors}")
                else:
                    print("  YAML Validation: SUCCESS (No tabs, structured indentation)")
            except Exception as e:
                print(f"  YAML Validation: ERROR - {e}")

        # Test Protobuf
        elif lang == 'protobuf' or lang == 'proto':
            # Check syntax, braces matching, field numbering
            open_braces = block.count('{')
            close_braces = block.count('}')
            print(f"  Protobuf Braces Balance: {'BALANCED' if open_braces == close_braces else 'MISMATCH'} ({open_braces} vs {close_braces})")
            
            # Check for duplicate field tags in messages
            messages = re.findall(r'message\s+(\w+)\s*\{([^}]+)\}', block, re.DOTALL)
            for msg_name, msg_body in messages:
                field_tags = re.findall(r'=\s*(\d+);', msg_body)
                if len(field_tags) != len(set(field_tags)):
                    print(f"  Protobuf ERROR in {msg_name}: Duplicate field tags found! {field_tags}")
                else:
                    print(f"  Protobuf message {msg_name}: {len(field_tags)} fields verified unique tags.")

        # Test Rust
        elif lang == 'rust':
            open_braces = block.count('{')
            close_braces = block.count('}')
            print(f"  Rust Braces Balance: {'BALANCED' if open_braces == close_braces else 'MISMATCH'} ({open_braces} vs {close_braces})")
            # Check trait or struct definitions
            traits = re.findall(r'trait\s+(\w+)', block)
            structs = re.findall(r'struct\s+(\w+)', block)
            enums = re.findall(r'enum\s+(\w+)', block)
            print(f"  Rust Definitions: Traits: {traits}, Structs: {structs}, Enums: {enums}")

        # Test Mermaid
        elif lang == 'mermaid':
            print(f"  Mermaid Diagram Type: {block.splitlines()[0] if block.splitlines() else 'Empty'}")
            participants = re.findall(r'participant\s+.*', block)
            print(f"  Mermaid Participants: {len(participants)}")
            arrows = re.findall(r'->>|-->>|->|-->', block)
            print(f"  Mermaid Flow arrows: {len(arrows)}")

    # Check 3: Mathematical LaTeX formulas
    math_blocks = re.findall(r'\$\$(.*?)\$\$|\$(.*?)\$', content, re.DOTALL)
    print(f"\n[Check 3: Mathematical Formulas] Found {len(math_blocks)} math expressions.")
    key_formulas = [
        r'\tau_{\text{harness}}',
        r'\Delta\text{TTFT}',
        r'E_{\text{turn}}',
        r'E_{\text{net}}'
    ]
    for kf in key_formulas:
        present = re.search(re.escape(kf), content) is not None
        print(f"  Formula '{kf}': {'FOUND' if present else 'MISSING'}")

    # Check 4: Requirement Coverage
    print("\n[Check 4: Requirement Coverage Check]")
    req_checks = {
        "R1: Landscape Taxonomy": "Taxonomy of Agent Runtime Architectures",
        "R1: 9 Frameworks Surveyed": ["AutoGen 0.2", "AutoGen 0.4", "CrewAI", "LangGraph", "OpenHands", "Smolagents", "Semantic Kernel", "Aider", "PydanticAI"],
        "R1: 5 Bottleneck Stages": ["Stage 1", "Stage 2", "Stage 3", "Stage 4", "Stage 5"],
        "R1: Quantitative Footprint Table": "QUANTITATIVE MEMORY FOOTPRINT PROFILING",
        "R1: 10x12 Comparative Matrix": "COMPREHENSIVE AGENT HARNESS COMPARATIVE MATRIX",
        "R2: 4 Architectural Configurations": ["Configuration A", "Configuration B", "Configuration C", "Configuration D"],
        "R2: Systems Stack Selection (Rust/Tokio)": "Recommendation: Rust 1.80+",
        "R2: Lockless SPSC SHM": "shm_ring_buf",
        "R2: 3-Tier Sandboxing": ["Tier 1", "Tier 2", "Tier 3"],
        "R2: Local C-FFI vs Cloud API": ["libllama", "SSE"],
        "R2: Host Resource Governor": ["EcoQoS", "SCHED_IDLE", "jemalloc"],
        "R3: 6-Component Architecture": "AEGIS-HARNESS COMPONENT MAP",
        "R3: End-to-End Sequence Diagram": "sequenceDiagram",
        "R3: OpenAPI 3.1 REST Spec": "openapi: 3.1.0",
        "R3: gRPC proto3 Spec": "syntax = \"proto3\";",
        "R3: MCP JSON-RPC Spec": "jsonrpc",
        "R3: Binary SHM Struct": "ShmSlotHeader",
        "R3: Rust Lifecycle Hook (10 points)": "pub trait AgentLifecycleHook",
        "R3: harness-bench Rubric & Micro-benchmarks": ["MB-01", "MB-02", "MB-03", "MB-04", "MB-05"],
        "R3: Macro-benchmarks (4 Scenarios)": ["Scenario 1", "Scenario 2", "Scenario 3", "Scenario 4"],
        "R3: Hardware Energy Protocol": ["RAPL", "SMC", "SYSTEM_BATTERY_STATE"],
        "R3: 16-Week Implementation Roadmap": ["Phase 1", "Phase 2", "Phase 3", "Phase 4"],
        "R3: Risk Assessment Matrix": ["R-01", "R-02", "R-03", "R-04", "R-05"],
        "R3: Feature Traceability Matrix": ["FEAT-01", "FEAT-16"]
    }

    for req, pattern in req_checks.items():
        if isinstance(pattern, list):
            missing = [p for p in pattern if p not in content]
            if missing:
                print(f"  {req}: INCOMPLETE (Missing: {missing})")
            else:
                print(f"  {req}: FULLY PRESENT ({len(pattern)}/{len(pattern)} items)")
        else:
            if pattern in content:
                print(f"  {req}: PRESENT")
            else:
                print(f"  {req}: MISSING ('{pattern}')")

if __name__ == '__main__':
    audit_file('harness_architecture_report.md')
