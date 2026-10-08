#![allow(
    clippy::all,
    non_camel_case_types,
    non_snake_case,
    unused_assignments
)]
#![deny(unsafe_op_in_unsafe_fn)]

use crate::codecs::mpeg::tables::*;
use crate::codecs::support::StageTimer;
use std::sync::OnceLock;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86_simd;


#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
mod arm_simd;

type MidsideFn = unsafe fn(&mut [f32], &mut [f32], usize);
type IntensityFn = unsafe fn(&mut [f32], &mut [f32], usize, f32, f32);
type AntialiasFn = unsafe fn(&mut [f32], usize, &[f32; 8], &[f32; 8]);
type ChangeSignFn = unsafe fn(&mut [f32]);
type SynthWindow4Fn = unsafe fn(&[f32], i32, usize, usize, &[f32]) -> ([f32; 4], [f32; 4]);
type ScaleFn = unsafe fn(&mut [f32], f32);

#[derive(Clone, Copy)]
struct Mp3Simd {
    midside: MidsideFn,
    intensity: IntensityFn,
    antialias: AntialiasFn,
    change_sign: ChangeSignFn,
    synth_window4: SynthWindow4Fn,
    scale: ScaleFn,
}

static MP3_SIMD: OnceLock<Mp3Simd> = OnceLock::new();

unsafe fn scale_scalar(samples: &mut [f32], gain: f32) {
    for sample in samples { *sample *= gain; }
}

#[inline]
fn mp3_simd() -> Mp3Simd {
    *MP3_SIMD.get_or_init(select_mp3_simd)
}

fn select_mp3_simd() -> Mp3Simd {
    // CPU probing happens exactly once. Every hot MP3 routine subsequently
    // performs a single indirect call through this cached dispatch table.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return Mp3Simd {
                midside: x86_simd::midside_avx,
                intensity: x86_simd::intensity_avx,
                antialias: x86_simd::antialias_avx2,
                change_sign: x86_simd::change_sign_avx,
                synth_window4: x86_simd::synth_window4_sse2,
                scale: x86_simd::scale_avx,
            };
        }
        if std::arch::is_x86_feature_detected!("avx") {
            return Mp3Simd {
                midside: x86_simd::midside_avx,
                intensity: x86_simd::intensity_avx,
                antialias: scalar_antialias,
                change_sign: x86_simd::change_sign_avx,
                synth_window4: x86_simd::synth_window4_sse2,
                scale: x86_simd::scale_avx,
            };
        }
        if std::arch::is_x86_feature_detected!("sse2") {
            return Mp3Simd {
                midside: x86_simd::midside_sse2,
                intensity: x86_simd::intensity_sse2,
                antialias: scalar_antialias,
                change_sign: x86_simd::change_sign_sse2,
                synth_window4: x86_simd::synth_window4_sse2,
                scale: scale_scalar,
            };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return Mp3Simd {
                midside: arm_simd::midside_neon,
                intensity: arm_simd::intensity_neon,
                antialias: arm_simd::antialias_neon,
                change_sign: arm_simd::change_sign_neon,
                synth_window4: arm_simd::synth_window4_neon,
                scale: arm_simd::scale_neon,
            };
        }
    }

    #[cfg(target_arch = "arm")]
    {
        if std::arch::is_arm_feature_detected!("neon") {
            return Mp3Simd {
                midside: arm_simd::midside_neon,
                intensity: arm_simd::intensity_neon,
                antialias: arm_simd::antialias_neon,
                change_sign: arm_simd::change_sign_neon,
                synth_window4: arm_simd::synth_window4_neon,
                scale: arm_simd::scale_neon,
            };
        }
    }

    Mp3Simd {
        midside: scalar_midside,
        intensity: scalar_intensity,
        antialias: scalar_antialias,
        change_sign: scalar_change_sign,
        synth_window4: scalar_synth_window4,
        scale: scale_scalar,
    }
}

unsafe fn scalar_midside(left: &mut [f32], right: &mut [f32], n: usize) {
    for i in 0..n {
        let a = left[i];
        let b = right[i];
        left[i] = a + b;
        right[i] = a - b;
    }
}

unsafe fn scalar_intensity(
    left: &mut [f32],
    right: &mut [f32],
    n: usize,
    kl: f32,
    kr: f32,
) {
    for i in 0..n {
        let value = left[i];
        right[i] = value * kr;
        left[i] = value * kl;
    }
}

unsafe fn scalar_antialias(
    grbuf: &mut [f32],
    bands: usize,
    aa0: &[f32; 8],
    aa1: &[f32; 8],
) {
    for band in 0..bands {
        let base = band * 18;
        for i in 0..8usize {
            let u = grbuf[base + 18 + i];
            let d = grbuf[base + 17 - i];
            grbuf[base + 18 + i] = u * aa0[i] - d * aa1[i];
            grbuf[base + 17 - i] = u * aa1[i] + d * aa0[i];
        }
    }
}

unsafe fn scalar_change_sign(grbuf: &mut [f32]) {
    for block in 0..16usize {
        let base = 18 + block * 36;
        for i in (1..18usize).step_by(2) {
            grbuf[base + i] = -grbuf[base + i];
        }
    }
}

unsafe fn scalar_synth_window4(
    lins: &[f32],
    i: i32,
    zlin: usize,
    win_start: usize,
    win: &[f32],
) -> ([f32; 4], [f32; 4]) {
    let mut a = [0.0f32; 4];
    let mut b = [0.0f32; 4];
    for stage in 0..8i32 {
        let w0 = win[win_start + stage as usize * 2];
        let w1 = win[win_start + stage as usize * 2 + 1];
        let vz = (zlin as i32 + 4 * i - stage * 64) as usize;
        let vy = (zlin as i32 + 4 * i - (15 - stage) * 64) as usize;
        for j in 0..4usize {
            let vzv = lins[vz + j];
            let vyv = lins[vy + j];
            b[j] += vzv * w1 + vyv * w0;
            if stage & 1 == 0 {
                a[j] += vzv * w0 - vyv * w1;
            } else {
                a[j] += vyv * w1 - vzv * w0;
            }
        }
    }
    (a, b)
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct mp3dec_frame_info_t {
    pub frame_bytes: usize,
    pub frame_offset: usize,
    pub channels: u32,
    pub hz: i32,
    pub layer: u8,
    pub bitrate_kbps: i32,
}
#[derive(Copy, Clone)]
#[repr(C)]
pub struct mp3dec_t {
    mdct_overlap: [[f32; 288]; 2],
    qmf_state: [f32; 960],
    reserv: i32,
    free_format_bytes: usize,
    header: [u8; 4],
    reserv_buf: [u8; 511],
}

impl mp3dec_t {
    pub const fn new() -> Self {
        Self {
            mdct_overlap: [[0.; 288]; 2],
            qmf_state: [0.; 960],
            reserv: 0,
            free_format_bytes: 0,
            header: [0; 4],
            reserv_buf: [0; 511]
        }
    }
}

type mp3d_sample_t = f32;
#[derive(Copy, Clone)]
#[repr(C)]
pub(crate) struct mp3dec_scratch_t {
    grbuf: [[f32; 576]; 2],
    scf: [f32; 40],
    syn: [[f32; 64]; 33],
    ist_pos: [[u8; 39]; 2],
    maindata: [u8; 2815],
}

impl mp3dec_scratch_t {
    /// Persistent Layer III work area. The wrapper owns one of these per
    /// decoder, so the ~12 KiB transform/Huffman workspace is not recreated
    /// on every compressed frame.
    pub(crate) const fn new() -> Self {
        Self {
            grbuf: [[0.0; 576]; 2],
            scf: [0.0; 40],
            syn: [[0.0; 64]; 33],
            ist_pos: [[0; 39]; 2],
            maindata: [0; 2815],
        }
    }
}
/// Borrowed Layer III work areas. Deliberately excludes `maindata`, which is
/// read by the bit reservoir for the duration of a granule decode.
struct Layer3Scratch<'a> {
    grbuf: &'a mut [[f32; 576]; 2],
    scf: &'a mut [f32; 40],
    syn: &'a mut [[f32; 64]; 33],
    ist_pos: &'a mut [[u8; 39]; 2],
}

#[derive(Copy, Clone, Debug, Default)]
enum SfbTable {
    #[default]
    Long0,
    Long(usize),
    Short(usize),
    Mixed(usize),
}

impl SfbTable {
    #[inline(always)]
    fn as_slice(self) -> &'static [u8] {
        match self {
            Self::Long0 => &L3_READ_SIDE_INFO_G_SCF_LONG[0],
            Self::Long(index) => &L3_READ_SIDE_INFO_G_SCF_LONG[index],
            Self::Short(index) => &L3_READ_SIDE_INFO_G_SCF_SHORT[index],
            Self::Mixed(index) => &L3_READ_SIDE_INFO_G_SCF_MIXED[index],
        }
    }
}

#[derive(Copy, Clone, Default)]
struct L3_gr_info_t {
    sfbtab: SfbTable,
    part_23_length: u16,
    big_values: u16,
    scalefac_compress: u16,
    global_gain: u8,
    block_type: u8,
    mixed_block_flag: u8,
    n_long_sfb: u8,
    n_short_sfb: u8,
    table_select: [u8; 3],
    region_count: [u8; 3],
    subblock_gain: [u8; 3],
    preflag: u8,
    scalefac_scale: u8,
    count1_table: u8,
    scfsi: u8,
}
#[derive(Copy, Clone)]
#[repr(C)]
struct bs_t<'a> {
    buf: &'a [u8],
    pos: i32,
    limit: i32
}
fn bs_init(
    data: &[u8],
    bytes: i32
) -> bs_t<'_> {
    bs_t {
        buf: data,
        pos: 0,
        limit: bytes * 8
    }
}

fn get_bits(bs: &mut bs_t, n: i32) -> u32 {
    if n <= 0 {
        return 0;
    }

    let old_pos = bs.pos;
    let new_pos = old_pos.saturating_add(n);
    bs.pos = new_pos;
    if new_pos > bs.limit || old_pos < 0 {
        return 0;
    }

    let s = (old_pos & 7) as u32;
    let mut shl = n + s as i32;
    let mut byte_index = (old_pos >> 3) as usize;
    let mut next = (bs.buf.get(byte_index).copied().unwrap_or(0) as u32) & (0xff >> s);
    byte_index += 1;
    let mut cache = 0u32;

    loop {
        shl -= 8;
        if shl <= 0 {
            break;
        }
        cache |= next << shl;
        next = bs.buf.get(byte_index).copied().unwrap_or(0) as u32;
        byte_index += 1;
    }

    cache | (next >> (-shl as u32))
}

