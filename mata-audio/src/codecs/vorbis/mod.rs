mod audio;
mod bit_reader;
mod codebook;
mod floor;
mod header;
mod imdct;
mod residue;

use std::io::{Cursor, Read, Seek};

use crate::{
    codecs::{AudioFrameBuffer, DecodedFrame, Decoder, DecoderMemoryUsage},
    AudioError,
};

use audio::{decode_audio_packet, DecoderState};
use super::support::StageTimer;
use header::{parse_comments, parse_identification, parse_setup, Setup};
use crate::demuxers::ogg::{OggDemuxer, OggPacket};

pub use header::{VorbisComments, VorbisIdent};

/// Streaming Ogg/Vorbis decoder with retained packet, floor, residue, IMDCT,
/// overlap and final interleave storage.
pub struct Vorbis<R>
where R: Read + Seek,
{
    ogg: OggDemuxer<R>,
    identification: VorbisIdent,
    comments: Box<VorbisComments>,
    setup: Box<Setup>,
    state: DecoderState,
    emitted_samples: u64,
    ended: bool,
}

impl<R> Vorbis<R>
where R: Read + Seek,
{
    pub fn new(reader:R)->Result<Self,AudioError>{
        let mut ogg=OggDemuxer::new(reader)?;
        let identification_packet=required_packet(&mut ogg,"identification")?;
        let identification=parse_identification(&identification_packet.data)?;
        let comment_packet=required_packet(&mut ogg,"comment")?;
        let comments=parse_comments(&comment_packet.data)?;
        let setup_packet=required_packet(&mut ogg,"setup")?;
        let setup=Box::new(parse_setup(&setup_packet.data,&identification)?);
        if setup.modes.is_empty(){return Err(AudioError::InvalidFormat("Vorbis setup header defines no modes".into()));}
        let state=DecoderState::new(&identification)?;
        Ok(Self{ogg,identification,comments:Box::new(comments),setup,state,emitted_samples:0,ended:false})
    }

    #[inline]pub fn identification(&self)->&VorbisIdent{&self.identification}
    #[inline]pub fn comments(&self)->&VorbisComments{self.comments.as_ref()}
    #[inline]pub fn emitted_samples(&self)->u64{self.emitted_samples}
    #[inline]pub fn ogg_page_index(&self)->&[crate::demuxers::ogg::OggPageIndexPoint]{self.ogg.page_index()}

    pub fn comment(&self,key:&str)->Option<&str>{self.comments.comments.iter().find(|(name,_)|name.eq_ignore_ascii_case(key)).map(|(_,value)|value.as_str())}

    fn cold_metadata_bytes(&self)->usize{
        self.comments.vendor.capacity()+self.comments.comments.capacity()*core::mem::size_of::<(String,String)>()
            +self.comments.comments.iter().map(|(k,v)|k.capacity()+v.capacity()).sum::<usize>()
            +self.setup.retained_bytes()
    }

    /// Reset without reparsing the comment or setup structures. The three Ogg
    /// header packets are consumed to restore packet position, but only the
    /// compact identification invariants/signatures are checked.
    pub fn reset(&mut self)->Result<(),AudioError>{
        self.ogg.reset()?;
        let ident_packet=required_packet(&mut self.ogg,"identification")?;
        let ident=parse_identification(&ident_packet.data)?;
        if ident.channels!=self.identification.channels||ident.sample_rate!=self.identification.sample_rate||ident.blocksize_small!=self.identification.blocksize_small||ident.blocksize_large!=self.identification.blocksize_large{return Err(AudioError::InvalidFormat("Vorbis stream headers changed while resetting".into()));}
        let comment_packet=required_packet(&mut self.ogg,"comment")?;verify_header_signature(&comment_packet.data,3,"comment")?;
        let setup_packet=required_packet(&mut self.ogg,"setup")?;verify_header_signature(&setup_packet.data,5,"setup")?;
        self.state.reset();self.emitted_samples=0;self.ended=false;Ok(())
    }

    /// Seek using the demuxer's learned Ogg page/granule index. The demuxer
    /// intentionally prerolls one indexed page when possible so overlap state
    /// can be rebuilt. The returned sample is the known timeline lower bound.
    pub fn seek_to_sample(&mut self,sample:u64)->Result<u64,AudioError>{
        let start=self.ogg.seek_granule(sample)?;self.state.reset();self.emitted_samples=start;self.ended=false;Ok(start)
    }

    pub fn seek_to_seconds(&mut self,seconds:f64)->Result<u64,AudioError>{
        if !seconds.is_finite()||seconds<0.0{return Err(AudioError::InvalidFormat("Vorbis seek time must be finite and non-negative".into()));}
        let sample=(seconds*self.identification.sample_rate as f64).round()as u64;self.seek_to_sample(sample)
    }
}

