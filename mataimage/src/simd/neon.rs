#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

#[cfg(target_arch = "arm")]
use core::arch::arm::*;

#[inline]
pub unsafe fn bgra_to_rgba_16(src: *const u8, dst: *mut u8) -> bool {
    // Load 16 bytes
    let v = vld1q_u8(src);

    // Table lookup for byte shuffle
    let table = vld1q_u8([
        2, 1, 0, 3,
        6, 5, 4, 7,
        10, 9, 8, 11,
        14, 13, 12, 15,
    ].as_ptr());

    let res = vqtbl1q_u8(v, table);

    vst1q_u8(dst, res);

    true
}