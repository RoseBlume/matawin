#!/usr/bin/env python3
"""Compare metadata parser timing/allocation results to a recorded baseline."""
from __future__ import annotations
import csv,sys
from pathlib import Path

def rows(path:Path):
    with path.open(newline='') as f: return list(csv.DictReader(f))

def key(row): return (row['feature'],row['family'])

baseline={key(r):r for r in rows(Path(sys.argv[1]))}
current={key(r):r for r in rows(Path(sys.argv[2]))}
thresholds={r['family']:(float(r['max_regression_percent']),float(r['max_allocation_regression_percent'])) for r in csv.DictReader(Path(sys.argv[3]).open())}
errors=[]
for k,b in baseline.items():
    c=current.get(k)
    if c is None: errors.append(f'missing current result {k}'); continue
    family=k[1]
    family_key='read_all' if family=='read_all-enabled-set' else family.replace('-comments','').replace('-','_')
    # Feature-qualified Vorbis rows use the canonical threshold names.
    if family.startswith('vorbis-flac'): family_key='vorbis_flac'
    if family.startswith('vorbis-ogg'): family_key='vorbis_ogg'
    limit=thresholds.get(family_key)
    if limit is None: continue
    bt=max(float(b['total_us']),1.0); ct=float(c['total_us'])
    ba=max(float(b['allocations']),1.0); ca=float(c['allocations'])
    time_pct=(ct/bt-1.0)*100.0; alloc_pct=(ca/ba-1.0)*100.0
    if time_pct>limit[0]: errors.append(f'{k}: time regression {time_pct:.1f}% > {limit[0]:.1f}%')
    if alloc_pct>limit[1]: errors.append(f'{k}: allocation regression {alloc_pct:.1f}% > {limit[1]:.1f}%')
if errors:
    print('\n'.join(errors),file=sys.stderr);raise SystemExit(1)
print('metadata regression gates: OK')
