#!/usr/bin/env python3
"""Require an accompanying benchmark-note document for changed Rust hot paths.

Run in a git repo with: python3 performance/scripts/check_benchmark_notes.py BASE
A clean tree or changes outside src/ do not require new notes.
"""
import subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[2]
base=sys.argv[1] if len(sys.argv)>1 else 'HEAD'
try:
    changed=subprocess.check_output(['git','diff','--name-only',base,'--','src/','performance/benchmark-notes/'],cwd=root,text=True,stderr=subprocess.DEVNULL).splitlines()
except subprocess.CalledProcessError:
    sys.exit('Could not inspect git diff; specify a valid base revision')
hot=[p for p in changed if p.startswith('src/') and p.endswith('.rs')]
notes=[p for p in changed if p.startswith('performance/benchmark-notes/') and p.endswith('.md')]
if hot and not notes:sys.exit('Rust source was changed but no performance/benchmark-notes/*.md accompanied it')
print(f'Benchmark note gate passed ({len(hot)} Rust files, {len(notes)} notes)')
