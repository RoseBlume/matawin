use std::{fs::File,io::BufReader,path::PathBuf};
use mata_audio::{AudioDecoder,Decoder,AudioError,AudioFrameBuffer};

fn corpus(name:&str)->PathBuf{PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("performance/corpus").join(name)}

#[test]
#[cfg(feature="auto-detect")]
fn repeated_native_seeks_stay_in_range(){
    for name in ["wav_s16_stereo_44100.wav","aiff_s16_stereo_44100.aiff","flac_s16_stereo_44100.flac","mpeg_layer3_128k_44100.mp3","vorbis_96k_48000.ogg"]{
        let p=corpus(name);if !p.exists(){continue}let mut d=AudioDecoder::new(BufReader::new(File::open(p).unwrap())).unwrap();let rate=d.sample_rate() as u64;let mut out=AudioFrameBuffer::new();
        for n in 0..32u64{let requested=n*rate/31;let actual=d.seek_to_sample(requested).unwrap().expect("native seek");assert!(actual<=requested||requested==0);match d.decode_into(&mut out){Ok(f)=>assert!(!f.samples.is_empty()),Err(AudioError::NoNextFrame)=>{},Err(e)=>panic!("{e}")}}
    }
}

#[test]
fn q32_timeline_has_no_hour_scale_drift(){
    for &(source,output) in &[(44100u64,48000u64),(48000,44100),(48000,48000)]{let step=((source as u128)<<32)/output as u128;let frames=output*60*60*8;let phase=step*frames as u128;let ideal=source as u128*60*60*8;let whole=phase>>32;let error=whole.abs_diff(ideal);assert!(error<=1,"{source}->{output} drifted {error} samples");}
}

#[test]
#[cfg(feature="auto-detect")]
fn exact_pcm_seek_and_loop_boundary_repeat_same_samples(){
    for name in ["wav_s16_stereo_44100.wav","aiff_s16_stereo_44100.aiff"]{
        let p=corpus(name);if !p.exists(){continue}
        let mut d=AudioDecoder::new(BufReader::new(File::open(p).unwrap())).unwrap();let mut a=AudioFrameBuffer::new();let frame=d.decode_into(&mut a).unwrap();let first=frame.samples[..128.min(frame.samples.len())].to_vec();
        assert_eq!(d.seek_to_sample(0).unwrap(),Some(0));let mut b=AudioFrameBuffer::new();let second=d.decode_into(&mut b).unwrap();assert_eq!(&first[..],&second.samples[..first.len()]);
        let target=137u64;assert_eq!(d.seek_to_sample(target).unwrap(),Some(target));
    }
}

#[test]
#[cfg(feature="auto-detect")]
fn loop_prefix_handoff_has_no_duplicate_or_missing_pcm_samples(){
    for name in ["wav_s16_stereo_44100.wav","aiff_s16_stereo_44100.aiff"]{
        let p=corpus(name);if !p.exists(){continue}
        let mut d=AudioDecoder::new(BufReader::new(File::open(p).unwrap())).unwrap();
        let channels=d.channels() as usize;let prefix_frames=64u64;let continuation_frames=64usize;
        let mut first_buf=AudioFrameBuffer::new();let first=d.decode_into(&mut first_buf).unwrap();
        let needed=(prefix_frames as usize+continuation_frames)*channels;assert!(first.samples.len()>=needed);
        let expected=first.samples[..needed].to_vec();
        assert_eq!(d.seek_to_sample(prefix_frames).unwrap(),Some(prefix_frames));
        let mut continuation_buf=AudioFrameBuffer::new();let continuation=d.decode_into(&mut continuation_buf).unwrap();
        let mut stitched=expected[..prefix_frames as usize*channels].to_vec();
        stitched.extend_from_slice(&continuation.samples[..continuation_frames*channels]);
        assert_eq!(stitched,expected,"loop prefix handoff inserted or removed PCM samples for {name}");
    }
}
