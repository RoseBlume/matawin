use super::*;

#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

pub(super) fn detect() -> AudioSimd {
    let mut d = scalar_dispatch();
    if !std::arch::is_x86_feature_detected!("sse2") { return d; }

    d.isa = RuntimeIsa::Sse2;
    d.pcm_u8 = pcm_u8_sse2;
    d.pcm_i16_le = pcm_i16_le_sse2;
    d.pcm_i16_be = pcm_i16_be_sse2;
    d.mul = mul_sse2;
    d.overlap = overlap_sse2;
    d.gain = gain_sse2;
    d.limiter = limiter_sse2;

    if std::arch::is_x86_feature_detected!("ssse3") {
        d.isa = RuntimeIsa::Ssse3;
        d.pcm_i24_le = pcm_i24_le_ssse3;
        d.pcm_i24_be = pcm_i24_be_ssse3;
    }
    if std::arch::is_x86_feature_detected!("sse4.1") {
        d.isa = RuntimeIsa::Sse41;
        d.pcm_i32_le = pcm_i32_le_sse41;
        d.pcm_i32_be = pcm_i32_be_sse41;
    }
    if std::arch::is_x86_feature_detected!("avx2") {
        d.isa = RuntimeIsa::Avx2;
        d.pcm_u8 = pcm_u8_avx2;
        d.pcm_i16_le = pcm_i16_le_avx2;
        d.pcm_i16_be = pcm_i16_be_avx2;
        d.pcm_i32_le = pcm_i32_le_avx2;
        d.pcm_i32_be = pcm_i32_be_avx2;
        d.pcm_f32_le = pcm_f32_le_avx2;
        d.pcm_f64_le = pcm_f64_le_avx2;
        d.mul = mul_avx2;
        d.mix_add = mix_add_avx2;
        d.overlap = overlap_avx2;
        d.interleave_stereo = interleave_stereo_avx2;
        d.deinterleave_stereo = deinterleave_stereo_avx2;
        d.gain = gain_avx2;
        d.limiter = limiter_avx2;
        d.stereo_average = stereo_average_avx2;
        d.mono_duplicate = mono_duplicate_avx2;
        d.flac_left_side = flac_left_side_avx2;
        d.flac_right_side = flac_right_side_avx2;
        d.complex_butterfly = complex_butterfly_avx2;
        d.vorbis_couple = vorbis_couple_avx2;
        if std::arch::is_x86_feature_detected!("fma") {
            d.isa = RuntimeIsa::Avx2Fma;
            d.lerp = lerp_fma;
            d.butterfly = butterfly_fma;
        }
    }
    d
}

#[target_feature(enable = "sse2")]
unsafe fn pcm_u8_sse2(data: &[u8], out: &mut [f32]) {
    let zero=_mm_setzero_si128(); let offset=_mm_set1_epi32(128); let scale=_mm_set1_ps(1.0/128.0); let mut i=0;
    while i+16<=out.len(){
        let bytes=unsafe{_mm_loadu_si128(data.as_ptr().add(i) as *const __m128i)};
        let lo16=_mm_unpacklo_epi8(bytes,zero); let hi16=_mm_unpackhi_epi8(bytes,zero);
        let groups=[_mm_unpacklo_epi16(lo16,zero),_mm_unpackhi_epi16(lo16,zero),_mm_unpacklo_epi16(hi16,zero),_mm_unpackhi_epi16(hi16,zero)];
        for (g,o) in groups.into_iter().zip([0usize,4,8,12]){let v=_mm_mul_ps(_mm_cvtepi32_ps(_mm_sub_epi32(g,offset)),scale);unsafe{_mm_storeu_ps(out.as_mut_ptr().add(i+o),v)}}
        i+=16;
    }
    unsafe{scalar_pcm_u8(&data[i..],&mut out[i..])}
}

