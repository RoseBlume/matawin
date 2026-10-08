#!/usr/bin/env python3
import csv,sys
from pathlib import Path
results=Path(sys.argv[1] if len(sys.argv)>1 else 'performance/results/demuxers.csv')
thresholds=Path(sys.argv[2] if len(sys.argv)>2 else 'performance/demuxer_thresholds.tsv')
limits={}
with thresholds.open() as f:
    for i,line in enumerate(f):
        if i==0 or not line.strip(): continue
        k,start,rate=line.rstrip().split('\t');limits[k]=(int(start),float(rate))
failed=[]
with results.open() as f:
    for row in csv.DictReader(f):
        if row['id'] not in limits: continue
        start,rate=limits[row['id']]
        if int(row['startup_ns'])>start: failed.append(f"{row['id']}: startup {row['startup_ns']} > {start}")
        if float(row['throughput_mib_s'])<rate: failed.append(f"{row['id']}: throughput {row['throughput_mib_s']} < {rate}")
if failed:
    print('\n'.join(failed));sys.exit(1)
print('demuxer regression thresholds passed')
