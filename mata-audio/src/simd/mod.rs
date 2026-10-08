//! Runtime-dispatched audio SIMD kernels with scalar fallbacks.
//!
//! Dispatch is selected once and cached in a `OnceLock`. All public(crate)
//! entry points are safe wrappers around ISA-specific functions and handle
//! unaligned buffers and scalar tails.

use std::sync::OnceLock;
use crate::AudioError;

#[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
mod x86;
#[cfg(all(feature = "simd", any(target_arch = "arm", target_arch = "aarch64")))]
mod neon;

#[cfg(all(
    feature = "simd",
    any(feature = "codec-mp1", feature = "codec-mp2", feature = "codec-mp3")
))]
pub(crate) mod mpeg;

type PcmU8Fn = unsafe fn(&[u8], &mut [f32]);
type PcmI16Fn = unsafe fn(&[u8], &mut [f32], u32, f32);
type PcmI24Fn = unsafe fn(&[u8], &mut [f32], u16);
type PcmI32Fn = unsafe fn(&[u8], &mut [f32], u16);
type PcmF32Fn = unsafe fn(&[u8], &mut [f32]);
type PcmF64Fn = unsafe fn(&[u8], &mut [f32]);
type MulFn = unsafe fn(&mut [f32], &[f32]);
type MixAddFn = unsafe fn(&mut [f32], &[f32], f32);
type OverlapFn = unsafe fn(&mut [f32], &[f32], &[f32]);
type StereoInterleaveFn = unsafe fn(&[f32], &[f32], &mut [f32]);
type StereoDeinterleaveFn = unsafe fn(&[f32], &mut [f32], &mut [f32]);
type LerpFn = unsafe fn(&[f32], &[f32], f32, &mut [f32]);
type ButterflyFn = unsafe fn(&mut [f32], &mut [f32], f32);
type UnaryFn = unsafe fn(&mut [f32]);
type StereoAverageFn = unsafe fn(&[f32], &mut [f32]);
type MonoDuplicateFn = unsafe fn(&[f32], &mut [f32]);
type FlacLeftSideFn = unsafe fn(&[i64], &mut [i64]);
type FlacRightSideFn = unsafe fn(&mut [i64], &[i64]);
type ComplexButterflyFn = unsafe fn(&mut [f32], &mut [f32], &mut [f32], &mut [f32], f32, f32);
type VorbisCoupleFn = unsafe fn(&mut [f32], &mut [f32]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeIsa {
    Scalar,
    Sse2,
    Ssse3,
    Sse41,
    Avx2,
    Avx2Fma,
    Neon,
}

#[derive(Clone, Copy)]
pub(super) struct AudioSimd {
    pub(super) isa: RuntimeIsa,
    pub(super) pcm_u8: PcmU8Fn,
    pub(super) pcm_i16_le: PcmI16Fn,
    pub(super) pcm_i16_be: PcmI16Fn,
    pub(super) pcm_i24_le: PcmI24Fn,
    pub(super) pcm_i24_be: PcmI24Fn,
    pub(super) pcm_i32_le: PcmI32Fn,
    pub(super) pcm_i32_be: PcmI32Fn,
    pub(super) pcm_f32_le: PcmF32Fn,
    pub(super) pcm_f32_be: PcmF32Fn,
    pub(super) pcm_f64_le: PcmF64Fn,
    pub(super) pcm_f64_be: PcmF64Fn,
    pub(super) mul: MulFn,
    pub(super) mix_add: MixAddFn,
    pub(super) overlap: OverlapFn,
    pub(super) interleave_stereo: StereoInterleaveFn,
    pub(super) deinterleave_stereo: StereoDeinterleaveFn,
    pub(super) lerp: LerpFn,
    pub(super) butterfly: ButterflyFn,
    pub(super) gain: unsafe fn(&mut [f32], f32),
    pub(super) limiter: UnaryFn,
    pub(super) stereo_average: StereoAverageFn,
    pub(super) mono_duplicate: MonoDuplicateFn,
    pub(super) flac_left_side: FlacLeftSideFn,
    pub(super) flac_right_side: FlacRightSideFn,
    pub(super) complex_butterfly: ComplexButterflyFn,
    pub(super) vorbis_couple: VorbisCoupleFn,
}

static DISPATCH: OnceLock<AudioSimd> = OnceLock::new();

#[inline]
fn dispatch() -> AudioSimd { *DISPATCH.get_or_init(select) }

pub fn runtime_isa() -> RuntimeIsa { dispatch().isa }

/// Optional L1 read-prefetch used only by profiling-selected hot tables. The
/// feature is deliberately off by default; normal portable builds rely on the
/// hardware prefetcher.
#[inline]
pub(crate) fn prefetch_read<T>(ptr: *const T) {
    #[cfg(all(feature = "prefetch", any(target_arch = "x86", target_arch = "x86_64")))]
    unsafe {
        #[cfg(target_arch = "x86")] use core::arch::x86::{_mm_prefetch, _MM_HINT_T0};
        #[cfg(target_arch = "x86_64")] use core::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
        _mm_prefetch(ptr.cast::<i8>(), _MM_HINT_T0);
    }
    #[cfg(all(feature = "prefetch", target_arch = "aarch64"))]
    unsafe { core::arch::asm!("prfm pldl1keep, [{addr}]", addr = in(reg) ptr, options(readonly, nostack, preserves_flags)); }
    #[cfg(not(all(feature = "prefetch", any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
    { let _ = ptr; }
}

fn scalar_dispatch() -> AudioSimd {
    AudioSimd {
        isa: RuntimeIsa::Scalar,
        pcm_u8: scalar_pcm_u8,
        pcm_i16_le: scalar_pcm_i16_le,
        pcm_i16_be: scalar_pcm_i16_be,
        pcm_i24_le: scalar_pcm_i24_le,
        pcm_i24_be: scalar_pcm_i24_be,
        pcm_i32_le: scalar_pcm_i32_le,
        pcm_i32_be: scalar_pcm_i32_be,
        pcm_f32_le: scalar_pcm_f32_le,
        pcm_f32_be: scalar_pcm_f32_be,
        pcm_f64_le: scalar_pcm_f64_le,
        pcm_f64_be: scalar_pcm_f64_be,
        mul: scalar_mul,
        mix_add: scalar_mix_add,
        overlap: scalar_overlap,
        interleave_stereo: scalar_interleave_stereo,
        deinterleave_stereo: scalar_deinterleave_stereo,
        lerp: scalar_lerp,
        butterfly: scalar_butterfly,
        gain: scalar_gain,
        limiter: scalar_limiter,
        stereo_average: scalar_stereo_average,
        mono_duplicate: scalar_mono_duplicate,
        flac_left_side: scalar_flac_left_side,
        flac_right_side: scalar_flac_right_side,
        complex_butterfly: scalar_complex_butterfly,
        vorbis_couple: scalar_vorbis_couple,
    }
}

fn select() -> AudioSimd {
    #[cfg(all(feature = "simd", any(target_arch = "x86", target_arch = "x86_64")))]
    { return x86::detect(); }
    #[cfg(all(feature = "simd", any(target_arch = "arm", target_arch = "aarch64")))]
    { return neon::detect(); }
    #[allow(unreachable_code)]
    scalar_dispatch()
}

#[inline]
pub(crate) fn pcm_u8_to_f32_into(data: &[u8], out: &mut [f32]) {
    debug_assert_eq!(data.len(), out.len());
    unsafe { (dispatch().pcm_u8)(data, out) }
}

pub(crate) fn pcm_u8_to_f32(data: &[u8]) -> Vec<f32> {
    let mut out = vec![0.0; data.len()]; pcm_u8_to_f32_into(data, &mut out); out
}

#[inline]
pub(crate) fn pcm_i16_le_to_f32_into(data: &[u8], out: &mut [f32], shift: u32, scale: f32) {
    debug_assert_eq!(data.len() / 2, out.len());
    unsafe { (dispatch().pcm_i16_le)(data, out, shift, scale) }
}
#[inline]
pub(crate) fn pcm_i16_be_to_f32_into(data: &[u8], out: &mut [f32], shift: u32, scale: f32) {
    debug_assert_eq!(data.len() / 2, out.len());
    unsafe { (dispatch().pcm_i16_be)(data, out, shift, scale) }
}
pub(crate) fn pcm_i16_le_to_f32(data: &[u8], shift: u32, scale: f32) -> Vec<f32> {
    let mut out=vec![0.0; data.len()/2]; pcm_i16_le_to_f32_into(data,&mut out,shift,scale); out
}
pub(crate) fn pcm_i16_be_to_f32(data: &[u8], shift: u32, scale: f32) -> Vec<f32> {
    let mut out=vec![0.0; data.len()/2]; pcm_i16_be_to_f32_into(data,&mut out,shift,scale); out
}

#[inline]
pub(crate) fn pcm_i24_le_to_f32_into(data: &[u8], bits: u16, out: &mut [f32]) -> Result<(), AudioError> {
    if !(17..=24).contains(&bits) { return Err(AudioError::UnsupportedBitDepth(bits)); }
    debug_assert_eq!(data.len()/3, out.len()); unsafe { (dispatch().pcm_i24_le)(data,out,bits) }; Ok(())
}
#[inline]
pub(crate) fn pcm_i24_be_to_f32_into(data: &[u8], bits: u16, out: &mut [f32]) -> Result<(), AudioError> {
    if !(17..=24).contains(&bits) { return Err(AudioError::UnsupportedBitDepth(bits)); }
    debug_assert_eq!(data.len()/3, out.len()); unsafe { (dispatch().pcm_i24_be)(data,out,bits) }; Ok(())
}
#[inline]
pub(crate) fn pcm_i32_le_to_f32_into(data: &[u8], bits: u16, out: &mut [f32]) -> Result<(), AudioError> {
    if !(25..=32).contains(&bits) { return Err(AudioError::UnsupportedBitDepth(bits)); }
    debug_assert_eq!(data.len()/4, out.len()); unsafe { (dispatch().pcm_i32_le)(data,out,bits) }; Ok(())
}
#[inline]
pub(crate) fn pcm_i32_be_to_f32_into(data: &[u8], bits: u16, out: &mut [f32]) -> Result<(), AudioError> {
    if !(25..=32).contains(&bits) { return Err(AudioError::UnsupportedBitDepth(bits)); }
    debug_assert_eq!(data.len()/4, out.len()); unsafe { (dispatch().pcm_i32_be)(data,out,bits) }; Ok(())
}

pub(crate) fn pcm_f32_le_to_f32_into(data: &[u8], out: &mut [f32]) -> Result<(), AudioError> {
    if data.len()/4 != out.len() { return Err(AudioError::TruncatedData); }
    let mut already_normalized = cfg!(target_endian = "little");
    for c in data.chunks_exact(4) {
        let value = f32::from_le_bytes(c.try_into().unwrap());
        if !value.is_finite() { return Err(AudioError::InvalidFormat("PCM contains non-finite float sample".into())); }
        already_normalized &= (-1.0..=1.0).contains(&value);
    }
    if already_normalized {
        // Native-endian IEEE f32 can be copied in bulk when no sanitizing clamp
        // is required. `copy_nonoverlapping` permits unaligned input bytes.
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), out.as_mut_ptr().cast::<u8>(), data.len());
        }
    } else {
        unsafe { (dispatch().pcm_f32_le)(data,out) };
    }
    Ok(())
}
pub(crate) fn pcm_f32_be_to_f32_into(data: &[u8], out: &mut [f32]) -> Result<(), AudioError> {
    if data.len()/4 != out.len() { return Err(AudioError::TruncatedData); }
    for c in data.chunks_exact(4) {
        if !f32::from_be_bytes(c.try_into().unwrap()).is_finite() { return Err(AudioError::InvalidFormat("PCM contains non-finite float sample".into())); }
    }
    unsafe { (dispatch().pcm_f32_be)(data,out) }; Ok(())
}
pub(crate) fn pcm_f64_le_to_f32_into(data: &[u8], out: &mut [f32]) -> Result<(), AudioError> {
    if data.len()/8 != out.len() { return Err(AudioError::TruncatedData); }
    for c in data.chunks_exact(8) {
        if !f64::from_le_bytes(c.try_into().unwrap()).is_finite() { return Err(AudioError::InvalidFormat("PCM contains non-finite float sample".into())); }
    }
    unsafe { (dispatch().pcm_f64_le)(data,out) }; Ok(())
}
pub(crate) fn pcm_f64_be_to_f32_into(data: &[u8], out: &mut [f32]) -> Result<(), AudioError> {
    if data.len()/8 != out.len() { return Err(AudioError::TruncatedData); }
    for c in data.chunks_exact(8) {
        if !f64::from_be_bytes(c.try_into().unwrap()).is_finite() { return Err(AudioError::InvalidFormat("PCM contains non-finite float sample".into())); }
    }
    unsafe { (dispatch().pcm_f64_be)(data,out) }; Ok(())
}