#[target_feature(enable = "avx2")]
unsafe fn pcm_u8_avx2(data:&[u8],out:&mut[f32]){
    let mut i=0; while i+32<=out.len(){
        let bytes=unsafe{_mm256_loadu_si256(data.as_ptr().add(i) as *const __m256i)};
        let lo128=_mm256_castsi256_si128(bytes); let hi128=_mm256_extracti128_si256(bytes,1);
        for (part,base) in [(lo128,0usize),(hi128,16usize)]{
            let lo16=_mm256_cvtepu8_epi16(part); let low=_mm256_cvtepu16_epi32(_mm256_castsi256_si128(lo16)); let high=_mm256_cvtepu16_epi32(_mm256_extracti128_si256(lo16,1));
            let off=_mm256_set1_epi32(128); let scale=_mm256_set1_ps(1.0/128.0);
            unsafe{_mm256_storeu_ps(out.as_mut_ptr().add(i+base),_mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_sub_epi32(low,off)),scale));_mm256_storeu_ps(out.as_mut_ptr().add(i+base+8),_mm256_mul_ps(_mm256_cvtepi32_ps(_mm256_sub_epi32(high,off)),scale));}
        }
        i+=32;
    }
    unsafe{pcm_u8_sse2(&data[i..],&mut out[i..])}
}

#[target_feature(enable = "sse2")]
unsafe fn pcm_i16_le_sse2(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_sse2::<false>(d,o,s,k)}}
#[target_feature(enable = "sse2")]
unsafe fn pcm_i16_be_sse2(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_sse2::<true>(d,o,s,k)}}
#[target_feature(enable = "sse2")]
unsafe fn pcm_i16_sse2<const SWAP:bool>(data:&[u8],out:&mut[f32],shift:u32,scale:f32){
    let inv=_mm_set1_ps(1.0/scale);let sc=_mm_cvtsi32_si128(shift as i32);let mut i=0;
    while i+8<=out.len(){let mut v=unsafe{_mm_loadu_si128(data.as_ptr().add(i*2)as*const __m128i)};if SWAP{v=_mm_or_si128(_mm_slli_epi16(v,8),_mm_srli_epi16(v,8));}let sign=_mm_srai_epi16(v,15);let lo=_mm_sra_epi32(_mm_unpacklo_epi16(v,sign),sc);let hi=_mm_sra_epi32(_mm_unpackhi_epi16(v,sign),sc);unsafe{_mm_storeu_ps(out.as_mut_ptr().add(i),_mm_mul_ps(_mm_cvtepi32_ps(lo),inv));_mm_storeu_ps(out.as_mut_ptr().add(i+4),_mm_mul_ps(_mm_cvtepi32_ps(hi),inv));}i+=8;}
    if SWAP{unsafe{scalar_pcm_i16_be(&data[i*2..],&mut out[i..],shift,scale)}}else{unsafe{scalar_pcm_i16_le(&data[i*2..],&mut out[i..],shift,scale)}}
}
#[target_feature(enable = "avx2")]
unsafe fn pcm_i16_le_avx2(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_avx2::<false>(d,o,s,k)}}
#[target_feature(enable = "avx2")]
unsafe fn pcm_i16_be_avx2(d:&[u8],o:&mut[f32],s:u32,k:f32){unsafe{pcm_i16_avx2::<true>(d,o,s,k)}}
#[target_feature(enable = "avx2")]
unsafe fn pcm_i16_avx2<const SWAP:bool>(data:&[u8],out:&mut[f32],shift:u32,scale:f32){
    let inv=_mm256_set1_ps(1.0/scale);let mut i=0; while i+8<=out.len(){let mut raw=unsafe{_mm_loadu_si128(data.as_ptr().add(i*2)as*const __m128i)};if SWAP{raw=_mm_or_si128(_mm_slli_epi16(raw,8),_mm_srli_epi16(raw,8));}let mut v=_mm256_cvtepi16_epi32(raw);if shift!=0{let sc=_mm_cvtsi32_si128(shift as i32);v=_mm256_sra_epi32(v,sc);}unsafe{_mm256_storeu_ps(out.as_mut_ptr().add(i),_mm256_mul_ps(_mm256_cvtepi32_ps(v),inv));}i+=8;}if SWAP{unsafe{scalar_pcm_i16_be(&data[i*2..],&mut out[i..],shift,scale)}}else{unsafe{scalar_pcm_i16_le(&data[i*2..],&mut out[i..],shift,scale)}}
}

