//! Developer-only full-stream PCM emission for P602 cross-revision comparisons.
//! Usage: cargo run --release --no-default-features --features auto-detect \
//!   --example emit_pcm -- INPUT OUTPUT.f32
use mata_audio::{AudioDecoder,AudioError,AudioFrameBuffer,Decoder};
use std::{env,fs::File,io::{BufReader,BufWriter,Write}};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut args=env::args().skip(1);
    let input=args.next().ok_or("usage: emit_pcm INPUT OUTPUT.f32")?;
    let output=args.next().ok_or("usage: emit_pcm INPUT OUTPUT.f32")?;
    let mut decoder=AudioDecoder::new(BufReader::new(File::open(input)?))?;
    let mut frame=AudioFrameBuffer::default();
    let mut writer=BufWriter::new(File::create(output)?);
    loop {match decoder.decode_into(&mut frame){
        Ok(decoded)=>for s in decoded.samples{writer.write_all(&s.to_le_bytes())?},
        Err(AudioError::NoNextFrame)=>break,
        Err(e)=>return Err(e.into()),
    }}
    writer.flush()?;Ok(())
}
