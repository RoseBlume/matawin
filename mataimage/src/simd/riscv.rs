use core::arch::riscv::*;
#[inline]
pub unsafe fn bgra_to_rgba_16(src: *const u8, dst: *mut u8) -> bool {

    // -------- Nightly RVV path --------
    #[cfg(all(feature = "simd", nightly, target_feature = "v"))]
    {
        

        // Load 16 bytes into vector (u8)
        let vl = vsetvl_e8m1(16);

        let v = vle8_v_u8m1(src, vl);

        // Create index vector for shuffle
        // BGRA → RGBA
        let idx = [
            2, 1, 0, 3,
            6, 5, 4, 7,
            10, 9, 8, 11,
            14, 13, 12, 15,
        ];

        let vidx = vle8_v_u8m1(idx.as_ptr(), vl);

        // Gather (shuffle)
        let shuffled = vrgather_vv_u8m1(v, vidx, vl);

        vse8_v_u8m1(dst, shuffled, vl);

        return true;
    }

    // -------- Stable fallback --------
    for i in 0..4 {
        let s = src.add(i * 4);
        let d = dst.add(i * 4);

        let b = *s;
        let g = *s.add(1);
        let r = *s.add(2);
        let a = *s.add(3);

        *d = r;
        *d.add(1) = g;
        *d.add(2) = b;
        *d.add(3) = a;
    }

    true
}