#[target_feature(enable = "ssse3")]
unsafe fn pcm_i24_le_ssse3(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i24_ssse3::<false>(d,o,b)}}
#[target_feature(enable = "ssse3")]
unsafe fn pcm_i24_be_ssse3(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i24_ssse3::<true>(d,o,b)}}
#[target_feature(enable = "ssse3")]
unsafe fn pcm_i24_ssse3<const BE:bool>(data:&[u8],out:&mut[f32],bits:u16){
    let mask=if BE{_mm_setr_epi8(2,1,0,-1,5,4,3,-1,8,7,6,-1,11,10,9,-1)}else{_mm_setr_epi8(0,1,2,-1,3,4,5,-1,6,7,8,-1,9,10,11,-1)};
    let shift=(24-bits)as i32;let sc=_mm_cvtsi32_si128(shift);let inv=_mm_set1_ps(1.0/2.0_f32.powi(bits as i32-1));let mut i=0usize;let mut byte=0usize;
    while i+4<=out.len() && byte+16<=data.len(){let raw=unsafe{_mm_loadu_si128(data.as_ptr().add(byte)as*const __m128i)};let packed=_mm_shuffle_epi8(raw,mask);let signed=_mm_srai_epi32::<8>(_mm_slli_epi32::<8>(packed));let shifted=_mm_sra_epi32(signed,sc);unsafe{_mm_storeu_ps(out.as_mut_ptr().add(i),_mm_mul_ps(_mm_cvtepi32_ps(shifted),inv));}i+=4;byte+=12;}
    if BE{unsafe{scalar_pcm_i24_be(&data[byte..],&mut out[i..],bits)}}else{unsafe{scalar_pcm_i24_le(&data[byte..],&mut out[i..],bits)}}
}

#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn pcm_i32_le_sse41(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_sse41::<false>(d,o,b)}}
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn pcm_i32_be_sse41(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_sse41::<true>(d,o,b)}}
#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn pcm_i32_sse41<const BE:bool>(data:&[u8],out:&mut[f32],bits:u16){let shift=(32-bits)as i32;let sc=_mm_cvtsi32_si128(shift);let inv=_mm_set1_ps((1.0/2.0_f64.powi(bits as i32-1))as f32);let swap=_mm_setr_epi8(3,2,1,0,7,6,5,4,11,10,9,8,15,14,13,12);let mut i=0;while i+4<=out.len(){let mut v=unsafe{_mm_loadu_si128(data.as_ptr().add(i*4)as*const __m128i)};if BE{v=_mm_shuffle_epi8(v,swap);}if shift!=0{v=_mm_sra_epi32(v,sc);}unsafe{_mm_storeu_ps(out.as_mut_ptr().add(i),_mm_mul_ps(_mm_cvtepi32_ps(v),inv));}i+=4;}if BE{unsafe{scalar_pcm_i32_be(&data[i*4..],&mut out[i..],bits)}}else{unsafe{scalar_pcm_i32_le(&data[i*4..],&mut out[i..],bits)}}}

#[target_feature(enable = "avx2")]
unsafe fn pcm_i32_le_avx2(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_avx2::<false>(d,o,b)}}
#[target_feature(enable = "avx2")]
unsafe fn pcm_i32_be_avx2(d:&[u8],o:&mut[f32],b:u16){unsafe{pcm_i32_avx2::<true>(d,o,b)}}
#[target_feature(enable = "avx2")]
unsafe fn pcm_i32_avx2<const BE:bool>(data:&[u8],out:&mut[f32],bits:u16){let inv=_mm256_set1_ps((1.0/2.0_f64.powi(bits as i32-1))as f32);let swap=_mm256_setr_epi8(3,2,1,0,7,6,5,4,11,10,9,8,15,14,13,12,3,2,1,0,7,6,5,4,11,10,9,8,15,14,13,12);let sc=_mm_cvtsi32_si128((32-bits)as i32);let mut i=0;while i+8<=out.len(){let mut v=unsafe{_mm256_loadu_si256(data.as_ptr().add(i*4)as*const __m256i)};if BE{v=_mm256_shuffle_epi8(v,swap);}if bits<32{v=_mm256_sra_epi32(v,sc);}unsafe{_mm256_storeu_ps(out.as_mut_ptr().add(i),_mm256_mul_ps(_mm256_cvtepi32_ps(v),inv));}i+=8;}if BE{unsafe{scalar_pcm_i32_be(&data[i*4..],&mut out[i..],bits)}}else{unsafe{scalar_pcm_i32_le(&data[i*4..],&mut out[i..],bits)}}}

