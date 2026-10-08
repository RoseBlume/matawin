//! P602-P610, P614-P615, P624: corpus differential and bounded mutation tests.
//! All fixtures have independently stored reference PCM in performance/reference.
#![cfg(feature = "auto-detect")]
use mata_audio::{AudioDecoder, AudioError, AudioFrameBuffer, Decoder};
use std::{fs, io::{BufReader, Cursor}, path::{Path,PathBuf}};

fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }
fn entries() -> Vec<(String,PathBuf,PathBuf,f32)> {
    let root=root(); let manifest=fs::read_to_string(root.join("performance/corpus/manifest.tsv")).unwrap();
    manifest.lines().skip(1).filter(|s|!s.is_empty()).map(|s| {
        let f:Vec<_>=s.split('\t').collect();
        (f[0].into(),root.join("performance/corpus").join(f[1]),root.join("performance/reference").join(f[5]),f[8].parse().unwrap())
    }).collect()
}
fn pcm_file(path:&Path)->Vec<f32>{
    fs::read(path).unwrap().chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect()
}
fn decode(bytes:Vec<u8>, max_calls:usize) -> Result<(Vec<f32>,usize),AudioError> {
    let mut decoder=AudioDecoder::new(BufReader::new(Cursor::new(bytes)))?;
    let mut out=AudioFrameBuffer::default(); let mut pcm=Vec::new();
    for _ in 0..max_calls {
        match decoder.decode_into(&mut out) {
            Ok(frame) => {
                assert!(frame.samples.len()<=1_048_576,"oversized frame");
                assert!(decoder.memory_usage().retained_bytes() <= 256*1024*1024,"unbounded decoder retention");
                pcm.extend_from_slice(frame.samples)
            }
            Err(AudioError::NoNextFrame)=>return Ok((pcm, decoder.channels() as usize)),
            Err(e)=>return Err(e),
        }
    }
    Ok((pcm, decoder.channels() as usize))
}
fn digest(samples:&[f32])->u64 {
    let mut hash=0xcbf29ce484222325_u64;
    for s in samples { for b in s.to_bits().to_le_bytes() { hash=(hash^b as u64).wrapping_mul(0x100000001b3); } }
    hash
}
#[test]
fn every_fixture_matches_pinned_reference_pcm() {
    for (id,input,reference,tolerance) in entries() {
        let expected=pcm_file(&reference);
        let (actual,_)=decode(fs::read(&input).unwrap(),100_000).unwrap_or_else(|e|panic!("{id}: {e}"));
        assert_eq!(actual.len(),expected.len(),"{id}: sample count differs");
        let max_error=actual.iter().zip(expected.iter()).map(|(&a,&b)|if a.is_finite()&&b.is_finite(){(a-b).abs()}else{f32::INFINITY}).fold(0.0_f32,f32::max);
        assert!(max_error<=tolerance,"{id}: max error {max_error} > {tolerance}, actual fingerprint {:016x}",digest(&actual));
        // Lossless PCM is required to produce the exact pinned bytes.
        if id.starts_with("wav_")||id.starts_with("aiff_") { assert_eq!(digest(&actual),digest(&expected),"{id}: PCM bytes changed"); }
    }
}
fn next(seed:&mut u64)->u64{*seed^=*seed<<13;*seed^=*seed>>7;*seed^=*seed<<17;*seed}
#[test]
fn bounded_mutation_truncation_and_corrupt_size_fields_do_not_panic() {
    for (id,path,_,_) in entries() {
        let original=fs::read(path).unwrap(); let mut seed=0xF00D_BAAD_D15C_A11Eu64;
        // Whole-record truncation, small headers, invalid size fields and bit flips.
        for iteration in 0..112 {
            let mut bytes=original.clone();
            match iteration%4 {
                0=>{bytes.truncate((next(&mut seed) as usize)%bytes.len().max(1));},
                1=>{let position=(next(&mut seed) as usize)%bytes.len().max(1);if !bytes.is_empty(){bytes[position]^=(next(&mut seed) as u8)|1;}},
                2=>{let position=(next(&mut seed) as usize)%bytes.len().min(128).max(1);if position+4<=bytes.len(){bytes[position..position+4].fill(0xff);}},
                _=>{let position=(next(&mut seed) as usize)%bytes.len().min(128).max(1);if position+4<=bytes.len(){bytes[position..position+4].fill(0);}},
            }
            // Fuzzing must never panic, allocate without a bound, or loop forever.
            let _=decode(bytes,4).map_err(|_|());
        }
        eprintln!("bounded mutation corpus completed: {id}");
    }
}
#[test]
fn random_partial_decode_seek_reset_and_loop_are_stable() {
    for (id,path,_,_) in entries() {
        let mut decoder=AudioDecoder::new(BufReader::new(fs::File::open(path).unwrap())).unwrap();
        let mut buffer=AudioFrameBuffer::default();
        let mut seed=0xACE0_0123_9876_ABCDu64;
        let mut visited=0usize;
        for iteration in 0..128 {
            if iteration%7==0 { assert!(decoder.reset_decoder().unwrap(),"{id} reset unsupported"); }
            else if iteration%3==0 {
                let sample=next(&mut seed)%decoder.sample_rate().max(1) as u64;
                let _=decoder.seek_to_sample(sample).unwrap_or_else(|e|panic!("{id} seek: {e}"));
            }
            for _ in 0..(iteration%4+1) {
                match decoder.decode_into(&mut buffer) {
                    Ok(frame)=>{assert!(frame.samples.iter().all(|v|v.is_finite()),"{id} nonfinite PCM");visited+=1;},
                    Err(AudioError::NoNextFrame)=>break,
                    Err(e)=>panic!("{id}: {e}"),
                }
            }
        }
        assert!(visited>0,"{id} never produced PCM");
    }
}
