use std::sync::OnceLock;

use crate::AudioError;

use super::{
    bit_reader::{ilog, BitReader},
    codebook::Codebook,
    header::{Floor, Floor0, Floor1},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodedFloorKind { Unused, Type0, Type1 }

/// Reusable decoded-floor packet storage. Both floor-0 and floor-1 scratch
/// allocations are retained across packets instead of being dropped whenever
/// a channel changes mode/submap.
#[derive(Debug, Clone)]
pub(super) struct DecodedFloor {
    kind: DecodedFloorKind,
    cos_coefficients: Vec<f32>,
    amplitude: u64,
    y: Vec<u32>,
}

impl Default for DecodedFloor {
    fn default() -> Self {
        Self { kind: DecodedFloorKind::Unused, cos_coefficients: Vec::new(), amplitude: 0, y: Vec::new() }
    }
}

impl DecodedFloor {
    #[inline] pub fn is_unused(&self) -> bool { self.kind == DecodedFloorKind::Unused }
    #[inline] fn set_unused(&mut self) { self.kind = DecodedFloorKind::Unused; self.amplitude = 0; self.cos_coefficients.clear(); self.y.clear(); }
    pub(super) fn retained_bytes(&self) -> usize { self.cos_coefficients.capacity()*core::mem::size_of::<f32>() + self.y.capacity()*core::mem::size_of::<u32>() }
    pub(super) fn trim_memory(&mut self) { self.cos_coefficients.shrink_to_fit(); self.y.shrink_to_fit(); }
}

#[derive(Debug, Default, Clone)]
pub(super) struct FloorScratch {
    final_y: Vec<u32>,
    step2: Vec<bool>,
    rendered: Vec<u32>,
}
impl FloorScratch {
    pub(super) fn retained_bytes(&self)->usize{self.final_y.capacity()*core::mem::size_of::<u32>()+self.step2.capacity()*core::mem::size_of::<bool>()+self.rendered.capacity()*core::mem::size_of::<u32>()}
    pub(super) fn trim_memory(&mut self){self.final_y.shrink_to_fit();self.step2.shrink_to_fit();self.rendered.shrink_to_fit();}
}

pub(super) fn decode_floor_into(
    bits: &mut BitReader<'_>,
    floor: &Floor,
    codebooks: &[Codebook],
    output: &mut DecodedFloor,
) -> Result<(), AudioError> {
    let result = match floor {
        Floor::Type0(floor) => decode_floor0_into(bits, floor, codebooks, output),
        Floor::Type1(floor) => decode_floor1_into(bits, floor, codebooks, output),
    };
    match result {
        Err(AudioError::TruncatedData) => { output.set_unused(); Ok(()) }
        other => other,
    }
}

pub(super) fn synthesize_floor_into(
    decoded: &DecodedFloor,
    floor: &Floor,
    large_block: bool,
    n: usize,
    output: &mut Vec<f32>,
    scratch: &mut FloorScratch,
) -> Result<(), AudioError> {
    match (decoded.kind, floor) {
        (DecodedFloorKind::Unused, _) => { output.resize(n, 0.0); output.fill(0.0); Ok(()) }
        (DecodedFloorKind::Type0, Floor::Type0(floor)) => {
            synthesize_floor0_into(&decoded.cos_coefficients, decoded.amplitude, floor, large_block, n, output)
        }
        (DecodedFloorKind::Type1, Floor::Type1(floor)) => synthesize_floor1_into(&decoded.y, floor, n, output, scratch),
        _ => Err(AudioError::InvalidFormat("Vorbis decoded floor type does not match setup floor type".into())),
    }
}

fn decode_floor0_into(
    bits: &mut BitReader<'_>, floor: &Floor0, codebooks: &[Codebook], output: &mut DecodedFloor,
) -> Result<(), AudioError> {
    let amplitude = match bits.read_u64(floor.amplitude_bits) {
        Ok(value) => value,
        Err(AudioError::TruncatedData) => { output.set_unused(); return Ok(()); }
        Err(error) => return Err(error),
    };
    if amplitude == 0 { output.set_unused(); return Ok(()); }

    let book_bits = ilog(floor.books.len() as u64);
    let book_number = match bits.read_u32(book_bits) {
        Ok(value) => value as usize,
        Err(AudioError::TruncatedData) => { output.set_unused(); return Ok(()); }
        Err(error) => return Err(error),
    };
    let book_index = *floor.books.get(book_number).ok_or_else(|| AudioError::InvalidFormat("Vorbis floor-0 selected a nonexistent codebook".into()))?;
    let codebook = &codebooks[book_index];

    output.kind = DecodedFloorKind::Type0;
    output.amplitude = amplitude;
    output.cos_coefficients.clear();
    if output.cos_coefficients.capacity() < floor.order { output.cos_coefficients.reserve(floor.order - output.cos_coefficients.capacity()); }
    output.y.clear();
    let mut last = 0.0f32;
    while output.cos_coefficients.len() < floor.order {
        let vector = match codebook.decode_vector(bits) {
            Ok(vector) => vector,
            Err(AudioError::TruncatedData) => { output.set_unused(); return Ok(()); }
            Err(error) => return Err(error),
        };
        let mut last_new = last;
        for &value in vector {
            output.cos_coefficients.push((last + value).cos());
            last_new = value;
            if output.cos_coefficients.len() == floor.order { break; }
        }
        last += last_new;
    }
    Ok(())
}

fn synthesize_floor0_into(
    coefficients: &[f32], amplitude: u64, floor: &Floor0, large_block: bool, n: usize, output: &mut Vec<f32>,
) -> Result<(), AudioError> {
    if coefficients.len() != floor.order {
        return Err(AudioError::InvalidFormat("Vorbis floor-0 coefficient count does not match its order".into()));
    }
    let bark = &floor.bark_cos[large_block as usize];
    if bark.len() < n { return Err(AudioError::InvalidFormat("Vorbis floor-0 Bark map is shorter than the current spectrum".into())); }
    let max_amplitude = 2.0f64.powi(floor.amplitude_bits as i32) - 1.0;
    let common = amplitude as f64 * floor.amplitude_offset as f64 / max_amplitude;
    output.clear();
    output.reserve(n.saturating_sub(output.capacity()));
    let mut i = 0usize;
    while i < n {
        let cos_omega = bark[i] as f64;
        let (mut p, mut q) = if floor.order & 1 != 0 { (1.0 - cos_omega * cos_omega, 0.25) } else { ((1.0 - cos_omega) * 0.5, (1.0 + cos_omega) * 0.5) };
        let mut j = 1usize;
        while j < floor.order { let d = coefficients[j] as f64 - cos_omega; p *= 4.0 * d * d; j += 2; }
        let mut j = 0usize;
        while j < floor.order { let d = coefficients[j] as f64 - cos_omega; q *= 4.0 * d * d; j += 2; }
        let divisor = (p + q).sqrt();
        let linear = if divisor == 0.0 { 0.0 } else { (0.115_129_25f64 * (common / divisor - floor.amplitude_offset as f64)).exp() as f32 };
        let same = bark[i];
        while i < n && bark[i] == same { output.push(linear); i += 1; }
    }
    Ok(())
}

fn decode_floor1_into(
    bits: &mut BitReader<'_>, floor: &Floor1, codebooks: &[Codebook], output: &mut DecodedFloor,
) -> Result<(), AudioError> {
    let used = match bits.read_bool() {
        Ok(value) => value,
        Err(AudioError::TruncatedData) => { output.set_unused(); return Ok(()); }
        Err(error) => return Err(error),
    };
    if !used { output.set_unused(); return Ok(()); }
    let ranges = [256u32, 128, 86, 64];
    let range = ranges[(floor.multiplier - 1) as usize];
    let range_bits = ilog((range - 1) as u64);
    output.kind = DecodedFloorKind::Type1;
    output.amplitude = 0;
    output.cos_coefficients.clear();
    output.y.clear();
    output.y.reserve(floor.x_list.len().saturating_sub(output.y.capacity()));
    output.y.push(bits.read_u32(range_bits)?);
    output.y.push(bits.read_u32(range_bits)?);
    for &class in &floor.partition_classes {
        let dimensions = floor.class_dimensions[class];
        let subclass_bits = floor.class_subclasses[class];
        let subclass_mask = (1usize << subclass_bits) - 1;
        let mut class_value = if subclass_bits != 0 {
            let master = floor.class_masterbooks[class].ok_or_else(|| AudioError::InvalidFormat("Vorbis floor-1 class lacks its masterbook".into()))?;
            codebooks[master].decode_scalar(bits)? as usize
        } else { 0 };
        for _ in 0..dimensions {
            let book = floor.subclass_books[class][class_value & subclass_mask];
            class_value >>= subclass_bits;
            output.y.push(match book { Some(book) => codebooks[book].decode_scalar(bits)?, None => 0 });
        }
    }
    if output.y.len() != floor.x_list.len() { return Err(AudioError::InvalidFormat("Vorbis floor-1 packet produced the wrong number of posts".into())); }
    Ok(())
}

#[inline]
fn render_point(x0: u32, y0: u32, x1: u32, y1: u32, x: u32) -> u32 {
    let dy = y1 as i64 - y0 as i64; let adx = (x1 - x0) as i64; let ady = dy.abs(); let offset = ady * (x - x0) as i64 / adx;
    if dy < 0 { (y0 as i64 - offset) as u32 } else { (y0 as i64 + offset) as u32 }
}

fn floor1_amplitudes_into(y: &[u32], floor: &Floor1, final_y: &mut Vec<u32>, step2: &mut Vec<bool>) -> Result<(), AudioError> {
    if y.len() != floor.x_list.len() || y.len() < 2 { return Err(AudioError::InvalidFormat("Invalid Vorbis floor-1 post vector".into())); }
    let ranges = [256i64, 128, 86, 64]; let range = ranges[(floor.multiplier - 1) as usize];
    final_y.resize(y.len(), 0); step2.resize(y.len(), false); step2.fill(false);
    final_y[0] = y[0]; final_y[1] = y[1]; step2[0] = true; step2[1] = true;
    for i in 2..y.len() {
        let low = floor.low_neighbors[i]; let high = floor.high_neighbors[i];
        let predicted = render_point(floor.x_list[low], final_y[low], floor.x_list[high], final_y[high], floor.x_list[i]) as i64;
        let value = y[i] as i64; let highroom = range - predicted; let lowroom = predicted; let room = highroom.min(lowroom) * 2;
        if value != 0 {
            step2[low] = true; step2[high] = true; step2[i] = true;
            let resolved = if value >= room { if highroom > lowroom { predicted + value - lowroom } else { predicted - value + highroom - 1 } }
                else if value & 1 != 0 { predicted - ((value + 1) >> 1) } else { predicted + (value >> 1) };
            final_y[i] = resolved.clamp(0, range - 1) as u32;
        } else { final_y[i] = predicted as u32; }
    }
    for value in final_y { *value = (*value).min((range - 1) as u32); }
    Ok(())
}

fn render_line(x0:u32,y0:u32,x1:u32,y1:u32,output:&mut Vec<u32>)->Result<(),AudioError>{
    if x1<=x0{return Err(AudioError::InvalidFormat("Vorbis floor-1 render line has a non-positive width".into()));}
    let dy=y1 as i64-y0 as i64;let adx=(x1-x0)as i64;let ady=dy.abs();let base=dy/adx;let mut y=y0 as i64;let mut error=0i64;let sy=base+if dy<0{-1}else{1};let remainder=ady-base.abs()*adx;
    output.push(y as u32);for _ in (x0+1)..x1{error+=remainder;if error>=adx{error-=adx;y+=sy}else{y+=base}output.push(y as u32);}Ok(())
}

fn inverse_db_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; 256];
        for (index, value) in table.iter_mut().enumerate() {
            let db = (index as f32 - 255.0) * (140.0 / 256.0);
            *value = (db * (std::f32::consts::LN_10 / 20.0)).exp();
        }
        table
    })
}

