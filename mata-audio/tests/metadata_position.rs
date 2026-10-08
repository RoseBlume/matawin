use std::io::{Cursor,Seek,SeekFrom};

#[test]
#[cfg(feature="metadata-id3v1")]
fn id3v1_preserves_position(){
    let mut bytes=vec![0u8;256];bytes[128..131].copy_from_slice(b"TAG");bytes[131..136].copy_from_slice(b"title");
    let mut c=Cursor::new(bytes);c.seek(SeekFrom::Start(37)).unwrap();let before=c.position();let _=mata_audio::metadata::id3v1::read(&mut c).unwrap();assert_eq!(c.position(),before);
}

#[test]
#[cfg(feature="metadata-id3v2")]
fn id3v2_preserves_position(){
    let mut bytes=b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();bytes.resize(64,0);let mut c=Cursor::new(bytes);c.seek(SeekFrom::Start(0)).unwrap();let before=c.position();let _=mata_audio::metadata::id3v2::read(&mut c).unwrap();assert_eq!(c.position(),before);
}
