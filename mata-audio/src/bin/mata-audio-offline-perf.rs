//! Offline pipeline/read-ahead/mmap performance harness for P575-P601.

use mata_audio::{codecs::offline::{self, OfflineDecodeOptions}, AudioDecoder, AudioError, AudioFrameBuffer, Decoder};
use std::{fs::File, io::BufReader, path::{Path, PathBuf}, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args=std::env::args().skip(1);
    let path=PathBuf::from(args.next().unwrap_or_else(||"performance/corpus/flac_s16_stereo_44100.flac".into()));
    let iterations=args.next().and_then(|v|v.parse::<usize>().ok()).unwrap_or(3).max(1);
    let mut opts=OfflineDecodeOptions::default();
    if let Some(v)=args.next().and_then(|v|v.parse::<usize>().ok()){opts.read_ahead_bytes=v;}
    if let Some(v)=args.next().and_then(|v|v.parse::<usize>().ok()){opts.read_chunk_bytes=v;}

    let normal=median_ns(iterations,||decode_normal(&path))?;
    let read_ahead=median_ns(iterations,||offline::decode_file(&path,opts).map(|v|v.samples.len()))?;
    println!("mode,ns");
    println!("normal,{normal}");
    println!("read_ahead,{read_ahead}");
    #[cfg(feature="mmap")]
    {
        let mmap=median_ns(iterations,||offline::decode_mmap_file(&path).map(|v|v.samples.len()))?;
        println!("mmap,{mmap}");
    }
    Ok(())
}

fn median_ns<F>(iterations:usize,mut f:F)->Result<u128,AudioError> where F:FnMut()->Result<usize,AudioError>{
    let mut values=Vec::with_capacity(iterations);
    for _ in 0..iterations{let t=Instant::now();std::hint::black_box(f()?);values.push(t.elapsed().as_nanos());}
    values.sort_unstable();Ok(values[values.len()/2])
}
fn decode_normal(path:&Path)->Result<usize,AudioError>{
    let mut d=AudioDecoder::new(BufReader::with_capacity(128*1024,File::open(path)?))?;
    let mut b=AudioFrameBuffer::with_capacity(d.preferred_frame_samples().unwrap_or(4096));let mut samples=0;
    loop{match d.decode_into(&mut b){Ok(f)=>samples+=f.samples.len(),Err(AudioError::NoNextFrame)=>break,Err(e)=>return Err(e)}}Ok(samples)
}
