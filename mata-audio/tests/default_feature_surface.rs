#![cfg(all(
    feature="codec-flac", feature="codec-mp1", feature="codec-mp2", feature="codec-mp3", feature="codec-pcm", feature="codec-vorbis",
    feature="demux-aiff", feature="demux-flac", feature="demux-matroska", feature="demux-ogg", feature="demux-wav",
    feature="metadata-ape", feature="metadata-id3v1", feature="metadata-id3v2", feature="metadata-iso-mp4", feature="metadata-riff", feature="metadata-vorbis-flac", feature="metadata-vorbis-ogg"
))]

#[test]
fn default_surface_exposes_every_requested_family() {
    let _ = core::mem::size_of::<mata_audio::MetadataSurfaceProbe>();
    let _ = mata_audio::simd::runtime_isa();
    let _ = mata_audio::demuxers::SeekCapability::Exact;
    let _ = mata_audio::metadata::Metadata::new(mata_audio::metadata::MetadataKind::Id3V2);
    fn accepts_decoder<D: mata_audio::Decoder>() {}
    accepts_decoder::<mata_audio::Wav<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Aiff<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Flac<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Mp1<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Mp2<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Mp3<std::io::Cursor<Vec<u8>>>>();
    accepts_decoder::<mata_audio::Vorbis<std::io::Cursor<Vec<u8>>>>();
}