fn hdr_valid(h: &[u8]) -> bool {
    h[0] == 0xff &&
        (h[1] & 0xf0 == 0xf0 || h[1] & 0xfe == 0xe2) &&
        h[1] >> 1 & 3 != 0 &&
        h[2] >> 4 != 15 &&
        h[2] >> 2 & 3 != 3
}

fn hdr_compare(
    h1: &[u8],
    h2: &[u8],
) -> bool {
    hdr_valid(h2) &&
        (h1[1] ^ h2[1]) & 0xfe == 0 &&
        (h1[2] ^ h2[2]) & 0xc == 0 &&
        (h1[2] & 0xf0 == 0) == (h2[2] & 0xf0 == 0)
}

fn hdr_bitrate_kbps(h: &[u8]) -> u32 {
    2 * (HDR_BITRATE_KBPS_HALFRATE
        [(h[1] & 0x8 != 0) as usize]
        [((h[1] >> 1 & 3) - 1) as usize]
        [(h[2] >> 4) as usize] as u32)
}

fn hdr_sample_rate_hz(h: &[u8]) -> u32 {
    HDR_SAMPLE_RATE_HZ_G_HZ[(h[2] >> 2 & 3) as usize]
        >> (h[1] & 0x8 == 0) as u32
        >> (h[1] & 0x10 == 0) as u32
}

fn hdr_frame_samples(h: &[u8]) -> u32 {
    if h[1] & 6 == 6 {
        384
    } else {
        1152 >> (h[1] & 14 == 2) as u32
    }
}

fn hdr_frame_bytes(
    h: &[u8],
    free_format_size: usize,
) -> usize {
    let mut frame_bytes = (hdr_frame_samples(h))
        .wrapping_mul(hdr_bitrate_kbps(h))
        .wrapping_mul(125)
        .wrapping_div(hdr_sample_rate_hz(h)) as usize;
    if h[1] & 6 == 6 {
        frame_bytes &= !3;
    }
    if frame_bytes != 0 { frame_bytes } else { free_format_size }
}

fn hdr_padding(h: &[u8]) -> usize {
    if h[2] & 0x2 != 0 {
        if h[1] & 6 == 6 {
            4
        } else {
            1
        }
    } else {
        0
    }
}

#[derive(Copy, Clone)]
struct L12_scale_info {
    scf: [f32; 3 * 64],
    total_bands: u8,
    stereo_bands: u8,
    bitalloc: [u8; 64],
    scfcod: [u8; 64],
}

impl L12_scale_info {
    const fn new() -> Self {
        Self {
            scf: [0.0; 3 * 64],
            total_bands: 0,
            stereo_bands: 0,
            bitalloc: [0; 64],
            scfcod: [0; 64],
        }
    }
}

#[derive(Copy, Clone)]
struct L12_subband_alloc_t {
    tab_offset: u8,
    code_tab_width: u8,
    band_count: u8,
}

const L12_ALLOC_L1: [L12_subband_alloc_t; 1] = [
    L12_subband_alloc_t {
        tab_offset: 76,
        code_tab_width: 4,
        band_count: 32,
    },
];

const L12_ALLOC_L2_MPEG2: [L12_subband_alloc_t; 3] = [
    L12_subband_alloc_t {
        tab_offset: 60,
        code_tab_width: 4,
        band_count: 4,
    },
    L12_subband_alloc_t {
        tab_offset: 44,
        code_tab_width: 3,
        band_count: 7,
    },
    L12_subband_alloc_t {
        tab_offset: 44,
        code_tab_width: 2,
        band_count: 19,
    },
];

const L12_ALLOC_L2_MPEG1: [L12_subband_alloc_t; 4] = [
    L12_subband_alloc_t {
        tab_offset: 0,
        code_tab_width: 4,
        band_count: 3,
    },
    L12_subband_alloc_t {
        tab_offset: 16,
        code_tab_width: 4,
        band_count: 8,
    },
    L12_subband_alloc_t {
        tab_offset: 32,
        code_tab_width: 3,
        band_count: 12,
    },
    L12_subband_alloc_t {
        tab_offset: 40,
        code_tab_width: 2,
        band_count: 7,
    },
];

const L12_ALLOC_L2_MPEG1_LOW_RATE: [L12_subband_alloc_t; 2] = [
    L12_subband_alloc_t {
        tab_offset: 44,
        code_tab_width: 4,
        band_count: 2,
    },
    L12_subband_alloc_t {
        tab_offset: 44,
        code_tab_width: 3,
        band_count: 10,
    },
];

const L12_BITALLOC_CODE_TAB: [u8; 92] = [
    0, 17, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    0, 17, 18, 3, 19, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 16,
    0, 17, 18, 3, 19, 4, 5, 16,
    0, 17, 18, 16,
    0, 17, 18, 19, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    0, 17, 18, 3, 19, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
    0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
];

const L12_DEQUANT_DENOMINATORS: [u32; 18] = [
    3, 7, 15, 31, 63, 127, 255, 511, 1023,
    2047, 4095, 8191, 16383, 32767, 65535,
    3, 5, 9,
];

#[inline(always)]
fn hdr_is_layer_1(hdr: &[u8]) -> bool {
    hdr[1] & 6 == 6
}

#[inline(always)]
fn hdr_is_mpeg1(hdr: &[u8]) -> bool {
    hdr[1] & 0x8 != 0
}

#[inline(always)]
fn hdr_stereo_mode(hdr: &[u8]) -> u8 {
    (hdr[3] >> 6) & 3
}

#[inline(always)]
fn hdr_stereo_mode_ext(hdr: &[u8]) -> u8 {
    (hdr[3] >> 4) & 3
}

fn L12_subband_alloc_table(
    hdr: &[u8],
    sci: &mut L12_scale_info,
) -> &'static [L12_subband_alloc_t] {
    const MODE_MONO: u8 = 3;
    const MODE_JOINT_STEREO: u8 = 1;

    let mode = hdr_stereo_mode(hdr);
    let stereo_bands = if mode == MODE_MONO {
        0usize
    } else if mode == MODE_JOINT_STEREO {
        ((hdr_stereo_mode_ext(hdr) as usize) << 2) + 4
    } else {
        32
    };

    let (alloc, total_bands): (&'static [L12_subband_alloc_t], usize) =
        if hdr_is_layer_1(hdr) {
            (&L12_ALLOC_L1, 32)
        } else if !hdr_is_mpeg1(hdr) {
            (&L12_ALLOC_L2_MPEG2, 30)
        } else {
            let sample_rate_idx = ((hdr[2] >> 2) & 3) as usize;
            let mut kbps = hdr_bitrate_kbps(hdr)
                >> usize::from(mode != MODE_MONO);

            // The reference decoder selects the normal MPEG-1 Layer II
            // table at 192 kbps/channel for free-format streams.
            if kbps == 0 {
                kbps = 192;
            }

            if kbps < 56 {
                (
                    &L12_ALLOC_L2_MPEG1_LOW_RATE,
                    if sample_rate_idx == 2 { 12 } else { 8 },
                )
            } else {
                (
                    &L12_ALLOC_L2_MPEG1,
                    if kbps >= 96 && sample_rate_idx != 1 {
                        30
                    } else {
                        27
                    },
                )
            }
        };

    sci.total_bands = total_bands as u8;
    sci.stereo_bands = stereo_bands.min(total_bands) as u8;
    alloc
}

#[inline(always)]
fn L12_dequant_scale(ba: usize, selector: usize) -> f32 {
    let denom_index = ba.saturating_sub(2).min(L12_DEQUANT_DENOMINATORS.len() - 1);
    let denominator = L12_DEQUANT_DENOMINATORS[denom_index] as f32;
    match selector {
        0 => 9.53674316e-07f32 / denominator,
        1 => 7.56931807e-07f32 / denominator,
        _ => 6.00777173e-07f32 / denominator,
    }
}

fn L12_read_scalefactors(
    bs: &mut bs_t,
    bitalloc: &[u8],
    scfcod: &[u8],
    bands: usize,
    scf: &mut [f32],
) {
    for i in 0..bands.min(bitalloc.len()).min(scfcod.len()) {
        let ba = bitalloc[i] as usize;
        let mask = if ba != 0 {
            4 + ((19u32 >> scfcod[i]) & 3) as usize
        } else {
            0
        };

        let mut scale = 0.0f32;
        for (slot, m) in [4usize, 2, 1].into_iter().enumerate() {
            if mask & m != 0 {
                let b = get_bits(bs, 6) as usize;
                let deq = L12_dequant_scale(ba, b % 3);
                let shift = 21usize.saturating_sub(b / 3);
                scale = deq * ((1u32 << shift) as f32);
            }

            if let Some(out) = scf.get_mut(i * 3 + slot) {
                *out = scale;
            }
        }
    }
}

fn L12_read_scale_info(
    hdr: &[u8],
    bs: &mut bs_t,
    sci: &mut L12_scale_info,
) {
    let subband_alloc = L12_subband_alloc_table(hdr, sci);

    let total_bands = sci.total_bands as usize;
    let stereo_bands = sci.stereo_bands as usize;

    let mut alloc_index = 0usize;
    let mut next_alloc_band = 0usize;
    let mut ba_bits = 0i32;
    let mut ba_offset = 0usize;

    for band in 0..total_bands {
        if band == next_alloc_band {
            if let Some(alloc) = subband_alloc.get(alloc_index) {
                next_alloc_band += alloc.band_count as usize;
                ba_bits = alloc.code_tab_width as i32;
                ba_offset = alloc.tab_offset as usize;
                alloc_index += 1;
            }
        }

        let code_index = get_bits(bs, ba_bits) as usize;
        let mut ba = L12_BITALLOC_CODE_TAB
            .get(ba_offset + code_index)
            .copied()
            .unwrap_or(0);

        sci.bitalloc[2 * band] = ba;

        if band < stereo_bands {
            let code_index = get_bits(bs, ba_bits) as usize;
            ba = L12_BITALLOC_CODE_TAB
                .get(ba_offset + code_index)
                .copied()
                .unwrap_or(0);
        }

        sci.bitalloc[2 * band + 1] = if stereo_bands != 0 { ba } else { 0 };
    }

    for i in 0..(2 * total_bands) {
        sci.scfcod[i] = if sci.bitalloc[i] != 0 {
            if hdr_is_layer_1(hdr) {
                2
            } else {
                get_bits(bs, 2) as u8
            }
        } else {
            6
        };
    }

    L12_read_scalefactors(
        bs,
        &sci.bitalloc,
        &sci.scfcod,
        2 * total_bands,
        &mut sci.scf,
    );

    // Above the joint-stereo boundary, samples are coded once and copied
    // to the right channel before applying channel-specific scale factors.
    for band in stereo_bands..total_bands {
        sci.bitalloc[2 * band + 1] = 0;
    }
}

