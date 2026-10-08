//! Deterministic mutation smoke-fuzzer for all built-in demuxers (P358).
use mata_audio::demuxers::{DemuxerSeek,aiff::AiffDemuxer,flac::FlacDemuxer,matroska::MatroskaDemuxer,ogg::OggDemuxer,wav::WavDemuxer};
use std::{fs,io::Cursor,path::{Path,PathBuf}};

fn exercise(kind:&str,data:Vec<u8>){
    match kind{
        "wav"=>{if let Ok(mut d)=WavDemuxer::new(Cursor::new(data)){let mut b=Vec::new();let _=d.next_pcm_chunk_into(&mut b);let _=d.seek_time_seconds(0.0);}}
        "aiff"=>{if let Ok(mut d)=AiffDemuxer::new(Cursor::new(data)){let mut b=Vec::new();let _=d.next_pcm_chunk_into(&mut b);let _=d.seek_time_seconds(0.0);}}
        "flac"=>{if let Ok(mut d)=FlacDemuxer::new(Cursor::new(data)){let _=d.seek_time_seconds(0.0);}}
        "ogg"=>{if let Ok(mut d)=OggDemuxer::new(Cursor::new(data)){let _=d.next_packet();let _=d.seek_granule(0);}}
        "matroska"=>{if let Ok(mut d)=MatroskaDemuxer::new(Cursor::new(data)){let _=d.next_packet();let _=d.seek_time_seconds(0.0);}}
        _=>{}
    }
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let manifest=PathBuf::from(std::env::args().nth(1).unwrap_or_else(||"performance/demuxer_manifest.tsv".into()));
    let base=manifest.parent().unwrap_or(Path::new("."));
    for (i,line) in fs::read_to_string(&manifest)?.lines().enumerate(){
        if i==0||line.trim().is_empty(){continue}let c:Vec<_>=line.split('\t').collect();let original=fs::read(base.join(c[2]))?;exercise(c[1],original.clone());
        let mut cuts=vec![0,1,2,3,4,7,8,11,12,16,24,32,64,128,original.len()/2,original.len().saturating_sub(1)];cuts.sort_unstable();cuts.dedup();
        for cut in cuts{if cut<=original.len(){exercise(c[1],original[..cut].to_vec());}}
        for seed in 0..64usize{let mut m=original.clone();if !m.is_empty(){let at=seed.wrapping_mul(2654435761usize)%m.len();m[at]^=(1u8).rotate_left((seed&7)as u32);if seed%8==0{for b in m.iter_mut().skip(at).take(4){*b=0xff;}}}exercise(c[1],m);}
    }
    Ok(())
}
