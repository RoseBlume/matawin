use crate::{AudioError, codecs::support::AlignedScratch};

/// Reusable FFT-backed Vorbis IMDCT plan.
///
/// A DCT-IV of `m = N/2` real values can be obtained from the odd bins of an
/// `8m`-point DFT whose only non-zero time samples are the input values at odd
/// indices. Since `8m == 4N`, a radix-2 FFT turns the former O(N²) transform
/// into O(N log N) while preserving the compact rearrangement used by Vorbis.
#[derive(Debug, Clone)]
pub(super) struct ImdctPlan {
    block_size: usize,
    fft_len: usize,
    bit_reverse: Vec<usize>,
    twiddle_re: Vec<f32>,
    twiddle_im: Vec<f32>,
    real: AlignedScratch<f32>,
    imag: AlignedScratch<f32>,
    lane_ar: AlignedScratch<f32>,
    lane_ai: AlignedScratch<f32>,
    lane_br: AlignedScratch<f32>,
    lane_bi: AlignedScratch<f32>,
}

impl ImdctPlan {
    pub fn new(block_size: usize) -> Result<Self, AudioError> {
        if block_size < 64 || !block_size.is_power_of_two() {
            return Err(AudioError::InvalidFormat("Invalid Vorbis IMDCT block size".into()));
        }
        let m = block_size / 2;
        let fft_len = m.checked_mul(8)
            .ok_or_else(|| AudioError::InvalidFormat("Vorbis IMDCT FFT size overflow".into()))?;
        let bits = fft_len.trailing_zeros();
        let mut bit_reverse = Vec::with_capacity(fft_len);
        for i in 0..fft_len {
            bit_reverse.push(i.reverse_bits() >> (usize::BITS - bits));
        }

        let mut twiddle_re = Vec::with_capacity(fft_len / 2);
        let mut twiddle_im = Vec::with_capacity(fft_len / 2);
        for k in 0..fft_len / 2 {
            let angle = -2.0f64 * std::f64::consts::PI * k as f64 / fft_len as f64;
            twiddle_re.push(angle.cos() as f32);
            twiddle_im.push(angle.sin() as f32);
        }

        let lanes = fft_len / 2;
        Ok(Self {
            block_size,
            fft_len,
            bit_reverse,
            twiddle_re,
            twiddle_im,
            real: AlignedScratch::with_len(fft_len, 0.0)?,
            imag: AlignedScratch::with_len(fft_len, 0.0)?,
            lane_ar: AlignedScratch::with_len(lanes, 0.0)?,
            lane_ai: AlignedScratch::with_len(lanes, 0.0)?,
            lane_br: AlignedScratch::with_len(lanes, 0.0)?,
            lane_bi: AlignedScratch::with_len(lanes, 0.0)?,
        })
    }

    #[inline]
    pub fn block_size(&self) -> usize { self.block_size }

    pub(super) fn scratch_bytes(&self) -> usize {
        self.real.allocated_bytes().saturating_add(self.imag.allocated_bytes())
            .saturating_add(self.lane_ar.allocated_bytes()).saturating_add(self.lane_ai.allocated_bytes())
            .saturating_add(self.lane_br.allocated_bytes()).saturating_add(self.lane_bi.allocated_bytes())
    }
    pub(super) fn peak_scratch_bytes(&self) -> usize {
        self.real.peak_allocated_bytes().saturating_add(self.imag.peak_allocated_bytes())
            .saturating_add(self.lane_ar.peak_allocated_bytes()).saturating_add(self.lane_ai.peak_allocated_bytes())
            .saturating_add(self.lane_br.peak_allocated_bytes()).saturating_add(self.lane_bi.peak_allocated_bytes())
    }