fn L12_dequantize_granule(
    grbuf: &mut [f32],
    bs: &mut bs_t,
    sci: &L12_scale_info,
    group_size: usize,
) -> usize {
    let total_bands = sci.total_bands as usize;

    for group in 0..4usize {
        for coded_channel_band in 0..(2 * total_bands) {
            let ba = sci.bitalloc[coded_channel_band] as usize;
            if ba == 0 {
                continue;
            }

            let channel = coded_channel_band & 1;
            let band = coded_channel_band >> 1;
            let base = channel * 576 + band * 18 + group_size * group;

            if ba < 17 {
                let half = (1i32 << (ba - 1)) - 1;
                for k in 0..group_size {
                    let value = get_bits(bs, ba as i32) as i32 - half;
                    if let Some(dst) = grbuf.get_mut(base + k) {
                        *dst = value as f32;
                    }
                }
            } else {
                let modulus = (2u32 << (ba - 17)) + 1; // 3, 5 or 9
                let bit_count = modulus + 2 - (modulus >> 3); // 5, 7 or 10
                let mut code = get_bits(bs, bit_count as i32);

                for k in 0..group_size {
                    let value = (code % modulus) as i32 - (modulus / 2) as i32;
                    if let Some(dst) = grbuf.get_mut(base + k) {
                        *dst = value as f32;
                    }
                    code /= modulus;
                }
            }
        }
    }

    group_size * 4
}

fn L12_apply_scf_384(
    sci: &L12_scale_info,
    scf_phase: usize,
    dst: &mut [f32],
) {
    let total_bands = sci.total_bands as usize;
    let stereo_bands = sci.stereo_bands as usize;

    // Joint-stereo bands share the quantized sample values. Duplicate the
    // left-channel values to the right channel before applying each
    // channel's independent scalefactor.
    for band in stereo_bands..total_bands {
        let left = band * 18;
        let right = 576 + band * 18;

        if left + 18 <= dst.len() && right + 18 <= dst.len() {
            let mut tmp = [0.0f32; 18];
            tmp.copy_from_slice(&dst[left..left + 18]);
            dst[right..right + 18].copy_from_slice(&tmp);
        }
    }

    for band in 0..total_bands {
        let left_scale = sci.scf
            .get(band * 6 + scf_phase)
            .copied()
            .unwrap_or(0.0);
        let right_scale = sci.scf
            .get(band * 6 + 3 + scf_phase)
            .copied()
            .unwrap_or(0.0);

        let left = band * 18;
        let right = 576 + band * 18;

        for k in 0..12usize {
            if let Some(sample) = dst.get_mut(left + k) {
                *sample *= left_scale;
            }
            if let Some(sample) = dst.get_mut(right + k) {
                *sample *= right_scale;
            }
        }
    }
}

fn L3_read_side_info(
    bs: &mut bs_t,
    mut gr: &mut [L3_gr_info_t],
    hdr: &[u8],
) -> i32 {
    let mut tables: u32 = 0;
    let mut scfsi: u32 = 0 as i32 as u32;
    let mut main_data_begin: i32 = 0;
    let mut part_23_sum: i32 = 0 as i32;
    let mut sr_idx: i32 = (hdr[2] as i32
        >> 2 as i32 & 3 as i32)
        + ((hdr[1] as i32 >> 3 as i32
            & 1 as i32)
            + (hdr[1] as i32 >> 4 as i32
                & 1 as i32)) * 3 as i32;
    sr_idx -= (sr_idx != 0 as i32) as i32;
    let mut gr_count: i32 = if hdr[3] & 0xc0 == 0xc0 {
        1
    } else {
        2
    };
    if hdr[1] & 0x8 != 0 {
        gr_count *= 2 as i32;
        main_data_begin = get_bits(bs, 9 as i32) as i32;
        scfsi = get_bits(bs, 7 as i32 + gr_count);
    } else {
        main_data_begin = (get_bits(bs, 8 as i32 + gr_count) >> gr_count)
            as i32;
    }
    loop {
        if hdr[3] & 0xc0 == 0xc0 {
            scfsi <<= 4 as i32;
        }
        gr[0].part_23_length = get_bits(bs, 12 as i32) as u16;
        part_23_sum += gr[0].part_23_length as i32;
        gr[0].big_values = get_bits(bs, 9 as i32) as u16;
        if gr[0].big_values as i32 > 288 as i32 {
            return -(1 as i32);
        }
        gr[0].global_gain = get_bits(bs, 8 as i32) as u8;
        gr[0]
            .scalefac_compress = get_bits(
            bs,
            if hdr[1] & 0x8 != 0 {
                4 as i32
            } else {
                9 as i32
            },
        ) as u16;
        gr[0].sfbtab = SfbTable::Long(sr_idx as usize);
        gr[0].n_long_sfb = 22 as i32 as u8;
        gr[0].n_short_sfb = 0 as i32 as u8;
        if get_bits(bs, 1 as i32) != 0 {
            gr[0].block_type = get_bits(bs, 2 as i32) as u8;
            if gr[0].block_type == 0 {
                return -(1 as i32);
            }
            gr[0].mixed_block_flag = get_bits(bs, 1 as i32) as u8;
            gr[0].region_count[0 as i32 as usize] = 7 as i32 as u8;
            gr[0]
                .region_count[1 as i32 as usize] = 255 as i32 as u8;
            if gr[0].block_type as i32 == 2 as i32 {
                scfsi &= 0xf0f as i32 as u32;
                if gr[0].mixed_block_flag == 0 {
                    gr[0]
                        .region_count[0 as i32
                        as usize] = 8 as i32 as u8;
                    gr[0].sfbtab = SfbTable::Short(sr_idx as usize);
                    gr[0].n_long_sfb = 0 as i32 as u8;
                    gr[0].n_short_sfb = 39 as i32 as u8;
                } else {
                    gr[0].sfbtab = SfbTable::Mixed(sr_idx as usize);
                    gr[0]
                        .n_long_sfb = (if hdr[1] & 0x8 != 0 {
                        8 as i32
                    } else {
                        6 as i32
                    }) as u8;
                    gr[0].n_short_sfb = 30 as i32 as u8;
                }
            }
            tables = get_bits(bs, 10 as i32);
            tables <<= 5 as i32;
            gr[0]
                .subblock_gain[0 as i32
                as usize] = get_bits(bs, 3 as i32) as u8;
            gr[0]
                .subblock_gain[1 as i32
                as usize] = get_bits(bs, 3 as i32) as u8;
            gr[0]
                .subblock_gain[2 as i32
                as usize] = get_bits(bs, 3 as i32) as u8;
        } else {
            gr[0].block_type = 0 as i32 as u8;
            gr[0].mixed_block_flag = 0 as i32 as u8;
            tables = get_bits(bs, 15 as i32);
            gr[0]
                .region_count[0 as i32
                as usize] = get_bits(bs, 4 as i32) as u8;
            gr[0]
                .region_count[1 as i32
                as usize] = get_bits(bs, 3 as i32) as u8;
            gr[0]
                .region_count[2 as i32 as usize] = 255 as i32 as u8;
        }
        gr[0]
            .table_select[0 as i32
            as usize] = (tables >> 10 as i32) as u8;
        gr[0]
            .table_select[1 as i32
            as usize] = (tables >> 5 as i32 & 31 as i32 as u32)
            as u8;
        gr[0]
            .table_select[2 as i32
            as usize] = (tables & 31 as i32 as u32) as u8;
        gr[0]
            .preflag = (if hdr[1] & 0x8 != 0 {
            get_bits(bs, 1 as i32)
        } else {
            (gr[0].scalefac_compress as i32 >= 500 as i32) as i32
                as u32
        }) as u8;
        gr[0].scalefac_scale = get_bits(bs, 1 as i32) as u8;
        gr[0].count1_table = get_bits(bs, 1 as i32) as u8;
        gr[0]
            .scfsi = (scfsi >> 12 as i32 & 15 as i32 as u32)
            as u8;
        scfsi <<= 4 as i32;
        gr = &mut gr[1..];
        gr_count -= 1;
        if !(gr_count != 0) {
            break;
        }
    }
    if part_23_sum + bs.pos > bs.limit + main_data_begin * 8 {
        return -(1 as i32);
    }
    return main_data_begin;
}
fn L3_read_scalefactors(
    scf: &mut [u8],
    ist_pos: &mut [u8],
    scf_size: &[u8],
    scf_count: &[u8],
    bitbuf: &mut bs_t,
    mut scfsi: i32,
) {
    let mut out = 0usize;

    for i in 0..4usize {
        let cnt = scf_count.get(i).copied().unwrap_or(0) as usize;
        if cnt == 0 {
            break;
        }

        let end = out.saturating_add(cnt).min(scf.len()).min(ist_pos.len());
        let count = end.saturating_sub(out);
        if count == 0 {
            break;
        }

        if scfsi & 8 != 0 {
            scf[out..end].copy_from_slice(&ist_pos[out..end]);
        } else {
            let bits = scf_size.get(i).copied().unwrap_or(0) as i32;
            if bits == 0 {
                scf[out..end].fill(0);
                ist_pos[out..end].fill(0);
            } else {
                let max_scf = if scfsi < 0 { (1i32 << bits) - 1 } else { -1 };
                for k in out..end {
                    let value = get_bits(bitbuf, bits) as i32;
                    ist_pos[k] = if value == max_scf { u8::MAX } else { value as u8 };
                    scf[k] = value as u8;
                }
            }
        }

        out = end;
        scfsi *= 2;
    }

    if out + 3 <= scf.len() {
        scf[out..out + 3].fill(0);
    }
}
fn L3_ldexp_q2(
    mut y: f32,
    mut exp_q2: i32,
) -> f32 {
    let mut e: i32 = 0;
    loop {
        e = if 30 as i32 * 4 as i32 > exp_q2 {
            exp_q2
        } else {
            30 as i32 * 4 as i32
        };
        y
            *= L3_LDEXP_Q2_G_EXPFRAC[(e & 3 as i32) as usize]
                * ((1 as i32) << 30 as i32 >> (e >> 2 as i32))
                    as f32;
        exp_q2 -= e;
        if !(exp_q2 > 0 as i32) {
            break;
        }
    }
    return y;
}
fn L3_decode_scalefactors(
    hdr: &[u8],
    ist_pos: &mut [u8],
    bs: &mut bs_t,
    gr: &L3_gr_info_t,
    scf: &mut [f32],
    ch: u32,
) {
    let _stage_timer = StageTimer::start("mpeg.scalefactors");
    let partition_index = (gr.n_short_sfb != 0) as usize + (gr.n_long_sfb == 0) as usize;
    let partition_base: &[u8] = &L3_DECODE_SCALEFACTORS_G_SCM_PARTITIONS[partition_index];
    let mut partition_offset = 0usize;
    let mut scf_size = [0u8; 4];
    let mut iscf = [0u8; 40];
    let scf_shift = gr.scalefac_scale as i32 + 1;
    let mut scfsi = gr.scfsi as i32;

    if hdr[1] & 0x8 != 0 {
        let part = L3_DECODE_SCALEFACTORS_G_SCFC_DECODE[gr.scalefac_compress as usize] as i32;
        scf_size[0] = (part >> 2) as u8;
        scf_size[1] = scf_size[0];
        scf_size[2] = (part & 3) as u8;
        scf_size[3] = scf_size[2];
    } else {
        let ist = (hdr[3] & 0x10 != 0 && ch != 0) as i32;
        let mut sfc = gr.scalefac_compress as i32 >> ist;
        let mut k = ist * 12;

        while sfc >= 0 {
            let mut modprod = 1i32;
            for i in (0..4usize).rev() {
                let modulus = L3_DECODE_SCALEFACTORS_G_MOD[(k + i as i32) as usize] as i32;
                scf_size[i] = (sfc / modprod % modulus) as u8;
                modprod *= modulus;
            }
            sfc -= modprod;
            k += 4;
        }

        partition_offset = k as usize;
        scfsi = -16;
    }

    let scf_partition = partition_base.get(partition_offset..).unwrap_or(&[]);
    L3_read_scalefactors(
        &mut iscf,
        ist_pos,
        &scf_size,
        scf_partition,
        bs,
        scfsi,
    );

    if gr.n_short_sfb != 0 {
        let sh = 3 - scf_shift;
        let start = gr.n_long_sfb as usize;
        for i in (0..gr.n_short_sfb as usize).step_by(3) {
            if start + i + 2 >= iscf.len() {
                break;
            }
            iscf[start + i] = (iscf[start + i] as i32 + ((gr.subblock_gain[0] as i32) << sh)) as u8;
            iscf[start + i + 1] = (iscf[start + i + 1] as i32 + ((gr.subblock_gain[1] as i32) << sh)) as u8;
            iscf[start + i + 2] = (iscf[start + i + 2] as i32 + ((gr.subblock_gain[2] as i32) << sh)) as u8;
        }
    } else if gr.preflag != 0 {
        for i in 0..10usize {
            iscf[11 + i] = (iscf[11 + i] as i32 + L3_DECODE_SCALEFACTORS_G_PREAMP[i] as i32) as u8;
        }
    }

    let gain_exp = gr.global_gain as i32 - 4 - 210 - if hdr[3] & 0xe0 == 0x60 { 2 } else { 0 };
    let base_exp = (255 - 4 - 210 + 3) & !3;
    let gain = L3_ldexp_q2((1i32 << (base_exp / 4)) as f32, base_exp - gain_exp);
    let count = (gr.n_long_sfb as usize + gr.n_short_sfb as usize).min(scf.len()).min(iscf.len());

    for i in 0..count {
        scf[i] = L3_ldexp_q2(gain, (iscf[i] as i32) << scf_shift);
    }
}

