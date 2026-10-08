#!/usr/bin/env sh
set -eu
# Linux hardware-counter pass for the P1 suite. The benchmark itself prints
# decoded megapixels so perf output can be normalized to cycles/instructions per
# pixel when comparing revisions.
exec perf stat \
  -e cycles,instructions,branches,branch-misses,L1-dcache-loads,L1-dcache-load-misses,LLC-loads,LLC-load-misses,cache-references,cache-misses \
  cargo bench --bench decode "$@"
