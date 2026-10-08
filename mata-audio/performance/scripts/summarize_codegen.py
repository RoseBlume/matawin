#!/usr/bin/env python3
import csv,glob,os,sys
files=sys.argv[1:] or glob.glob('performance/results/codegen/*.csv')
print('file,decode_realtime_factor_mean')
for path in files:
    vals=[]
    with open(path,newline='') as f:
        for r in csv.DictReader(f):
            if r.get('metric')=='realtime_factor':
                try: vals.append(float(r['value']))
                except: pass
    print(f"{os.path.basename(path)},{sum(vals)/len(vals) if vals else 0:.6f}")
