

//! # rand_asm
//!
//! A minimal cross-platform random number generator using:
//! - Inline assembly where available
//! - OS-specific entropy sources
//! - A fast xorshift64* PRNG core
//!
//! ## Features
//! - Works on x86, x86_64, and AArch64
//! - Supports Linux, macOS (Darwin), Windows, and fallback OSes
//! - No external dependencies
//!
//! ## Example
//! ```
//! let mut rng = rand_asm::Rand::new();
//!
//! let i = rng.randint(1, 10);
//! let f = rng.randfloat(0.0, 1.0);
//! ```
//!
//! ## ⚠️ Warning
//! This is NOT cryptographically secure.

use core::arch::asm;

/// A simple random number generator.
///
/// Internally uses:
/// - `u64` state
/// - xorshift64* algorithm
///
/// Seed is derived from:
/// - CPU entropy (assembly)
/// - OS-specific timing sources
pub struct Rand {
    /// Internal PRNG state
    seed: u64,
}

impl Rand {
    /// Creates a new `Rand` instance using system entropy.
    ///
    /// This combines:
    /// - CPU-based entropy (e.g., `rdtsc`, `rdrand`, ARM counters)
    /// - OS-based entropy (system time APIs)
    ///
    /// Falls back to a constant if all sources fail.
    pub fn new() -> Self {
        Self { seed: seed() }
    }

    /// Generates the next raw random `u64`.
    ///
    /// Uses the **xorshift64\*** algorithm:
    /// - Fast
    /// - Low overhead
    /// - Not cryptographically secure
    pub fn next(&mut self) -> u64 {
        let mut x = self.seed;

        // Xorshift operations
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;

        self.seed = x;

        // Final mixing multiplication
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Generates a random integer in the range `[start, end)`.
    ///
    /// # Arguments
    /// - `start`: inclusive lower bound
    /// - `end`: exclusive upper bound
    ///
    /// # Panics
    /// Panics if `start >= end`
    ///
    /// # Notes
    /// - Uses modulo reduction (may introduce slight bias)
    /// - Suitable for general use, not statistical precision
    pub fn randint(&mut self, start: i64, end: i64) -> i64 {
        assert!(start < end, "start must be less than end");

        let range = (end - start) as u64;
        let value = self.next() % range;

        start + value as i64
    }

    /// Generates a random floating-point number in `[start, end)`.
    ///
    /// # Arguments
    /// - `start`: lower bound
    /// - `end`: upper bound
    ///
    /// # Panics
    /// Panics if `start >= end`
    ///
    /// # Details
    /// - Converts a `u64` into a normalized `[0, 1)` float
    /// - Scales to desired range
    pub fn randfloat(&mut self, start: f64, end: f64) -> f64 {
        assert!(start < end, "start must be less than end");

        let normalized = self.next() as f64 / u64::MAX as f64;
        start + (end - start) * normalized
    }
}

//
// =========================
// Seeding Implementation
// =========================
//

/// Generates a seed using all available entropy sources.
///
/// Combines:
/// - Architecture-specific entropy
/// - OS-specific entropy
///
/// Ensures a non-zero seed.
fn seed() -> u64 {
    let mut s = 0u64;

    s ^= arch_entropy();
    s ^= os_entropy();

    if s == 0 {
        0x9E3779B97F4A7C15
    } else {
        s
    }
}

//
// =========================
// Architecture Entropy
// =========================
//

/// Architecture-specific entropy source.
///
/// Uses:
/// - x86/x86_64: `rdrand` + `rdtsc`
/// - AArch64: `cntvct_el0`
/// - fallback: 0
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn arch_entropy() -> u64 {
    let mut out = 0;

    if let Some(r) = rdrand() {
        out ^= r;
    }

    out ^ rdtsc()
}

/// Attempts to use the `RDRAND` CPU instruction.
///
/// Returns `None` if unsupported or failed.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn rdrand() -> Option<u64> {
    let mut val: u64;
    let mut ok: u8;

    unsafe {
        asm!(
            "rdrand {val}",
            "setc {ok}",
            val = out(reg) val,
            ok = out(reg_byte) ok,
        );
    }

    if ok == 1 {
        Some(val)
    } else {
        None
    }
}

/// Reads the CPU timestamp counter (`RDTSC`).
///
/// Provides high-resolution timing entropy.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn rdtsc() -> u64 {
    let hi: u32;
    let lo: u32;

    unsafe {
        asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
        );
    }

    ((hi as u64) << 32) | lo as u64
}

/// AArch64 entropy using virtual counter register.
#[cfg(target_arch = "aarch64")]
fn arch_entropy() -> u64 {
    let value: u64;

    unsafe {
        asm!(
            "mrs {0}, cntvct_el0",
            out(reg) value,
        );
    }

    value
}

/// Fallback for unsupported architectures.
#[cfg(not(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "aarch64"
)))]
fn arch_entropy() -> u64 {
    0
}

//
// =========================
// OS Entropy
// =========================
//

/// Linux / Android entropy via `clock_gettime` syscall.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn os_entropy() -> u64 {
    let mut ts: [u64; 2] = [0; 2];

    unsafe {
        asm!(
            "mov rax, 228", // clock_gettime
            "mov rdi, 0",   // CLOCK_REALTIME
            "syscall",
            in("rsi") ts.as_mut_ptr(),
            out("rax") _,
        );
    }

    ts[0] ^ ts[1]
}

/// macOS / iOS entropy using `mach_absolute_time`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn os_entropy() -> u64 {
    unsafe extern "C" {
        fn mach_absolute_time() -> u64;
    }

    unsafe { mach_absolute_time() }
}

/// Windows entropy using `QueryPerformanceCounter`.
#[cfg(target_os = "windows")]
fn os_entropy() -> u64 {
    unsafe extern "system" {
        fn QueryPerformanceCounter(lp: *mut i64) -> i32;
    }

    let mut val: i64 = 0;

    unsafe {
        QueryPerformanceCounter(&mut val);
    }

    val as u64
}

/// Fallback entropy using system time.
///
/// Used for BSD and unknown platforms.
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "windows"
)))]
fn os_entropy() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}