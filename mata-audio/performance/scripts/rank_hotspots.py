#!/usr/bin/env python3
"""Rank real profiles only. Missing profile input never produces invented hotspots."""
import argparse, re, sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('input',type=Path);p.add_argument('--output',type=Path,default=Path('performance/results/follow-on-hotspots.md'));args=p.parse_args()
if not args.input.is_file() or args.input.stat().st_size==0:
    print('No profile data: follow-on hotspot ranking requires an actual perf report',file=sys.stderr);sys.exit(1)
found=[]
for line in args.input.read_text(errors='replace').splitlines():
    m=re.search(r'^\s*([0-9]+(?:\.[0-9]+)?)%\s+(.+)$',line)
    if m:
        pct=float(m.group(1));symbol=m.group(2).strip()
        if pct>0 and not any(s==symbol for _,s in found):found.append((pct,symbol))
found=sorted(found,reverse=True)[:5]
if len(found)<5:
    print('Profile contains fewer than five ranked samples; cannot confidently prioritize five bottlenecks',file=sys.stderr);sys.exit(1)
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text('# Follow-on optimization candidates (measured)\n\n'+''.join(f'- [ ] **Follow-on {i}** — Profile `{s}` ({pct:.2f}%); measure a targeted optimization before implementing.\n' for i,(pct,s) in enumerate(found,1)))
print(args.output)
