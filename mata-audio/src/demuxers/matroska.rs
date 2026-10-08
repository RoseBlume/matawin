//! Matroska/WebM demuxer.
//!
//! The demuxer is codec-agnostic. It exposes encoded packets and track
//! descriptors; callers choose an appropriate decoder from [`crate::codecs`].

use std::{collections::VecDeque, io::{Read, Seek, SeekFrom}};
use crate::{AudioError, demuxers::{DemuxerSeek, SeekCapability}};

const ID_EBML: u64 = 0x1A45_DFA3;
const ID_SEGMENT: u64 = 0x1853_8067;
const ID_INFO: u64 = 0x1549_A966;
const ID_TIMECODE_SCALE: u64 = 0x2AD7_B1;
const ID_TRACKS: u64 = 0x1654_AE6B;
const ID_TRACK_ENTRY: u64 = 0xAE;
const ID_TRACK_NUMBER: u64 = 0xD7;
const ID_TRACK_TYPE: u64 = 0x83;
const ID_CODEC_ID: u64 = 0x86;
const ID_CODEC_PRIVATE: u64 = 0x63A2;
const ID_AUDIO: u64 = 0xE1;
const ID_SAMPLING_FREQUENCY: u64 = 0xB5;
const ID_CHANNELS: u64 = 0x9F;
const ID_BIT_DEPTH: u64 = 0x6264;
const ID_CLUSTER: u64 = 0x1F43_B675;
const ID_CLUSTER_TIMECODE: u64 = 0xE7;
const ID_SIMPLE_BLOCK: u64 = 0xA3;
const ID_BLOCK_GROUP: u64 = 0xA0;
const ID_BLOCK: u64 = 0xA1;
const ID_REFERENCE_BLOCK: u64 = 0xFB;

