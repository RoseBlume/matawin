#!/usr/bin/env python3
import csv,hashlib,pathlib,sys
manifest=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'performance/corpus/manifest.tsv')
root=manifest.parent
refs=root.parent/'reference'
with manifest.open(newline='') as f:
    rows=csv.DictReader(f,delimiter='\t')
    for r in rows:
        for path,key in [(root/r['file'],'input_sha256'),(refs/r['reference'],'reference_sha256')]:
            got=hashlib.sha256(path.read_bytes()).hexdigest()
            if got != r[key]: raise SystemExit(f'{path}: hash mismatch {got} != {r[key]}')
print('corpus hashes OK')