#[target_feature(enable = "avx2")]
unsafe fn pcm_f32_le_avx2(data:&[u8],out:&mut[f32]){let lo=_mm256_set1_ps(-1.0);let hi=_mm256_set1_ps(1.0);let mut i=0;while i+8<=out.len(){let v=unsafe{_mm256_loadu_ps(data.as_ptr().add(i*4)as*const f32)};unsafe{_mm256_storeu_ps(out.as_mut_ptr().add(i),_mm256_min_ps(hi,_mm256_max_ps(lo,v)));}i+=8;}unsafe{scalar_pcm_f32_le(&data[i*4..],&mut out[i..])}}
#[target_feature(enable = "avx2")]
unsafe fn pcm_f64_le_avx2(data:&[u8],out:&mut[f32]){let lo=_mm_set1_ps(-1.0);let hi=_mm_set1_ps(1.0);let mut i=0;while i+4<=out.len(){let v=unsafe{_mm256_loadu_pd(data.as_ptr().add(i*8)as*const f64)};let f=_mm256_cvtpd_ps(v);unsafe{_mm_storeu_ps(out.as_mut_ptr().add(i),_mm_min_ps(hi,_mm_max_ps(lo,f)));}i+=4;}unsafe{scalar_pcm_f64_le(&data[i*8..],&mut out[i..])}}

#[target_feature(enable = "sse2")]
unsafe fn mul_sse2(l:&mut[f32],r:&[f32]){let mut i=0;while i+4<=l.len(){let a=unsafe{_mm_loadu_ps(l.as_ptr().add(i))};let b=unsafe{_mm_loadu_ps(r.as_ptr().add(i))};unsafe{_mm_storeu_ps(l.as_mut_ptr().add(i),_mm_mul_ps(a,b));}i+=4;}unsafe{scalar_mul(&mut l[i..],&r[i..])}}
#[target_feature(enable = "avx2")]
unsafe fn mul_avx2(l:&mut[f32],r:&[f32]){let mut i=0;while i+8<=l.len(){let a=unsafe{_mm256_loadu_ps(l.as_ptr().add(i))};let b=unsafe{_mm256_loadu_ps(r.as_ptr().add(i))};unsafe{_mm256_storeu_ps(l.as_mut_ptr().add(i),_mm256_mul_ps(a,b));}i+=8;}unsafe{mul_sse2(&mut l[i..],&r[i..])}}
#[target_feature(enable = "avx2")]
unsafe fn mix_add_avx2(d:&mut[f32],s:&[f32],g:f32){let vg=_mm256_set1_ps(g);let mut i=0;while i+8<=d.len(){let a=unsafe{_mm256_loadu_ps(d.as_ptr().add(i))};let b=unsafe{_mm256_loadu_ps(s.as_ptr().add(i))};unsafe{_mm256_storeu_ps(d.as_mut_ptr().add(i),_mm256_add_ps(a,_mm256_mul_ps(b,vg)));}i+=8;}unsafe{scalar_mix_add(&mut d[i..],&s[i..],g)}}