const MAX_BLOCK_PAYLOAD: u64 = 64 * 1024 * 1024;
const MAX_CODEC_PRIVATE: u64 = 16 * 1024 * 1024;
const MAX_TEXT_FIELD: u64 = 1024 * 1024;
const MAX_CLUSTER_INDEX_POINTS: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub struct MatroskaTrack {
    pub number: u64,
    pub track_type: u64,
    pub codec_id: String,
    pub codec_private: Vec<u8>,
    pub sample_rate: Option<f64>,
    pub channels: Option<u64>,
    pub bit_depth: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatroskaClusterPoint {
    pub byte_offset: u64,
    pub timestamp_ns: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatroskaPacket {
    pub track_number: u64,
    pub timestamp_ns: i64,
    pub keyframe: bool,
    pub invisible: bool,
    pub data: Vec<u8>,
}

pub struct MatroskaDemuxer<R: Read + Seek> {
    reader: R,
    segment_start: u64,
    segment_end: u64,
    first_cluster: u64,
    timecode_scale: u64,
    tracks: Vec<MatroskaTrack>,
    cluster_end: Option<u64>,
    cluster_offset: u64,
    cluster_timecode: i64,
    cluster_index: Vec<MatroskaClusterPoint>,
    queued: VecDeque<MatroskaPacket>,
}

impl<R: Read + Seek> MatroskaDemuxer<R> {
    pub fn new(mut reader: R) -> Result<Self, AudioError> {
        let file_end = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        let ebml = read_element_header(&mut reader, file_end)?;
        if ebml.id != ID_EBML {
            return Err(AudioError::InvalidHeader("Missing Matroska EBML header".into()));
        }
        reader.seek(SeekFrom::Start(ebml.end))?;

        let mut segment = None;
        while reader.stream_position()? < file_end {
            let h = read_element_header(&mut reader, file_end)?;
            if h.id == ID_SEGMENT { segment = Some(h); break; }
            reader.seek(SeekFrom::Start(h.end))?;
        }
        let segment = segment.ok_or_else(|| AudioError::InvalidHeader("Missing Matroska Segment".into()))?;
        let segment_start = segment.data_start;
        let segment_end = segment.end;
        let mut first_cluster = None;
        let mut timecode_scale = 1_000_000u64;
        let mut tracks = Vec::new();

        let mut pos = segment_start;
        while pos < segment_end {
            reader.seek(SeekFrom::Start(pos))?;
            let h = read_element_header(&mut reader, segment_end)?;
            match h.id {
                ID_INFO => parse_info(&mut reader, h.data_start, h.end, &mut timecode_scale)?,
                ID_TRACKS => tracks = parse_tracks(&mut reader, h.data_start, h.end)?,
                ID_CLUSTER => {
                    if first_cluster.is_none() { first_cluster = Some(pos); }
                    if h.unknown_size {
                        // Unknown-size clusters are legal for streaming. Once
                        // tracks are known, packet iteration can take over.
                        if !tracks.is_empty() { break; }
                    }
                }
                _ => {}
            }
            if h.unknown_size { break; }
            pos = h.end;
        }

        let first_cluster = first_cluster.ok_or_else(|| AudioError::InvalidFormat("Matroska contains no Cluster".into()))?;
        if tracks.is_empty() {
            return Err(AudioError::InvalidFormat("Matroska contains no parsed Tracks".into()));
        }
        reader.seek(SeekFrom::Start(first_cluster))?;
        Ok(Self {
            reader,
            segment_start,
            segment_end,
            first_cluster,
            timecode_scale,
            tracks,
            cluster_end: None,
            cluster_offset: first_cluster,
            cluster_timecode: 0,
            cluster_index: vec![MatroskaClusterPoint { byte_offset: first_cluster, timestamp_ns: 0 }],
            queued: VecDeque::new(),
        })
    }

    pub fn tracks(&self) -> &[MatroskaTrack] { &self.tracks }
    pub fn timecode_scale(&self) -> u64 { self.timecode_scale }
    pub fn segment_start(&self) -> u64 { self.segment_start }
    pub fn cluster_index(&self) -> &[MatroskaClusterPoint] { &self.cluster_index }

    pub fn reset(&mut self) -> Result<(), AudioError> {
        self.reader.seek(SeekFrom::Start(self.first_cluster))?;
        self.cluster_end = None;
        self.cluster_offset = self.first_cluster;
        self.cluster_timecode = 0;
        self.queued.clear();
        Ok(())
    }

    pub fn into_inner(self) -> R { self.reader }

    pub fn next_packet(&mut self) -> Result<Option<MatroskaPacket>, AudioError> {
        loop {
            if let Some(packet) = self.queued.pop_front() { return Ok(Some(packet)); }
            let pos = self.reader.stream_position()?;
            if pos >= self.segment_end { return Ok(None); }

            if let Some(cluster_end) = self.cluster_end {
                if pos >= cluster_end {
                    self.cluster_end = None;
                    continue;
                }
                let h = read_element_header(&mut self.reader, cluster_end)?;
                match h.id {
                    ID_CLUSTER_TIMECODE => {
                        self.cluster_timecode = read_uint(&mut self.reader, h.data_len()? )? as i64;
                        let timestamp_ns = self.cluster_timecode
                            .checked_mul(i64::try_from(self.timecode_scale).map_err(|_| AudioError::InvalidFormat("Matroska TimecodeScale too large".into()))?)
                            .ok_or_else(|| AudioError::InvalidFormat("Matroska cluster timestamp overflow".into()))?;
                        push_cluster_index(&mut self.cluster_index, MatroskaClusterPoint {
                            byte_offset: self.cluster_offset, timestamp_ns,
                        });
                    }
                    ID_SIMPLE_BLOCK => {
                        let len = bounded_payload_len(h.data_len()?, MAX_BLOCK_PAYLOAD, "Matroska SimpleBlock")?;
                        let mut bytes = vec![0u8; len];
                        self.reader.read_exact(&mut bytes)?;
                        enqueue_block(&bytes, self.cluster_timecode, self.timecode_scale, false, &mut self.queued)?;
                    }
                    ID_BLOCK_GROUP => {
                        let packets = parse_block_group(&mut self.reader, h.data_start, h.end, self.cluster_timecode, self.timecode_scale)?;
                        self.queued.extend(packets);
                    }
                    ID_CLUSTER if self.cluster_end == Some(self.segment_end) => {
                        // Streaming files may use unknown-sized Clusters. A new
                        // Cluster closes the previous one implicitly.
                        self.cluster_end = Some(h.end);
                        self.cluster_offset = pos;
                        self.cluster_timecode = 0;
                        continue;
                    }
                    _ => {}
                }
                self.reader.seek(SeekFrom::Start(h.end))?;
                continue;
            }

            let h = read_element_header(&mut self.reader, self.segment_end)?;
            match h.id {
                ID_CLUSTER => {
                    self.cluster_offset = pos;
                    self.cluster_end = Some(if h.unknown_size { self.segment_end } else { h.end });
                    self.cluster_timecode = 0;
                    push_cluster_index(&mut self.cluster_index, MatroskaClusterPoint { byte_offset: pos, timestamp_ns: 0 });
                    self.reader.seek(SeekFrom::Start(h.data_start))?;
                }
                ID_INFO => {
                    parse_info(&mut self.reader, h.data_start, h.end, &mut self.timecode_scale)?;
                    self.reader.seek(SeekFrom::Start(h.end))?;
                }
                _ => { self.reader.seek(SeekFrom::Start(h.end))?; },
            }
        }
    }
}

fn parse_info<R: Read + Seek>(reader: &mut R, start: u64, end: u64, scale: &mut u64) -> Result<(), AudioError> {
    let mut pos=start;
    while pos < end {
        reader.seek(SeekFrom::Start(pos))?; let h=read_element_header(reader,end)?;
        if h.id == ID_TIMECODE_SCALE { *scale = read_uint(reader,h.data_len()?)?; if *scale==0 { return Err(AudioError::InvalidFormat("Matroska TimecodeScale is zero".into())); } }
        pos=h.end;
    }
    Ok(())
}

fn parse_tracks<R: Read + Seek>(reader: &mut R, start:u64, end:u64)->Result<Vec<MatroskaTrack>,AudioError>{
    let mut out=Vec::new(); let mut pos=start;
    while pos<end { reader.seek(SeekFrom::Start(pos))?; let h=read_element_header(reader,end)?; if h.id==ID_TRACK_ENTRY { out.push(parse_track_entry(reader,h.data_start,h.end)?); } pos=h.end; }
    Ok(out)
}

fn parse_track_entry<R:Read+Seek>(reader:&mut R,start:u64,end:u64)->Result<MatroskaTrack,AudioError>{
    let mut track=MatroskaTrack{number:0,track_type:0,codec_id:String::new(),codec_private:Vec::new(),sample_rate:None,channels:None,bit_depth:None};
    let mut pos=start;
    while pos<end { reader.seek(SeekFrom::Start(pos))?; let h=read_element_header(reader,end)?; match h.id {
        ID_TRACK_NUMBER=>track.number=read_uint(reader,h.data_len()?)?,
        ID_TRACK_TYPE=>track.track_type=read_uint(reader,h.data_len()?)?,
        ID_CODEC_ID=>{let mut b=vec![0u8;bounded_payload_len(h.data_len()?,MAX_TEXT_FIELD,"Matroska CodecID")?];reader.read_exact(&mut b)?;track.codec_id=String::from_utf8_lossy(&b).into_owned();},
        ID_CODEC_PRIVATE=>{track.codec_private=vec![0u8;bounded_payload_len(h.data_len()?,MAX_CODEC_PRIVATE,"Matroska CodecPrivate")?];reader.read_exact(&mut track.codec_private)?;},
        ID_AUDIO=>parse_audio(reader,h.data_start,h.end,&mut track)?,
        _=>{}
    } pos=h.end; }
    if track.number==0 || track.codec_id.is_empty(){return Err(AudioError::InvalidFormat("Matroska TrackEntry is missing TrackNumber or CodecID".into()));}
    Ok(track)
}

fn parse_audio<R:Read+Seek>(reader:&mut R,start:u64,end:u64,track:&mut MatroskaTrack)->Result<(),AudioError>{
    let mut pos=start; while pos<end { reader.seek(SeekFrom::Start(pos))?; let h=read_element_header(reader,end)?; match h.id {
        ID_SAMPLING_FREQUENCY=>track.sample_rate=Some(read_float(reader,h.data_len()?)?),
        ID_CHANNELS=>track.channels=Some(read_uint(reader,h.data_len()?)?),
        ID_BIT_DEPTH=>track.bit_depth=Some(read_uint(reader,h.data_len()?)?),
        _=>{}
    } pos=h.end;} Ok(())
}

fn parse_block_group<R:Read+Seek>(reader:&mut R,start:u64,end:u64,cluster_tc:i64,scale:u64)->Result<Vec<MatroskaPacket>,AudioError>{
    let mut block=None; let mut has_reference=false; let mut pos=start;
    while pos<end {reader.seek(SeekFrom::Start(pos))?;let h=read_element_header(reader,end)?;match h.id{ID_BLOCK=>{let mut b=vec![0u8;bounded_payload_len(h.data_len()?,MAX_BLOCK_PAYLOAD,"Matroska Block")?];reader.read_exact(&mut b)?;block=Some(b);},ID_REFERENCE_BLOCK=>has_reference=true,_=>{}}pos=h.end;}
    let mut q=VecDeque::new(); if let Some(b)=block {enqueue_block(&b,cluster_tc,scale,!has_reference,&mut q)?;} Ok(q.into_iter().collect())
}

fn enqueue_block(bytes:&[u8],cluster_tc:i64,scale:u64,keyframe_default:bool,out:&mut VecDeque<MatroskaPacket>)->Result<(),AudioError>{
    let (track,used)=read_vint_from_slice(bytes,false)?; if used+3>bytes.len(){return Err(AudioError::TruncatedData);}
    let rel=i16::from_be_bytes([bytes[used],bytes[used+1]]) as i64; let flags=bytes[used+2]; let payload=&bytes[used+3..];
    let keyframe=if flags&0x80!=0{true}else{keyframe_default}; let invisible=flags&0x08!=0; let lacing=(flags>>1)&0x03;
    let frames=split_lacing(payload,lacing)?;
    let timecode=cluster_tc.checked_add(rel).ok_or_else(||AudioError::InvalidFormat("Matroska timecode overflow".into()))?;
    let ts=timecode.checked_mul(i64::try_from(scale).map_err(|_|AudioError::InvalidFormat("Matroska TimecodeScale too large".into()))?).ok_or_else(||AudioError::InvalidFormat("Matroska timestamp overflow".into()))?;
    for data in frames {out.push_back(MatroskaPacket{track_number:track,timestamp_ns:ts,keyframe,invisible,data});}
    Ok(())
}

fn split_lacing(payload:&[u8],kind:u8)->Result<Vec<Vec<u8>>,AudioError>{
    if kind==0{return Ok(vec![payload.to_vec()]);}
    if payload.is_empty(){return Err(AudioError::TruncatedData);} let count=payload[0] as usize+1; let mut pos=1usize; let mut sizes=Vec::with_capacity(count);
    match kind {
        1=>{ // Xiph
            for _ in 0..count-1 {let mut n=0usize;loop{if pos>=payload.len(){return Err(AudioError::TruncatedData);}let b=payload[pos] as usize;pos+=1;n=n.checked_add(b).ok_or_else(||AudioError::InvalidFormat("Matroska lace size overflow".into()))?;if b!=255{break;}}sizes.push(n);}
        }
        2=>{ // fixed
            let remaining=payload.len().saturating_sub(pos);if remaining%count!=0{return Err(AudioError::InvalidFormat("Invalid fixed Matroska lacing".into()));}let each=remaining/count;sizes.resize(count-1,each);
        }
        3=>{ // EBML
            let (first,u)=read_vint_from_slice(&payload[pos..],true)?;pos+=u;sizes.push(usize::try_from(first).map_err(|_|AudioError::InvalidFormat("Matroska lace too large".into()))?);
            for _ in 1..count-1 {let (raw,u)=read_vint_from_slice(&payload[pos..],true)?;pos+=u;let bias=(1i64<<(7*u-1))-1;let delta=i64::try_from(raw).unwrap_or(i64::MAX)-bias;let prev=*sizes.last().unwrap() as i64;let next=prev.checked_add(delta).ok_or_else(||AudioError::InvalidFormat("Matroska lace delta overflow".into()))?;if next<0{return Err(AudioError::InvalidFormat("Negative Matroska lace size".into()));}sizes.push(next as usize);}
        }
        _=>unreachable!(),
    }
    let explicit:usize=sizes.iter().try_fold(0usize,|a,&b|a.checked_add(b)).ok_or_else(||AudioError::InvalidFormat("Matroska lace sizes overflow".into()))?;
    if explicit>payload.len().saturating_sub(pos){return Err(AudioError::TruncatedData);} sizes.push(payload.len()-pos-explicit);
    let mut frames=Vec::with_capacity(count);for n in sizes{let end=pos.checked_add(n).ok_or_else(||AudioError::InvalidFormat("Matroska lace offset overflow".into()))?;if end>payload.len(){return Err(AudioError::TruncatedData);}frames.push(payload[pos..end].to_vec());pos=end;} Ok(frames)
}

#[derive(Clone,Copy)]
struct ElementHeader{id:u64,data_start:u64,end:u64,unknown_size:bool}
impl ElementHeader{fn data_len(self)->Result<u64,AudioError>{self.end.checked_sub(self.data_start).ok_or_else(||AudioError::InvalidFormat("Invalid EBML element bounds".into()))}}

fn read_element_header<R:Read+Seek>(reader:&mut R,parent_end:u64)->Result<ElementHeader,AudioError>{
    let start=reader.stream_position()?;
    if start>=parent_end{return Err(AudioError::TruncatedData);}
    let (id,_)=read_vint_reader(reader,false)?;
    let (size,_size_len,unknown)=read_size_vint(reader)?;
    let data_start=reader.stream_position()?;
    if unknown && id != ID_SEGMENT && id != ID_CLUSTER {
        return Err(AudioError::InvalidFormat(format!("unknown-sized EBML leaf/master 0x{id:x} is not supported")));
    }
    let end=if unknown{parent_end}else{data_start.checked_add(size).ok_or_else(||AudioError::InvalidFormat("EBML element size overflow".into()))?};
    if end>parent_end{return Err(AudioError::TruncatedData);}
    Ok(ElementHeader{id,data_start,end,unknown_size:unknown})
}

fn read_size_vint<R: Read>(reader: &mut R) -> Result<(u64, usize, bool), AudioError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes[..1])?;
    let len = vint_len(bytes[0])?;
    if len > 1 { reader.read_exact(&mut bytes[1..len])?; }
    let marker = 1u8 << (8 - len);
    let mut value = (bytes[0] & (marker - 1)) as u64;
    let mut all_ones = (bytes[0] & (marker - 1)) == marker - 1;
    for &byte in &bytes[1..len] { value = (value << 8) | byte as u64; all_ones &= byte == 0xff; }
    Ok((value, len, all_ones))
}

