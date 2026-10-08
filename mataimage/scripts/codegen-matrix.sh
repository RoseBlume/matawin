#!/usr/bin/env bash
set -euo pipefail

# Compare release code-generation choices on the actual deployment CPU/compiler.
# No machine-specific winner is baked into the library API.
for lto in off thin fat; do
  for units in 1 4 16; do
    echo "=== lto=$lto codegen-units=$units ==="
    case "$lto" in
      off)  CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$units" cargo bench --bench decode ;;
      thin) CARGO_PROFILE_RELEASE_LTO=thin  CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$units" cargo bench --bench decode ;;
      fat)  CARGO_PROFILE_RELEASE_LTO=fat   CARGO_PROFILE_RELEASE_CODEGEN_UNITS="$units" cargo bench --bench decode ;;
    esac
  done
done

# Application-only experiment. mataimage itself deliberately does not force this.
echo "=== thin-lto codegen-units=1 panic=abort (application policy experiment) ==="
CARGO_PROFILE_RELEASE_LTO=thin \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
CARGO_PROFILE_RELEASE_PANIC=abort \
  cargo bench --bench decode