/// Convert normalized f32 samples to i16 using AVX2 where available.
pub fn f32_to_i16(input:&[f32],out:&mut[i16]){
    // The AVX2 path writes through raw pointers; validate the output slice even
    // in release builds to prevent out-of-bounds stores.
    assert_eq!(input.len(), out.len(), "f32_to_i16 output length must equal input length");
    #[cfg(all(feature="simd",any(target_arch="x86",target_arch="x86_64")))]
    if std::arch::is_x86_feature_detected!("avx2") { unsafe { f32_to_i16_avx2(input,out); } return; }
    for (dst,&sample) in out.iter_mut().zip(input){let s=if sample.is_finite(){sample.clamp(-1.0,32767.0/32768.0)}else{0.0};*dst=(s*32768.0).round_ties_even() as i16;}
}

#[cfg(all(feature="simd",any(target_arch="x86",target_arch="x86_64")))]
#[target_feature(enable="avx2")]
unsafe fn f32_to_i16_avx2(input:&[f32],out:&mut[i16]){
    #[cfg(target_arch="x86")] use core::arch::x86::*;
    #[cfg(target_arch="x86_64")] use core::arch::x86_64::*;
    let mut i = 0usize;
    // SAFETY: this function is only reached after AVX2 runtime dispatch.
    // Both loads and the store are bounded by the 16-sample loop guard,
    // and the unaligned intrinsic variants do not require aligned slices.
    unsafe {
        let lo=_mm256_set1_ps(-1.0);let hi=_mm256_set1_ps(32767.0/32768.0);let scale=_mm256_set1_ps(32768.0);let max_finite=_mm256_set1_ps(f32::MAX);let abs_mask=_mm256_castsi256_ps(_mm256_set1_epi32(0x7fff_ffff));
        while i+16<=input.len(){
            let raw_a=_mm256_loadu_ps(input.as_ptr().add(i));let raw_b=_mm256_loadu_ps(input.as_ptr().add(i+8));
            let finite_a=_mm256_cmp_ps::<_CMP_LE_OQ>(_mm256_and_ps(raw_a,abs_mask),max_finite);let finite_b=_mm256_cmp_ps::<_CMP_LE_OQ>(_mm256_and_ps(raw_b,abs_mask),max_finite);
            let clean_a=_mm256_and_ps(raw_a,finite_a);let clean_b=_mm256_and_ps(raw_b,finite_b);
            let a=_mm256_mul_ps(_mm256_min_ps(hi,_mm256_max_ps(lo,clean_a)),scale);let b=_mm256_mul_ps(_mm256_min_ps(hi,_mm256_max_ps(lo,clean_b)),scale);
            let ai=_mm256_cvtps_epi32(a);let bi=_mm256_cvtps_epi32(b);let packed=_mm256_packs_epi32(ai,bi);let ordered=_mm256_permute4x64_epi64::<0xD8>(packed);_mm256_storeu_si256(out.as_mut_ptr().add(i) as *mut __m256i,ordered);i+=16;
        }
    }
    for n in i..input.len(){let s=if input[n].is_finite(){input[n].clamp(-1.0,32767.0/32768.0)}else{0.0};out[n]=(s*32768.0).round_ties_even() as i16;}
}