fn L3_pow_43_approx(mut x: i32) -> f32 {
    let mut frac: f32 = 0.;
    let mut sign: i32 = 0;
    let mut mult: i32 = 256 as i32;
    if x < 129 as i32 {
        return G_POW43[(16 as i32 + x) as usize];
    }
    if x < 1024 as i32 {
        mult = 16 as i32;
        x <<= 3 as i32;
    }
    sign = 2 as i32 * x & 64 as i32;
    frac = ((x & 63 as i32) - sign) as f32
        / ((x & !(63 as i32)) + sign) as f32;
    return G_POW43[(16 as i32 + (x + sign >> 6 as i32)) as usize]
        * (1.0f32
            + frac
                * (4.0f32 / 3 as i32 as f32
                    + frac * (2.0f32 / 9 as i32 as f32)))
        * mult as f32;
}

const POW43_TABLE_MAX: usize = 8206;
static POW43_TABLE: OnceLock<Box<[f32]>> = OnceLock::new();

#[inline]
fn L3_pow_43(x: i32) -> f32 {
    if x < 0 { return -L3_pow_43(-x); }
    let ux = x as usize;
    if ux <= POW43_TABLE_MAX {
        let table = POW43_TABLE.get_or_init(|| {
            let mut values = Vec::with_capacity(POW43_TABLE_MAX + 1);
            for value in 0..=POW43_TABLE_MAX {
                values.push(L3_pow_43_approx(value as i32));
            }
            values.into_boxed_slice()
        });
        if ux + 32 < table.len() { crate::simd::prefetch_read(unsafe { table.as_ptr().add(ux + 32) }); }
        return table[ux];
    }
    // Valid Layer III Huffman+linbits values fit the table. Keep the legacy
    // approximation as a defensive fallback for malformed streams.
    L3_pow_43_approx(x)
}

#[inline(always)]
fn huffman_byte(buf: &[u8], index: usize) -> u8 {
    buf.get(index).copied().unwrap_or(0)
}

#[inline(always)]
fn huffman_refill(buf: &[u8], next_index: &mut usize, cache: &mut u32, shift: &mut i32) {
    while *shift >= 0 {
        *cache |= (huffman_byte(buf, *next_index) as u32) << (*shift as u32);
        *next_index = (*next_index).saturating_add(1);
        *shift -= 8;
    }
}

#[inline(always)]
fn huffman_codebook_value(codebook: &[i16], index: u32) -> i32 {
    codebook.get(index as usize).copied().unwrap_or(0) as i32
}

#[inline(always)]
fn shr_u32(value: u32, shift: i32) -> u32 {
    if !(0..32).contains(&shift) {
        0
    } else {
        value >> shift as u32
    }
}

fn L3_huffman(
    dst: &mut [f32],
    bs: &mut bs_t,
    gr_info: &L3_gr_info_t,
    scf: &[f32],
    layer3gr_limit: i32,
) {
    let _stage_timer = StageTimer::start("mpeg.huffman");
    let mut one = 0.0f32;
    let mut ireg = 0usize;
    let mut big_val_cnt = gr_info.big_values as i32;
    let sfb = gr_info.sfbtab.as_slice();
    let mut sfb_index = 0usize;
    let mut scf_index = 0usize;
    let mut dst_index = 0usize;

    let start = (bs.pos.max(0) as usize) / 8;
    let mut next_index = start + 4;
    let mut bs_cache = u32::from_be_bytes([
        huffman_byte(bs.buf, start),
        huffman_byte(bs.buf, start + 1),
        huffman_byte(bs.buf, start + 2),
        huffman_byte(bs.buf, start + 3),
    ]) << ((bs.pos & 7) as u32);
    let mut bs_sh = (bs.pos & 7) - 8;
    let mut np = 0i32;

    while big_val_cnt > 0 && ireg < gr_info.table_select.len() {
        let tab_num = gr_info.table_select[ireg] as usize;
        let mut sfb_cnt = gr_info.region_count[ireg] as i32;
        ireg += 1;

        let codebook_start = L3_HUFFMAN_TABINDEX.get(tab_num).copied().unwrap_or(0) as usize;
        let codebook = L3_HUFFMAN_TABS.get(codebook_start..).unwrap_or(&[]);
        if codebook.len() > 32 { crate::simd::prefetch_read(unsafe { codebook.as_ptr().add(32) }); }
        let linbits = L3_HUFFMAN_G_LINBITS.get(tab_num).copied().unwrap_or(0) as i32;

        loop {
            np = sfb.get(sfb_index).copied().unwrap_or(0) as i32 / 2;
            sfb_index += 1;
            if np <= 0 {
                break;
            }

            let mut pairs_to_decode = big_val_cnt.min(np);
            one = scf.get(scf_index).copied().unwrap_or(0.0);
            scf_index += 1;
            let band_start = dst_index;

            while pairs_to_decode > 0 {
                let mut w = 5i32;
                let mut leaf = huffman_codebook_value(codebook, shr_u32(bs_cache, 32 - w));
                while leaf < 0 {
                    bs_cache <<= w as u32;
                    bs_sh += w;
                    w = leaf & 7;
                    let index = (shr_u32(bs_cache, 32 - w)).wrapping_sub((leaf >> 3) as u32);
                    leaf = huffman_codebook_value(codebook, index);
                }

                let used = leaf >> 8;
                bs_cache <<= used as u32;
                bs_sh += used;

                for _ in 0..2 {
                    let mut lsb = leaf & 0xf;
                    let value = if lsb == 15 && linbits != 0 {
                        lsb = (lsb as u32).wrapping_add(shr_u32(bs_cache, 32 - linbits)) as i32;
                        bs_cache <<= linbits as u32;
                        bs_sh += linbits;
                        huffman_refill(bs.buf, &mut next_index, &mut bs_cache, &mut bs_sh);
                        L3_pow_43(lsb) * if (bs_cache as i32) < 0 { -1.0 } else { 1.0 }
                    } else {
                        let pow_index = ((16 + lsb) as u32)
                            .wrapping_sub(16u32.wrapping_mul(bs_cache >> 31)) as usize;
                        G_POW43.get(pow_index).copied().unwrap_or(0.0)
                    };

                    if let Some(slot) = dst.get_mut(dst_index) {
                        *slot = value;
                    }
                    dst_index += 1;

                    if lsb != 0 {
                        bs_cache <<= 1;
                        bs_sh += 1;
                    }
                    leaf >>= 4;
                }

                huffman_refill(bs.buf, &mut next_index, &mut bs_cache, &mut bs_sh);
                pairs_to_decode -= 1;
            }

            let start = band_start.min(dst.len());
            let end = dst_index.min(dst.len());
            if start < end { unsafe { (mp3_simd().scale)(&mut dst[start..end], one); } }

            big_val_cnt -= np;
            if big_val_cnt <= 0 {
                break;
            }
            sfb_cnt -= 1;
            if sfb_cnt < 0 {
                break;
            }
        }
    }

    np = 1 - big_val_cnt;
    let count1: &[u8] = if gr_info.count1_table != 0 {
        &L3_HUFFMAN_TAB33
    } else {
        &L3_HUFFMAN_TAB32
    };

    loop {
        let mut leaf = count1.get((bs_cache >> 28) as usize).copied().unwrap_or(0) as i32;
        if leaf & 8 == 0 {
            let index = ((leaf >> 3) as u32)
                .wrapping_add(shr_u32(bs_cache << 4, 32 - (leaf & 3))) as usize;
            leaf = count1.get(index).copied().unwrap_or(0) as i32;
        }

        let used = leaf & 7;
        bs_cache <<= used as u32;
        bs_sh += used;

        let consumed_bits = (next_index as i64 * 8) - 24 + bs_sh as i64;
        if consumed_bits > layer3gr_limit as i64 {
            break;
        }

        np -= 1;
        if np == 0 {
            np = sfb.get(sfb_index).copied().unwrap_or(0) as i32 / 2;
            sfb_index += 1;
            if np == 0 {
                break;
            }
            one = scf.get(scf_index).copied().unwrap_or(0.0);
            scf_index += 1;
        }

        for bit in 0..2usize {
            if leaf & (128 >> bit) != 0 {
                if let Some(slot) = dst.get_mut(dst_index + bit) {
                    *slot = if (bs_cache as i32) < 0 { -one } else { one };
                }
                bs_cache <<= 1;
                bs_sh += 1;
            }
        }

        np -= 1;
        if np == 0 {
            np = sfb.get(sfb_index).copied().unwrap_or(0) as i32 / 2;
            sfb_index += 1;
            if np == 0 {
                break;
            }
            one = scf.get(scf_index).copied().unwrap_or(0.0);
            scf_index += 1;
        }

        for bit in 2..4usize {
            if leaf & (128 >> bit) != 0 {
                if let Some(slot) = dst.get_mut(dst_index + bit) {
                    *slot = if (bs_cache as i32) < 0 { -one } else { one };
                }
                bs_cache <<= 1;
                bs_sh += 1;
            }
        }

        huffman_refill(bs.buf, &mut next_index, &mut bs_cache, &mut bs_sh);
        dst_index += 4;
        if dst_index >= dst.len() {
            break;
        }
    }

    bs.pos = layer3gr_limit;
}

