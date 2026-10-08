#!/usr/bin/env python3
import csv,sys
if len(sys.argv)<3:
    raise SystemExit('usage: compare_results.py BASELINE CURRENT [MAX_REGRESSION_PERCENT]')
limit=float(sys.argv[3]) if len(sys.argv)>3 else 5.0
def load(p):
    with open(p,newline='') as f:
        return {(r['kind'],r['id'],r['metric']):r for r in csv.DictReader(f)}
b,c=load(sys.argv[1]),load(sys.argv[2]); bad=[]
for k,base in b.items():
    cur=c.get(k)
    if cur is None:
        continue
    if base.get('unit')=='hash':
        if base['value'] != cur['value']:
            bad.append((k,float('inf'),base['value'],cur['value']))
        continue
    try:
        old=float(base['value']); new=float(cur['value'])
    except ValueError:
        continue
    if old<=0: continue
    # Higher is better only for throughput/realtime factor; the remaining
    # baseline metrics are latency/allocation/memory/error measurements.
    regression=((old-new)/old*100.0) if k[2]=='realtime_factor' else ((new-old)/old*100.0)
    if regression>limit: bad.append((k,regression,old,new))
for k,r,o,n in bad:
    if r==float('inf'): print(f'CORRECTNESS CHANGE {k}: {o} -> {n}')
    else: print(f'REGRESSION {k}: {r:.2f}% ({o} -> {n})')
raise SystemExit(1 if bad else 0)