#[inline] pub(crate) fn mul_in_place(left:&mut[f32],right:&[f32]) { debug_assert_eq!(left.len(),right.len()); unsafe{(dispatch().mul)(left,right)} }
#[inline] pub fn mix_add(dst:&mut[f32],src:&[f32],gain:f32){ debug_assert_eq!(dst.len(),src.len()); unsafe{(dispatch().mix_add)(dst,src,gain)} }
#[inline] pub(crate) fn overlap_window(current:&mut[f32],previous:&[f32],window:&[f32]){ debug_assert_eq!(current.len(),previous.len()); debug_assert!(window.len()>=current.len()); unsafe{(dispatch().overlap)(current,previous,window)} }
#[inline] pub fn interleave_stereo(left:&[f32],right:&[f32],out:&mut[f32]){ debug_assert_eq!(left.len(),right.len()); debug_assert_eq!(out.len(),left.len()*2); unsafe{(dispatch().interleave_stereo)(left,right,out)} }
#[inline] pub fn deinterleave_stereo(input:&[f32],left:&mut[f32],right:&mut[f32]){ debug_assert_eq!(left.len(),right.len()); debug_assert_eq!(input.len(),left.len()*2); unsafe{(dispatch().deinterleave_stereo)(input,left,right)} }
#[inline] pub fn lerp_slice(a:&[f32],b:&[f32],t:f32,out:&mut[f32]){ debug_assert_eq!(a.len(),b.len()); debug_assert_eq!(a.len(),out.len()); unsafe{(dispatch().lerp)(a,b,t,out)} }
#[inline] pub fn transform_butterfly(a:&mut[f32],b:&mut[f32],factor:f32){ debug_assert_eq!(a.len(),b.len()); unsafe{(dispatch().butterfly)(a,b,factor)} }

