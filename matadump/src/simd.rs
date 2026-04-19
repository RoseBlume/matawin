#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
unsafe fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    use std::arch::x86_64::*;
    unsafe {
        let v = _mm_loadu_si128(input.as_ptr() as *const __m128i);

        let low = _mm_set1_epi8(0x20 as i8);
        let high = _mm_set1_epi8(0x7e as i8);

        let mask_low = _mm_cmpgt_epi8(v, _mm_sub_epi8(low, _mm_set1_epi8(1)));
        let mask_high = _mm_cmplt_epi8(v, _mm_add_epi8(high, _mm_set1_epi8(1)));
        let mask = _mm_and_si128(mask_low, mask_high);

        let dots = _mm_set1_epi8(b'.' as i8);

        let result = _mm_or_si128(_mm_and_si128(mask, v), _mm_andnot_si128(mask, dots));

        _mm_storeu_si128(out.as_mut_ptr() as *mut __m128i, result);
    }
}

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
unsafe fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    use std::arch::aarch64::*;

    let v = vld1q_u8(input.as_ptr());

    let low = vdupq_n_u8(0x20);
    let high = vdupq_n_u8(0x7e);

    let mask_low = vcgeq_u8(v, low);
    let mask_high = vcleq_u8(v, high);
    let mask = vandq_u8(mask_low, mask_high);

    let dots = vdupq_n_u8(b'.');

    let result = vbslq_u8(mask, v, dots);

    vst1q_u8(out.as_mut_ptr(), result);
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    process_ascii_scalar(input, out);
}

#[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
unsafe fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    use std::arch::powerpc64::*;

    let v = vec_ld(0, input.as_ptr());

    let low = vec_splats(0x20u8);
    let high = vec_splats(0x7eu8);

    let mask_low = vec_cmpge(v, low);
    let mask_high = vec_cmple(v, high);
    let mask = vec_and(mask_low, mask_high);

    let dots = vec_splats(b'.');

    let result = vec_sel(dots, v, mask);

    vec_st(result, 0, out.as_mut_ptr());
}

#[cfg(target_arch = "s390x")]
fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    process_ascii_scalar(input, out); // fallback (SIMD unstable)
}

#[cfg(any(target_arch = "mips", target_arch = "mips64"))]
fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    process_ascii_scalar(input, out);
}

#[cfg(any(target_arch = "loongarch32", target_arch = "loongarch64"))]
fn process_ascii_simd(input: &[u8; 16], out: &mut [u8; 16]) {
    process_ascii_scalar(input, out);
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64", target_arch = "powerpc", target_arch = "powerpc64", target_arch = "riscv32", target_arch = "riscv64", target_arch = "s390x", target_arch = "mips", target_arch = "mips64", target_arch = "loongarch32", target_arch = "loongarch64")))]
fn process_ascii_scalar(input: &[u8; 16], out: &mut [u8; 16]) {
    for i in 0..16 {
        let b = input[i];
        out[i] = if b >= 0x20 && b <= 0x7e { b } else { b'.' };
    }
}

#[cfg(any(target_arch = "nvptx64", target_arch = "amdgpu"))]
compile_error!("GPU targets are not supported for this program.");

pub fn process_ascii(input: &[u8; 16], out: &mut [u8; 16]) {
    unsafe {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        process_ascii_simd(input, out);

        #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
        process_ascii_simd(input, out);

        #[cfg(any(target_arch = "powerpc", target_arch = "powerpc64"))]
        process_ascii_simd(input, out);
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64", target_arch = "powerpc", target_arch = "powerpc64")))]
        process_ascii_scalar(input, out);
    }
}