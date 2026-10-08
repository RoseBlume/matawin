use crate::{AudioError, codecs::support::StageTimer};

use super::{
    bit_reader::{ilog, BitReader},
    floor::{decode_floor_into, synthesize_floor_into, DecodedFloor, FloorScratch},
    header::{Setup, VorbisIdent},
    imdct::ImdctPlan,
    residue::{decode_residue_into, ResidueScratch},
};

#[derive(Debug, Clone)]
pub(super) struct DecoderState {
    previous_right: Vec<f32>,
    previous_len: usize,
    next_right: Vec<f32>,
    small_window: Vec<f32>,
    large_window: Vec<f32>,
    small_imdct: ImdctPlan,
    large_imdct: ImdctPlan,
    decoded_floors: Vec<DecodedFloor>,
    floor_scratch: FloorScratch,
    no_residue: Vec<bool>,
    residues: Vec<f32>,
    residue_scratch: ResidueScratch,
    submap_channels: Vec<usize>,
    submap_skip: Vec<bool>,
    submap_residue: Vec<f32>,
    floor_curve: Vec<f32>,
    imdct_output: Vec<f32>,
    time_channels: Vec<f32>,
    interleaved_output: Vec<f32>,
}

impl DecoderState {
    pub fn new(ident: &VorbisIdent) -> Result<Self, AudioError> {
        let channels = ident.channels as usize;
        let mut decoded_floors = Vec::with_capacity(channels);
        decoded_floors.resize_with(channels, DecodedFloor::default);
        Ok(Self {
            previous_right: Vec::with_capacity(channels * ident.blocksize_large / 2),
            previous_len: 0,
            next_right: Vec::with_capacity(channels * ident.blocksize_large / 2),
            small_window: make_window_slope(ident.blocksize_small / 2),
            large_window: make_window_slope(ident.blocksize_large / 2),
            small_imdct: ImdctPlan::new(ident.blocksize_small)?,
            large_imdct: ImdctPlan::new(ident.blocksize_large)?,
            decoded_floors,
            floor_scratch: FloorScratch::default(),
            no_residue: Vec::with_capacity(channels),
            residues: Vec::with_capacity(channels * ident.blocksize_large / 2),
            residue_scratch: ResidueScratch::default(),
            submap_channels: Vec::with_capacity(channels),
            submap_skip: Vec::with_capacity(channels),
            submap_residue: Vec::with_capacity(channels * ident.blocksize_large / 2),
            floor_curve: Vec::with_capacity(ident.blocksize_large / 2),
            imdct_output: Vec::with_capacity(ident.blocksize_large),
            time_channels: Vec::with_capacity(channels * ident.blocksize_large),
            interleaved_output: Vec::with_capacity(channels * ident.blocksize_large / 2),
        })
    }

    pub fn reset(&mut self) {
        self.previous_right.clear();
        self.previous_len = 0;
        self.next_right.clear();
        self.no_residue.clear();
        self.residues.clear();
        self.submap_channels.clear();
        self.submap_skip.clear();
        self.submap_residue.clear();
        self.floor_curve.clear();
        self.imdct_output.clear();
        self.time_channels.clear();
        self.interleaved_output.clear();
    }

    #[inline]
    pub fn output(&self) -> &[f32] { &self.interleaved_output }

    #[inline]
    pub fn output_vec_mut(&mut self) -> &mut Vec<f32> { &mut self.interleaved_output }

    pub(super) fn retained_bytes(&self) -> usize {
        let f32_bytes = |v: &Vec<f32>| v.capacity().saturating_mul(core::mem::size_of::<f32>());
        let usize_bytes = |v: &Vec<usize>| v.capacity().saturating_mul(core::mem::size_of::<usize>());
        let bool_bytes = |v: &Vec<bool>| v.capacity().div_ceil(8);
        let floors = self.decoded_floors.iter().map(DecodedFloor::retained_bytes).sum::<usize>();
        f32_bytes(&self.previous_right)+f32_bytes(&self.next_right)+f32_bytes(&self.small_window)+f32_bytes(&self.large_window)
            +f32_bytes(&self.residues)+f32_bytes(&self.submap_residue)+f32_bytes(&self.floor_curve)+f32_bytes(&self.imdct_output)
            +f32_bytes(&self.time_channels)+f32_bytes(&self.interleaved_output)+usize_bytes(&self.submap_channels)
            +bool_bytes(&self.no_residue)+bool_bytes(&self.submap_skip)+floors+self.floor_scratch.retained_bytes()+self.residue_scratch.retained_bytes()
            +self.small_imdct.scratch_bytes()+self.large_imdct.scratch_bytes()
    }

