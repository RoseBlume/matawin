
#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;
#[cfg(target_arch = "arm")]
use core::arch::arm::*;

#[target_feature(enable = "neon")]
pub(super) unsafe fn midside_neon(left: &mut [f32], right: &mut [f32], n: usize) {
    let mut i = 0usize;
    while i + 4 <= n {
        let l = unsafe { vld1q_f32(left.as_ptr().add(i)) };
        let r = unsafe { vld1q_f32(right.as_ptr().add(i)) };
        unsafe {
            vst1q_f32(left.as_mut_ptr().add(i), vaddq_f32(l, r));
            vst1q_f32(right.as_mut_ptr().add(i), vsubq_f32(l, r));
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

#[target_feature(enable = "neon")]
pub(super) unsafe fn intensity_neon(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
    kl: f32,
    kr: f32,
) {
    let klv = vdupq_n_f32(kl);
    let krv = vdupq_n_f32(kr);
    let mut i = 0usize;
    while i + 4 <= n {
        let v = unsafe { vld1q_f32(left.as_ptr().add(i)) };
        unsafe {
            vst1q_f32(left.as_mut_ptr().add(i), vmulq_f32(v, klv));
            vst1q_f32(right.as_mut_ptr().add(i), vmulq_f32(v, krv));
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

#[target_feature(enable = "neon")]
unsafe fn reverse4(v: float32x4_t) -> float32x4_t {
    let pair_reversed = vrev64q_f32(v);
    vcombine_f32(vget_high_f32(pair_reversed), vget_low_f32(pair_reversed))
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn antialias_neon(
    grbuf: &mut [f32],
    bands: usize,
    aa0: &[f32; 8],
    aa1: &[f32; 8],
) {
    for band in 0..bands {
        let base = band * 18;
        for half in 0..2usize {
            let i = half * 4;
            let u = unsafe { vld1q_f32(grbuf.as_ptr().add(base + 18 + i)) };
            let d_start = base + 14 - i;
            let d = unsafe { reverse4(vld1q_f32(grbuf.as_ptr().add(d_start))) };
            let c0 = unsafe { vld1q_f32(aa0.as_ptr().add(i)) };
            let c1 = unsafe { vld1q_f32(aa1.as_ptr().add(i)) };
            let out_u = vsubq_f32(vmulq_f32(u, c0), vmulq_f32(d, c1));
            let out_d = vaddq_f32(vmulq_f32(u, c1), vmulq_f32(d, c0));
            unsafe {
                vst1q_f32(grbuf.as_mut_ptr().add(base + 18 + i), out_u);
                vst1q_f32(
                    grbuf.as_mut_ptr().add(d_start),
                    reverse4(out_d),
                );
            }
        }
    }
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn change_sign_neon(grbuf: &mut [f32]) {
    let sign = [1.0f32, -1.0, 1.0, -1.0];
    let mask = unsafe { vld1q_f32(sign.as_ptr()) };
    for block in 0..16usize {
        let base = 18 + block * 36;
        for offset in [0usize, 4, 8, 12] {
            let v = unsafe { vld1q_f32(grbuf.as_ptr().add(base + offset)) };
            unsafe { vst1q_f32(grbuf.as_mut_ptr().add(base + offset), vmulq_f32(v, mask)) };
        }
        grbuf[base + 17] = -grbuf[base + 17];
    }
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn scale_neon(samples: &mut [f32], gain: f32) {
    let g = vdupq_n_f32(gain);
    let mut i = 0usize;
    while i + 4 <= samples.len() {
        let v = unsafe { vld1q_f32(samples.as_ptr().add(i)) };
        unsafe { vst1q_f32(samples.as_mut_ptr().add(i), vmulq_f32(v, g)); }
        i += 4;
    }
    while i < samples.len() { samples[i] *= gain; i += 1; }
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn synth_window4_neon(
    lins: &[f32],
    i: i32,
    zlin: usize,
    win_start: usize,
    win: &[f32],
) -> ([f32; 4], [f32; 4]) {
    let mut a = vdupq_n_f32(0.0);
    let mut b = vdupq_n_f32(0.0);
    for stage in 0..8i32 {
        let w0 = vdupq_n_f32(win[win_start + stage as usize * 2]);
        let w1 = vdupq_n_f32(win[win_start + stage as usize * 2 + 1]);
        let vz = (zlin as i32 + 4 * i - stage * 64) as usize;
        let vy = (zlin as i32 + 4 * i - (15 - stage) * 64) as usize;
        let zv = unsafe { vld1q_f32(lins.as_ptr().add(vz)) };
        let yv = unsafe { vld1q_f32(lins.as_ptr().add(vy)) };
        b = vaddq_f32(b, vaddq_f32(vmulq_f32(zv, w1), vmulq_f32(yv, w0)));
        let term = if stage & 1 == 0 {
            vsubq_f32(vmulq_f32(zv, w0), vmulq_f32(yv, w1))
        } else {
            vsubq_f32(vmulq_f32(yv, w1), vmulq_f32(zv, w0))
        };
        a = vaddq_f32(a, term);
    }
    let mut ao = [0.0f32; 4];
    let mut bo = [0.0f32; 4];
    unsafe {
        vst1q_f32(ao.as_mut_ptr(), a);
        vst1q_f32(bo.as_mut_ptr(), b);
    }
    (ao, bo)
}