#!/usr/bin/env python3
"""Compare full old/new decoder PCM streams with corpus tolerance.

Invoke after producing `legacy/ID.f32` and `current/ID.f32` with emit_pcm.
This performs a true samplewise two-build differential, not an indirect hash.
"""
from pathlib import Path
from array import array
import argparse,csv,sys
p=argparse.ArgumentParser()
p.add_argument('legacy',type=Path);p.add_argument('current',type=Path)
p.add_argument('--manifest',type=Path,default=Path('performance/corpus/manifest.tsv'))
a=p.parse_args();errors=[]
for ent in csv.DictReader(a.manifest.open(newline=''),delimiter='\t'):
    name=ent['id'];one=a.legacy/(name+'.f32');two=a.current/(name+'.f32')
    if not one.exists() or not two.exists():errors.append(f'{name}: both output files must exist');continue
    lhs=array('f');rhs=array('f');lhs.frombytes(one.read_bytes());rhs.frombytes(two.read_bytes())
    if sys.byteorder!='little':lhs.byteswap();rhs.byteswap()
    if len(lhs)!=len(rhs):errors.append(f'{name}: count {len(lhs)} vs {len(rhs)}');continue
    limit=float(ent['tolerance']);maximum=0.0
    for x,y in zip(lhs,rhs):
        if x!=x or y!=y or abs(x)==float('inf') or abs(y)==float('inf'):
            errors.append(f'{name}: nonfinite sample');break
        maximum=max(maximum,abs(x-y))
    if maximum>limit:errors.append(f'{name}: numerical divergence {maximum} > {limit}')
    print(f'{name}: {len(lhs)} samples, max abs difference={maximum:.9g}, tolerance={limit:.9g}')
if errors:
    print('\n'.join(errors),file=sys.stderr);sys.exit(1)
print('Old/current PCM differential passed for every corpus entry')