#[target_feature(enable = "sse2")]
unsafe fn overlap_sse2(c:&mut[f32],p:&[f32],w:&[f32]){let n=c.len();let mut i=0;while i+4<=n{let cv=unsafe{_mm_loadu_ps(c.as_ptr().add(i))};let pv=unsafe{_mm_loadu_ps(p.as_ptr().add(i))};let wf=unsafe{_mm_loadu_ps(w.as_ptr().add(i))};let rr=unsafe{_mm_loadu_ps(w.as_ptr().add(n-i-4))};let wr=_mm_shuffle_ps(rr,rr,0x1b);unsafe{_mm_storeu_ps(c.as_mut_ptr().add(i),_mm_add_ps(_mm_mul_ps(cv,wf),_mm_mul_ps(pv,wr)));}i+=4;}while i<n{c[i]=c[i]*w[i]+p[i]*w[n-1-i];i+=1;}}
#[target_feature(enable = "avx2")]
unsafe fn overlap_avx2(c:&mut[f32],p:&[f32],w:&[f32]){let n=c.len();let rev=_mm256_setr_epi32(7,6,5,4,3,2,1,0);let mut i=0;while i+8<=n{let cv=unsafe{_mm256_loadu_ps(c.as_ptr().add(i))};let pv=unsafe{_mm256_loadu_ps(p.as_ptr().add(i))};let wf=unsafe{_mm256_loadu_ps(w.as_ptr().add(i))};let rr=unsafe{_mm256_loadu_ps(w.as_ptr().add(n-i-8))};let wr=_mm256_permutevar8x32_ps(rr,rev);unsafe{_mm256_storeu_ps(c.as_mut_ptr().add(i),_mm256_add_ps(_mm256_mul_ps(cv,wf),_mm256_mul_ps(pv,wr)));}i+=8;}while i<n{c[i]=c[i]*w[i]+p[i]*w[n-1-i];i+=1;}}

#[target_feature(enable = "avx2")]
unsafe fn interleave_stereo_avx2(l:&[f32],r:&[f32],o:&mut[f32]){let mut i=0;while i+8<=l.len(){let a=unsafe{_mm256_loadu_ps(l.as_ptr().add(i))};let b=unsafe{_mm256_loadu_ps(r.as_ptr().add(i))};let lo=_mm256_unpacklo_ps(a,b);let hi=_mm256_unpackhi_ps(a,b);let first=_mm256_permute2f128_ps(lo,hi,0x20);let second=_mm256_permute2f128_ps(lo,hi,0x31);unsafe{_mm256_storeu_ps(o.as_mut_ptr().add(i*2),first);_mm256_storeu_ps(o.as_mut_ptr().add(i*2+8),second);}i+=8;}unsafe{scalar_interleave_stereo(&l[i..],&r[i..],&mut o[i*2..])}}
#[target_feature(enable = "avx2")]
unsafe fn deinterleave_stereo_avx2(input:&[f32],l:&mut[f32],r:&mut[f32]){let idx=_mm256_setr_epi32(0,2,4,6,1,3,5,7);let mut i=0;while i+8<=l.len(){let a=unsafe{_mm256_loadu_ps(input.as_ptr().add(i*2))};let b=unsafe{_mm256_loadu_ps(input.as_ptr().add(i*2+8))};let pa=_mm256_permutevar8x32_ps(a,idx);let pb=_mm256_permutevar8x32_ps(b,idx);let lv=_mm256_permute2f128_ps(pa,pb,0x20);let rv=_mm256_permute2f128_ps(pa,pb,0x31);unsafe{_mm256_storeu_ps(l.as_mut_ptr().add(i),lv);_mm256_storeu_ps(r.as_mut_ptr().add(i),rv);}i+=8;}unsafe{scalar_deinterleave_stereo(&input[i*2..],&mut l[i..],&mut r[i..])}}

#[target_feature(enable = "avx2,fma")]
unsafe fn lerp_fma(a:&[f32],b:&[f32],t:f32,o:&mut[f32]){let vt=_mm256_set1_ps(t);let mut i=0;while i+8<=o.len(){let av=unsafe{_mm256_loadu_ps(a.as_ptr().add(i))};let bv=unsafe{_mm256_loadu_ps(b.as_ptr().add(i))};let v=_mm256_fmadd_ps(_mm256_sub_ps(bv,av),vt,av);unsafe{_mm256_storeu_ps(o.as_mut_ptr().add(i),v);}i+=8;}unsafe{scalar_lerp(&a[i..],&b[i..],t,&mut o[i..])}}
#[target_feature(enable = "avx2,fma")]
unsafe fn butterfly_fma(a:&mut[f32],b:&mut[f32],f:f32){let vf=_mm256_set1_ps(f);let mut i=0;while i+8<=a.len(){let av=unsafe{_mm256_loadu_ps(a.as_ptr().add(i))};let bv=unsafe{_mm256_loadu_ps(b.as_ptr().add(i))};let add=_mm256_fmadd_ps(bv,vf,av);let sub=_mm256_fnmadd_ps(bv,vf,av);unsafe{_mm256_storeu_ps(a.as_mut_ptr().add(i),add);_mm256_storeu_ps(b.as_mut_ptr().add(i),sub);}i+=8;}unsafe{scalar_butterfly(&mut a[i..],&mut b[i..],f)}}

