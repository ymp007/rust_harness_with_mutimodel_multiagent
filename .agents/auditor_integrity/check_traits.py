import re
import sys

sys.stdout.reconfigure(encoding='utf-8')

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

rust_code_blocks = re.findall(r'```rust\n(.*?)```', text, re.DOTALL)
for i, block in enumerate(rust_code_blocks):
    print(f"\n=======================================================")
    print(f"RUST BLOCK #{i+1} ({len(block.splitlines())} lines)")
    print(f"=======================================================")
    lines = block.splitlines()
    in_trait = False
    trait_lines = []
    for line in lines:
        if 'pub trait ' in line:
            in_trait = True
            trait_lines = [line]
        elif in_trait:
            trait_lines.append(line)
            if line.startswith('}'):
                in_trait = False
                print("\n".join(trait_lines))
                print("-------------------------------------------------------")