fn L3_midside_stereo(samples: &mut [f32], n: usize) {
    if samples.len() <= 576 {
        return;
    }
    let (left, right) = samples.split_at_mut(576);
    let n = n.min(left.len()).min(right.len());
    unsafe { (mp3_simd().midside)(left, right, n) };
}
fn L3_intensity_stereo_band(
    samples: &mut [f32],
    n: usize,
    kl: f32,
    kr: f32,
) {
    if samples.len() <= 576 {
        return;
    }
    let (left, right) = samples.split_at_mut(576);
    let n = n.min(left.len()).min(right.len());
    unsafe { (mp3_simd().intensity)(left, right, n, kl, kr) };
}
fn L3_stereo_top_band(
    right: &[f32],
    sfb: &[u8],
    nbands: i32,
    max_band: &mut [i32; 3],
) {
    max_band.fill(-1);
    let mut right_offset = 0usize;

    for i in 0..nbands.max(0) as usize {
        let len = sfb.get(i).copied().unwrap_or(0) as usize;
        let mut k = 0usize;
        while k + 1 < len && right_offset + k + 1 < right.len() {
            if right[right_offset + k] != 0.0 || right[right_offset + k + 1] != 0.0 {
                max_band[i % 3] = i as i32;
                break;
            }
            k += 2;
        }
        right_offset = right_offset.saturating_add(len);
        if right_offset >= right.len() {
            break;
        }
    }
}
fn L3_stereo_process(
    left: &mut [f32],
    ist_pos: &[u8],
    sfb: &[u8],
    hdr: &[u8],
    max_band: &[i32; 3],
    mpeg2_sh: i32,
) {
    let max_pos = if hdr[1] & 0x8 != 0 { 7u32 } else { 64u32 };
    let mut offset = 0usize;

    for i in 0..sfb.len() {
        let len = sfb[i] as usize;
        if len == 0 {
            break;
        }
        if offset + 576 + len > left.len() {
            break;
        }

        let ipos = ist_pos.get(i).copied().unwrap_or(u8::MAX) as u32;
        let band = &mut left[offset..];
        if i as i32 > max_band[i % 3] && ipos < max_pos {
            let scale = if hdr[3] & 0x20 != 0 { 1.41421356f32 } else { 1.0 };
            let mut kl;
            let mut kr;
            if hdr[1] & 0x8 != 0 {
                kl = L3_STEREO_PROCESS_G_PAN[(2 * ipos) as usize];
                kr = L3_STEREO_PROCESS_G_PAN[(2 * ipos + 1) as usize];
            } else {
                kl = 1.0;
                kr = L3_ldexp_q2(1.0, (((ipos + 1) >> 1) << mpeg2_sh) as i32);
                if ipos & 1 != 0 {
                    core::mem::swap(&mut kl, &mut kr);
                }
            }
            L3_intensity_stereo_band(band, len, kl * scale, kr * scale);
        } else if hdr[3] & 0x20 != 0 {
            L3_midside_stereo(band, len);
        }
        offset += len;
    }
}
fn L3_intensity_stereo(
    left: &mut [f32],
    ist_pos: &mut [u8],
    gr: &[L3_gr_info_t],
    hdr: &[u8],
) {
    if gr.len() < 2 {
        return;
    }

    let mut max_band = [0i32; 3];
    let n_sfb = gr[0].n_long_sfb as i32 + gr[0].n_short_sfb as i32;
    let max_blocks = if gr[0].n_short_sfb != 0 { 3 } else { 1 };
    let sfb = gr[0].sfbtab.as_slice();

    if left.len() < 576 {
        return;
    }
    L3_stereo_top_band(&left[576..], sfb, n_sfb, &mut max_band);

    if gr[0].n_long_sfb != 0 {
        let top = max_band[0].max(max_band[1]).max(max_band[2]);
        max_band.fill(top);
    }

    for i in 0..max_blocks as usize {
        let default_pos = if hdr[1] & 0x8 != 0 { 3 } else { 0 };
        let itop = n_sfb - max_blocks + i as i32;
        let prev = itop - max_blocks;
        if itop < 0 {
            continue;
        }
        let inherited = if prev >= 0 {
            ist_pos.get(prev as usize).copied().unwrap_or(default_pos as u8)
        } else {
            default_pos as u8
        };
        if let Some(slot) = ist_pos.get_mut(itop as usize) {
            *slot = if max_band[i] >= prev { default_pos as u8 } else { inherited };
        }
    }

    L3_stereo_process(
        left,
        ist_pos,
        sfb,
        hdr,
        &max_band,
        gr[1].scalefac_compress as i32 & 1,
    );
}
fn L3_reorder(grbuf: &mut [f32], scratch: &mut [f32], sfb: &[u8]) {
    let _stage_timer = StageTimer::start("mpeg.reorder");
    let mut src = 0usize;
    let mut dst = 0usize;
    let mut sfb_index = 0usize;

    loop {
        let len = sfb.get(sfb_index).copied().unwrap_or(0) as usize;
        if len == 0 {
            break;
        }
        if src + 3 * len > grbuf.len() || dst + 3 * len > scratch.len() {
            break;
        }

        for i in 0..len {
            scratch[dst] = grbuf[src + i];
            scratch[dst + 1] = grbuf[src + len + i];
            scratch[dst + 2] = grbuf[src + 2 * len + i];
            dst += 3;
        }
        src += 3 * len;
        sfb_index += 3;
    }

    let count = dst.min(grbuf.len()).min(scratch.len());
    grbuf[..count].copy_from_slice(&scratch[..count]);
}
fn L3_antialias(grbuf: &mut [f32], nbands: i32) {
    let requested = nbands.max(0) as usize;
    let available = if grbuf.len() >= 26 {
        1 + (grbuf.len() - 26) / 18
    } else {
        0
    };
    let bands = requested.min(available);
    if bands == 0 {
        return;
    }

    unsafe {
        (mp3_simd().antialias)(
            grbuf,
            bands,
            &L3_ANTIALIAS_G_AA[0],
            &L3_ANTIALIAS_G_AA[1],
        )
    };
}
fn L3_dct3_9(y: &mut [f32]) {
    let mut s0: f32 = 0.;
    let mut s1: f32 = 0.;
    let mut s2: f32 = 0.;
    let mut s3: f32 = 0.;
    let mut s4: f32 = 0.;
    let mut s5: f32 = 0.;
    let mut s6: f32 = 0.;
    let mut s7: f32 = 0.;
    let mut s8: f32 = 0.;
    let mut t0: f32 = 0.;
    let mut t2: f32 = 0.;
    let mut t4: f32 = 0.;
    s0 = y[0];
    s2 = y[2];
    s4 = y[4];
    s6 = y[6];
    s8 = y[8];
    t0 = s0 + s6 * 0.5f32;
    s0 -= s6;
    t4 = (s4 + s2) * 0.93969262f32;
    t2 = (s8 + s2) * 0.76604444f32;
    s6 = (s4 - s8) * 0.17364818f32;
    s4 += s8 - s2;
    s2 = s0 - s4 * 0.5f32;
    y[4] = s4 + s0;
    s8 = t0 - t2 + s6;
    s0 = t0 - t4 + t2;
    s4 = t0 + t4 - s6;
    s1 = y[1];
    s3 = y[3];
    s5 = y[5];
    s7 = y[7];
    s3 *= 0.86602540f32;
    t0 = (s5 + s1) * 0.98480775f32;
    t4 = (s5 - s7) * 0.34202014f32;
    t2 = (s1 + s7) * 0.64278761f32;
    s1 = (s1 - s5 - s7) * 0.86602540f32;
    s5 = t0 - s3 - t2;
    s7 = t4 - s3 - t0;
    s3 = t4 + s3 - t2;
    y[0] = s4 - s7;
    y[1] = s2 + s1;
    y[2] = s0 - s3;
    y[3] = s8 + s5;
    y[5] = s8 - s5;
    y[6] = s0 + s3;
    y[7] = s2 - s1;
    y[8] = s4 + s7;
}

