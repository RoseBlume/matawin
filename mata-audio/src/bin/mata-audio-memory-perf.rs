//! Decoder retained/high-water memory reporter for P502-P526.

use mata_audio::{AudioDecoder, AudioError, AudioFrameBuffer, Decoder, DecoderMemoryUsage};
use std::{fs::{self,File},io::BufReader,path::{Path,PathBuf},time::{Duration,Instant}};

#[derive(Debug)]
struct Entry{ id:String,path:PathBuf }

fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut args=std::env::args().skip(1);
    let manifest=PathBuf::from(args.next().unwrap_or_else(||"performance/corpus/manifest.tsv".into()));
    let output=PathBuf::from(args.next().unwrap_or_else(||"performance/results/memory.csv".into()));
    let steady_seconds=args.next().and_then(|v|v.parse::<u64>().ok()).unwrap_or(0);
    let entries=read_manifest(&manifest)?;
    if let Some(parent)=output.parent(){fs::create_dir_all(parent)?;}
    let mut rows=String::from("id,persistent_state_bytes,scratch_bytes,compressed_bytes,index_bytes,metadata_bytes,retained_bytes,high_water_bytes,steady_growth_bytes\n");
    for entry in entries{
        let mut decoder=open(&entry.path)?;
        let mut frame=AudioFrameBuffer::with_capacity(decoder.preferred_frame_samples().unwrap_or(4096));
        let mut peak=decoder.memory_usage();
        decode_pass(&mut decoder,&mut frame,&mut peak)?;
        let warm_high=peak.high_water_bytes();
        let started=Instant::now();
        if steady_seconds!=0{
            while started.elapsed()<Duration::from_secs(steady_seconds){
                if !decoder.reset_decoder()?{break;}
                decode_pass(&mut decoder,&mut frame,&mut peak)?;
            }
        }
        let final_usage=decoder.memory_usage();
        let growth=final_usage.high_water_bytes().saturating_sub(warm_high);
        rows.push_str(&format!("{},{},{},{},{},{},{},{},{}\n",entry.id,final_usage.persistent_state_bytes,final_usage.scratch_capacity_bytes,final_usage.compressed_capacity_bytes,final_usage.index_capacity_bytes,final_usage.metadata_capacity_bytes,final_usage.retained_bytes(),peak.high_water_bytes(),growth));
    }
    fs::write(&output,rows)?;
    println!("wrote {}",output.display());
    Ok(())
}

fn decode_pass<R:std::io::Read+std::io::Seek>(decoder:&mut AudioDecoder<R>,frame:&mut AudioFrameBuffer,peak:&mut DecoderMemoryUsage)->Result<(),AudioError>{
    loop{
        match decoder.decode_into(frame){
            Ok(_)=>merge_peak(peak,decoder.memory_usage()),
            Err(AudioError::NoNextFrame)=>break,
            Err(error)=>return Err(error),
        }
    }
    Ok(())
}
fn merge_peak(peak:&mut DecoderMemoryUsage,current:DecoderMemoryUsage){
    peak.persistent_state_bytes=peak.persistent_state_bytes.max(current.persistent_state_bytes);
    peak.scratch_capacity_bytes=peak.scratch_capacity_bytes.max(current.scratch_capacity_bytes);
    peak.compressed_capacity_bytes=peak.compressed_capacity_bytes.max(current.compressed_capacity_bytes);
    peak.index_capacity_bytes=peak.index_capacity_bytes.max(current.index_capacity_bytes);
    peak.metadata_capacity_bytes=peak.metadata_capacity_bytes.max(current.metadata_capacity_bytes);
    peak.peak_scratch_capacity_bytes=peak.peak_scratch_capacity_bytes.max(current.peak_scratch_capacity_bytes).max(current.scratch_capacity_bytes);
    peak.peak_compressed_capacity_bytes=peak.peak_compressed_capacity_bytes.max(current.peak_compressed_capacity_bytes).max(current.compressed_capacity_bytes);
}
fn open(path:&Path)->Result<AudioDecoder<BufReader<File>>,AudioError>{AudioDecoder::new(BufReader::with_capacity(128*1024,File::open(path)?))}
fn read_manifest(path:&Path)->Result<Vec<Entry>,Box<dyn std::error::Error>>{
    let root=path.parent().unwrap_or(Path::new("."));let text=fs::read_to_string(path)?;let mut out=Vec::new();
    for(index,line)in text.lines().enumerate(){if index==0||line.trim().is_empty(){continue}let cols:Vec<&str>=line.split('\t').collect();if cols.len()<2{return Err(format!("bad manifest row {}",index+1).into())}out.push(Entry{id:cols[0].into(),path:root.join(cols[1])});}Ok(out)
}
