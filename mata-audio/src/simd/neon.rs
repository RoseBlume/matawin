use super::*;

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;
#[cfg(target_arch = "arm")]
use core::arch::arm::*;

#[cfg(target_arch = "aarch64")]
pub(super) fn detect() -> AudioSimd {
    let mut d = scalar_dispatch();
    if !std::arch::is_aarch64_feature_detected!("neon") { return d; }
    d.isa = RuntimeIsa::Neon;
    d.pcm_u8 = pcm_u8_neon;
    d.pcm_i16_le = pcm_i16_le_neon;
    d.pcm_i16_be = pcm_i16_be_neon;
    d.pcm_i32_le = pcm_i32_le_neon;
    d.pcm_i32_be = pcm_i32_be_neon;
    d.pcm_f32_le = pcm_f32_le_neon;
    d.mul = mul_neon;
    d.mix_add = mix_add_neon;
    d.overlap = overlap_neon;
    d.interleave_stereo = interleave_stereo_neon;
    d.deinterleave_stereo = deinterleave_stereo_neon;
    d.lerp = lerp_neon;
    d.butterfly = butterfly_neon;
    d.gain = gain_neon;
    d.limiter = limiter_neon;
    d.stereo_average = stereo_average_neon;
    d.mono_duplicate = mono_duplicate_neon;
    d.complex_butterfly = complex_butterfly_neon;
    d.vorbis_couple = vorbis_couple_neon;
    d
}

