use crate::AudioError;

use super::{
    bit_reader::{ilog, BitReader},
    codebook::{read_codebook, Codebook},
};

const VORBIS_SIGNATURE: &[u8; 6] = b"vorbis";

#[derive(Debug, Clone)]
pub struct VorbisIdent {
    pub channels: u8,
    pub sample_rate: u32,
    pub bitrate_maximum: i32,
    pub bitrate_nominal: i32,
    pub bitrate_minimum: i32,
    pub blocksize_small: usize,
    pub blocksize_large: usize,
}

#[derive(Debug, Clone, Default)]
pub struct VorbisComments {
    pub vendor: String,
    pub comments: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(super) struct Floor0 {
    pub order: usize,
    pub amplitude_bits: u8,
    pub amplitude_offset: u8,
    pub books: Vec<usize>,
    pub bark_cos: [Vec<f32>; 2],
}

#[derive(Debug, Clone)]
pub(super) struct Floor1 {
    pub multiplier: u8,
    pub partition_classes: Vec<usize>,
    pub class_dimensions: Vec<usize>,
    pub class_subclasses: Vec<u8>,
    pub class_masterbooks: Vec<Option<usize>>,
    /// Fixed-width subclass rows avoid one heap allocation per class. Only
    /// the first `1 << class_subclasses[class]` entries are used.
    pub subclass_books: Vec<[Option<usize>; 8]> ,
    pub x_list: Vec<u32>,
    pub sorted_x: Vec<(usize, u32)>,
    pub low_neighbors: Vec<usize>,
    pub high_neighbors: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(super) enum Floor {
    Type0(Floor0),
    Type1(Floor1),
}

#[derive(Debug, Clone)]
pub(super) struct Residue {
    pub residue_type: u8,
    pub begin: usize,
    pub end: usize,
    pub partition_size: usize,
    pub classifications: usize,
    pub classbook: usize,
    /// [classification][pass] -> codebook index.
    pub books: Vec<[Option<usize>; 8]>,
}

#[derive(Debug, Clone)]
pub(super) struct Mapping {
    pub magnitudes: Vec<usize>,
    pub angles: Vec<usize>,
    pub mux: Vec<usize>,
    pub submap_floors: Vec<usize>,
    pub submap_residues: Vec<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Mode {
    pub blockflag: bool,
    pub mapping: usize,
}

#[derive(Debug, Clone)]
pub(super) struct Setup {
    pub codebooks: Vec<Codebook>,
    pub floors: Vec<Floor>,
    pub residues: Vec<Residue>,
    pub mappings: Vec<Mapping>,
    pub modes: Vec<Mode>,
}

impl Setup {
    pub(super) fn retained_bytes(&self) -> usize {
        let mut bytes=self.codebooks.capacity()*core::mem::size_of::<Codebook>()
            +self.codebooks.iter().map(Codebook::retained_bytes).sum::<usize>()
            +self.floors.capacity()*core::mem::size_of::<Floor>()
            +self.residues.capacity()*core::mem::size_of::<Residue>()
            +self.mappings.capacity()*core::mem::size_of::<Mapping>()
            +self.modes.capacity()*core::mem::size_of::<Mode>();
        for floor in &self.floors { match floor {
            Floor::Type0(v)=>{bytes=bytes.saturating_add(v.books.capacity()*core::mem::size_of::<usize>());for table in &v.bark_cos{bytes=bytes.saturating_add(table.capacity()*core::mem::size_of::<f32>());}},
            Floor::Type1(v)=>{
                bytes=bytes.saturating_add(v.partition_classes.capacity()*core::mem::size_of::<usize>())
                    .saturating_add(v.class_dimensions.capacity()*core::mem::size_of::<usize>())
                    .saturating_add(v.class_subclasses.capacity()*core::mem::size_of::<u8>())
                    .saturating_add(v.class_masterbooks.capacity()*core::mem::size_of::<Option<usize>>())
                    .saturating_add(v.subclass_books.capacity()*core::mem::size_of::<[Option<usize>;8]>())
                    .saturating_add(v.x_list.capacity()*core::mem::size_of::<u32>())
                    .saturating_add(v.sorted_x.capacity()*core::mem::size_of::<(usize,u32)>())
                    .saturating_add(v.low_neighbors.capacity()*core::mem::size_of::<usize>())
                    .saturating_add(v.high_neighbors.capacity()*core::mem::size_of::<usize>());
            }
        }}
        for residue in &self.residues { bytes=bytes.saturating_add(residue.books.capacity()*core::mem::size_of::<[Option<usize>;8]>()); }
        for mapping in &self.mappings {
            bytes=bytes.saturating_add(mapping.magnitudes.capacity()*core::mem::size_of::<usize>())
                .saturating_add(mapping.angles.capacity()*core::mem::size_of::<usize>())
                .saturating_add(mapping.mux.capacity()*core::mem::size_of::<usize>())
                .saturating_add(mapping.submap_floors.capacity()*core::mem::size_of::<usize>())
                .saturating_add(mapping.submap_residues.capacity()*core::mem::size_of::<usize>());
        }
        bytes
    }
}


pub(super) fn parse_identification(packet: &[u8]) -> Result<VorbisIdent, AudioError> {
    verify_byte_header(packet, 1)?;
    if packet.len() < 30 {
        return Err(AudioError::TruncatedData);
    }

    let version = le_u32(&packet[7..11]);
    if version != 0 {
        return Err(AudioError::InvalidFormat(format!(
            "Unsupported Vorbis version {version}"
        )));
    }

    let channels = packet[11];
    let sample_rate = le_u32(&packet[12..16]);
    let bitrate_maximum = le_i32(&packet[16..20]);
    let bitrate_nominal = le_i32(&packet[20..24]);
    let bitrate_minimum = le_i32(&packet[24..28]);
    let block = packet[28];
    let small_exp = block & 0x0f;
    let large_exp = block >> 4;
    let framing = packet[29] & 1;

    if channels == 0
        || sample_rate == 0
        || !(6..=13).contains(&small_exp)
        || !(6..=13).contains(&large_exp)
        || small_exp > large_exp
        || framing != 1
    {
        return Err(AudioError::InvalidFormat(
            "Invalid Vorbis identification header".into(),
        ));
    }

    Ok(VorbisIdent {
        channels,
        sample_rate,
        bitrate_maximum,
        bitrate_nominal,
        bitrate_minimum,
        blocksize_small: 1usize << small_exp,
        blocksize_large: 1usize << large_exp,
    })
}

pub(super) fn parse_comments(packet: &[u8]) -> Result<VorbisComments, AudioError> {
    verify_byte_header(packet, 3)?;
    let mut cursor = 7usize;

    let vendor_len = take_u32(packet, &mut cursor)? as usize;
    let vendor_bytes = take(packet, &mut cursor, vendor_len)?;
    let vendor = String::from_utf8(vendor_bytes.to_vec()).map_err(|_| {
        AudioError::InvalidFormat("Vorbis vendor string is not valid UTF-8".into())
    })?;

    let count = take_u32(packet, &mut cursor)? as usize;
    let mut comments = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let len = take_u32(packet, &mut cursor)? as usize;
        let bytes = take(packet, &mut cursor, len)?;
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let Some(eq) = text.find('=') else {
            continue;
        };
        comments.push((text[..eq].to_owned(), text[eq + 1..].to_owned()));
    }