    /// Execute the plan into retained caller storage.
    pub fn inverse_into(&mut self, spectrum: &[f32], output: &mut Vec<f32>) -> Result<(), AudioError> {
        if spectrum.len() != self.block_size / 2 {
            return Err(AudioError::InvalidFormat("Vorbis IMDCT spectrum length mismatch".into()));
        }

        self.real.fill(0.0);
        self.imag.fill(0.0);
        for (n, &sample) in spectrum.iter().enumerate() {
            self.real[2 * n + 1] = sample;
        }

        // Precomputed bit-reversal order, paid once during setup.
        for i in 0..self.fft_len {
            let j = self.bit_reverse[i];
            if j > i {
                self.real.swap(i, j);
                self.imag.swap(i, j);
            }
        }

        let mut len = 2usize;
        while len <= self.fft_len {
            let half = len / 2;
            let groups = self.fft_len / len;
            let twiddle_stride = self.fft_len / len;

            // Group equal-twiddle butterflies together. This makes the four
            // temporary arrays contiguous and lets the root SIMD dispatcher
            // use AVX2/NEON across independent FFT groups.
            const GROUP_BLOCK: usize = 256;
            for j in 0..half {
                let tw = j * twiddle_stride;
                let mut group_base = 0usize;
                while group_base < groups {
                    let block_groups = (groups - group_base).min(GROUP_BLOCK);
                    for local in 0..block_groups {
                        let group = group_base + local;
                        let a = group * len + j;
                        let b = a + half;
                        self.lane_ar[local] = self.real[a];
                        self.lane_ai[local] = self.imag[a];
                        self.lane_br[local] = self.real[b];
                        self.lane_bi[local] = self.imag[b];
                    }
                    crate::simd::complex_fft_butterfly(
                        &mut self.lane_ar[..block_groups],
                        &mut self.lane_ai[..block_groups],
                        &mut self.lane_br[..block_groups],
                        &mut self.lane_bi[..block_groups],
                        self.twiddle_re[tw],
                        self.twiddle_im[tw],
                    );
                    for local in 0..block_groups {
                        let group = group_base + local;
                        let a = group * len + j;
                        let b = a + half;
                        self.real[a] = self.lane_ar[local];
                        self.imag[a] = self.lane_ai[local];
                        self.real[b] = self.lane_br[local];
                        self.imag[b] = self.lane_bi[local];
                    }
                    group_base += block_groups;
                }
            }
            len <<= 1;
        }

        output.resize(self.block_size, 0.0);
        let n4 = self.block_size / 4;
        let n3_4 = self.block_size - n4;

        // DCT-IV coefficient k is the real component of odd FFT bin 2k+1.
        #[inline]
        fn dct(real: &[f32], k: usize) -> f32 { real[2 * k + 1] }

        for i in 0..n4 {
            output[i] = dct(&self.real, i + n4);
        }
        for i in n4..n3_4 {
            output[i] = -dct(&self.real, n3_4 - i - 1);
        }
        for i in n3_4..self.block_size {
            output[i] = -dct(&self.real, i - n3_4);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ImdctPlan;

    fn reference(spectrum: &[f32], block_size: usize) -> Vec<f32> {
        let m = block_size / 2;
        let mut dct = vec![0.0f32; m];
        let factor = std::f64::consts::PI / m as f64;
        for i in 0..m {
            let mut sum = 0.0f64;
            for (n, &sample) in spectrum.iter().enumerate() {
                sum += sample as f64 * (factor * (n as f64 + 0.5) * (i as f64 + 0.5)).cos();
            }
            dct[i] = sum as f32;
        }
        let n4 = block_size / 4;
        let n3_4 = block_size - n4;
        let mut out = vec![0.0; block_size];
        for i in 0..n4 { out[i] = dct[i + n4]; }
        for i in n4..n3_4 { out[i] = -dct[n3_4 - i - 1]; }
        for i in n3_4..block_size { out[i] = -dct[i - n3_4]; }
        out
    }

    #[test]
    fn fft_imdct_matches_reference() {
        for n in [64usize, 128, 256] {
            let spectrum: Vec<f32> = (0..n / 2).map(|i| ((i * 17 + 3) as f32).sin() * 0.125).collect();
            let expected = reference(&spectrum, n);
            let mut actual = Vec::new();
            ImdctPlan::new(n).unwrap().inverse_into(&spectrum, &mut actual).unwrap();
            for (a, b) in actual.iter().zip(expected.iter()) {
                assert!((a - b).abs() <= 2.0e-3 * (1.0 + b.abs()), "{a} != {b}");
            }
        }
    }
}
