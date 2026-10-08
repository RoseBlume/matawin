# Build fixes (October 8, 2026)

This patch addresses the five compiler errors reported on Windows for the
P0–P651 `mata-audio` source:

1. `playback.rs`: terminate the `tune_worker(index)` statement before `loop`.
2. `playback.rs`: replace a missing `atomic_max_usize` helper with the standard
   atomic `fetch_max` operation.
3. `demuxers/matroska.rs`: discard the `Seek::seek` result in the wildcard
   `match` branch, keeping all branches at `()`.
4. `codecs/flac/bit_reader.rs`: return the `bool` expression directly.
5. `simd/mpeg.rs`: use a `Layer3Scratch` view to borrow only the Layer III
   work areas while the bitstream independently borrows `maindata`.

Also: added an explicit Rust 2024 unsafe region around AVX2 intrinsics,
checked SIMD output length even in release builds, and removed reported
unused locals/imports where safe to do so.

## Verify in your Mata workspace

Run these from the **parent Mata workspace root**:

```powershell
cargo check -p mata-audio
cargo check -p mata-audio --no-default-features --features 'auto-detect,simd'
cargo test -p mata-audio --no-default-features --features 'auto-detect,simd' --lib
```

The code was statically validated but **not compiled** in the packaging
container (no `cargo` or `rustc` there). The first command above is the
necessary compile verification.

The notice about ignored `[profile.*]` sections is a Cargo workspace warning:
Cargo requires those profile sections to live in the parent workspace root's
`Cargo.toml`, not the `mata-audio/Cargo.toml` member manifest.
