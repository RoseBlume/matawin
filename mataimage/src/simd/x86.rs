use core::arch::x86_64::*;

#[inline]
pub unsafe fn bgra_to_rgba_16(src: *const u8, dst: *mut u8) -> bool {
    let v = unsafe { _mm_loadu_si128(src as *const __m128i) };

    // BGRA → RGBA shuffle mask
    let shuffle = unsafe { _mm_set_epi8(
        14, 13, 12, 15,
        10, 9, 8, 11,
        6, 5, 4, 7,
        2, 1, 0, 3,
    ) };

    let res = unsafe { _mm_shuffle_epi8(v, shuffle) };
    unsafe { _mm_storeu_si128(dst as *mut __m128i, res) };

    true
}