fn read_vint_reader<R: Read>(reader: &mut R, strip_marker: bool) -> Result<(u64, usize), AudioError> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes[..1])?;
    let len = vint_len(bytes[0])?;
    if len > 1 { reader.read_exact(&mut bytes[1..len])?; }
    let marker = 1u8 << (8 - len);
    let mut value = if strip_marker { (bytes[0] & (marker - 1)) as u64 } else { bytes[0] as u64 };
    for &byte in &bytes[1..len] { value = (value << 8) | byte as u64; }
    Ok((value, len))
}

fn read_vint_from_slice(data: &[u8], _strip_marker: bool) -> Result<(u64, usize), AudioError> {
    if data.is_empty() { return Err(AudioError::TruncatedData); }
    let len = vint_len(data[0])?;
    if data.len() < len { return Err(AudioError::TruncatedData); }
    // Block track numbers and EBML lace sizes are VINT payload values, so the
    // marker bit is stripped for both call sites.
    let marker = 1u8 << (8 - len);
    let mut value = (data[0] & (marker - 1)) as u64;
    for &byte in &data[1..len] { value = (value << 8) | byte as u64; }
    Ok((value, len))
}

fn vint_len(first: u8) -> Result<usize, AudioError> {
    let zeros = first.leading_zeros() as usize;
    let len = zeros + 1;
    if len <= 8 { Ok(len) } else { Err(AudioError::InvalidFormat("Invalid EBML variable-length integer".into())) }
}

