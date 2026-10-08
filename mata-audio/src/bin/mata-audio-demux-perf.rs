//! Dependency-free demuxer-only startup/throughput harness for P340/P359.
use mata_audio::demuxers::{aiff::AiffDemuxer, flac::FlacDemuxer, matroska::MatroskaDemuxer, ogg::OggDemuxer, wav::WavDemuxer};
use mata_audio::AudioError;
use std::{fs::{self,File}, hint::black_box, io::{BufReader,Read,Write}, path::{Path,PathBuf}, time::Instant};

fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut a=std::env::args().skip(1);
    let manifest=PathBuf::from(a.next().unwrap_or_else(||"performance/demuxer_manifest.tsv".into()));
    let output=PathBuf::from(a.next().unwrap_or_else(||"performance/results/demuxers.csv".into()));
    if let Some(parent)=output.parent(){fs::create_dir_all(parent)?;}
    let mut out=File::create(output)?;
    writeln!(out,"id,kind,startup_ns,throughput_ns,bytes,throughput_mib_s")?;
    let base=manifest.parent().unwrap_or(Path::new("."));
    for (line_no,line) in fs::read_to_string(&manifest)?.lines().enumerate(){
        if line_no==0||line.trim().is_empty(){continue}
        let c:Vec<_>=line.split('\t').collect(); if c.len()!=3{return Err(format!("bad demuxer manifest row {}",line_no+1).into())}
        let path=base.join(c[2]); let kind=c[1];
        let open=Instant::now();
        let (startup_ns,throughput_ns,bytes)=match kind{
            "wav"=>{let mut d=WavDemuxer::new(BufReader::new(File::open(&path)?))?;let startup=open.elapsed().as_nanos();let start=Instant::now();let mut b=Vec::new();let mut n=0u64;loop{match d.next_pcm_chunk_into(&mut b){Ok(())=>{n+=b.len()as u64;black_box(&b);},Err(AudioError::NoNextFrame)=>break,Err(e)=>return Err(e.into())}}(startup,start.elapsed().as_nanos(),n)}
            "aiff"=>{let mut d=AiffDemuxer::new(BufReader::new(File::open(&path)?))?;let startup=open.elapsed().as_nanos();let start=Instant::now();let mut b=Vec::new();let mut n=0u64;loop{match d.next_pcm_chunk_into(&mut b){Ok(())=>{n+=b.len()as u64;black_box(&b);},Err(AudioError::NoNextFrame)=>break,Err(e)=>return Err(e.into())}}(startup,start.elapsed().as_nanos(),n)}
            "flac"=>{let mut d=FlacDemuxer::new(BufReader::new(File::open(&path)?))?;let startup=open.elapsed().as_nanos();d.reset_audio()?;let start=Instant::now();let mut scratch=[0u8;65536];let mut n=0u64;loop{let r=d.reader_mut().read(&mut scratch)?;if r==0{break}n+=r as u64;black_box(&scratch[..r]);}(startup,start.elapsed().as_nanos(),n)}
            "ogg"=>{let mut d=OggDemuxer::new(BufReader::new(File::open(&path)?))?;let startup=open.elapsed().as_nanos();let start=Instant::now();let mut n=0u64;while let Some(p)=d.next_packet_view()?{n+=p.data.len()as u64;black_box(p.data);}(startup,start.elapsed().as_nanos(),n)}
            "matroska"=>{let mut d=MatroskaDemuxer::new(BufReader::new(File::open(&path)?))?;let startup=open.elapsed().as_nanos();let start=Instant::now();let mut n=0u64;while let Some(p)=d.next_packet()?{n+=p.data.len()as u64;black_box(p.data);}(startup,start.elapsed().as_nanos(),n)}
            _=>return Err(format!("unknown demuxer kind {kind}").into()),
        };
        let secs=throughput_ns as f64/1e9;let mib_s=if secs>0.0{bytes as f64/(1024.0*1024.0)/secs}else{0.0};
        writeln!(out,"{},{},{},{},{},{:.6}",c[0],kind,startup_ns,throughput_ns,bytes,mib_s)?;
    }
    Ok(())
}