/// Batched radix-2 complex FFT butterfly for a shared twiddle. Vorbis' IMDCT
/// groups butterflies by twiddle so x86 AVX2/AArch64 NEON can process several
/// independent FFT groups at once.
#[inline]
pub(crate) fn complex_fft_butterfly(
    ar: &mut [f32], ai: &mut [f32], br: &mut [f32], bi: &mut [f32], cos: f32, sin: f32,
) {
    debug_assert_eq!(ar.len(), ai.len());
    debug_assert_eq!(ar.len(), br.len());
    debug_assert_eq!(ar.len(), bi.len());
    unsafe { (dispatch().complex_butterfly)(ar, ai, br, bi, cos, sin) }
}

#[inline]
pub(crate) fn vorbis_inverse_couple(magnitude:&mut[f32],angle:&mut[f32]){debug_assert_eq!(magnitude.len(),angle.len());unsafe{(dispatch().vorbis_couple)(magnitude,angle)}}

#[inline] pub fn apply_gain(samples:&mut[f32],gain:f32){ unsafe{(dispatch().gain)(samples,gain)} }
#[cfg(feature = "benchmarking")]
#[inline]
pub fn benchmark_scalar_gain(samples: &mut [f32], gain: f32) { unsafe { scalar_gain(samples, gain) } }
#[inline] pub fn limit_samples(samples:&mut[f32]){ unsafe{(dispatch().limiter)(samples)} }
#[inline] pub fn stereo_to_mono(input:&[f32],out:&mut[f32]){ debug_assert_eq!(input.len(),out.len()*2); unsafe{(dispatch().stereo_average)(input,out)} }
#[inline] pub fn mono_to_stereo(input:&[f32],out:&mut[f32]){ debug_assert_eq!(out.len(),input.len()*2); unsafe{(dispatch().mono_duplicate)(input,out)} }
#[inline] pub(crate) fn flac_restore_left_side(left:&[i64], side_as_right:&mut[i64]){ debug_assert_eq!(left.len(),side_as_right.len()); unsafe{(dispatch().flac_left_side)(left,side_as_right)} }
#[inline] pub(crate) fn flac_restore_right_side(side_as_left:&mut[i64], right:&[i64]){ debug_assert_eq!(side_as_left.len(),right.len()); unsafe{(dispatch().flac_right_side)(side_as_left,right)} }
#[inline] pub(crate) fn flac_restore_mid_side(mid_as_left:&mut[i64], side_as_right:&mut[i64]) -> Result<(), AudioError> {
    debug_assert_eq!(mid_as_left.len(), side_as_right.len());
    for (mid, side) in mid_as_left.iter_mut().zip(side_as_right.iter_mut()) {
        let doubled_mid = (*mid as i128) << 1;
        let combined = doubled_mid | (*side as i128 & 1);
        let left = (combined + *side as i128) >> 1;
        let right = (combined - *side as i128) >> 1;
        *mid = i64::try_from(left).map_err(|_| AudioError::InvalidFormat("FLAC mid-side left-channel overflow".into()))?;
        *side = i64::try_from(right).map_err(|_| AudioError::InvalidFormat("FLAC mid-side right-channel overflow".into()))?;
    }
    Ok(())
}

