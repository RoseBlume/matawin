//! Audio metadata parsers independent of playback and decoding.
//!
//! Each metadata family is independently feature gated. `all-metadata`, which
//! is enabled by default, turns on the complete parser set.

#[cfg(feature = "metadata-ape")]
pub mod ape;
#[cfg(feature = "metadata-id3v1")]
pub mod id3v1;
#[cfg(feature = "metadata-id3v2")]
pub mod id3v2;
#[cfg(feature = "metadata-iso-mp4")]
pub mod iso_mp4;
#[cfg(feature = "metadata-riff")]
pub mod riff;
#[cfg(feature = "metadata-vorbis-flac")]
pub mod vorbis_flac;
#[cfg(feature = "metadata-vorbis-ogg")]
pub mod vorbis_ogg;
#[cfg(any(feature = "metadata-vorbis-flac", feature = "metadata-vorbis-ogg"))]
mod vorbis_common;
mod support;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataKind {
    ApeV1,
    ApeV2,
    Id3V1,
    Id3V2,
    IsoMp4,
    Riff,
    VorbisFlac,
    VorbisOgg,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataField {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub mime_type: String,
    pub picture_type: u32,
    pub description: String,
    pub data: Vec<u8>,
}


/// Borrowed metadata field view. This avoids duplicating key/value strings when
/// callers only need to inspect an already parsed [`Metadata`] value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataFieldRef<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// Borrowed picture view. Image bytes are borrowed from the owned compatibility
/// representation and are never copied merely to inspect metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PictureRef<'a> {
    pub mime_type: &'a str,
    pub picture_type: u32,
    pub description: &'a str,
    pub data: &'a [u8],
}

/// Borrowed view over an owned metadata object.
#[derive(Debug, Clone, Copy)]
pub struct MetadataView<'a> {
    metadata: &'a Metadata,
}

impl<'a> MetadataView<'a> {
    pub fn kind(self) -> MetadataKind { self.metadata.kind }
    pub fn fields(self) -> impl Iterator<Item = MetadataFieldRef<'a>> + 'a {
        self.metadata.fields.iter().map(|field| MetadataFieldRef { key: &field.key, value: &field.value })
    }
    pub fn pictures(self) -> impl Iterator<Item = PictureRef<'a>> + 'a {
        self.metadata.pictures.iter().map(|picture| PictureRef {
            mime_type: &picture.mime_type,
            picture_type: picture.picture_type,
            description: &picture.description,
            data: &picture.data,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub kind: MetadataKind,
    pub fields: Vec<MetadataField>,
    pub pictures: Vec<Picture>,
}

impl Metadata {
    pub fn new(kind: MetadataKind) -> Self {
        Self { kind, fields: Vec::new(), pictures: Vec::new() }
    }

    pub fn as_view(&self) -> MetadataView<'_> { MetadataView { metadata: self } }

    pub fn push(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.fields.push(MetadataField { key: key.into(), value: value.into() });
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.key.eq_ignore_ascii_case(key)).map(|f| f.value.as_str())
    }

    pub fn values<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.fields.iter().filter(move |f| f.key.eq_ignore_ascii_case(key)).map(|f| f.value.as_str())
    }
}

/// Reads every enabled metadata family that can be recognized from a seekable
/// file. Disabled metadata features are not probed.
///
/// Format-specific readers remain available in their submodules when a caller
/// wants to avoid probing unrelated tag families.
pub fn read_all<R>(reader: &mut R) -> Result<Vec<Metadata>, crate::AudioError>
where
    R: std::io::Read + std::io::Seek,
{
    support::preserve_position(reader, read_all_inner)
}

fn read_all_inner<R>(reader: &mut R) -> Result<Vec<Metadata>, crate::AudioError>
where
    R: std::io::Read + std::io::Seek,
{
    use std::io::SeekFrom;

    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut prefix = [0u8; 16];
    let prefix_len = reader.read(&mut prefix)?;
    let prefix = &prefix[..prefix_len];
    let mut found = Vec::new();

    #[cfg(feature = "metadata-id3v2")]
    {
        reader.seek(SeekFrom::Start(0))?;
        if let Some(meta) = id3v2::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-vorbis-flac")]
    if prefix.starts_with(b"fLaC") {
        reader.seek(SeekFrom::Start(0))?;
        if let Some(meta) = vorbis_flac::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-vorbis-ogg")]
    if prefix.starts_with(b"OggS") {
        reader.seek(SeekFrom::Start(0))?;
        if let Some(meta) = vorbis_ogg::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-riff")]
    if prefix.len() >= 12 && (&prefix[0..4] == b"RIFF" || &prefix[0..4] == b"RF64") {
        reader.seek(SeekFrom::Start(0))?;
        if let Some(meta) = riff::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-iso-mp4")]
    if prefix.len() >= 8 && &prefix[4..8] == b"ftyp" {
        reader.seek(SeekFrom::Start(0))?;
        if let Some(meta) = iso_mp4::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-ape")]
    if end >= 32 {
        if let Some(meta) = ape::read_at_current_position(reader)? { found.push(meta); }
    }

    #[cfg(feature = "metadata-id3v1")]
    if end >= 128 {
        if let Some(meta) = id3v1::read_at_current_position(reader)? { found.push(meta); }
    }

    Ok(found)
}
