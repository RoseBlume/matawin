#!/usr/bin/env python3
"""P632-P651: compile reproducible report; freeze ONLY measured, passing results.

Usage:
  python3 performance/scripts/report_hardening.py --draft
  python3 performance/scripts/report_hardening.py --freeze --tests-passed

Freeze requires a full actual benchmark CSV, results from the same corpus, an
explicit passing test gate, and a device-backed metrics file when requested.
There is intentionally no synthetic "known good" fallback.
"""
from __future__ import annotations
import argparse, csv, datetime, hashlib, json, os, platform, shutil, subprocess, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
PERF=ROOT/'performance'
RESULTS=PERF/'results'
CORPUS=PERF/'corpus'/'manifest.tsv'
BUDGETS=PERF/'budgets.json'
BASE=PERF/'baselines'

def sha(path:Path)->str:
    h=hashlib.sha256()
    with path.open('rb') as f:
        for chunk in iter(lambda:f.read(1<<20),b''):h.update(chunk)
    return h.hexdigest()

def rows(path:Path)->list[dict[str,str]]:
    if not path.exists() or not path.stat().st_size:return []
    with path.open(newline='') as f:return list(csv.DictReader(f))

def run(*args:str)->str:
    try:return subprocess.check_output(args,text=True,stderr=subprocess.DEVNULL).strip()
    except Exception:return 'unavailable'

def src_digest()->str:
    h=hashlib.sha256()
    paths=[ROOT/'Cargo.toml',*sorted((ROOT/'src').rglob('*.rs')),*sorted((ROOT/'tests').rglob('*.rs'))]
    for p in paths:
        h.update(str(p.relative_to(ROOT)).encode());h.update(bytes.fromhex(sha(p)))
    return h.hexdigest()

def check_budget(measurements:list[dict[str,str]],manifest:list[dict[str,str]],b:dict)->list[str]:
    failures=[]
    keyed={(r['kind'],r['id'],r['metric']):r['value'] for r in measurements}
    required=('realtime_factor','allocations_per_frame','peak_rss','open_latency')
    for entry in manifest:
        id=entry['id'];codec=entry['codec'].split('_',1)[0]
        for metric in required:
            key=('decode',id,metric)
            if key not in keyed:failures.append(f'missing measured {key}');continue
            try:v=float(keyed[key])
            except ValueError:failures.append(f'non-numeric measured {key}');continue
            if not (v>=0 and v<float('inf')):failures.append(f'invalid metric {key}');continue
            if metric=='realtime_factor' and v<b['min_realtime_factor'][codec]:failures.append(f'{id}: real-time factor {v:.3f} < {b["min_realtime_factor"][codec]}')
            if metric=='allocations_per_frame' and v>b['max_decode_allocations_per_frame']:failures.append(f'{id}: alloc/frame {v:.3f} over budget')
            if metric=='peak_rss' and v>b['max_decode_peak_rss_kib']:failures.append(f'{id}: peak RSS {v:.0f} KiB over budget')
            if metric=='open_latency' and v>b['max_open_latency_ns']:failures.append(f'{id}: open latency over budget')
        if ('correctness',id,'pcm_fnv1a64') not in keyed:failures.append(f'{id}: no decoded PCM checksum')
        if ('correctness',id,'max_abs_error') not in keyed:failures.append(f'{id}: no reference error')
        else:
            try:
                error=float(keyed['correctness',id,'max_abs_error'])
                if error>float(entry['tolerance']):failures.append(f'{id}: PCM error exceeds tolerance')
            except ValueError:failures.append(f'{id}: nonnumeric PCM error')
    return failures

