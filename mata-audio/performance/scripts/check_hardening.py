#!/usr/bin/env python3
"""Static P602-P651 invariants (not a substitute for Rust integration)."""
from pathlib import Path
import re,json
root=Path(__file__).resolve().parents[2]
todo=(root/'todo.md').read_text()
phases=[int(i) for i in re.findall(r'\*\*P(\d+)\s+—',todo)]
assert phases==list(range(652))
for n in range(652):
    assert re.search(rf'^- \[x\] \*\*P{n} —',todo,re.M),f'unchecked P{n}'
for path in ('tests/hardening_decode.rs','performance/budgets.json','performance/scripts/report_hardening.py',
             'performance/scripts/run_unsafe_validation.sh','performance/scripts/benchmark_thermal.sh','performance/scripts/profile_final.sh'):
    assert (root/path).is_file(),path
budgets=json.loads((root/'performance/budgets.json').read_text())
assert budgets['schema_version']==1
lock=root/'performance/baselines/known-good.json'
if lock.exists():
    import hashlib
    obj=json.loads(lock.read_text())
    blob=root/'performance/baselines/known-good.csv'
    assert blob.exists() and hashlib.sha256(blob.read_bytes()).hexdigest()==obj['benchmark_sha256'], 'frozen baseline integrity mismatch' 
print('P602-P651 static source/roadmap/hardening invariants: OK')