pub(super) unsafe fn scalar_pcm_u8(data:&[u8],out:&mut[f32]){ for(dst,&s)in out.iter_mut().zip(data){*dst=(s as f32-128.0)/128.0;} }
pub(super) unsafe fn scalar_pcm_i16_le(data:&[u8],out:&mut[f32],shift:u32,scale:f32){ for(dst,c)in out.iter_mut().zip(data.chunks_exact(2)){*dst=((i16::from_le_bytes([c[0],c[1]])>>shift)as f32)/scale;} }
pub(super) unsafe fn scalar_pcm_i16_be(data:&[u8],out:&mut[f32],shift:u32,scale:f32){ for(dst,c)in out.iter_mut().zip(data.chunks_exact(2)){*dst=((i16::from_be_bytes([c[0],c[1]])>>shift)as f32)/scale;} }

unsafe fn scalar_i24(data:&[u8],out:&mut[f32],bits:u16,le:bool){
    let shift=24-bits; let scale=2.0_f32.powi(bits as i32-1);
    for(dst,c)in out.iter_mut().zip(data.chunks_exact(3)){
        let raw=if le{(c[0]as i32)|((c[1]as i32)<<8)|((c[2]as i32)<<16)}else{((c[0]as i32)<<16)|((c[1]as i32)<<8)|c[2]as i32};
        let signed=(raw<<8)>>8; *dst=((signed>>shift)as f32)/scale;
    }
}
pub(super) unsafe fn scalar_pcm_i24_le(d:&[u8],o:&mut[f32],b:u16){unsafe{scalar_i24(d,o,b,true)}}
pub(super) unsafe fn scalar_pcm_i24_be(d:&[u8],o:&mut[f32],b:u16){unsafe{scalar_i24(d,o,b,false)}}
unsafe fn scalar_i32(data:&[u8],out:&mut[f32],bits:u16,le:bool){let shift=32-bits;let scale=2.0_f64.powi(bits as i32-1);for(dst,c)in out.iter_mut().zip(data.chunks_exact(4)){let raw=if le{i32::from_le_bytes(c.try_into().unwrap())}else{i32::from_be_bytes(c.try_into().unwrap())};*dst=((raw>>shift)as f64/scale)as f32;}}
pub(super) unsafe fn scalar_pcm_i32_le(d:&[u8],o:&mut[f32],b:u16){unsafe{scalar_i32(d,o,b,true)}}
pub(super) unsafe fn scalar_pcm_i32_be(d:&[u8],o:&mut[f32],b:u16){unsafe{scalar_i32(d,o,b,false)}}
pub(super) unsafe fn scalar_pcm_f32_le(d:&[u8],o:&mut[f32]){for(dst,c)in o.iter_mut().zip(d.chunks_exact(4)){*dst=f32::from_le_bytes(c.try_into().unwrap()).clamp(-1.0,1.0)}}
pub(super) unsafe fn scalar_pcm_f32_be(d:&[u8],o:&mut[f32]){for(dst,c)in o.iter_mut().zip(d.chunks_exact(4)){*dst=f32::from_be_bytes(c.try_into().unwrap()).clamp(-1.0,1.0)}}
pub(super) unsafe fn scalar_pcm_f64_le(d:&[u8],o:&mut[f32]){for(dst,c)in o.iter_mut().zip(d.chunks_exact(8)){*dst=f64::from_le_bytes(c.try_into().unwrap()).clamp(-1.0,1.0)as f32}}
pub(super) unsafe fn scalar_pcm_f64_be(d:&[u8],o:&mut[f32]){for(dst,c)in o.iter_mut().zip(d.chunks_exact(8)){*dst=f64::from_be_bytes(c.try_into().unwrap()).clamp(-1.0,1.0)as f32}}
pub(super) unsafe fn scalar_mul(a:&mut[f32],b:&[f32]){for(x,&y)in a.iter_mut().zip(b){*x*=y}}
pub(super) unsafe fn scalar_mix_add(d:&mut[f32],s:&[f32],g:f32){for(x,&y)in d.iter_mut().zip(s){*x+=y*g}}
pub(super) unsafe fn scalar_overlap(c:&mut[f32],p:&[f32],w:&[f32]){let n=c.len();for i in 0..n{c[i]=c[i]*w[i]+p[i]*w[n-1-i]}}
pub(super) unsafe fn scalar_interleave_stereo(l:&[f32],r:&[f32],o:&mut[f32]){for i in 0..l.len(){o[i*2]=l[i];o[i*2+1]=r[i]}}
pub(super) unsafe fn scalar_deinterleave_stereo(i:&[f32],l:&mut[f32],r:&mut[f32]){for n in 0..l.len(){l[n]=i[n*2];r[n]=i[n*2+1]}}
pub(super) unsafe fn scalar_lerp(a:&[f32],b:&[f32],t:f32,o:&mut[f32]){for((o,&a),&b)in o.iter_mut().zip(a).zip(b){*o=a+(b-a)*t}}
pub(super) unsafe fn scalar_butterfly(a:&mut[f32],b:&mut[f32],f:f32){for(a,b)in a.iter_mut().zip(b){let x=*a;let y=*b;*a=x+y*f;*b=x-y*f}}
pub(super) unsafe fn scalar_complex_butterfly(ar:&mut[f32],ai:&mut[f32],br:&mut[f32],bi:&mut[f32],c:f32,s:f32){
    for i in 0..ar.len(){let xr=ar[i];let xi=ai[i];let yr=br[i];let yi=bi[i];let tr=yr*c-yi*s;let ti=yr*s+yi*c;ar[i]=xr+tr;ai[i]=xi+ti;br[i]=xr-tr;bi[i]=xi-ti;}
}
pub(super) unsafe fn scalar_vorbis_couple(m:&mut[f32],a:&mut[f32]){for i in 0..m.len(){let mag=m[i];let ang=a[i];let(nm,na)=if mag>0.0{if ang>0.0{(mag,mag-ang)}else{(mag+ang,mag)}}else if ang>0.0{(mag,mag+ang)}else{(mag-ang,mag)};m[i]=nm;a[i]=na;}}

