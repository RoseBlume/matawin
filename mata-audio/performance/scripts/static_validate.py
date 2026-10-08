#!/usr/bin/env python3
"""Static consistency checks usable even when cargo/rustc are unavailable.

This does not replace `cargo check`, `cargo test`, or benchmark execution. It is
intended to catch feature drift, roadmap numbering mistakes, stale refactor
paths, and corpus damage before handing the crate to a Rust-enabled host.
"""
from __future__ import annotations

import hashlib
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "src"
TODO = ROOT / "todo.md"
MANIFEST = ROOT / "performance" / "corpus" / "manifest.tsv"


def fail(message: str) -> None:
    print(f"ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def cargo_features() -> set[str]:
    data = tomllib.loads((ROOT / "Cargo.toml").read_text())
    return set(data.get("features", {}))


def source_cfg_features() -> set[str]:
    found: set[str] = set()
    pattern = re.compile(r'feature\s*=\s*"([^"]+)"')
    for path in SRC.rglob("*.rs"):
        found.update(pattern.findall(path.read_text(errors="replace")))
    return found


def check_features() -> None:
    declared = cargo_features()
    used = source_cfg_features()
    missing = sorted(used - declared)
    if missing:
        fail("source cfg features missing from Cargo.toml: " + ", ".join(missing))

    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    features = cargo["features"]
    required = {
        "codec-flac", "codec-mp1", "codec-mp2", "codec-mp3", "codec-pcm", "codec-vorbis",
        "demux-aiff", "demux-flac", "demux-matroska", "demux-ogg", "demux-wav",
        "metadata-ape", "metadata-id3v1", "metadata-id3v2", "metadata-iso-mp4",
        "metadata-riff", "metadata-vorbis-flac", "metadata-vorbis-ogg",
        "all-codecs", "all-demuxers", "all-metadata", "auto-detect", "simd", "benchmarking",
        "perf-instrumentation", "offline", "fast-math", "prefetch", "mmap",
    }
    absent = sorted(required - declared)
    if absent:
        fail("required features are absent: " + ", ".join(absent))

    default_closure: set[str] = set()
    stack = list(features.get("default", []))
    while stack:
        item = stack.pop()
        if item.startswith("dep:") or item in default_closure:
            continue
        default_closure.add(item)
        stack.extend(x for x in features.get(item, []) if not x.startswith("dep:"))
    component_features = {x for x in required if x.startswith(("codec-", "demux-", "metadata-"))}
    missing_default = sorted(component_features - default_closure)
    if missing_default:
        fail("component features not reachable from default: " + ", ".join(missing_default))
    if "offline" not in default_closure:
        fail("default feature closure no longer includes offline support")
    for experimental in ("fast-math", "prefetch", "mmap"):
        if experimental in default_closure:
            fail(f"experimental feature unexpectedly enabled by default: {experimental}")


def check_todo_numbers() -> None:
    text = TODO.read_text()
    phases = [int(x) for x in re.findall(r"\*\*P(\d+)\s+—", text)]
    expected = list(range(652))
    if phases != expected:
        missing = sorted(set(expected) - set(phases))
        dupes = sorted({x for x in phases if phases.count(x) > 1})
        fail(f"todo phase numbering mismatch; missing={missing}, duplicates={dupes}")


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def check_corpus() -> None:
    lines = MANIFEST.read_text().splitlines()
    if len(lines) < 2:
        fail("corpus manifest is empty")
    headers = lines[0].split("\t")
    idx = {name: n for n, name in enumerate(headers)}
    corpus = MANIFEST.parent
    reference = corpus.parent / "reference"
    for row_no, line in enumerate(lines[1:], 2):
        if not line.strip():
            continue
        c = line.split("\t")
        input_path = corpus / c[idx["file"]]
        ref_path = reference / c[idx["reference"]]
        if not input_path.is_file() or not ref_path.is_file():
            fail(f"manifest row {row_no} references a missing file")
        if sha256(input_path) != c[idx["input_sha256"]]:
            fail(f"input SHA mismatch on row {row_no}: {input_path.name}")
        if sha256(ref_path) != c[idx["reference_sha256"]]:
            fail(f"reference SHA mismatch on row {row_no}: {ref_path.name}")


def strip_rust_literals(text: str) -> str:
    # Lightweight lexer sufficient for delimiter consistency. Preserve newlines
    # while blanking line comments, nested-ish block comments, char/string/raw
    # string contents. Cargo/rustc remain the authoritative parser.
    out: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0
    while i < n:
        if block_depth:
            if text.startswith("/*", i):
                block_depth += 1; out.extend("  "); i += 2; continue
            if text.startswith("*/", i):
                block_depth -= 1; out.extend("  "); i += 2; continue
            out.append("\n" if text[i] == "\n" else " "); i += 1; continue
        if text.startswith("//", i):
            j = text.find("\n", i)
            if j < 0: out.extend(" " * (n - i)); break
            out.extend(" " * (j - i)); out.append("\n"); i = j + 1; continue
        if text.startswith("/*", i):
            block_depth = 1; out.extend("  "); i += 2; continue
        # raw strings: r"...", r#"..."#, br#"..."#
        m = re.match(r'(?:b)?r(#{0,16})"', text[i:])
        if m:
            hashes = m.group(1); token_len = m.end(); out.extend(" " * token_len); i += token_len
            end = '"' + hashes; j = text.find(end, i)
            if j < 0: out.extend(" " * (n - i)); break
            segment = text[i:j+len(end)]; out.extend("\n" if ch == "\n" else " " for ch in segment); i = j + len(end); continue
        if text[i] in ('"', "'"):
            quote = text[i]
            # A lifetime starts with apostrophe followed by an identifier and no
            # matching quote immediately; leave it alone.
            if quote == "'" and i + 1 < n and (text[i+1].isalpha() or text[i+1] == '_'):
                end = i + 2
                while end < n and (text[end].isalnum() or text[end] == '_'): end += 1
                if end >= n or text[end] != "'":
                    out.append(text[i]); i += 1; continue
            out.append(" "); i += 1
            escaped = False
            while i < n:
                ch = text[i]; out.append("\n" if ch == "\n" else " "); i += 1
                if escaped: escaped = False; continue
                if ch == "\\": escaped = True; continue
                if ch == quote: break
            continue
        out.append(text[i]); i += 1
    return "".join(out)


def check_delimiters() -> None:
    pairs = {')': '(', ']': '[', '}': '{'}
    for path in SRC.rglob("*.rs"):
        stack: list[tuple[str, int]] = []
        cleaned = strip_rust_literals(path.read_text(errors="replace"))
        for pos, ch in enumerate(cleaned):
            if ch in "([{": stack.append((ch, pos))
            elif ch in ")]}":
                if not stack or stack[-1][0] != pairs[ch]:
                    fail(f"unbalanced {ch!r} in {path.relative_to(ROOT)} near byte {pos}")
                stack.pop()
        if stack:
            fail(f"unclosed delimiter in {path.relative_to(ROOT)} near byte {stack[-1][1]}")


def check_refactor_invariants() -> None:
    if (SRC / "codecs" / "simd").exists():
        fail("stale src/codecs/simd directory exists")
    if not (SRC / "simd" / "mod.rs").is_file():
        fail("root SIMD module is missing")
    lib_surface = (SRC / "lib.rs").read_text()
    if "pub mod offline;" in lib_surface:
        fail("offline mode must remain under codecs; do not add an extra root module")
    mpeg = (SRC / "codecs" / "mpeg" / "mod.rs").read_text()
    if ".drain(" in mpeg:
        fail("MPEG compressed-input path still contains Vec::drain")
    # `shrink_to_fit` is now permitted only as an explicitly named cold/non-
    # realtime reclamation mechanism. Steady-state reset/seek paths retain
    # capacity; P518 intentionally added trim APIs.

    simd = (SRC / "simd" / "mod.rs").read_text()
    required_simd = [
        "pcm_i24_le", "pcm_i24_be", "pcm_i32_le", "pcm_i32_be", "pcm_f32_le", "pcm_f64_le",
        "mix_add", "overlap", "interleave_stereo", "deinterleave_stereo", "lerp", "butterfly",
        "gain", "limiter", "stereo_average", "mono_duplicate",
    ]
    for token in required_simd:
        if token not in simd:
            fail(f"SIMD dispatch surface missing {token}")

    wav = (SRC / "demuxers" / "wav" / "mod.rs").read_text()
    aiff = (SRC / "demuxers" / "aiff" / "mod.rs").read_text()
    pcm = (SRC / "codecs" / "pcm.rs").read_text()
    flac_mod = (SRC / "codecs" / "flac" / "mod.rs").read_text()
    flac_bits = (SRC / "codecs" / "flac" / "bit_reader.rs").read_text()
    flac_frame = (SRC / "codecs" / "flac" / "frame.rs").read_text()
    flac_residual = (SRC / "codecs" / "flac" / "residual.rs").read_text()
    flac_demux = (SRC / "demuxers" / "flac.rs").read_text()
    required = {
        "WAV direct seek": (wav, "seek_frame"),
        "AIFF direct seek": (aiff, "seek_frame"),
        "cached PCM decode plan": (pcm, "PcmDecodePlan"),
        "AIFC little endian": (pcm, "PcmLittleEndian"),
        "persistent FLAC bit reader": (flac_mod, "BitReader<R>"),
        "FLAC word reservoir": (flac_bits, "reservoir: u64"),
        "FLAC incremental crc8": (flac_bits, "crc8_update"),
        "FLAC aligned flat scratch": (flac_frame, "channels: AlignedScratch<i64>"),
        "FLAC aligned residual scratch": (flac_frame, "residual: AlignedScratch<i64>"),
        "FLAC Rice zero specialization": (flac_residual, "rice_parameter == 0"),
        "FLAC seektable": (flac_demux, "SeekPoint"),
        "FLAC lazy index": (flac_mod, "frame_index"),
    }
    for label, (text, token) in required.items():
        if token not in text:
            fail(f"P121-P216 invariant missing: {label}")
    if "raw: Vec<u8>" in flac_bits:
        fail("FLAC bit reader still captures raw frame bytes")

    mpeg_core = (SRC / "codecs" / "mpeg" / "mod.rs").read_text()
    mpeg_simd = (SRC / "simd" / "mpeg.rs").read_text()
    vorbis_imdct = (SRC / "codecs" / "vorbis" / "imdct.rs").read_text()
    vorbis_codebook = (SRC / "codecs" / "vorbis" / "codebook.rs").read_text()
    ogg = (SRC / "demuxers" / "ogg.rs").read_text()
    matroska = (SRC / "demuxers" / "matroska.rs").read_text()
    demux_mod = (SRC / "demuxers" / "mod.rs").read_text()
    later_required = {
        "persistent MPEG scratch": (mpeg_core, "mp3dec_scratch_t"),
        "MPEG leading ID3 skip": (mpeg_core, "skip_leading_id3v2"),
        "MPEG sparse seek index": (mpeg_core, "MpegSeekPoint"),
        "MPEG full pow43 table": (mpeg_simd, "POW43_TABLE_MAX"),
        "FFT-backed Vorbis IMDCT": (vorbis_imdct, "fft_len"),
        "Vorbis multilevel Huffman": (vorbis_codebook, "second:Vec<FastEntry>"),
        "incremental Ogg CRC": (ogg, "ogg_crc32_update"),
        "Ogg borrowed packet view": (ogg, "OggPacketView"),
        "common demux seek": (demux_mod, "trait DemuxerSeek"),
        "Matroska bounded payload": (matroska, "bounded_payload_len"),
        "Matroska cluster index": (matroska, "MatroskaClusterPoint"),
    }
    for label, (text, token) in later_required.items():
        if token not in text:
            fail(f"P217-P359 invariant missing: {label}")
    for required_file in [
        ROOT / "src/bin/mata-audio-demux-perf.rs",
        ROOT / "src/bin/mata-audio-demux-fuzz.rs",
        ROOT / "performance/demuxer_manifest.tsv",
        ROOT / "performance/demuxer_thresholds.tsv",
    ]:
        if not required_file.is_file(): fail(f"P340-P359 harness file missing: {required_file.relative_to(ROOT)}")


    metadata_mod = (SRC / "metadata" / "mod.rs").read_text()
    metadata_support = (SRC / "metadata" / "support.rs").read_text()
    id3v2 = (SRC / "metadata" / "id3v2.rs").read_text()
    iso_mp4 = (SRC / "metadata" / "iso_mp4.rs").read_text()
    vorbis_common = (SRC / "metadata" / "vorbis_common.rs").read_text()
    playback = (SRC / "playback.rs").read_text()
    codecs_mod = (SRC / "codecs" / "mod.rs").read_text()
    lib = (SRC / "lib.rs").read_text()
    p360_required = {
        "metadata position preservation": (metadata_support, "preserve_position"),
        "metadata bounded reads": (metadata_support, "read_exact_vec"),
        "borrowed metadata view": (metadata_mod, "MetadataView"),
        "ID3 in-place unsynchronization": (id3v2, "deunsynchronize_in_place"),
        "MP4 extended size traversal": (iso_mp4, "size32 == 1"),
        "shared Vorbis comment iterator": (vorbis_common, "CommentIter"),
        "packed generational voice store": (playback, "VoiceStore"),
        "bounded decoder worker pool": (playback, "DecodeWorkerPool"),
        "fixed point playback phase": (playback, "phase_q32"),
        "adaptive decoder demand": (playback, "requested_samples"),
        "cache padded SPSC indices": (playback, "CachePaddedIndex"),
        "atomic RT error publication": (playback, "RT_ERROR_CODE"),
        "playback telemetry": (playback, "PlaybackTelemetry"),
        "native decoder sample seek": (codecs_mod, "seek_to_sample"),
        "feature-aware AudioDecoder seek": (lib, "fn seek_to_sample"),
        "loop prefix cache": (playback, "loop_prefix"),
    }
    for label, (text, token) in p360_required.items():
        if token not in text:
            fail(f"P360-P501 invariant missing: {label}")
    if "self.buffer.drain(" in playback:
        fail("Voice staging still drains consumed prefixes")
    if "sync_channel::<Arc<DecodeProducer>>" in playback:
        fail("central mixer still submits decoder jobs through a locking sync_channel")
    if "DecodeJobQueue" not in playback:
        fail("lock-free per-worker decoder job queue is missing")
    if "device_period_frames" not in playback or "requested_samples" not in playback:
        fail("device-period/decode-demand adaptive playback tuning is missing")
    for required_file in [
        ROOT / "src/bin/mata-audio-metadata-perf.rs",
        ROOT / "src/bin/mata-audio-metadata-fuzz.rs",
        ROOT / "src/bin/mata-audio-seek-perf.rs",
        ROOT / "performance/metadata/thresholds.tsv",
        ROOT / "performance/FEATURE_COMBINATIONS.md",
        ROOT / "performance/P360_P501_IMPLEMENTATION.md",
        ROOT / "performance/scripts/run_metadata_matrix.sh",
        ROOT / "performance/scripts/check_feature_matrix.sh",
        ROOT / "performance/scripts/measure_feature_costs.sh",
        ROOT / "performance/scripts/run_seek_validation.sh",
        ROOT / "performance/scripts/validate_p360_p501.sh",
        ROOT / ".github/workflows/feature-matrix.yml",
        ROOT / "tests/default_feature_surface.rs",
        ROOT / "tests/metadata_position.rs",
        ROOT / "tests/seek_timeline.rs",
    ]:
        if not required_file.is_file(): fail(f"P360-P501 harness file missing: {required_file.relative_to(ROOT)}")

    memory_support = (SRC / "codecs" / "support" / "memory.rs").read_text()
    offline = (SRC / "codecs" / "offline.rs").read_text()
    pcm = (SRC / "codecs" / "pcm.rs").read_text()
    codebook = (SRC / "codecs" / "vorbis" / "codebook.rs").read_text()
    imdct = (SRC / "codecs" / "vorbis" / "imdct.rs").read_text()
    error = (SRC / "error.rs").read_text()
    cargo_text = (ROOT / "Cargo.toml").read_text()
    p502_required = {
        "decoder memory accounting": (codecs_mod, "DecoderMemoryUsage"),
        "bounded scratch allocation": (memory_support, "MAX_DECODER_SCRATCH_BYTES"),
        "aligned hot scratch": (memory_support, "AlignedScratch"),
        "compact Vorbis entries": (codebook, "pub dimensions:u16,pub entries:u32"),
        "cold error formatting": (error, "#[cold]"),
        "portable perf profile": (cargo_text, "[profile.perf]"),
        "size perf profile": (cargo_text, "[profile.perf-size]"),
        "fat LTO profile": (cargo_text, "[profile.perf-fat]"),
        "offline bounded read ahead": (offline, "ReadAheadFile"),
        "offline parallel channels": (offline, "sanitize_channels_parallel"),
        "cached PCM conversion kernel": (pcm, "decode_fn"),
        "blocked Vorbis transform": (imdct, "256"),
        "optional mmap": (cargo_text, 'mmap = ["offline", "dep:memmap2"]'),
        "optional prefetch": (cargo_text, "prefetch = []"),
        "optional fast math": (cargo_text, "fast-math = []"),
        "playback retained memory telemetry": (playback, "VOICE_HIGH_WATER_BYTES"),
        "cached output session": (playback, "struct OutputSession"),
        "platform output policy": (playback, "platform_output_policy"),
        "function-pointer resampler": (playback, "render_kernel"),
        "function-pointer limiter": (playback, "limiter_kernel"),
        "Q32 timeline": (playback, "phase_q32"),
        "denormal mitigation": (playback, "sanitize_non_normal_sample"),
    }
    for label, (text, token) in p502_required.items():
        if token not in text:
            fail(f"P502-P601 invariant missing: {label}")
    if "Vec<Vec" in (SRC / "codecs" / "vorbis" / "header.rs").read_text():
        fail("Vorbis setup still contains nested Vec<Vec> allocations")
    for required_file in [
        ROOT / "src/bin/mata-audio-memory-perf.rs",
        ROOT / "src/bin/mata-audio-playback-memory-perf.rs",
        ROOT / "src/bin/mata-audio-math-perf.rs",
        ROOT / "src/bin/mata-audio-offline-perf.rs",
        ROOT / "src/bin/mata-audio-memory-strategies.rs",
        ROOT / "performance/MEMORY.md",
        ROOT / "performance/CODEGEN.md",
        ROOT / "performance/PLATFORM.md",
        ROOT / "performance/ADVANCED_PERFORMANCE.md",
        ROOT / "performance/P502_P601_IMPLEMENTATION.md",
        ROOT / "performance/scripts/run_memory_validation.sh",
        ROOT / "performance/scripts/run_playback_memory_validation.sh",
        ROOT / "performance/scripts/benchmark_codegen.sh",
        ROOT / "performance/scripts/inspect_assembly.sh",
        ROOT / "performance/scripts/benchmark_backends.sh",
        ROOT / "performance/scripts/platform_smoke.sh",
        ROOT / "performance/scripts/cross_target_smoke.sh",
        ROOT / "performance/scripts/benchmark_offline_pipeline.sh",
        ROOT / "performance/scripts/benchmark_prefetch.sh",
        ROOT / "performance/scripts/benchmark_math_strategies.sh",
        ROOT / ".github/workflows/performance-smoke.yml",
    ]:
        if not required_file.is_file(): fail(f"P502-P601 harness file missing: {required_file.relative_to(ROOT)}")


def main() -> None:
    check_features()
    check_todo_numbers()
    check_corpus()
    check_delimiters()
    check_refactor_invariants()
    print("static validation OK: features, TODO numbering, corpus hashes, delimiters, and refactor invariants")


if __name__ == "__main__":
    main()