fn synthesize_floor1_into(y:&[u32],floor:&Floor1,n:usize,output:&mut Vec<f32>,scratch:&mut FloorScratch)->Result<(),AudioError>{
    floor1_amplitudes_into(y,floor,&mut scratch.final_y,&mut scratch.step2)?;
    let sorted=&floor.sorted_x;if sorted.len()<2{return Err(AudioError::InvalidFormat("Vorbis floor-1 has too few posts".into()));}
    let first_index=sorted[0].0;let mut lx=0u32;let mut ly=scratch.final_y[first_index]*floor.multiplier as u32;let mut hx=0u32;let mut hy=ly;
    scratch.rendered.clear();scratch.rendered.reserve(n.saturating_sub(scratch.rendered.capacity()));
    for &(original,x) in sorted.iter().skip(1){if scratch.step2[original]{hx=x;hy=scratch.final_y[original]*floor.multiplier as u32;render_line(lx,ly,hx,hy,&mut scratch.rendered)?;lx=hx;ly=hy;}}
    if hx<n as u32{render_line(hx,hy,n as u32,hy,&mut scratch.rendered)?}else if hx>n as u32{scratch.rendered.truncate(n)}
    scratch.rendered.truncate(n);while scratch.rendered.len()<n{scratch.rendered.push(hy)}
    output.clear();output.reserve(scratch.rendered.len().saturating_sub(output.capacity()));let table=inverse_db_table();
    output.extend(scratch.rendered.iter().map(|&v| table[v.min(255) as usize]));Ok(())
}
