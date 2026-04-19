#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
mod neon;

#[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
mod riscv;
/// Convert 4 pixels (16 bytes) from BGRA → RGBA
/// Returns `true` if SIMD path was used, `false` if fallback is needed.
#[inline]
pub unsafe fn bgra_to_rgba_16(src: *const u8, dst: *mut u8) -> bool {
    // x86 / x86_64 (SSSE3)
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("ssse3") {
            return unsafe { x86::bgra_to_rgba_16(src, dst) };
        }
    }

    // aarch64 (always has NEON)
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { neon::bgra_to_rgba_16(src, dst) };
    }

    // 32-bit ARM (optional NEON)
    #[cfg(target_arch = "arm")]
    {
        #[cfg(target_feature = "neon")]
        {
            return unsafe { neon::bgra_to_rgba_16(src, dst) };
        }
    }

    // RISC-V (vector extension if available)
    #[cfg(any(target_arch = "riscv64", target_arch = "riscv32"))]
    {
        #[cfg(target_feature = "v")]
        {
            return unsafe { riscv::bgra_to_rgba_16(src, dst) };
        }
    }

    false
}

