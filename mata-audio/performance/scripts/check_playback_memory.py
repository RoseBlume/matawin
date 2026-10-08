#!/usr/bin/env python3
import csv, sys
from pathlib import Path

def rows(path):
    with Path(path).open(newline='') as f:
        return list(csv.DictReader(f))

def check(path, max_growth_per_voice=1024*1024):
    data=rows(path)
    if not data: raise SystemExit(f"empty playback memory result: {path}")
    voices=int(data[-1]['voices'])
    warm=data[min(len(data)-1, max(0, len(data)//10))]
    base=int(warm['retained_bytes'])
    peak=max(int(r['retained_bytes']) for r in data)
    growth=max(0, peak-base)
    budget=max_growth_per_voice*max(1, voices)
    if growth > budget:
        raise SystemExit(f"{path}: retained growth {growth} exceeds bounded budget {budget}")
    return voices,base,peak,growth

for path in sys.argv[1:]:
    v,b,p,g=check(path)
    print(f"{path}: voices={v} baseline={b} peak={p} growth={g}")
