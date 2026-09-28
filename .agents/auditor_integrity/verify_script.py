import re
import os

with open('master_implementation_plan.md', 'r', encoding='utf-8') as f:
    text = f.read()

# Find code blocks
code_blocks = re.findall(r'```([a-zA-Z0-9_-]*)\n(.*?)```', text, re.DOTALL)
print(f'Total code blocks: {len(code_blocks)}')

languages = {}
for lang, code in code_blocks:
    languages[lang] = languages.get(lang, 0) + 1

for lang, count in sorted(languages.items(), key=lambda x: -x[1]):
    print(f'  Language: {lang or "None"} -> {count} blocks')

print(f"\n--- Mermaid Diagrams ({len([c for l, c in code_blocks if l == 'mermaid'])}) ---")
for i, (l, c) in enumerate(code_blocks):
    if l == 'mermaid':
        first_line = c.strip().split('\n')[0] if c.strip() else ""
        print(f"Mermaid {i+1}: {first_line[:80]}")

print(f"\n--- Rust Code Blocks ({len([c for l, c in code_blocks if l == 'rust'])}) ---")
for i, (l, c) in enumerate(code_blocks):
    if l == 'rust':
        lines = c.strip().split('\n')
        traits = [line for line in lines if 'pub trait ' in line]
        structs = [line for line in lines if 'pub struct ' in line or 'pub enum ' in line]
        print(f"Rust Block {i+1} ({len(lines)} lines): Structs/Enums={len(structs)}, Traits={len(traits)}")
        for t in traits:
            print(f"   Trait: {t.strip()}")
        for s in structs:
            print(f"   Struct/Enum: {s.strip()}")

print(f"\n--- Mathematical Equations ---")
inline_math = re.findall(r'\$([^\$]+)\$', text)
block_math = re.findall(r'\$\$([^\$]+)\$\$', text)
print(f"Inline math expressions ($...$): {len(inline_math)}")
print(f"Block math expressions ($$...$$): {len(block_math)}")
for i, bm in enumerate(block_math):
    print(f"Block Math {i+1}:\n{bm.strip()}\n")
