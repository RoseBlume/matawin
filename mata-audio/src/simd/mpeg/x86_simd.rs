#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[target_feature(enable = "avx")]
pub(super) unsafe fn midside_avx(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
) {
    let mut i = 0usize;

    while i + 8 <= n {
        unsafe {
            let l = _mm256_loadu_ps(left.as_ptr().add(i));
            let r = _mm256_loadu_ps(right.as_ptr().add(i));
            _mm256_storeu_ps(left.as_mut_ptr().add(i), _mm256_add_ps(l, r));
            _mm256_storeu_ps(right.as_mut_ptr().add(i), _mm256_sub_ps(l, r));
        }
        i += 8;
    }

    while i < n {
        let a = left[i];
        let b = right[i];
        left[i] = a + b;
        right[i] = a - b;
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
pub(super) unsafe fn midside_sse2(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
) {
    let mut i = 0usize;

    while i + 4 <= n {
        unsafe {
            let l = _mm_loadu_ps(left.as_ptr().add(i));
            let r = _mm_loadu_ps(right.as_ptr().add(i));
            _mm_storeu_ps(left.as_mut_ptr().add(i), _mm_add_ps(l, r));
            _mm_storeu_ps(right.as_mut_ptr().add(i), _mm_sub_ps(l, r));
        }
        i += 4;
    }

    while i < n {
        let a = left[i];
        let b = right[i];
        left[i] = a + b;
        right[i] = a - b;
        i += 1;
    }
}

#[target_feature(enable = "avx")]
pub(super) unsafe fn intensity_avx(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
    kl: f32,
    kr: f32,
) {
    let klv = _mm256_set1_ps(kl);
    let krv = _mm256_set1_ps(kr);
    let mut i = 0usize;

    while i + 8 <= n {
        unsafe {
            let v = _mm256_loadu_ps(left.as_ptr().add(i));
            _mm256_storeu_ps(left.as_mut_ptr().add(i), _mm256_mul_ps(v, klv));
            _mm256_storeu_ps(right.as_mut_ptr().add(i), _mm256_mul_ps(v, krv));
        }
        i += 8;
    }

    while i < n {
        let value = left[i];
        left[i] = value * kl;
        right[i] = value * kr;
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
pub(super) unsafe fn intensity_sse2(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
    kl: f32,
    kr: f32,
) {
    let klv = _mm_set1_ps(kl);
    let krv = _mm_set1_ps(kr);
    let mut i = 0usize;

    while i + 4 <= n {
        unsafe {
            let v = _mm_loadu_ps(left.as_ptr().add(i));
            _mm_storeu_ps(left.as_mut_ptr().add(i), _mm_mul_ps(v, klv));
            _mm_storeu_ps(right.as_mut_ptr().add(i), _mm_mul_ps(v, krv));
        }
        i += 4;
    }

    while i < n {
        let value = left[i];
        left[i] = value * kl;
        right[i] = value * kr;
        i += 1;
    }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn antialias_avx2(
    grbuf: &mut [f32],
    bands: usize,
    aa0: &[f32; 8],
    aa1: &[f32; 8],
) {
    let reverse = _mm256_setr_epi32(7, 6, 5, 4, 3, 2, 1, 0);
    let c0 = unsafe { _mm256_loadu_ps(aa0.as_ptr()) };
    let c1 = unsafe { _mm256_loadu_ps(aa1.as_ptr()) };

    for band in 0..bands {
        let base = band * 18;

        unsafe {
            // u[i] = grbuf[18 + i]
            let u = _mm256_loadu_ps(grbuf.as_ptr().add(base + 18));

            // grbuf[10..18] is the reverse of d[i] = grbuf[17 - i].
            let d_ascending = _mm256_loadu_ps(grbuf.as_ptr().add(base + 10));
            let d = _mm256_permutevar8x32_ps(d_ascending, reverse);

            let out_u = _mm256_sub_ps(
                _mm256_mul_ps(u, c0),
                _mm256_mul_ps(d, c1),
            );

            let out_d = _mm256_add_ps(
                _mm256_mul_ps(u, c1),
                _mm256_mul_ps(d, c0),
            );

            _mm256_storeu_ps(grbuf.as_mut_ptr().add(base + 18), out_u);

            // out_d is ordered as destinations 17..10; reverse back so a
            // contiguous store writes destinations 10..17.
            let out_d_ascending = _mm256_permutevar8x32_ps(out_d, reverse);
            _mm256_storeu_ps(grbuf.as_mut_ptr().add(base + 10), out_d_ascending);
        }
    }
}

#[target_feature(enable = "avx")]
pub(super) unsafe fn change_sign_avx(grbuf: &mut [f32]) {
    let sign_mask = _mm256_setr_ps(
        0.0, -0.0, 0.0, -0.0,
        0.0, -0.0, 0.0, -0.0,
    );

    for block in 0..16usize {
        let base = 18 + block * 36;

        unsafe {
            let lo = _mm256_loadu_ps(grbuf.as_ptr().add(base));
            let hi = _mm256_loadu_ps(grbuf.as_ptr().add(base + 8));

            _mm256_storeu_ps(
                grbuf.as_mut_ptr().add(base),
                _mm256_xor_ps(lo, sign_mask),
            );

            _mm256_storeu_ps(
                grbuf.as_mut_ptr().add(base + 8),
                _mm256_xor_ps(hi, sign_mask),
            );
        }

        grbuf[base + 17] = -grbuf[base + 17];
    }
}

#[target_feature(enable = "sse2")]
pub(super) unsafe fn change_sign_sse2(grbuf: &mut [f32]) {
    let sign_mask = _mm_setr_ps(0.0, -0.0, 0.0, -0.0);

    for block in 0..16usize {
        let base = 18 + block * 36;

        for offset in [0usize, 4, 8, 12] {
            unsafe {
                let value = _mm_loadu_ps(grbuf.as_ptr().add(base + offset));
                _mm_storeu_ps(
                    grbuf.as_mut_ptr().add(base + offset),
                    _mm_xor_ps(value, sign_mask),
                );
            }
        }

        grbuf[base + 17] = -grbuf[base + 17];
    }
}

#[target_feature(enable = "avx")]
pub(super) unsafe fn scale_avx(samples: &mut [f32], gain: f32) {
    let g = _mm256_set1_ps(gain);
    let mut i = 0usize;
    while i + 8 <= samples.len() {
        unsafe {
            let v = _mm256_loadu_ps(samples.as_ptr().add(i));
            _mm256_storeu_ps(samples.as_mut_ptr().add(i), _mm256_mul_ps(v, g));
        }
        i += 8;
    }
    while i < samples.len() { samples[i] *= gain; i += 1; }
}

#[target_feature(enable = "sse2")]
pub(super) unsafe fn synth_window4_sse2(
    lins: &[f32],
    i: i32,
    zlin: usize,
    win_start: usize,
    win: &[f32],
) -> ([f32; 4], [f32; 4]) {
    let mut a = _mm_setzero_ps();
    let mut b = _mm_setzero_ps();

    for stage in 0..8i32 {
        let w0 = _mm_set1_ps(win[win_start + stage as usize * 2]);
        let w1 = _mm_set1_ps(win[win_start + stage as usize * 2 + 1]);

        let vz = (zlin as i32 + 4 * i - stage * 64) as usize;
        let vy = (zlin as i32 + 4 * i - (15 - stage) * 64) as usize;

        unsafe {
            let zv = _mm_loadu_ps(lins.as_ptr().add(vz));
            let yv = _mm_loadu_ps(lins.as_ptr().add(vy));

            b = _mm_add_ps(
                b,
                _mm_add_ps(
                    _mm_mul_ps(zv, w1),
                    _mm_mul_ps(yv, w0),
                ),
            );

            let term = if stage & 1 == 0 {
                _mm_sub_ps(
                    _mm_mul_ps(zv, w0),
                    _mm_mul_ps(yv, w1),
                )
            } else {
                _mm_sub_ps(
                    _mm_mul_ps(yv, w1),
                    _mm_mul_ps(zv, w0),
                )
            };

            a = _mm_add_ps(a, term);
        }
    }

    let mut a_out = [0.0f32; 4];
    let mut b_out = [0.0f32; 4];

    unsafe {
        _mm_storeu_ps(a_out.as_mut_ptr(), a);
        _mm_storeu_ps(b_out.as_mut_ptr(), b);
    }

    (a_out, b_out)
}