pub(super) unsafe fn scalar_gain(s:&mut[f32],g:f32){for x in s{*x*=g}}
pub(super) unsafe fn scalar_limiter(s:&mut[f32]){for x in s{if !x.is_finite(){*x=0.0}else{*x=x.clamp(-1.0,1.0)}}}
pub(super) unsafe fn scalar_stereo_average(i:&[f32],o:&mut[f32]){for(n,d)in o.iter_mut().enumerate(){*d=(i[n*2]+i[n*2+1])*0.5}}
pub(super) unsafe fn scalar_mono_duplicate(i:&[f32],o:&mut[f32]){for(n,&s)in i.iter().enumerate(){o[n*2]=s;o[n*2+1]=s}}
pub(super) unsafe fn scalar_flac_left_side(left:&[i64],right:&mut[i64]){for(l,r)in left.iter().zip(right.iter_mut()){*r=*l-*r}}
pub(super) unsafe fn scalar_flac_right_side(left:&mut[i64],right:&[i64]){for(l,r)in left.iter_mut().zip(right){*l=*l+*r}}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a:&[f32],b:&[f32]) { assert_eq!(a.len(),b.len()); for(&x,&y)in a.iter().zip(b){ assert!((x-y).abs() <= 2.0e-5, "{x} != {y}"); } }

    #[test]
    fn unaligned_pcm_and_tail_match_scalar() {
        let storage: Vec<u8>=(0..97).map(|x|(x*37)as u8).collect();
        let data=&storage[1..96];
        let mut got=vec![0.0;data.len()]; let mut expected=vec![0.0;data.len()];
        pcm_u8_to_f32_into(data,&mut got); unsafe{scalar_pcm_u8(data,&mut expected)}; approx(&got,&expected);
    }

    #[test]
    fn scalar_vs_dispatch_randomized_equivalence() {
        let mut state=0x1234_5678_9ABC_DEF0u64;
        let mut next=||{state^=state<<13;state^=state>>7;state^=state<<17;state};
        for len in 0..257usize {
            let a:Vec<f32>=(0..len).map(|_|((next()>>40)as i32 as f32)/8_388_608.0).collect();
            let b:Vec<f32>=(0..len).map(|_|((next()>>40)as i32 as f32)/8_388_608.0).collect();
            let mut got=a.clone(); let mut expected=a.clone();
            mix_add(&mut got,&b,0.371); unsafe{scalar_mix_add(&mut expected,&b,0.371)}; approx(&got,&expected);
            apply_gain(&mut got,0.75); unsafe{scalar_gain(&mut expected,0.75)}; approx(&got,&expected);
        }
    }

    #[test]
    fn f32_to_i16_handles_nonfinite_unaligned_and_tail_values() {
        let storage=[99.0,-2.0,-1.0,-0.5,-0.0,0.1,0.5,0.9999,1.0,2.0,f32::NAN,f32::INFINITY,f32::NEG_INFINITY,0.25,0.75,-0.25,0.3333,-0.3333];
        let input=&storage[1..];let mut got=vec![0i16;input.len()];f32_to_i16(input,&mut got);
        let expected:Vec<i16>=input.iter().map(|&sample|{let s=if sample.is_finite(){sample.clamp(-1.0,32767.0/32768.0)}else{0.0};(s*32768.0).round_ties_even() as i16}).collect();
        assert_eq!(got,expected);
    }

    #[test]
    fn interleave_deinterleave_round_trip() {
        let left:Vec<f32>=(0..37).map(|x|x as f32).collect();
        let right:Vec<f32>=(0..37).map(|x|-(x as f32)).collect();
        let mut inter=vec![0.0;74]; interleave_stereo(&left,&right,&mut inter);
        let mut l=vec![0.0;37];let mut r=vec![0.0;37];deinterleave_stereo(&inter,&mut l,&mut r);
        assert_eq!(l,left);assert_eq!(r,right);
    }


    #[test]
    fn every_dispatch_kernel_handles_unaligned_inputs_and_scalar_tails() {
        // Byte-oriented PCM kernels: offset the input by one byte so aligned
        // vector loads cannot be assumed, and use odd sample counts to force a
        // scalar tail after every vector-width loop.
        let mut u8_storage = vec![0xEE];
        u8_storage.extend((0..37).map(|x| (x * 7) as u8));
        let u8_data = &u8_storage[1..];
        let mut got = vec![0.0; 37]; let mut expected = vec![0.0; 37];
        pcm_u8_to_f32_into(u8_data, &mut got); unsafe { scalar_pcm_u8(u8_data, &mut expected) }; approx(&got,&expected);

        let i16_values:Vec<i16>=(0..37).map(|x| (x as i16 * 701).wrapping_sub(12000)).collect();
        for be in [false,true] {
            let mut bytes=vec![0xAA]; for &v in &i16_values { bytes.extend(if be {v.to_be_bytes()} else {v.to_le_bytes()}); }
            let data=&bytes[1..]; got.resize(37,0.0); expected.resize(37,0.0);
            if be { pcm_i16_be_to_f32_into(data,&mut got,0,32768.0); unsafe{scalar_pcm_i16_be(data,&mut expected,0,32768.0)} }
            else { pcm_i16_le_to_f32_into(data,&mut got,0,32768.0); unsafe{scalar_pcm_i16_le(data,&mut expected,0,32768.0)} }
            approx(&got,&expected);
        }

        let i24_values:Vec<i32>=(0..37).map(|x| x as i32*131071-2_000_000).collect();
        for be in [false,true] {
            let mut bytes=vec![0xAA]; for &v in &i24_values { let b=v.to_le_bytes(); if be {bytes.extend([b[2],b[1],b[0]])} else {bytes.extend([b[0],b[1],b[2]])} }
            let data=&bytes[1..]; got.resize(37,0.0); expected.resize(37,0.0);
            if be { pcm_i24_be_to_f32_into(data,24,&mut got).unwrap(); unsafe{scalar_pcm_i24_be(data,&mut expected,24)} }
            else { pcm_i24_le_to_f32_into(data,24,&mut got).unwrap(); unsafe{scalar_pcm_i24_le(data,&mut expected,24)} }
            approx(&got,&expected);
        }

        let i32_values:Vec<i32>=(0..37).map(|x| x as i32*10_000_003-150_000_000).collect();
        for be in [false,true] {
            let mut bytes=vec![0xAA]; for &v in &i32_values { bytes.extend(if be {v.to_be_bytes()} else {v.to_le_bytes()}); }
            let data=&bytes[1..]; got.resize(37,0.0); expected.resize(37,0.0);
            if be { pcm_i32_be_to_f32_into(data,32,&mut got).unwrap(); unsafe{scalar_pcm_i32_be(data,&mut expected,32)} }
            else { pcm_i32_le_to_f32_into(data,32,&mut got).unwrap(); unsafe{scalar_pcm_i32_le(data,&mut expected,32)} }
            approx(&got,&expected);
        }

        let float_values:Vec<f32>=(0..37).map(|x| x as f32/19.0-0.9).collect();
        for be in [false,true] {
            let mut bytes=vec![0xAA]; for &v in &float_values { bytes.extend(if be {v.to_be_bytes()} else {v.to_le_bytes()}); }
            let data=&bytes[1..]; got.resize(37,0.0); expected.resize(37,0.0);
            if be { pcm_f32_be_to_f32_into(data,&mut got).unwrap(); unsafe{scalar_pcm_f32_be(data,&mut expected)} }
            else { pcm_f32_le_to_f32_into(data,&mut got).unwrap(); unsafe{scalar_pcm_f32_le(data,&mut expected)} }
            approx(&got,&expected);
        }
        let double_values:Vec<f64>=(0..37).map(|x| x as f64/19.0-0.9).collect();
        for be in [false,true] {
            let mut bytes=vec![0xAA]; for &v in &double_values { bytes.extend(if be {v.to_be_bytes()} else {v.to_le_bytes()}); }
            let data=&bytes[1..]; got.resize(37,0.0); expected.resize(37,0.0);
            if be { pcm_f64_be_to_f32_into(data,&mut got).unwrap(); unsafe{scalar_pcm_f64_be(data,&mut expected)} }
            else { pcm_f64_le_to_f32_into(data,&mut got).unwrap(); unsafe{scalar_pcm_f64_le(data,&mut expected)} }
            approx(&got,&expected);
        }

        // f32 kernels use slices offset by one float and lengths that are not
        // multiples of SSE/AVX/NEON vector widths.
        let base_a:Vec<f32>=(0..40).map(|x| x as f32/31.0-0.5).collect();
        let base_b:Vec<f32>=(0..40).map(|x| 0.7-x as f32/47.0).collect();
        let a=&base_a[1..38]; let b=&base_b[1..38];

        let mut got=a.to_vec(); let mut expected=a.to_vec(); mul_in_place(&mut got,b); unsafe{scalar_mul(&mut expected,b)}; approx(&got,&expected);
        got.copy_from_slice(a); expected.copy_from_slice(a); mix_add(&mut got,b,0.37); unsafe{scalar_mix_add(&mut expected,b,0.37)}; approx(&got,&expected);
        let window:Vec<f32>=(0..37).map(|x|(x as f32+1.0)/38.0).collect(); got.copy_from_slice(a); expected.copy_from_slice(a); overlap_window(&mut got,b,&window); unsafe{scalar_overlap(&mut expected,b,&window)}; approx(&got,&expected);
        got.copy_from_slice(a); expected.copy_from_slice(a); apply_gain(&mut got,0.71); unsafe{scalar_gain(&mut expected,0.71)}; approx(&got,&expected);
        got.iter_mut().enumerate().for_each(|(i,x)|*x=if i%3==0{1.7}else if i%5==0{-1.8}else{*x}); expected.clone_from(&got); limit_samples(&mut got); unsafe{scalar_limiter(&mut expected)}; approx(&got,&expected);

        let mut gout=vec![0.0;37]; let mut eout=vec![0.0;37]; lerp_slice(a,b,0.42,&mut gout); unsafe{scalar_lerp(a,b,0.42,&mut eout)}; approx(&gout,&eout);
        let mut ga=a.to_vec();let mut gb=b.to_vec();let mut ea=ga.clone();let mut eb=gb.clone();transform_butterfly(&mut ga,&mut gb,0.33);unsafe{scalar_butterfly(&mut ea,&mut eb,0.33)};approx(&ga,&ea);approx(&gb,&eb);

        let left=&base_a[1..38]; let right=&base_b[1..38]; let mut gi=vec![0.0;74];let mut ei=vec![0.0;74];interleave_stereo(left,right,&mut gi);unsafe{scalar_interleave_stereo(left,right,&mut ei)};approx(&gi,&ei);
        let mut gl=vec![0.0;37];let mut gr=vec![0.0;37];let mut el=vec![0.0;37];let mut er=vec![0.0;37];deinterleave_stereo(&gi,&mut gl,&mut gr);unsafe{scalar_deinterleave_stereo(&gi,&mut el,&mut er)};approx(&gl,&el);approx(&gr,&er);
        let mut gm=vec![0.0;37];let mut em=vec![0.0;37];stereo_to_mono(&gi,&mut gm);unsafe{scalar_stereo_average(&gi,&mut em)};approx(&gm,&em);
        let mut gs=vec![0.0;74];let mut es=vec![0.0;74];mono_to_stereo(left,&mut gs);unsafe{scalar_mono_duplicate(left,&mut es)};approx(&gs,&es);
    }
}