impl Vorbis<Cursor<Vec<u8>>>{pub fn from_bytes(bytes:Vec<u8>)->Result<Self,AudioError>{Self::new(Cursor::new(bytes))}}

impl<R> Decoder for Vorbis<R>
where R:Read+Seek,
{
    #[inline]fn sample_rate(&self)->u32{self.identification.sample_rate}
    #[inline]fn channels(&self)->u16{self.identification.channels as u16}
    fn maximum_frame_samples(&self)->Option<usize>{self.identification.blocksize_large.checked_mul(self.identification.channels as usize)}
    fn memory_usage(&self)->DecoderMemoryUsage{
        let scratch=self.state.retained_bytes();
        DecoderMemoryUsage{persistent_state_bytes:core::mem::size_of_val(self),scratch_capacity_bytes:scratch,compressed_capacity_bytes:self.ogg.retained_buffer_bytes(),index_capacity_bytes:self.ogg.index_capacity_bytes(),metadata_capacity_bytes:self.cold_metadata_bytes(),peak_scratch_capacity_bytes:self.state.peak_retained_bytes(),peak_compressed_capacity_bytes:self.ogg.peak_buffer_bytes(),}
    }
    fn trim_memory(&mut self){self.state.trim_memory();self.ogg.trim_memory_non_realtime();}

    fn decode_into<'a>(&mut self,output:&'a mut AudioFrameBuffer)->Result<DecodedFrame<'a>,AudioError>{
        let _stage_timer=StageTimer::start("vorbis.decode");if self.ended{return Err(AudioError::NoNextFrame);}let channels=self.identification.channels as usize;
        loop{
            let (frames,granule,eos)={
                let packet=self.ogg.next_packet_view()?.ok_or(AudioError::NoNextFrame)?;
                let granule=packet.granule_position;let eos=packet.end_of_stream;
                let frames=decode_audio_packet(&self.identification,&self.setup,packet.data,&mut self.state)?;
                (frames,granule,eos)
            };

            let mut frames=frames;
            if eos{
                if let Some(granule)=granule{
                    if granule<self.emitted_samples{return Err(AudioError::InvalidFormat(format!("Vorbis EOS granule position {granule} precedes {} already-emitted samples",self.emitted_samples)));}
                    let remaining=granule-self.emitted_samples;
                    if remaining<frames as u64{frames=remaining as usize;self.state.output_vec_mut().truncate(frames*channels);}else if remaining>frames as u64&&frames!=0{return Err(AudioError::TruncatedData);}
                }
                self.ended=true;
            }
            if frames==0{if self.ended{return Err(AudioError::NoNextFrame);}continue;}
            self.emitted_samples=self.emitted_samples.checked_add(frames as u64).ok_or_else(||AudioError::InvalidFormat("Vorbis sample counter overflow".into()))?;
            output.swap_samples(self.state.output_vec_mut());
            output.truncate(frames*channels);
            return Ok(DecodedFrame{samples:output.samples(),channels:self.identification.channels as u16,sample_rate:self.identification.sample_rate});
        }
    }
}

fn verify_header_signature(packet:&[u8],kind:u8,name:&str)->Result<(),AudioError>{if packet.len()<7||packet[0]!=kind||&packet[1..7]!=b"vorbis"{return Err(AudioError::InvalidHeader(format!("Invalid Vorbis {name} header while resetting")));}Ok(())}
fn required_packet<R>(ogg:&mut OggDemuxer<R>,name:&str)->Result<OggPacket,AudioError>where R:Read+Seek{ogg.next_packet()?.ok_or_else(||AudioError::InvalidHeader(format!("Missing Vorbis {name} header packet")))}
