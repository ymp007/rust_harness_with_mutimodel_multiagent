import re
import sys

sys.stdout.reconfigure(encoding='utf-8')

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

rust_code_blocks = re.findall(r'```rust\n(.*?)```', text, re.DOTALL)
for i, block in enumerate(rust_code_blocks):
    print(f"\n=======================================================")
    print(f"RUST BLOCK #{i+1} - STRUCT & ENUM DETAIL")
    print(f"=======================================================")
    
    # Extract struct/enum blocks
    type_matches = re.finditer(r'(#\[[^\]]+\]\s*)*(pub (?:struct|enum) \w+(?:<[^>]+>)?\s*\{[^}]+\})', block, re.DOTALL)
    for tm in type_matches:
        print(tm.group(0).strip())
        print("---")