#[target_feature(enable = "sse2")]
unsafe fn gain_sse2(s:&mut[f32],g:f32){let vg=_mm_set1_ps(g);let mut i=0;while i+4<=s.len(){let v=unsafe{_mm_loadu_ps(s.as_ptr().add(i))};unsafe{_mm_storeu_ps(s.as_mut_ptr().add(i),_mm_mul_ps(v,vg));}i+=4;}unsafe{scalar_gain(&mut s[i..],g)}}
#[target_feature(enable = "avx2")]
unsafe fn gain_avx2(s:&mut[f32],g:f32){let vg=_mm256_set1_ps(g);let mut i=0;while i+8<=s.len(){let v=unsafe{_mm256_loadu_ps(s.as_ptr().add(i))};unsafe{_mm256_storeu_ps(s.as_mut_ptr().add(i),_mm256_mul_ps(v,vg));}i+=8;}unsafe{gain_sse2(&mut s[i..],g)}}
#[target_feature(enable = "sse2")]
unsafe fn limiter_sse2(s:&mut[f32]){let lo=_mm_set1_ps(-1.0);let hi=_mm_set1_ps(1.0);let zero=_mm_setzero_ps();let mut i=0;while i+4<=s.len(){let v=unsafe{_mm_loadu_ps(s.as_ptr().add(i))};let c=_mm_min_ps(hi,_mm_max_ps(lo,v));let ord=_mm_cmpord_ps(v,v);let clean=_mm_or_ps(_mm_and_ps(ord,c),_mm_andnot_ps(ord,zero));unsafe{_mm_storeu_ps(s.as_mut_ptr().add(i),clean);}i+=4;}unsafe{scalar_limiter(&mut s[i..])}}
#[target_feature(enable = "avx2")]
unsafe fn limiter_avx2(s:&mut[f32]){let lo=_mm256_set1_ps(-1.0);let hi=_mm256_set1_ps(1.0);let zero=_mm256_setzero_ps();let mut i=0;while i+8<=s.len(){let v=unsafe{_mm256_loadu_ps(s.as_ptr().add(i))};let c=_mm256_min_ps(hi,_mm256_max_ps(lo,v));let ord=_mm256_cmp_ps(v,v,_CMP_ORD_Q);let clean=_mm256_blendv_ps(zero,c,ord);unsafe{_mm256_storeu_ps(s.as_mut_ptr().add(i),clean);}i+=8;}unsafe{limiter_sse2(&mut s[i..])}}

#[target_feature(enable = "avx2")]
unsafe fn stereo_average_avx2(input:&[f32],out:&mut[f32]){let idx=_mm256_setr_epi32(0,2,4,6,1,3,5,7);let half=_mm256_set1_ps(0.5);let mut i=0;while i+8<=out.len(){let a=unsafe{_mm256_loadu_ps(input.as_ptr().add(i*2))};let b=unsafe{_mm256_loadu_ps(input.as_ptr().add(i*2+8))};let pa=_mm256_permutevar8x32_ps(a,idx);let pb=_mm256_permutevar8x32_ps(b,idx);let l=_mm256_permute2f128_ps(pa,pb,0x20);let r=_mm256_permute2f128_ps(pa,pb,0x31);unsafe{_mm256_storeu_ps(out.as_mut_ptr().add(i),_mm256_mul_ps(_mm256_add_ps(l,r),half));}i+=8;}unsafe{scalar_stereo_average(&input[i*2..],&mut out[i..])}}
#[target_feature(enable = "avx2")]
unsafe fn mono_duplicate_avx2(input:&[f32],out:&mut[f32]){unsafe{interleave_stereo_avx2(input,input,out)}}


