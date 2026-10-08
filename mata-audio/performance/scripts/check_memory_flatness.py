#!/usr/bin/env python3
import csv,sys
p=sys.argv[1] if len(sys.argv)>1 else 'performance/results/memory.csv'
rows=list(csv.DictReader(open(p,newline='')))
if not rows: raise SystemExit('no memory rows')
bad=[]
for r in rows:
    growth=int(r['steady_growth_bytes'])
    # Indexes may learn during the first additional pass; after warm-up this is
    # expected to remain bounded. 1 MiB is a conservative regression gate.
    if growth>1024*1024: bad.append((r['id'],growth))
if bad: raise SystemExit('memory-flatness regression: '+repr(bad))
print(f'memory-flatness gate passed for {len(rows)} decoders')