fn read_uint<R:Read>(reader:&mut R,len:u64)->Result<u64,AudioError>{if len==0||len>8{return Err(AudioError::InvalidFormat(format!("Invalid EBML integer length {len}")));}let mut value=0u64;for _ in 0..len{let mut b=[0u8;1];reader.read_exact(&mut b)?;value=(value<<8)|b[0]as u64;}Ok(value)}
fn read_float<R:Read>(reader:&mut R,len:u64)->Result<f64,AudioError>{match len{4=>{let mut b=[0u8;4];reader.read_exact(&mut b)?;Ok(f32::from_be_bytes(b)as f64)},8=>{let mut b=[0u8;8];reader.read_exact(&mut b)?;Ok(f64::from_be_bytes(b))},_=>Err(AudioError::InvalidFormat(format!("Invalid EBML float length {len}")))}}
fn bounded_payload_len(value: u64, limit: u64, what: &str) -> Result<usize, AudioError> {
    if value > limit { return Err(AudioError::InvalidFormat(format!("{what} exceeds bounded payload limit ({value} > {limit})"))); }
    usize_len(value)
}

fn push_cluster_index(index: &mut Vec<MatroskaClusterPoint>, point: MatroskaClusterPoint) {
    if let Some(last) = index.last_mut() {
        if last.byte_offset == point.byte_offset {
            if point.timestamp_ns != 0 { last.timestamp_ns = point.timestamp_ns; }
            return;
        }
    }
    index.push(point);
    if index.len() > MAX_CLUSTER_INDEX_POINTS {
        let mut write = 1usize;
        for read in (2..index.len()).step_by(2) { index[write] = index[read]; write += 1; }
        index.truncate(write);
    }
}