fn L3_imdct36(
    grbuf: &mut [f32],
    overlap: &mut [f32],
    window: &[f32],
    nbands: i32,
) {
    let bands = nbands.max(0) as usize;
    for band in 0..bands {
        let g = band * 18;
        let o = band * 9;
        if g + 18 > grbuf.len() || o + 9 > overlap.len() || window.len() < 18 {
            break;
        }

        let mut co = [0.0f32; 9];
        let mut si = [0.0f32; 9];
        co[0] = -grbuf[g];
        si[0] = grbuf[g + 17];

        for i in 0..4usize {
            si[8 - 2 * i] = grbuf[g + 4 * i + 1] - grbuf[g + 4 * i + 2];
            co[1 + 2 * i] = grbuf[g + 4 * i + 1] + grbuf[g + 4 * i + 2];
            si[7 - 2 * i] = grbuf[g + 4 * i + 4] - grbuf[g + 4 * i + 3];
            co[2 + 2 * i] = -(grbuf[g + 4 * i + 3] + grbuf[g + 4 * i + 4]);
        }

        L3_dct3_9(&mut co);
        L3_dct3_9(&mut si);
        for i in [1usize, 3, 5, 7] {
            si[i] = -si[i];
        }

        for i in 0..9usize {
            let ovl = overlap[o + i];
            let sum = co[i] * L3_IMDCT36_G_TWID9[9 + i] + si[i] * L3_IMDCT36_G_TWID9[i];
            overlap[o + i] = co[i] * L3_IMDCT36_G_TWID9[i] - si[i] * L3_IMDCT36_G_TWID9[9 + i];
            grbuf[g + i] = ovl * window[i] - sum * window[9 + i];
            grbuf[g + 17 - i] = ovl * window[9 + i] + sum * window[i];
        }
    }
}
fn L3_idct3(
    x0: f32,
    x1: f32,
    x2: f32,
    dst: &mut [f32],
) {
    let m1: f32 = x1 * 0.86602540f32;
    let a1: f32 = x0 - x2 * 0.5f32;
    dst[1] = x0 + x2;
    dst[0] = a1 + m1;
    dst[2] = a1 - m1;
}
fn L3_imdct12(x: &[f32], dst: &mut [f32], overlap: &mut [f32]) {
    if x.len() < 16 || dst.len() < 6 || overlap.len() < 3 {
        return;
    }

    let mut co = [0.0f32; 3];
    let mut si = [0.0f32; 3];
    L3_idct3(-x[0], x[6] + x[3], x[12] + x[9], &mut co);
    L3_idct3(x[15], x[12] - x[9], x[6] - x[3], &mut si);
    si[1] = -si[1];

    for i in 0..3usize {
        let ovl = overlap[i];
        let sum = co[i] * L3_IMDCT12_G_TWID3[3 + i] + si[i] * L3_IMDCT12_G_TWID3[i];
        overlap[i] = co[i] * L3_IMDCT12_G_TWID3[i] - si[i] * L3_IMDCT12_G_TWID3[3 + i];
        dst[i] = ovl * L3_IMDCT12_G_TWID3[2 - i] - sum * L3_IMDCT12_G_TWID3[5 - i];
        dst[5 - i] = ovl * L3_IMDCT12_G_TWID3[5 - i] + sum * L3_IMDCT12_G_TWID3[2 - i];
    }
}
fn L3_imdct_short(grbuf: &mut [f32], overlap: &mut [f32], nbands: i32) {
    for band in 0..nbands.max(0) as usize {
        let g = band * 18;
        let o = band * 9;
        if g + 18 > grbuf.len() || o + 9 > overlap.len() {
            break;
        }

        let mut tmp = [0.0f32; 18];
        tmp.copy_from_slice(&grbuf[g..g + 18]);
        grbuf[g..g + 6].copy_from_slice(&overlap[o..o + 6]);

        L3_imdct12(&tmp, &mut grbuf[g + 6..g + 12], &mut overlap[o + 6..o + 9]);
        L3_imdct12(&tmp[1..], &mut grbuf[g + 12..g + 18], &mut overlap[o + 6..o + 9]);

        let (dst, tail) = overlap[o..o + 9].split_at_mut(6);
        L3_imdct12(&tmp[2..], dst, tail);
    }
}
fn L3_change_sign(grbuf: &mut [f32]) {
    if grbuf.len() < 576 {
        return;
    }
    unsafe { (mp3_simd().change_sign)(grbuf) };
}
fn L3_imdct_gr(
    grbuf: &mut [f32],
    overlap: &mut [f32],
    block_type: u32,
    n_long_bands: u32,
) {
    let _stage_timer = StageTimer::start("mpeg.imdct");
    let long_bands = n_long_bands.min(32) as usize;
    if long_bands != 0 {
        L3_imdct36(
            grbuf,
            overlap,
            &L3_IMDCT_GR_G_MDCT_WINDOW[0],
            long_bands as i32,
        );
    }

    let gr_offset = long_bands * 18;
    let overlap_offset = long_bands * 9;
    let remaining = 32usize.saturating_sub(long_bands) as i32;
    if gr_offset > grbuf.len() || overlap_offset > overlap.len() {
        return;
    }

    if block_type == 2 {
        L3_imdct_short(&mut grbuf[gr_offset..], &mut overlap[overlap_offset..], remaining);
    } else {
        let window_index = (block_type == 3) as usize;
        L3_imdct36(
            &mut grbuf[gr_offset..],
            &mut overlap[overlap_offset..],
            &L3_IMDCT_GR_G_MDCT_WINDOW[window_index],
            remaining,
        );
    }
}

fn L3_save_reservoir(
    h: &mut mp3dec_t,
    s_bs: &mut bs_t
) {
    let _stage_timer = StageTimer::start("mpeg.reservoir_save");
    let mut pos: i32 = ((s_bs.pos + 7 as i32) as u32)
        .wrapping_div(8 as u32) as i32;
    let mut remains: i32 = (s_bs.limit as u32)
        .wrapping_div(8 as u32)
        .wrapping_sub(pos as u32) as i32;
    if remains > 511 as i32 {
        pos += remains - 511 as i32;
        remains = 511 as i32;
    }
    if remains > 0 as i32 {
        h.reserv_buf[..remains as usize].copy_from_slice(&s_bs.buf[pos as usize..(pos+remains) as usize]);
    }
    (*h).reserv = remains;
}

