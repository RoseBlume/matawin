use std::{alloc::{GlobalAlloc,Layout,System},env,fs::File,hint::black_box,io::{BufReader,Cursor,Seek},path::PathBuf,sync::atomic::{AtomicU64,Ordering},time::Instant};

struct CountingAllocator;
static ALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self,layout:Layout)->*mut u8{ALLOCS.fetch_add(1,Ordering::Relaxed);BYTES.fetch_add(layout.size() as u64,Ordering::Relaxed);unsafe{System.alloc(layout)}}
    unsafe fn alloc_zeroed(&self,layout:Layout)->*mut u8{ALLOCS.fetch_add(1,Ordering::Relaxed);BYTES.fetch_add(layout.size() as u64,Ordering::Relaxed);unsafe{System.alloc_zeroed(layout)}}
    unsafe fn realloc(&self,ptr:*mut u8,old:Layout,new_size:usize)->*mut u8{if new_size>old.size(){ALLOCS.fetch_add(1,Ordering::Relaxed);BYTES.fetch_add((new_size-old.size()) as u64,Ordering::Relaxed);}unsafe{System.realloc(ptr,old,new_size)}}
    unsafe fn dealloc(&self,ptr:*mut u8,layout:Layout){unsafe{System.dealloc(ptr,layout)}}
}
#[global_allocator] static GLOBAL:CountingAllocator=CountingAllocator;

fn run<F:FnMut()->usize>(name:&str,rounds:usize,mut f:F){
    // Warm setup once so process startup/cold allocator work is not attributed
    // to steady-state parser regressions.
    black_box(f());
    ALLOCS.store(0,Ordering::Relaxed);BYTES.store(0,Ordering::Relaxed);
    let start=Instant::now();let mut fields=0;for _ in 0..rounds{fields=black_box(f());}
    println!("{name},{rounds},{},{fields},{},{}",start.elapsed().as_micros(),ALLOCS.load(Ordering::Relaxed),BYTES.load(Ordering::Relaxed));
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let root=PathBuf::from(env::args().nth(1).unwrap_or_else(||"performance/corpus".into()));let rounds:usize=env::args().nth(2).and_then(|v|v.parse().ok()).unwrap_or(100);println!("family,rounds,total_us,fields,allocations,allocated_bytes");
    #[cfg(feature="metadata-id3v1")] run("id3v1",rounds,||{let mut b=vec![0u8;256];b[128..131].copy_from_slice(b"TAG");b[131..136].copy_from_slice(b"title");mata_audio::metadata::id3v1::read(&mut Cursor::new(b)).ok().flatten().map_or(0,|m|m.fields.len())});
    #[cfg(feature="metadata-id3v2")] run("id3v2",rounds,||{let b=b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();mata_audio::metadata::id3v2::read(&mut Cursor::new(b)).ok().flatten().map_or(0,|m|m.fields.len())});
    #[cfg(feature="metadata-ape")] run("ape",rounds,||{let mut b=vec![0u8;64];let f=&mut b[32..64];f[..8].copy_from_slice(b"APETAGEX");f[8..12].copy_from_slice(&2000u32.to_le_bytes());f[12..16].copy_from_slice(&32u32.to_le_bytes());mata_audio::metadata::ape::read(&mut Cursor::new(b)).ok().flatten().map_or(0,|m|m.fields.len())});
    #[cfg(feature="metadata-vorbis-flac")] run("vorbis-flac-comments",rounds,||{let mut b=Vec::new();b.extend(4u32.to_le_bytes());b.extend(b"mata");b.extend(1u32.to_le_bytes());b.extend(9u32.to_le_bytes());b.extend(b"TITLE=abc");mata_audio::metadata::vorbis_flac::parse(&b).map_or(0,|m|m.fields.len())});
    #[cfg(feature="metadata-vorbis-ogg")] run("vorbis-ogg-comments",rounds,||{let mut b=Vec::new();b.extend(4u32.to_le_bytes());b.extend(b"mata");b.extend(1u32.to_le_bytes());b.extend(9u32.to_le_bytes());b.extend(b"TITLE=abc");mata_audio::metadata::vorbis_ogg::parse(&b).map_or(0,|m|m.fields.len())});
    #[cfg(feature="metadata-riff")] {let path=root.join("wav_s16_stereo_44100.wav");if path.exists(){run("riff",rounds,||{let mut r=BufReader::new(File::open(&path).unwrap());mata_audio::metadata::riff::read(&mut r).ok().flatten().map_or(0,|m|m.fields.len())});}}
    #[cfg(feature="metadata-iso-mp4")] run("iso-mp4",rounds,||{let mut b=Vec::new();b.extend(24u32.to_be_bytes());b.extend(b"ftyp");b.extend(b"isom\0\0\0\0isomiso2");mata_audio::metadata::iso_mp4::read(&mut Cursor::new(b)).ok().flatten().map_or(0,|m|m.fields.len())});
    // read_all benchmark works with the exact feature set used to compile this binary.
    let path=root.join("wav_s16_stereo_44100.wav");if path.exists(){run("read_all-enabled-set",rounds,||{let mut r=BufReader::new(File::open(&path).unwrap());let before=r.stream_position().unwrap();let all=mata_audio::metadata::read_all(&mut r).unwrap();assert_eq!(r.stream_position().unwrap(),before);all.iter().map(|m|m.fields.len()).sum()});}
    Ok(())
}