#[cfg(test)]
mod hardening_extreme_tests {
    use super::*;
    #[test]
    fn all_short_tails_and_unaligned_f32_to_i16_sanitize_correctly() {
        let values=[f32::NAN,f32::INFINITY,f32::NEG_INFINITY,f32::MAX,f32::MIN,-1.0,-0.0,0.0,0.5,1.0,1.5,-1.5,0.000001];
        for offset in 0..8 {for len in 0..129 {
            let src=(0..(offset+len)).map(|i|values[i%values.len()]).collect::<Vec<_>>();
            let mut got=vec![i16::MIN;len];
            f32_to_i16(&src[offset..],&mut got);
            let expected=src[offset..].iter().map(|&v|{
                let v=if v.is_finite(){v.clamp(-1.0,32767.0/32768.0)}else{0.0};
                (v*32768.0).round_ties_even() as i16
            }).collect::<Vec<_>>();
            assert_eq!(got,expected,"offset={offset} len={len}");
        }}
    }
    #[test]
    fn tiny_and_vector_tail_mixing_equals_scalar() {
        for n in 0..129 {
            let src=(0..n).map(|i|((i*17%100) as f32-50.0)/100.0).collect::<Vec<_>>();
            let mut fast=vec![0.25;n];let mut scalar=fast.clone();
            mix_add(&mut fast,&src,0.5);
            unsafe { scalar_mix_add(&mut scalar,&src,0.5); }
            for (a,b) in fast.iter().zip(scalar.iter()) {assert!((a-b).abs()<=1e-6);}
        }
    }
}