fn L3_restore_reservoir<'a>(
    h: &mut mp3dec_t,
    bs: &mut bs_t,
    s_maindata: &'a mut [u8; 2815],
    s_bs: &mut bs_t<'a>,
    main_data_begin: i32,
) -> i32 {
    let _stage_timer = StageTimer::start("mpeg.reservoir_restore");
    let frame_bytes: i32 = (bs.limit - bs.pos) / 8 as i32;
    let bytes_have: i32 = if (*h).reserv > main_data_begin {
        main_data_begin
    } else {
        (*h).reserv
    };

    {
        let off = if (0 as i32) < (*h).reserv - main_data_begin {
            (*h).reserv - main_data_begin
        } else {
            0 as i32
        };
        let cnt = if (*h).reserv > main_data_begin { main_data_begin } else { (*h).reserv };
        s_maindata[..cnt as usize].copy_from_slice(&h.reserv_buf[off as usize..off as usize + cnt as usize]);
    }

    s_maindata[bytes_have as usize..bytes_have as usize + frame_bytes as usize]
        .copy_from_slice(&bs.buf[(bs.pos / 8) as usize..(bs.pos / 8) as usize + frame_bytes as usize]);

    *s_bs = bs_init(&s_maindata[..], bytes_have + frame_bytes);
    return ((*h).reserv >= main_data_begin) as i32;
}
fn L3_decode(
    h: &mut mp3dec_t,
    s: &mut Layer3Scratch<'_>,
    s_bs: &mut bs_t,
    mut gr_info: &mut [L3_gr_info_t],
    nch: u32,
) {
    let nch = nch.min(2) as usize;

    for ch in 0..nch {
        if ch >= gr_info.len() {
            break;
        }
        let layer3gr_limit = s_bs.pos + gr_info[ch].part_23_length as i32;
        L3_decode_scalefactors(
            &h.header,
            &mut s.ist_pos[ch],
            s_bs,
            &gr_info[ch],
            &mut s.scf[..],
            ch as u32,
        );
        L3_huffman(
            &mut s.grbuf[ch],
            s_bs,
            &gr_info[ch],
            &s.scf[..],
            layer3gr_limit,
        );
    }

    if h.header[3] & 0x10 != 0 && nch == 2 {
        L3_intensity_stereo(
            s.grbuf.as_flattened_mut(),
            &mut s.ist_pos[1],
            gr_info,
            &h.header,
        );
    } else if h.header[3] & 0xe0 == 0x60 && nch == 2 {
        L3_midside_stereo(s.grbuf.as_flattened_mut(), 576);
    }

    for ch in 0..nch {
        if gr_info.is_empty() {
            break;
        }

        let current = gr_info[0];
        let mut aa_bands = 31i32;
        let sr_index = ((h.header[2] as i32 >> 2) & 3)
            + ((((h.header[1] as i32 >> 3) & 1) + ((h.header[1] as i32 >> 4) & 1)) * 3);
        let n_long_bands = (if current.mixed_block_flag != 0 { 2 } else { 0 })
            << (sr_index == 2) as i32;

        if current.n_short_sfb != 0 {
            aa_bands = n_long_bands - 1;
            let gr_offset = (n_long_bands * 18).max(0) as usize;
            let sfb = current.sfbtab.as_slice();
            let sfb_offset = current.n_long_sfb as usize;
            if gr_offset <= s.grbuf[ch].len() {
                L3_reorder(
                    &mut s.grbuf[ch][gr_offset..],
                    s.syn.as_flattened_mut(),
                    sfb.get(sfb_offset..).unwrap_or(&[]),
                );
            }
        }

        L3_antialias(&mut s.grbuf[ch], aa_bands);
        L3_imdct_gr(
            &mut s.grbuf[ch],
            &mut h.mdct_overlap[ch],
            current.block_type as u32,
            n_long_bands.max(0) as u32,
        );
        L3_change_sign(&mut s.grbuf[ch]);
        gr_info = &mut gr_info[1..];
    }
}
fn mp3d_DCT_II(grbuf: &mut [f32], n: u32) {
    let _stage_timer = StageTimer::start("mpeg.synthesis_dct");
    for k in 0..n as usize {
        let mut t = [[0.0f32; 8]; 4];
        let x = t.as_flattened_mut();

        for i in 0..8usize {
            let idx = |row: usize| k + row * 18;
            let Some(&x0) = grbuf.get(idx(i)) else { return; };
            let Some(&x1) = grbuf.get(idx(15 - i)) else { return; };
            let Some(&x2) = grbuf.get(idx(16 + i)) else { return; };
            let Some(&x3) = grbuf.get(idx(31 - i)) else { return; };
            let t0 = x0 + x3;
            let t1 = x1 + x2;
            let t2 = (x1 - x2) * MP3D_DCT_II_G_SEC[3 * i];
            let t3 = (x0 - x3) * MP3D_DCT_II_G_SEC[3 * i + 1];
            x[i] = t0 + t1;
            x[8 + i] = (t0 - t1) * MP3D_DCT_II_G_SEC[3 * i + 2];
            x[16 + i] = t3 + t2;
            x[24 + i] = (t3 - t2) * MP3D_DCT_II_G_SEC[3 * i + 2];
        }

        for block in 0..4usize {
            let base = block * 8;
            let mut x0 = x[base];
            let mut x1 = x[base + 1];
            let mut x2 = x[base + 2];
            let mut x3 = x[base + 3];
            let mut x4 = x[base + 4];
            let mut x5 = x[base + 5];
            let mut x6 = x[base + 6];
            let mut x7 = x[base + 7];

            let mut xt = x0 - x7;
            x0 += x7;
            x7 = x1 - x6;
            x1 += x6;
            x6 = x2 - x5;
            x2 += x5;
            x5 = x3 - x4;
            x3 += x4;
            x4 = x0 - x3;
            x0 += x3;
            x3 = x1 - x2;
            x1 += x2;
            x[base] = x0 + x1;
            x[base + 4] = (x0 - x1) * 0.70710677;
            x5 += x6;
            x6 = (x6 + x7) * 0.70710677;
            x7 += xt;
            x3 = (x3 + x4) * 0.70710677;
            x5 -= x7 * 0.198912367;
            x7 += x5 * 0.382683432;
            x5 -= x7 * 0.198912367;
            x0 = xt - x6;
            xt += x6;
            x[base + 1] = (xt + x7) * 0.50979561;
            x[base + 2] = (x4 + x3) * 0.54119611;
            x[base + 3] = (x0 - x5) * 0.60134488;
            x[base + 5] = (x0 + x5) * 0.89997619;
            x[base + 6] = (x4 - x3) * 1.30656302;
            x[base + 7] = (xt - x7) * 2.56291556;
        }

        for i in 0..7usize {
            let base = k + i * 4 * 18;
            if base + 3 * 18 >= grbuf.len() {
                return;
            }
            grbuf[base] = t[0][i];
            grbuf[base + 18] = t[2][i] + t[3][i] + t[3][i + 1];
            grbuf[base + 36] = t[1][i] + t[1][i + 1];
            grbuf[base + 54] = t[2][i + 1] + t[3][i] + t[3][i + 1];
        }

        let base = k + 7 * 4 * 18;
        if base + 3 * 18 >= grbuf.len() {
            return;
        }
        grbuf[base] = t[0][7];
        grbuf[base + 18] = t[2][7] + t[3][7];
        grbuf[base + 36] = t[1][7];
        grbuf[base + 54] = t[3][7];
    }
}

fn mp3d_scale_pcm(sample: f32) -> f32 {
    sample * (1f32/32768f32)
}

fn mp3d_synth_pair(pcm: &mut [mp3d_sample_t], nch: u32, z: &[f32]) {
    let at = |base: usize, row: usize| -> f32 {
        z.get(base + row * 64).copied().unwrap_or(0.0)
    };

    let mut a = (at(0, 14) - at(0, 0)) * 29.0;
    a += (at(0, 1) + at(0, 13)) * 213.0;
    a += (at(0, 12) - at(0, 2)) * 459.0;
    a += (at(0, 3) + at(0, 11)) * 2037.0;
    a += (at(0, 10) - at(0, 4)) * 5153.0;
    a += (at(0, 5) + at(0, 9)) * 6574.0;
    a += (at(0, 8) - at(0, 6)) * 37489.0;
    a += at(0, 7) * 75038.0;
    if let Some(slot) = pcm.get_mut(0) {
        *slot = mp3d_scale_pcm(a);
    }

    let base = 2usize;
    a = at(base, 14) * 104.0;
    a += at(base, 12) * 1567.0;
    a += at(base, 10) * 9727.0;
    a += at(base, 8) * 64019.0;
    a -= at(base, 6) * 9975.0;
    a -= at(base, 4) * 45.0;
    a += at(base, 2) * 146.0;
    a -= at(base, 0) * 5.0;
    if let Some(slot) = pcm.get_mut((16 * nch) as usize) {
        *slot = mp3d_scale_pcm(a);
    }
}
fn mp3d_synth(
    xl: &[f32],
    dstl: &mut [mp3d_sample_t],
    nch: u32,
    lins: &mut [f32],
) {
    if nch == 0 {
        return;
    }
    let xr_offset = 576usize.saturating_mul((nch - 1) as usize);
    let dstr_off = (nch - 1) as usize;
    let zlin = 15usize * 64;
    let mut w_index = 0usize;

    let sample = |channel_offset: usize, index: usize| -> f32 {
        xl.get(channel_offset + index).copied().unwrap_or(0.0)
    };
    let zindex = |rel: i32| -> usize { (zlin as i32 + rel) as usize };

    let writes = [
        (4 * 15, sample(0, 18 * 16)),
        (4 * 15 + 1, sample(xr_offset, 18 * 16)),
        (4 * 15 + 2, sample(0, 0)),
        (4 * 15 + 3, sample(xr_offset, 0)),
        (4 * 31, sample(0, 1 + 18 * 16)),
        (4 * 31 + 1, sample(xr_offset, 1 + 18 * 16)),
        (4 * 31 + 2, sample(0, 1)),
        (4 * 31 + 3, sample(xr_offset, 1)),
    ];
    for (rel, value) in writes {
        if let Some(slot) = lins.get_mut(zindex(rel)) {
            *slot = value;
        }
    }

    let p0 = 4 * 15;
    let p1 = p0 + 64;
    let first_start = dstr_off.min(dstl.len());
    mp3d_synth_pair(&mut dstl[first_start..], nch, lins.get(p0 + 1..).unwrap_or(&[]));
    let start = dstr_off.saturating_add((32 * nch) as usize).min(dstl.len());
    mp3d_synth_pair(&mut dstl[start..], nch, lins.get(p1 + 1..).unwrap_or(&[]));
    mp3d_synth_pair(dstl, nch, lins.get(p0..).unwrap_or(&[]));
    let start = ((32 * nch) as usize).min(dstl.len());
    mp3d_synth_pair(&mut dstl[start..], nch, lins.get(p1..).unwrap_or(&[]));

    for i in (0..=14i32).rev() {
        let mut a = [0.0f32; 4];
        let mut b = [0.0f32; 4];
        let staged_writes = [
            (4 * i, sample(0, (18 * (31 - i)) as usize)),
            (4 * i + 1, sample(xr_offset, (18 * (31 - i)) as usize)),
            (4 * i + 2, sample(0, (1 + 18 * (31 - i)) as usize)),
            (4 * i + 3, sample(xr_offset, (1 + 18 * (31 - i)) as usize)),
            (4 * (i + 16), sample(0, (1 + 18 * (1 + i)) as usize)),
            (4 * (i + 16) + 1, sample(xr_offset, (1 + 18 * (1 + i)) as usize)),
            (4 * (i - 16) + 2, sample(0, (18 * (1 + i)) as usize)),
            (4 * (i - 16) + 3, sample(xr_offset, (18 * (1 + i)) as usize)),
        ];
        for (rel, value) in staged_writes {
            if let Some(slot) = lins.get_mut(zindex(rel)) {
                *slot = value;
            }
        }

        let window_start = w_index;
        let window_end = window_start.saturating_add(16);

        let simd_bounds_ok = window_end <= MP3D_SYNTH_G_WIN.len()
            && lins.len() >= zlin + 4 * 14 + 4;

        if simd_bounds_ok {
            let (dispatch_a, dispatch_b) = unsafe {
                (mp3_simd().synth_window4)(
                    lins,
                    i,
                    zlin,
                    window_start,
                    &MP3D_SYNTH_G_WIN,
                )
            };
            a = dispatch_a;
            b = dispatch_b;
        } else {
            // This path is only for defensive bounds handling. Backend
            // selection itself is already complete and cached in `mp3_simd()`.
            for stage in 0..8i32 {
                let w0 = MP3D_SYNTH_G_WIN.get(window_start + stage as usize * 2)
                    .copied()
                    .unwrap_or(0.0);
                let w1 = MP3D_SYNTH_G_WIN.get(window_start + stage as usize * 2 + 1)
                    .copied()
                    .unwrap_or(0.0);

                let vz = zindex(4 * i - stage * 64);
                let vy = zindex(4 * i - (15 - stage) * 64);

                for j in 0..4usize {
                    let vzv = lins.get(vz + j).copied().unwrap_or(0.0);
                    let vyv = lins.get(vy + j).copied().unwrap_or(0.0);

                    b[j] += vzv * w1 + vyv * w0;

                    if stage & 1 == 0 {
                        a[j] += vzv * w0 - vyv * w1;
                    } else {
                        a[j] += vyv * w1 - vzv * w0;
                    }
                }
            }
        }

        w_index = window_end;

        let outputs = [
            (dstr_off + ((15 - i as u32) * nch) as usize, a[1]),
            (dstr_off + ((17 + i as u32) * nch) as usize, b[1]),
            (((15 - i as u32) * nch) as usize, a[0]),
            (((17 + i as u32) * nch) as usize, b[0]),
            (dstr_off + ((47 - i as u32) * nch) as usize, a[3]),
            (dstr_off + ((49 + i as u32) * nch) as usize, b[3]),
            (((47 - i as u32) * nch) as usize, a[2]),
            (((49 + i as u32) * nch) as usize, b[2]),
        ];
        for (index, value) in outputs {
            if let Some(slot) = dstl.get_mut(index) {
                *slot = mp3d_scale_pcm(value);
            }
        }
    }
}
fn mp3d_synth_granule(
    qmf_state: &mut [f32],
    grbuf: &mut [f32],
    nbands: u32,
    nch: u32,
    pcm: &mut [mp3d_sample_t],
    lins: &mut [f32],
) {
    let _stage_timer = StageTimer::start("mpeg.synthesis");
    let channels = nch.min(2) as usize;
    for channel in 0..channels {
        let start = 576 * channel;
        if start >= grbuf.len() {
            break;
        }
        mp3d_DCT_II(&mut grbuf[start..], nbands);
    }

    let state_len = 15 * 64;
    let copy_len = state_len.min(qmf_state.len()).min(lins.len());
    lins[..copy_len].copy_from_slice(&qmf_state[..copy_len]);

    for i in (0..nbands as usize).step_by(2) {
        let gr_start = i.min(grbuf.len());
        let pcm_start = (32 * nch as usize * i).min(pcm.len());
        let lin_start = (i * 64).min(lins.len());
        mp3d_synth(&grbuf[gr_start..], &mut pcm[pcm_start..], nch, &mut lins[lin_start..]);
    }

    let source = nbands as usize * 64;
    if source < lins.len() {
        let copy_len = state_len.min(qmf_state.len()).min(lins.len() - source);
        qmf_state[..copy_len].copy_from_slice(&lins[source..source + copy_len]);
    }
}

