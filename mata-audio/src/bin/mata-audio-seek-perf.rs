use std::{env,fs::File,io::BufReader,path::PathBuf,time::Instant};
use mata_audio::{AudioDecoder,Decoder,AudioError,AudioFrameBuffer};

fn main()->Result<(),Box<dyn std::error::Error>>{
    let root=PathBuf::from(env::args().nth(1).unwrap_or_else(||"performance/corpus".into()));
    println!("file,seeks,total_us,decoded_samples");
    for name in ["wav_s16_stereo_44100.wav","aiff_s16_stereo_44100.aiff","flac_s16_stereo_44100.flac","mpeg_layer3_128k_44100.mp3","vorbis_96k_48000.ogg"]{
        let path=root.join(name);if !path.exists(){continue}let mut decoder=AudioDecoder::new(BufReader::new(File::open(&path)?))?;let rate=decoder.sample_rate() as u64;let mut seed=0x9E3779B97F4A7C15u64;let mut out=AudioFrameBuffer::new();let start=Instant::now();let mut decoded=0usize;
        for _ in 0..128{seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;let target=seed%(rate*2).max(1);let _=decoder.seek_to_sample(target)?;match decoder.decode_into(&mut out){Ok(frame)=>decoded+=frame.samples.len(),Err(AudioError::NoNextFrame)=>{},Err(e)=>return Err(e.into())}}
        println!("{name},128,{},{}",start.elapsed().as_micros(),decoded);
    }Ok(())
}