impl<R: Read + Seek> DemuxerSeek for MatroskaDemuxer<R> {
    fn seek_capability(&self) -> SeekCapability { SeekCapability::IndexedApproximate }

    fn seek_time_seconds(&mut self, seconds: f64) -> Result<(), AudioError> {
        let target = (seconds.max(0.0) * 1_000_000_000.0) as i64;
        let idx = self.cluster_index.partition_point(|point| point.timestamp_ns <= target).saturating_sub(1);
        let point = self.cluster_index.get(idx).copied().unwrap_or(MatroskaClusterPoint { byte_offset: self.first_cluster, timestamp_ns: 0 });
        self.reader.seek(SeekFrom::Start(point.byte_offset))?;
        self.cluster_end = None;
        self.cluster_offset = point.byte_offset;
        self.cluster_timecode = if self.timecode_scale == 0 { 0 } else { point.timestamp_ns / self.timecode_scale as i64 };
        self.queued.clear();
        Ok(())
    }

    fn position_seconds(&self) -> Option<f64> {
        Some(self.cluster_timecode as f64 * self.timecode_scale as f64 / 1_000_000_000.0)
    }
}

fn usize_len(v:u64)->Result<usize,AudioError>{usize::try_from(v).map_err(|_|AudioError::InvalidFormat("EBML element too large for this platform".into()))}