fn mp3d_match_frame(
    hdr: &[u8],
    frame_bytes: usize,
) -> bool {
    let mut i: usize = 0;
    let mut nmatch: i32 = 0;
    nmatch = 0 as i32;
    while nmatch < 10 as i32 {
        i += hdr_frame_bytes(&hdr[i..], frame_bytes) + hdr_padding(&hdr[i..]);
        if i + 4 > hdr.len() {
            return nmatch > 0;
        }
        if !hdr_compare(hdr, &hdr[i..]) {
            return false;
        }
        nmatch += 1;
    }
    true
}

fn mp3d_find_frame(
    mut mp3: &[u8],
    free_format_bytes: &mut usize,
    ptr_frame_bytes: &mut usize,
) -> usize {
    let mp3_bytes = mp3.len();
    let mut i: usize = 0;
    let mut k: usize = 0;
    if mp3_bytes < 4 {
        *ptr_frame_bytes = 0;
        return mp3_bytes;
    }
    while i + 4 < mp3_bytes {
        if hdr_valid(mp3) {
            let mut frame_bytes = hdr_frame_bytes(mp3, *free_format_bytes);
            let mut frame_and_padding = frame_bytes + hdr_padding(mp3);
            k = 4;
            while frame_bytes == 0 && k < 2304 && i.saturating_add(2 * k).saturating_add(4) < mp3_bytes
            {
                if hdr_compare(mp3, &mp3[k..]) {
                    let fb = k - hdr_padding(mp3);
                    let nextfb = fb + hdr_padding(&mp3[k..]);
                    if !(i + k + nextfb + 4 > mp3_bytes || !hdr_compare(mp3, &mp3[k+nextfb..])) {
                        frame_and_padding = k;
                        frame_bytes = fb;
                        *free_format_bytes = fb;
                    }
                }
                k += 1;
            }
            if frame_bytes != 0 && i + frame_and_padding <= mp3_bytes
                && mp3d_match_frame(mp3, frame_bytes)
                || i == 0 && frame_and_padding == mp3_bytes
            {
                *ptr_frame_bytes = frame_and_padding;
                return i;
            }
            *free_format_bytes = 0;
        }
        i += 1;
        mp3 = &mp3[1..];
    }
    *ptr_frame_bytes = 0;
    mp3_bytes
}

fn mp3dec_init(dec: &mut mp3dec_t) {
    dec.header[0] = 0;
}

pub fn mp3dec_decode_frame(
    dec: &mut mp3dec_t,
    scratch: &mut mp3dec_scratch_t,
    mp3: &[u8],
    mut pcm: &mut [mp3d_sample_t],
    info: &mut mp3dec_frame_info_t,
) -> i32 {
    let mut i = 0usize;
    let mut frame_size = 0usize;
    let mut success = 1i32;
    let mut scratch_bs = bs_t { buf: &[], pos: 0, limit: 0 };
    let mut scratch_gr_info = [L3_gr_info_t::default(); 4];

    if mp3.len() > 4 && dec.header[0] == 0xff && hdr_compare(&dec.header, mp3) {
        frame_size = hdr_frame_bytes(mp3, dec.free_format_bytes) + hdr_padding(mp3);
        if frame_size != mp3.len()
            && (frame_size + 4 > mp3.len() || !hdr_compare(mp3, &mp3[frame_size..]))
        {
            frame_size = 0;
        }
    }

    if frame_size == 0 {
        *dec = mp3dec_t::new();
        i = mp3d_find_frame(mp3, &mut dec.free_format_bytes, &mut frame_size);
        if frame_size == 0 || i + frame_size > mp3.len() {
            info.frame_bytes = i;
            return 0;
        }
    }

    let hdr = &mp3[i..];
    if hdr.len() < 4 {
        info.frame_bytes = i;
        return 0;
    }
    dec.header.copy_from_slice(&hdr[..4]);
    info.frame_bytes = i + frame_size;
    info.frame_offset = i;
    info.channels = if hdr[3] & 0xc0 == 0xc0 { 1 } else { 2 };
    info.hz = hdr_sample_rate_hz(hdr) as i32;
    info.layer = 4 - ((hdr[1] >> 1) & 3);
    info.bitrate_kbps = hdr_bitrate_kbps(hdr) as i32;

    if pcm.is_empty() {
        return hdr_frame_samples(hdr) as i32;
    }
    if frame_size < 4 || hdr.len() < frame_size {
        mp3dec_init(dec);
        return 0;
    }

    let mut bs_frame = bs_init(&hdr[4..frame_size], (frame_size - 4) as i32);
    if hdr[1] & 1 == 0 {
        get_bits(&mut bs_frame, 16);
    }

    if info.layer == 3 {
        let main_data_begin = L3_read_side_info(&mut bs_frame, &mut scratch_gr_info, hdr);
        if main_data_begin < 0 || bs_frame.pos > bs_frame.limit {
            mp3dec_init(dec);
            return 0;
        }

        success = L3_restore_reservoir(
            dec,
            &mut bs_frame,
            &mut scratch.maindata,
            &mut scratch_bs,
            main_data_begin,
        );

        if success != 0 {
            let granules = if hdr[1] & 0x8 != 0 { 2usize } else { 1usize };
            for igr in 0..granules {
                scratch.grbuf.as_flattened_mut().fill(0.0);
                let gr_start = igr * info.channels as usize;
                // `scratch_bs` reads from `scratch.maindata`. Split the
                // remaining disjoint scratch fields so borrow checking can
                // enforce non-aliasing without copying compressed data.
                {
                    let mut layer3 = Layer3Scratch {
                        grbuf: &mut scratch.grbuf,
                        scf: &mut scratch.scf,
                        syn: &mut scratch.syn,
                        ist_pos: &mut scratch.ist_pos,
                    };
                    L3_decode(
                        dec,
                        &mut layer3,
                        &mut scratch_bs,
                        &mut scratch_gr_info[gr_start..],
                        info.channels,
                    );
                }

                mp3d_synth_granule(
                    &mut dec.qmf_state,
                    scratch.grbuf.as_flattened_mut(),
                    18,
                    info.channels,
                    pcm,
                    scratch.syn.as_flattened_mut(),
                );

                let advance = 576usize
                    .saturating_mul(info.channels as usize)
                    .min(pcm.len());
                pcm = &mut pcm[advance..];
            }
        }

        L3_save_reservoir(dec, &mut scratch_bs);
    } else if info.layer == 1 || info.layer == 2 {
        let mut sci = L12_scale_info::new();
        L12_read_scale_info(hdr, &mut bs_frame, &mut sci);
        scratch.grbuf.as_flattened_mut().fill(0.0);

        let group_size = (info.layer | 1) as usize;
        let mut decoded_slots = 0usize;

        for igr in 0..3usize {
            let offset = decoded_slots;
            let flat = scratch.grbuf.as_flattened_mut();
            if offset >= flat.len() {
                mp3dec_init(dec);
                return 0;
            }

            decoded_slots += L12_dequantize_granule(
                &mut flat[offset..],
                &mut bs_frame,
                &sci,
                group_size,
            );

            if decoded_slots == 12 {
                decoded_slots = 0;
                L12_apply_scf_384(
                    &sci,
                    igr,
                    scratch.grbuf.as_flattened_mut(),
                );

                mp3d_synth_granule(
                    &mut dec.qmf_state,
                    scratch.grbuf.as_flattened_mut(),
                    12,
                    info.channels,
                    pcm,
                    scratch.syn.as_flattened_mut(),
                );

                scratch.grbuf.as_flattened_mut().fill(0.0);

                let advance = 384usize
                    .saturating_mul(info.channels as usize)
                    .min(pcm.len());
                pcm = &mut pcm[advance..];
            }

            if bs_frame.pos > bs_frame.limit {
                mp3dec_init(dec);
                return 0;
            }
        }
    } else {
        mp3dec_init(dec);
        return 0;
    }

    success.saturating_mul(hdr_frame_samples(&dec.header) as i32)
}