    pub(super) fn peak_retained_bytes(&self) -> usize {
        self.retained_bytes().saturating_sub(self.small_imdct.scratch_bytes()).saturating_sub(self.large_imdct.scratch_bytes())
            .saturating_add(self.small_imdct.peak_scratch_bytes()).saturating_add(self.large_imdct.peak_scratch_bytes())
    }

    pub(super) fn trim_memory(&mut self) {
        self.previous_right.shrink_to_fit(); self.next_right.shrink_to_fit(); self.no_residue.shrink_to_fit();
        self.residues.shrink_to_fit(); self.submap_channels.shrink_to_fit(); self.submap_skip.shrink_to_fit();
        self.submap_residue.shrink_to_fit(); self.floor_curve.shrink_to_fit(); self.imdct_output.shrink_to_fit();
        self.time_channels.shrink_to_fit(); self.interleaved_output.shrink_to_fit();
        for floor in &mut self.decoded_floors { floor.trim_memory(); }
        self.decoded_floors.shrink_to_fit(); self.floor_scratch.trim_memory(); self.residue_scratch.trim_memory();
    }

    fn window(&self, large: bool) -> &[f32] { if large { &self.large_window } else { &self.small_window } }
}

/// Decode one audio packet into retained, interleaved state storage and return
/// the number of PCM frames produced. The first packet after reset normally
/// returns zero because Vorbis requires a following block for overlap-add.
pub(super) fn decode_audio_packet(
    ident: &VorbisIdent,
    setup: &Setup,
    packet: &[u8],
    state: &mut DecoderState,
) -> Result<usize, AudioError> {
    let _packet_timer = StageTimer::start("vorbis.packet");
    let mut bits = BitReader::new(packet);
    if bits.read_bool()? { return Err(AudioError::InvalidFormat("A Vorbis header packet appeared after setup".into())); }

    let mode_bits = ilog(setup.modes.len().saturating_sub(1) as u64);
    let mode_number = bits.read_u32(mode_bits)? as usize;
    let mode = setup.modes.get(mode_number).ok_or_else(|| AudioError::InvalidFormat("Vorbis packet mode is out of range".into()))?;
    let mapping = setup.mappings.get(mode.mapping).ok_or_else(|| AudioError::InvalidFormat("Vorbis mode references an invalid mapping".into()))?;
    let block_size = if mode.blockflag { ident.blocksize_large } else { ident.blocksize_small };
    let half = block_size / 2;
    let window_flags = if mode.blockflag { Some((bits.read_bool()?, bits.read_bool()?)) } else { None };
    let channels = ident.channels as usize;

    if state.decoded_floors.len() != channels { state.decoded_floors.resize_with(channels, DecodedFloor::default); }
    for channel in 0..channels {
        let submap = mapping.mux[channel];
        let floor_index = mapping.submap_floors[submap];
        decode_floor_into(&mut bits, &setup.floors[floor_index], &setup.codebooks, &mut state.decoded_floors[channel])?;
    }

    state.no_residue.clear();
    state.no_residue.extend(state.decoded_floors.iter().map(DecodedFloor::is_unused));
    for (&magnitude, &angle) in mapping.magnitudes.iter().zip(&mapping.angles) {
        if !(state.no_residue[magnitude] && state.no_residue[angle]) { state.no_residue[magnitude] = false; state.no_residue[angle] = false; }
    }

    state.residues.resize(channels.checked_mul(half).ok_or_else(|| AudioError::InvalidFormat("Vorbis residue arena overflow".into()))?, 0.0);
    state.residues.fill(0.0);
    for submap in 0..mapping.submap_residues.len() {
        state.submap_channels.clear();
        state.submap_skip.clear();
        for channel in 0..channels {
            if mapping.mux[channel] == submap { state.submap_channels.push(channel); state.submap_skip.push(state.no_residue[channel]); }
        }
        if state.submap_channels.is_empty() { continue; }
        let count = state.submap_channels.len();
        state.submap_residue.resize(count * half, 0.0);
        let residue_index = mapping.submap_residues[submap];
        decode_residue_into(
            &mut bits, block_size, &state.submap_skip, &setup.residues[residue_index], &setup.codebooks,
            &mut state.submap_residue, &mut state.residue_scratch,
        )?;
        for (local, &channel) in state.submap_channels.iter().enumerate() {
            state.residues[channel * half..(channel + 1) * half]
                .copy_from_slice(&state.submap_residue[local * half..(local + 1) * half]);
        }
    }

    // Reverse coupling in setup-reverse order. The root dispatcher uses AVX2
    // or AArch64 NEON for full vectors and scalar tails otherwise.
    for (&magnitude, &angle) in mapping.magnitudes.iter().rev().zip(mapping.angles.iter().rev()) {
        let (mag, ang) = two_channel_slices(&mut state.residues, half, magnitude, angle)?;
        crate::simd::vorbis_inverse_couple(mag, ang);
    }

    state.time_channels.resize(channels.checked_mul(block_size).ok_or_else(|| AudioError::InvalidFormat("Vorbis time arena overflow".into()))?, 0.0);
    for channel in 0..channels {
        let submap = mapping.mux[channel];
        let floor_index = mapping.submap_floors[submap];
        synthesize_floor_into(
            &state.decoded_floors[channel], &setup.floors[floor_index], mode.blockflag, half,
            &mut state.floor_curve, &mut state.floor_scratch,
        )?;
        let residue = &state.residues[channel * half..(channel + 1) * half];
        crate::simd::mul_in_place(&mut state.floor_curve, residue);
        let plan = if mode.blockflag { &mut state.large_imdct } else { &mut state.small_imdct };
        let _imdct_timer = StageTimer::start(if mode.blockflag { "vorbis.imdct.large" } else { "vorbis.imdct.small" });
        plan.inverse_into(&state.floor_curve, &mut state.imdct_output)?;
        state.time_channels[channel * block_size..(channel + 1) * block_size].copy_from_slice(&state.imdct_output);
    }

    let center = block_size / 2;
    let (left_start, left_uses_large) = match window_flags {
        None => (0usize, mode.blockflag), Some((true, _)) => (0usize, true),
        Some((false, _)) => ((block_size - ident.blocksize_small) / 4, false),
    };
    let (right_start, right_end) = match window_flags {
        None => (center, block_size), Some((_, true)) => (center, block_size),
        Some((_, false)) => ((block_size * 3 - ident.blocksize_small) / 4, (block_size * 3 + ident.blocksize_small) / 4),
    };
    if right_end > block_size || right_start < left_start { return Err(AudioError::InvalidFormat("Invalid Vorbis window geometry".into())); }
    let future_len = right_end - right_start;
    state.next_right.resize(channels * future_len, 0.0);
    for channel in 0..channels {
        let time = &state.time_channels[channel * block_size..(channel + 1) * block_size];
        state.next_right[channel * future_len..(channel + 1) * future_len].copy_from_slice(&time[right_start..right_end]);
    }

    if state.previous_len == 0 {
        state.previous_right.clear();
        std::mem::swap(&mut state.previous_right, &mut state.next_right);
        state.previous_len = future_len;
        state.interleaved_output.clear();
        return Ok(0);
    }

    let plen = state.previous_len;
    let window = if left_uses_large { &state.large_window } else { &state.small_window };
    if plen > window.len() || left_start + plen > block_size { return Err(AudioError::InvalidFormat("Vorbis packet has incompatible overlap window sizes".into())); }
    for channel in 0..channels {
        let current = &mut state.time_channels[channel * block_size + left_start..channel * block_size + left_start + plen];
        let previous = &state.previous_right[channel * plen..(channel + 1) * plen];
        crate::simd::overlap_window(current, previous, &window[..plen]);
    }

    let output_frames = right_start - left_start;
    state.interleaved_output.resize(output_frames * channels, 0.0);
    if channels == 2 {
        let left = &state.time_channels[left_start..left_start + output_frames];
        let right_base = block_size + left_start;
        let right = &state.time_channels[right_base..right_base + output_frames];
        crate::simd::interleave_stereo(left, right, &mut state.interleaved_output);
    } else {
        for frame in 0..output_frames { for channel in 0..channels { state.interleaved_output[frame * channels + channel] = state.time_channels[channel * block_size + left_start + frame]; } }
    }

    state.previous_right.clear();
    std::mem::swap(&mut state.previous_right, &mut state.next_right);
    state.previous_len = future_len;
    Ok(output_frames)
}

fn two_channel_slices(values:&mut[f32],stride:usize,a:usize,b:usize)->Result<(&mut[f32],&mut[f32]),AudioError>{
    if a==b{return Err(AudioError::InvalidFormat("Invalid Vorbis coupling channel indices".into()));}
    let channels=if stride==0{0}else{values.len()/stride};if a>=channels||b>=channels{return Err(AudioError::InvalidFormat("Invalid Vorbis coupling channel indices".into()));}
    if a<b{let split=b*stride;let(left,right)=values.split_at_mut(split);Ok((&mut left[a*stride..(a+1)*stride],&mut right[..stride]))}
    else{let split=a*stride;let(left,right)=values.split_at_mut(split);Ok((&mut right[..stride],&mut left[b*stride..(b+1)*stride]))}
}

fn make_window_slope(n:usize)->Vec<f32>{let n_f=n.max(1)as f32;(0..n).map(|x|{let inner=(0.5*std::f32::consts::PI*(x as f32+0.5)/n_f).sin();(0.5*std::f32::consts::PI*inner*inner).sin()}).collect()}