#[target_feature(enable = "avx2")]
unsafe fn flac_left_side_avx2(left:&[i64], right:&mut[i64]) {
    let mut i=0usize;
    while i+4<=left.len(){
        let l=unsafe{_mm256_loadu_si256(left.as_ptr().add(i) as *const __m256i)};
        let s=unsafe{_mm256_loadu_si256(right.as_ptr().add(i) as *const __m256i)};
        unsafe{_mm256_storeu_si256(right.as_mut_ptr().add(i) as *mut __m256i,_mm256_sub_epi64(l,s));}
        i+=4;
    }
    unsafe{scalar_flac_left_side(&left[i..],&mut right[i..])}
}

#[target_feature(enable = "avx2")]
unsafe fn flac_right_side_avx2(left:&mut[i64], right:&[i64]) {
    let mut i=0usize;
    while i+4<=left.len(){
        let s=unsafe{_mm256_loadu_si256(left.as_ptr().add(i) as *const __m256i)};
        let r=unsafe{_mm256_loadu_si256(right.as_ptr().add(i) as *const __m256i)};
        unsafe{_mm256_storeu_si256(left.as_mut_ptr().add(i) as *mut __m256i,_mm256_add_epi64(s,r));}
        i+=4;
    }
    unsafe{scalar_flac_right_side(&mut left[i..],&right[i..])}
}

#[target_feature(enable = "avx2")]
unsafe fn complex_butterfly_avx2(ar:&mut[f32],ai:&mut[f32],br:&mut[f32],bi:&mut[f32],c:f32,s:f32){
    let cv=_mm256_set1_ps(c);let sv=_mm256_set1_ps(s);let mut i=0usize;
    while i+8<=ar.len(){
        let xr=unsafe{_mm256_loadu_ps(ar.as_ptr().add(i))};let xi=unsafe{_mm256_loadu_ps(ai.as_ptr().add(i))};
        let yr=unsafe{_mm256_loadu_ps(br.as_ptr().add(i))};let yi=unsafe{_mm256_loadu_ps(bi.as_ptr().add(i))};
        let tr=_mm256_sub_ps(_mm256_mul_ps(yr,cv),_mm256_mul_ps(yi,sv));
        let ti=_mm256_add_ps(_mm256_mul_ps(yr,sv),_mm256_mul_ps(yi,cv));
        unsafe{_mm256_storeu_ps(ar.as_mut_ptr().add(i),_mm256_add_ps(xr,tr));_mm256_storeu_ps(ai.as_mut_ptr().add(i),_mm256_add_ps(xi,ti));_mm256_storeu_ps(br.as_mut_ptr().add(i),_mm256_sub_ps(xr,tr));_mm256_storeu_ps(bi.as_mut_ptr().add(i),_mm256_sub_ps(xi,ti));}
        i+=8;
    }
    unsafe{scalar_complex_butterfly(&mut ar[i..],&mut ai[i..],&mut br[i..],&mut bi[i..],c,s)}
}

#[target_feature(enable = "avx2")]
unsafe fn vorbis_couple_avx2(m:&mut[f32],a:&mut[f32]){let zero=_mm256_setzero_ps();let mut i=0usize;while i+8<=m.len(){let mv=unsafe{_mm256_loadu_ps(m.as_ptr().add(i))};let av=unsafe{_mm256_loadu_ps(a.as_ptr().add(i))};let mp=_mm256_cmp_ps::<_CMP_GT_OQ>(mv,zero);let ap=_mm256_cmp_ps::<_CMP_GT_OQ>(av,zero);let mplus=_mm256_add_ps(mv,av);let mminus=_mm256_sub_ps(mv,av);let a_when_pos=_mm256_blendv_ps(mplus,mminus,mp);let na=_mm256_blendv_ps(mv,a_when_pos,ap);let m_when_nonpos=_mm256_blendv_ps(mminus,mplus,mp);let nm=_mm256_blendv_ps(m_when_nonpos,mv,ap);unsafe{_mm256_storeu_ps(m.as_mut_ptr().add(i),nm);_mm256_storeu_ps(a.as_mut_ptr().add(i),na);}i+=8;}unsafe{scalar_vorbis_couple(&mut m[i..],&mut a[i..])}}