    let framing = *packet.get(cursor).ok_or(AudioError::TruncatedData)?;
    if framing & 1 == 0 {
        return Err(AudioError::InvalidFormat(
            "Vorbis comment framing bit is not set".into(),
        ));
    }

    Ok(VorbisComments { vendor, comments })
}

pub(super) fn parse_setup(packet: &[u8], ident: &VorbisIdent) -> Result<Setup, AudioError> {
    let mut bits = BitReader::new(packet);
    verify_bit_header(&mut bits, 5)?;

    let codebook_count = bits.read_u8(8)? as usize + 1;
    let mut codebooks = Vec::with_capacity(codebook_count);
    for _ in 0..codebook_count {
        codebooks.push(read_codebook(&mut bits)?);
    }

    let time_count = bits.read_u8(6)? as usize + 1;
    for _ in 0..time_count {
        if bits.read_u16(16)? != 0 {
            return Err(AudioError::InvalidFormat(
                "Unsupported non-zero Vorbis time-domain transform".into(),
            ));
        }
    }

    let floor_count = bits.read_u8(6)? as usize + 1;
    let mut floors = Vec::with_capacity(floor_count);
    for _ in 0..floor_count {
        floors.push(read_floor(&mut bits, &codebooks, ident)?);
    }

    let residue_count = bits.read_u8(6)? as usize + 1;
    let mut residues = Vec::with_capacity(residue_count);
    for _ in 0..residue_count {
        residues.push(read_residue(&mut bits, &codebooks)?);
    }

    let mapping_count = bits.read_u8(6)? as usize + 1;
    let mut mappings = Vec::with_capacity(mapping_count);
    for _ in 0..mapping_count {
        mappings.push(read_mapping(
            &mut bits,
            ident.channels as usize,
            floor_count,
            residue_count,
        )?);
    }

    let mode_count = bits.read_u8(6)? as usize + 1;
    let mut modes = Vec::with_capacity(mode_count);
    for _ in 0..mode_count {
        let blockflag = bits.read_bool()?;
        let window_type = bits.read_u16(16)?;
        let transform_type = bits.read_u16(16)?;
        let mapping = bits.read_u8(8)? as usize;
        if window_type != 0 || transform_type != 0 || mapping >= mapping_count {
            return Err(AudioError::InvalidFormat("Invalid Vorbis mode".into()));
        }
        modes.push(Mode { blockflag, mapping });
    }

    if !bits.read_bool()? {
        return Err(AudioError::InvalidFormat(
            "Vorbis setup framing bit is not set".into(),
        ));
    }

    Ok(Setup {
        codebooks,
        floors,
        residues,
        mappings,
        modes,
    })
}

fn read_floor(
    bits: &mut BitReader<'_>,
    codebooks: &[Codebook],
    ident: &VorbisIdent,
) -> Result<Floor, AudioError> {
    match bits.read_u16(16)? {
        0 => {
            let order = bits.read_u8(8)? as usize;
            let rate = bits.read_u16(16)?;
            let bark_map_size = bits.read_u16(16)?;
            let amplitude_bits = bits.read_u8(6)?;
            let amplitude_offset = bits.read_u8(8)?;
            let book_count = bits.read_u8(4)? as usize + 1;

            if rate == 0 || bark_map_size == 0 {
                return Err(AudioError::InvalidFormat("Invalid Vorbis floor-0 setup".into()));
            }
            let mut books = Vec::with_capacity(book_count);
            for _ in 0..book_count {
                let book = bits.read_u8(8)? as usize;
                if book >= codebooks.len() || !codebooks[book].has_lookup() {
                    return Err(AudioError::InvalidFormat(
                        "Vorbis floor-0 references an invalid VQ codebook".into(),
                    ));
                }
                books.push(book);
            }

            Ok(Floor::Type0(Floor0 {
                order,
                amplitude_bits,
                amplitude_offset,
                books,
                bark_cos: [
                    compute_bark_cos(ident.blocksize_small / 2, rate, bark_map_size),
                    compute_bark_cos(ident.blocksize_large / 2, rate, bark_map_size),
                ],
            }))
        }
        1 => {
            let partition_count = bits.read_u8(5)? as usize;
            let mut partition_classes = Vec::with_capacity(partition_count);
            let mut max_class = None::<usize>;
            for _ in 0..partition_count {
                let class = bits.read_u8(4)? as usize;
                max_class = Some(max_class.map_or(class, |m| m.max(class)));
                partition_classes.push(class);
            }

            let class_count = max_class.map_or(0, |v| v + 1);
            let mut class_dimensions = Vec::with_capacity(class_count);
            let mut class_subclasses = Vec::with_capacity(class_count);
            let mut class_masterbooks = Vec::with_capacity(class_count);
            let mut subclass_books = Vec::with_capacity(class_count);

            for _ in 0..class_count {
                let dimensions = bits.read_u8(3)? as usize + 1;
                let subclasses = bits.read_u8(2)?;
                let master = if subclasses != 0 {
                    let idx = bits.read_u8(8)? as usize;
                    if idx >= codebooks.len() {
                        return Err(AudioError::InvalidFormat(
                            "Vorbis floor-1 masterbook is out of range".into(),
                        ));
                    }
                    Some(idx)
                } else {
                    None
                };

                let subclass_count = 1usize << subclasses;
                let mut subbooks = [None; 8];
                for slot in subbooks.iter_mut().take(subclass_count) {
                    let encoded = bits.read_u8(8)? as i16 - 1;
                    if encoded >= 0 {
                        let idx = encoded as usize;
                        if idx >= codebooks.len() {
                            return Err(AudioError::InvalidFormat(
                                "Vorbis floor-1 subclass book is out of range".into(),
                            ));
                        }
                        *slot = Some(idx);
                    } else {
                        *slot = None;
                    }
                }

                class_dimensions.push(dimensions);
                class_subclasses.push(subclasses);
                class_masterbooks.push(master);
                subclass_books.push(subbooks);
            }

            let multiplier = bits.read_u8(2)? + 1;
            let range_bits = bits.read_u8(4)?;
            let mut x_list = Vec::new();
            x_list.push(0);
            x_list.push(1u32 << range_bits);
            for &class in &partition_classes {
                for _ in 0..class_dimensions[class] {
                    x_list.push(bits.read_u32(range_bits)?);
                }
            }
            if x_list.len() > 65 {
                return Err(AudioError::InvalidFormat(
                    "Vorbis floor-1 contains more than 65 posts".into(),
                ));
            }

            let mut sorted_x: Vec<_> = x_list.iter().copied().enumerate().collect();
            sorted_x.sort_unstable_by_key(|&(_, x)| x);
            if sorted_x.windows(2).any(|w| w[0].1 == w[1].1) {
                return Err(AudioError::InvalidFormat(
                    "Vorbis floor-1 contains duplicate X coordinates".into(),
                ));
            }

            // Neighbor relationships depend only on setup X coordinates, so
            // compute them once instead of rescanning posts for every packet.
            let mut low_neighbors = vec![0usize; x_list.len()];
            let mut high_neighbors = vec![0usize; x_list.len()];
            for i in 2..x_list.len() {
                let bound = x_list[i];
                let mut low: Option<(usize, u32)> = None;
                let mut high: Option<(usize, u32)> = None;
                for (j, &x) in x_list[..i].iter().enumerate() {
                    if x < bound && low.map_or(true, |(_, best)| x > best) { low = Some((j, x)); }
                    if x > bound && high.map_or(true, |(_, best)| x < best) { high = Some((j, x)); }
                }
                low_neighbors[i] = low.ok_or_else(|| AudioError::InvalidFormat("Vorbis floor-1 post has no low neighbor".into()))?.0;
                high_neighbors[i] = high.ok_or_else(|| AudioError::InvalidFormat("Vorbis floor-1 post has no high neighbor".into()))?.0;
            }

            Ok(Floor::Type1(Floor1 {
                multiplier,
                partition_classes,
                class_dimensions,
                class_subclasses,
                class_masterbooks,
                subclass_books,
                x_list,
                sorted_x,
                low_neighbors,
                high_neighbors,
            }))
        }
        other => Err(AudioError::InvalidFormat(format!(
            "Unsupported Vorbis floor type {other}"
        ))),
    }
}

fn read_residue(bits: &mut BitReader<'_>, codebooks: &[Codebook]) -> Result<Residue, AudioError> {
    let residue_type = bits.read_u16(16)? as u8;
    if residue_type > 2 {
        return Err(AudioError::InvalidFormat(format!(
            "Unsupported Vorbis residue type {residue_type}"
        )));
    }

    let begin = bits.read_u32(24)? as usize;
    let end = bits.read_u32(24)? as usize;
    if begin > end {
        return Err(AudioError::InvalidFormat(
            "Vorbis residue begin is after residue end".into(),
        ));
    }
    let partition_size = bits.read_u32(24)? as usize + 1;
    let classifications = bits.read_u8(6)? as usize + 1;
    let classbook = bits.read_u8(8)? as usize;
    if classbook >= codebooks.len() {
        return Err(AudioError::InvalidFormat(
            "Vorbis residue classbook is out of range".into(),
        ));
    }

    let mut cascades = Vec::with_capacity(classifications);
    for _ in 0..classifications {
        let low = bits.read_u8(3)?;
        let high = if bits.read_bool()? { bits.read_u8(5)? } else { 0 };
        cascades.push((high << 3) | low);
    }

    let mut books = Vec::with_capacity(classifications);
    for cascade in cascades {
        let mut passes = [None; 8];
        for pass in 0..8 {
            if cascade & (1 << pass) != 0 {
                let idx = bits.read_u8(8)? as usize;
                if idx >= codebooks.len() || !codebooks[idx].has_lookup() {
                    return Err(AudioError::InvalidFormat(
                        "Vorbis residue references an invalid VQ codebook".into(),
                    ));
                }
                passes[pass] = Some(idx);
            }
        }
        books.push(passes);
    }

    Ok(Residue {
        residue_type,
        begin,
        end,
        partition_size,
        classifications,
        classbook,
        books,
    })
}

fn read_mapping(
    bits: &mut BitReader<'_>,
    channels: usize,
    floor_count: usize,
    residue_count: usize,
) -> Result<Mapping, AudioError> {
    if bits.read_u16(16)? != 0 {
        return Err(AudioError::InvalidFormat(
            "Only Vorbis mapping type 0 is supported".into(),
        ));
    }

    let submaps = if bits.read_bool()? {
        bits.read_u8(4)? as usize + 1
    } else {
        1
    };
    let coupling_steps = if bits.read_bool()? {
        bits.read_u8(8)? as usize + 1
    } else {
        0
    };

    let channel_bits = ilog(channels.saturating_sub(1) as u64);
    let mut magnitudes = Vec::with_capacity(coupling_steps);
    let mut angles = Vec::with_capacity(coupling_steps);
    for _ in 0..coupling_steps {
        let magnitude = bits.read_u32(channel_bits)? as usize;
        let angle = bits.read_u32(channel_bits)? as usize;
        if magnitude >= channels || angle >= channels || magnitude == angle {
            return Err(AudioError::InvalidFormat(
                "Invalid Vorbis channel coupling pair".into(),
            ));
        }
        magnitudes.push(magnitude);
        angles.push(angle);
    }

    if bits.read_u8(2)? != 0 {
        return Err(AudioError::InvalidFormat(
            "Reserved Vorbis mapping bits are non-zero".into(),
        ));
    }

    let mux = if submaps > 1 {
        let mut mux = Vec::with_capacity(channels);
        for _ in 0..channels {
            let value = bits.read_u8(4)? as usize;
            if value >= submaps {
                return Err(AudioError::InvalidFormat(
                    "Vorbis mapping mux is out of range".into(),
                ));
            }
            mux.push(value);
        }
        mux
    } else {
        vec![0; channels]
    };

    let mut submap_floors = Vec::with_capacity(submaps);
    let mut submap_residues = Vec::with_capacity(submaps);
    for _ in 0..submaps {
        let _time_config = bits.read_u8(8)?;
        let floor = bits.read_u8(8)? as usize;
        let residue = bits.read_u8(8)? as usize;
        if floor >= floor_count || residue >= residue_count {
            return Err(AudioError::InvalidFormat(
                "Vorbis mapping references an invalid floor or residue".into(),
            ));
        }
        submap_floors.push(floor);
        submap_residues.push(residue);
    }

    Ok(Mapping {
        magnitudes,
        angles,
        mux,
        submap_floors,
        submap_residues,
    })
}

fn compute_bark_cos(n: usize, rate: u16, bark_map_size: u16) -> Vec<f32> {
    fn bark(x: f32) -> f32 {
        13.1 * (0.00074 * x).atan()
            + 2.24 * (0.0000000185 * x * x).atan()
            + 0.0001 * x
    }

    let half_rate = rate as f32 / 2.0;
    let hz_per_bin = half_rate / n.max(1) as f32;
    let scale = bark_map_size as f32 / bark(half_rate);
    let max_map = bark_map_size.saturating_sub(1) as f32;
    let omega_factor = std::f32::consts::PI / bark_map_size as f32;

    (0..n)
        .map(|i| {
            let mapped = (bark(i as f32 * hz_per_bin) * scale).floor().min(max_map);
            (mapped * omega_factor).cos()
        })
        .collect()
}

fn verify_byte_header(packet: &[u8], packet_type: u8) -> Result<(), AudioError> {
    if packet.len() < 7 {
        return Err(AudioError::TruncatedData);
    }
    if packet[0] != packet_type || &packet[1..7] != VORBIS_SIGNATURE {
        return Err(AudioError::InvalidHeader(format!(
            "Expected Vorbis header packet type {packet_type}"
        )));
    }
    Ok(())
}

fn verify_bit_header(bits: &mut BitReader<'_>, packet_type: u8) -> Result<(), AudioError> {
    if bits.read_u8(8)? != packet_type {
        return Err(AudioError::InvalidHeader(format!(
            "Expected Vorbis header packet type {packet_type}"
        )));
    }
    for &byte in VORBIS_SIGNATURE {
        if bits.read_u8(8)? != byte {
            return Err(AudioError::InvalidHeader(
                "Missing Vorbis capture pattern".into(),
            ));
        }
    }
    Ok(())
}

#[inline]
fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[inline]
fn le_i32(bytes: &[u8]) -> i32 {
    i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

fn take<'a>(data: &'a [u8], cursor: &mut usize, count: usize) -> Result<&'a [u8], AudioError> {
    let end = cursor.checked_add(count).ok_or_else(|| {
        AudioError::InvalidFormat("Vorbis comment size overflow".into())
    })?;
    let out = data.get(*cursor..end).ok_or(AudioError::TruncatedData)?;
    *cursor = end;
    Ok(out)
}

fn take_u32(data: &[u8], cursor: &mut usize) -> Result<u32, AudioError> {
    let bytes = take(data, cursor, 4)?;
    Ok(le_u32(bytes))
}