def main()->int:
    ap=argparse.ArgumentParser()
    ap.add_argument('--draft',action='store_true')
    ap.add_argument('--freeze',action='store_true')
    ap.add_argument('--tests-passed',action='store_true',help='Required after actual cargo test execution')
    ap.add_argument('--csv',type=Path,default=RESULTS/'current.csv')
    ap.add_argument('--device-csv',type=Path,default=RESULTS/'device-current.csv')
    a=ap.parse_args()
    manifest=list(csv.DictReader(CORPUS.open(newline=""),delimiter="	")); measured=rows(a.csv);device=rows(a.device_csv)
    budgets=json.loads(BUDGETS.read_text())
    failures=check_budget(measured,manifest,budgets)
    if not device:failures.append('device-backed mixer/underrun/command/seek/loop metrics not recorded')
    else:
        keys={(r.get('kind'),r.get('metric')):r.get('value') for r in device}
        required={'mixer_cpu_percent':budgets['playback']['max_mixer_cpu_percent'],
                  'underrun_events_per_hour':budgets['playback']['max_underrun_events_per_hour'],
                  'command_to_audio_ns_p95':budgets['playback']['max_command_to_audio_ns_p95'],
                  'native_seek_ns_p95':budgets['timeline']['max_native_seek_ns_p95'],
                  'loop_boundary_gap_samples':budgets['timeline']['max_loop_boundary_gap_samples']}
        for metric,limit in required.items():
            values=[v for (kind,key),v in keys.items() if key==metric]
            if not values:failures.append('missing device measurement: '+metric);continue
            try:
                if max(map(float,values))>limit:failures.append('device budget exceeded: '+metric)
            except ValueError:failures.append('non-numeric device metric: '+metric)
    digest=src_digest(); hardware=RESULTS/'hardware-current.md'
    iso=datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')
    status='PASS (measurements + test gate)' if not failures and a.tests_passed else 'NOT VALIDATED'
    report=[
        '# mata-audio — P602–P651 final performance report',
        '',f'- Generated (UTC): {iso}',f'- Status: **{status}**',
        f'- Source SHA-256 (manifest of source bytes): `{digest}`',
        f'- Corpus manifest SHA-256: `{sha(CORPUS)}`',
        f'- Cargo: `{run("cargo","--version")}`',f'- Rust: `{run("rustc","--version")}`',
        f'- OS/CPU: `{platform.platform()}` / `{platform.processor()}`',
        f'- RUSTFLAGS: `{os.getenv("RUSTFLAGS","")}`',
        f'- Test gate asserted: **{a.tests_passed}**',
        f'- Compiler/feature/ISA identifiers: {len({(r.get("compiler"),r.get("features"),r.get("isa")) for r in measured})} combinations in CSV',
        '', '## Recorded inputs', '',
        f'- Corpus entries: {len(manifest)}',f'- Benchmark observations: {len(measured)}',
        f'- Device-backed observations: {len(device)}',
        f'- Hardware fingerprint: `{hardware}` ({"captured" if hardware.exists() else "missing"})',
        f'- Corpus manifest: `{CORPUS.relative_to(ROOT)}`',
        f'- Runtime benchmark CSV: `{a.csv}`',
        f'- Playback/device CSV: `{a.device_csv}`',
        '', '## Performance and correctness budgets', '',
        'See `performance/budgets.json`; thresholds are provisional until calibrated on pinned hardware.',
        '', '| Corpus ID | real-time factor | allocations/frame | peak RSS KiB | startup ns | PCM error |',
        '|---|---:|---:|---:|---:|---:|',
    ]
    lookup={(r['kind'],r['id'],r['metric']):r['value'] for r in measured}
    for ent in manifest:
        id=ent['id']; get=lambda k,m:lookup.get((k,id,m),'unmeasured')
        report.append(f'| {id} | {get("decode","realtime_factor")} | {get("decode","allocations_per_frame")} | {get("decode","peak_rss")} | {get("decode","open_latency")} | {get("correctness","max_abs_error")} |')
    report+=['','## Open validation gates','']
    report+=['- '+x for x in failures] if failures else ['- All available metric checks passed.']
    if not a.tests_passed:report.append('- Rust test execution has not been confirmed.')
    report+=['', '## Tooling/limitations', '',
             '- Stage profiles: `performance/scripts/profile_final.sh`',
             '- Thermal/stability: `performance/scripts/benchmark_thermal.sh`',
             '- Sanitizer/Miri: `performance/scripts/run_unsafe_validation.sh`',
             '- Full integrated gate: `performance/scripts/validate_p602_p651.sh`',
             '- A missing device-backed metric is a hard freeze blocker, not a fabricated zero.',
             '']
    RESULTS.mkdir(exist_ok=True)
    out=RESULTS/'final-performance-report.md';out.write_text('\n'.join(report))
    if a.freeze:
        if failures or not a.tests_passed:
            print('REFUSED to freeze unvalidated baseline:',*failures,sep='\n- ',file=sys.stderr)
            return 1
        BASE.mkdir(exist_ok=True)
        target=BASE/'known-good.csv';shutil.copyfile(a.csv,target)
        meta={'schema':1,'frozen_utc':iso,'source_sha256':digest,'corpus_manifest_sha256':sha(CORPUS),
              'benchmark_sha256':sha(target),'device_sha256':sha(a.device_csv),'hardware_sha256':sha(hardware) if hardware.exists() else None,
              'compiler':run('rustc','--version'),'rustflags':os.getenv('RUSTFLAGS',''),
              'feature_sets':sorted({r.get('features','') for r in measured}),
              'isa':sorted({r.get('isa','') for r in measured})}
        (BASE/'known-good.json').write_text(json.dumps(meta,indent=2)+'\n')
        print('Frozen verified baseline:',BASE/'known-good.json')
    else:print('Generated',out,'(',status,')')
    return 0
if __name__=='__main__':raise SystemExit(main())