#[cfg(target_arch = "arm")]
pub(super) fn detect() -> AudioSimd {
    let mut d = scalar_dispatch();
    if !std::arch::is_arm_feature_detected!("neon") { return d; }
    d.isa = RuntimeIsa::Neon;
    d.pcm_u8 = pcm_u8_neon_arm;
    d.mul = mul_neon_arm;
    d.mix_add = mix_add_neon_arm;
    d.gain = gain_neon_arm;
    d
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_u8_neon(data:&[u8],out:&mut[f32]){
    let offset=vdupq_n_f32(128.0);let scale=vdupq_n_f32(1.0/128.0);let mut i=0;
    while i+16<=out.len(){let b=unsafe{vld1q_u8(data.as_ptr().add(i))};let lo16=vmovl_u8(vget_low_u8(b));let hi16=vmovl_high_u8(b);let parts=[vmovl_u16(vget_low_u16(lo16)),vmovl_high_u16(lo16),vmovl_u16(vget_low_u16(hi16)),vmovl_high_u16(hi16)];for(p,o)in parts.into_iter().zip([0usize,4,8,12]){let f=vmulq_f32(vsubq_f32(vcvtq_f32_u32(p),offset),scale);unsafe{vst1q_f32(out.as_mut_ptr().add(i+o),f)}}i+=16;}unsafe{scalar_pcm_u8(&data[i..],&mut out[i..])}
}

#[cfg(target_arch = "arm")]
#[target_feature(enable = "neon")]
unsafe fn pcm_u8_neon_arm(data:&[u8],out:&mut[f32]){
    let offset=vdupq_n_f32(128.0);let scale=vdupq_n_f32(1.0/128.0);let mut i=0;
    while i+8<=out.len(){let b=unsafe{vld1_u8(data.as_ptr().add(i))};let u16s=vmovl_u8(b);let lo=vmovl_u16(vget_low_u16(u16s));let hi=vmovl_u16(vget_high_u16(u16s));unsafe{vst1q_f32(out.as_mut_ptr().add(i),vmulq_f32(vsubq_f32(vcvtq_f32_u32(lo),offset),scale));vst1q_f32(out.as_mut_ptr().add(i+4),vmulq_f32(vsubq_f32(vcvtq_f32_u32(hi),offset),scale));}i+=8;}unsafe{scalar_pcm_u8(&data[i..],&mut out[i..])}
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i16_le_neon(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_neon::<false>(d,o,s,k)}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i16_be_neon(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_neon::<true>(d,o,s,k)}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i16_neon<const BE:bool>(data:&[u8],out:&mut[f32],shift:u32,scale:f32){let inv=vdupq_n_f32(1.0/scale);let sh=vdupq_n_s16(-(shift as i16));let mut i=0;while i+8<=out.len(){let mut v=unsafe{vld1q_s16(data.as_ptr().add(i*2)as*const i16)};if BE{v=vreinterpretq_s16_u8(vrev16q_u8(vreinterpretq_u8_s16(v)));}if shift!=0{v=vshlq_s16(v,sh);}let lo=vmovl_s16(vget_low_s16(v));let hi=vmovl_high_s16(v);unsafe{vst1q_f32(out.as_mut_ptr().add(i),vmulq_f32(vcvtq_f32_s32(lo),inv));vst1q_f32(out.as_mut_ptr().add(i+4),vmulq_f32(vcvtq_f32_s32(hi),inv));}i+=8;}if BE{unsafe{scalar_pcm_i16_be(&data[i*2..],&mut out[i..],shift,scale)}}else{unsafe{scalar_pcm_i16_le(&data[i*2..],&mut out[i..],shift,scale)}}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i32_le_neon(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_neon::<false>(d,o,b)}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i32_be_neon(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_neon::<true>(d,o,b)}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_i32_neon<const BE:bool>(data:&[u8],out:&mut[f32],bits:u16){let inv=vdupq_n_f32((1.0/2.0_f64.powi(bits as i32-1))as f32);let sh=vdupq_n_s32(-((32-bits)as i32));let mut i=0;while i+4<=out.len(){let mut v=unsafe{vld1q_s32(data.as_ptr().add(i*4)as*const i32)};if BE{v=vreinterpretq_s32_u8(vrev32q_u8(vreinterpretq_u8_s32(v)));}if bits<32{v=vshlq_s32(v,sh);}unsafe{vst1q_f32(out.as_mut_ptr().add(i),vmulq_f32(vcvtq_f32_s32(v),inv));}i+=4;}if BE{unsafe{scalar_pcm_i32_be(&data[i*4..],&mut out[i..],bits)}}else{unsafe{scalar_pcm_i32_le(&data[i*4..],&mut out[i..],bits)}}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn pcm_f32_le_neon(data:&[u8],out:&mut[f32]){let lo=vdupq_n_f32(-1.0);let hi=vdupq_n_f32(1.0);let mut i=0;while i+4<=out.len(){let v=unsafe{vld1q_f32(data.as_ptr().add(i*4)as*const f32)};unsafe{vst1q_f32(out.as_mut_ptr().add(i),vminq_f32(hi,vmaxq_f32(lo,v)));}i+=4;}unsafe{scalar_pcm_f32_le(&data[i*4..],&mut out[i..])}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn mul_neon(a:&mut[f32],b:&[f32]){let mut i=0;while i+4<=a.len(){let x=unsafe{vld1q_f32(a.as_ptr().add(i))};let y=unsafe{vld1q_f32(b.as_ptr().add(i))};unsafe{vst1q_f32(a.as_mut_ptr().add(i),vmulq_f32(x,y));}i+=4;}unsafe{scalar_mul(&mut a[i..],&b[i..])}}
#[cfg(target_arch = "arm")]
#[target_feature(enable = "neon")]
unsafe fn mul_neon_arm(a:&mut[f32],b:&[f32]){let mut i=0;while i+4<=a.len(){let x=unsafe{vld1q_f32(a.as_ptr().add(i))};let y=unsafe{vld1q_f32(b.as_ptr().add(i))};unsafe{vst1q_f32(a.as_mut_ptr().add(i),vmulq_f32(x,y));}i+=4;}unsafe{scalar_mul(&mut a[i..],&b[i..])}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn mix_add_neon(d:&mut[f32],s:&[f32],g:f32){let vg=vdupq_n_f32(g);let mut i=0;while i+4<=d.len(){let a=unsafe{vld1q_f32(d.as_ptr().add(i))};let b=unsafe{vld1q_f32(s.as_ptr().add(i))};unsafe{vst1q_f32(d.as_mut_ptr().add(i),vfmaq_f32(a,b,vg));}i+=4;}unsafe{scalar_mix_add(&mut d[i..],&s[i..],g)}}
#[cfg(target_arch = "arm")]
#[target_feature(enable = "neon")]
unsafe fn mix_add_neon_arm(d:&mut[f32],s:&[f32],g:f32){let mut i=0;while i+4<=d.len(){let a=unsafe{vld1q_f32(d.as_ptr().add(i))};let b=unsafe{vld1q_f32(s.as_ptr().add(i))};unsafe{vst1q_f32(d.as_mut_ptr().add(i),vmlaq_n_f32(a,b,g));}i+=4;}unsafe{scalar_mix_add(&mut d[i..],&s[i..],g)}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn overlap_neon(c:&mut[f32],p:&[f32],w:&[f32]){let n=c.len();let mut i=0;while i+4<=n{let cv=unsafe{vld1q_f32(c.as_ptr().add(i))};let pv=unsafe{vld1q_f32(p.as_ptr().add(i))};let wf=unsafe{vld1q_f32(w.as_ptr().add(i))};let raw=unsafe{vld1q_f32(w.as_ptr().add(n-i-4))};let rev=vextq_f32(vrev64q_f32(raw),vrev64q_f32(raw),2);unsafe{vst1q_f32(c.as_mut_ptr().add(i),vfmaq_f32(vmulq_f32(cv,wf),pv,rev));}i+=4;}while i<n{c[i]=c[i]*w[i]+p[i]*w[n-1-i];i+=1;}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn interleave_stereo_neon(l:&[f32],r:&[f32],o:&mut[f32]){let mut i=0;while i+4<=l.len(){let a=unsafe{vld1q_f32(l.as_ptr().add(i))};let b=unsafe{vld1q_f32(r.as_ptr().add(i))};unsafe{vst1q_f32(o.as_mut_ptr().add(i*2),vzip1q_f32(a,b));vst1q_f32(o.as_mut_ptr().add(i*2+4),vzip2q_f32(a,b));}i+=4;}unsafe{scalar_interleave_stereo(&l[i..],&r[i..],&mut o[i*2..])}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn deinterleave_stereo_neon(input:&[f32],l:&mut[f32],r:&mut[f32]){let mut i=0;while i+4<=l.len(){let a=unsafe{vld1q_f32(input.as_ptr().add(i*2))};let b=unsafe{vld1q_f32(input.as_ptr().add(i*2+4))};unsafe{vst1q_f32(l.as_mut_ptr().add(i),vuzp1q_f32(a,b));vst1q_f32(r.as_mut_ptr().add(i),vuzp2q_f32(a,b));}i+=4;}unsafe{scalar_deinterleave_stereo(&input[i*2..],&mut l[i..],&mut r[i..])}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn lerp_neon(a:&[f32],b:&[f32],t:f32,o:&mut[f32]){let vt=vdupq_n_f32(t);let mut i=0;while i+4<=o.len(){let av=unsafe{vld1q_f32(a.as_ptr().add(i))};let bv=unsafe{vld1q_f32(b.as_ptr().add(i))};unsafe{vst1q_f32(o.as_mut_ptr().add(i),vfmaq_f32(av,vsubq_f32(bv,av),vt));}i+=4;}unsafe{scalar_lerp(&a[i..],&b[i..],t,&mut o[i..])}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn butterfly_neon(a:&mut[f32],b:&mut[f32],f:f32){let vf=vdupq_n_f32(f);let mut i=0;while i+4<=a.len(){let av=unsafe{vld1q_f32(a.as_ptr().add(i))};let bv=unsafe{vld1q_f32(b.as_ptr().add(i))};let y=vmulq_f32(bv,vf);unsafe{vst1q_f32(a.as_mut_ptr().add(i),vaddq_f32(av,y));vst1q_f32(b.as_mut_ptr().add(i),vsubq_f32(av,y));}i+=4;}unsafe{scalar_butterfly(&mut a[i..],&mut b[i..],f)}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn gain_neon(s:&mut[f32],g:f32){let vg=vdupq_n_f32(g);let mut i=0;while i+4<=s.len(){let v=unsafe{vld1q_f32(s.as_ptr().add(i))};unsafe{vst1q_f32(s.as_mut_ptr().add(i),vmulq_f32(v,vg));}i+=4;}unsafe{scalar_gain(&mut s[i..],g)}}
#[cfg(target_arch = "arm")]
#[target_feature(enable = "neon")]
unsafe fn gain_neon_arm(s:&mut[f32],g:f32){let vg=vdupq_n_f32(g);let mut i=0;while i+4<=s.len(){let v=unsafe{vld1q_f32(s.as_ptr().add(i))};unsafe{vst1q_f32(s.as_mut_ptr().add(i),vmulq_f32(v,vg));}i+=4;}unsafe{scalar_gain(&mut s[i..],g)}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn limiter_neon(s:&mut[f32]){let lo=vdupq_n_f32(-1.0);let hi=vdupq_n_f32(1.0);let zero=vdupq_n_f32(0.0);let mut i=0;while i+4<=s.len(){let v=unsafe{vld1q_f32(s.as_ptr().add(i))};let c=vminq_f32(hi,vmaxq_f32(lo,v));let mask=vceqq_f32(v,v);unsafe{vst1q_f32(s.as_mut_ptr().add(i),vbslq_f32(mask,c,zero));}i+=4;}unsafe{scalar_limiter(&mut s[i..])}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn stereo_average_neon(input:&[f32],out:&mut[f32]){let half=vdupq_n_f32(0.5);let mut i=0;while i+4<=out.len(){let a=unsafe{vld1q_f32(input.as_ptr().add(i*2))};let b=unsafe{vld1q_f32(input.as_ptr().add(i*2+4))};let l=vuzp1q_f32(a,b);let r=vuzp2q_f32(a,b);unsafe{vst1q_f32(out.as_mut_ptr().add(i),vmulq_f32(vaddq_f32(l,r),half));}i+=4;}unsafe{scalar_stereo_average(&input[i*2..],&mut out[i..])}}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn mono_duplicate_neon(input:&[f32],out:&mut[f32]){unsafe{interleave_stereo_neon(input,input,out)}}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn complex_butterfly_neon(ar:&mut[f32],ai:&mut[f32],br:&mut[f32],bi:&mut[f32],c:f32,s:f32){
    let cv=vdupq_n_f32(c);let sv=vdupq_n_f32(s);let mut i=0usize;
    while i+4<=ar.len(){
        let xr=unsafe{vld1q_f32(ar.as_ptr().add(i))};let xi=unsafe{vld1q_f32(ai.as_ptr().add(i))};
        let yr=unsafe{vld1q_f32(br.as_ptr().add(i))};let yi=unsafe{vld1q_f32(bi.as_ptr().add(i))};
        let tr=vsubq_f32(vmulq_f32(yr,cv),vmulq_f32(yi,sv));let ti=vaddq_f32(vmulq_f32(yr,sv),vmulq_f32(yi,cv));
        unsafe{vst1q_f32(ar.as_mut_ptr().add(i),vaddq_f32(xr,tr));vst1q_f32(ai.as_mut_ptr().add(i),vaddq_f32(xi,ti));vst1q_f32(br.as_mut_ptr().add(i),vsubq_f32(xr,tr));vst1q_f32(bi.as_mut_ptr().add(i),vsubq_f32(xi,ti));}
        i+=4;
    }
    unsafe{scalar_complex_butterfly(&mut ar[i..],&mut ai[i..],&mut br[i..],&mut bi[i..],c,s)}
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn vorbis_couple_neon(m:&mut[f32],a:&mut[f32]){let zero=vdupq_n_f32(0.0);let mut i=0usize;while i+4<=m.len(){let mv=unsafe{vld1q_f32(m.as_ptr().add(i))};let av=unsafe{vld1q_f32(a.as_ptr().add(i))};let mp=vcgtq_f32(mv,zero);let ap=vcgtq_f32(av,zero);let mplus=vaddq_f32(mv,av);let mminus=vsubq_f32(mv,av);let a_when_pos=vbslq_f32(mp,mminus,mplus);let na=vbslq_f32(ap,a_when_pos,mv);let m_when_nonpos=vbslq_f32(mp,mplus,mminus);let nm=vbslq_f32(ap,mv,m_when_nonpos);unsafe{vst1q_f32(m.as_mut_ptr().add(i),nm);vst1q_f32(a.as_mut_ptr().add(i),na);}i+=4;}unsafe{scalar_vorbis_couple(&mut m[i..],&mut a[i..])}}
