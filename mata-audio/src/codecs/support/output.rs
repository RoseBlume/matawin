use crate::AudioError;

/// Reusable contiguous planar scratch: channel `c` occupies
/// `c * frames_per_channel .. (c + 1) * frames_per_channel`.
#[derive(Debug, Default, Clone)]
pub(crate) struct PlanarAudioScratch {
    data: Vec<f32>,
    channels: usize,
    frames_per_channel: usize,
}

impl PlanarAudioScratch {
    pub(crate) fn resize(&mut self, channels: usize, frames: usize) -> Result<(), AudioError> {
        let len = channels.checked_mul(frames)
            .ok_or_else(|| AudioError::InvalidFormat("planar scratch size overflow".into()))?;
        self.data.resize(len, 0.0);
        self.channels = channels;
        self.frames_per_channel = frames;
        Ok(())
    }
    pub(crate) fn channel(&self, channel: usize) -> &[f32] {
        let start = channel * self.frames_per_channel;
        &self.data[start..start + self.frames_per_channel]
    }
    pub(crate) fn channel_mut(&mut self, channel: usize) -> &mut [f32] {
        let start = channel * self.frames_per_channel;
        &mut self.data[start..start + self.frames_per_channel]
    }
    pub(crate) fn channels(&self) -> usize { self.channels }
    pub(crate) fn frames_per_channel(&self) -> usize { self.frames_per_channel }
}

pub(crate) struct InterleavedWriter<'a> {
    output: &'a mut Vec<f32>,
    channels: usize,
}
impl<'a> InterleavedWriter<'a> {
    pub(crate) fn new(output: &'a mut Vec<f32>, channels: usize) -> Self {
        output.clear();
        Self { output, channels: channels.max(1) }
    }
    pub(crate) fn reserve_frames(&mut self, frames: usize) { self.output.reserve(frames.saturating_mul(self.channels)); }
    #[inline] pub(crate) fn push_sample(&mut self, sample: f32) { self.output.push(sample); }
    pub(crate) fn extend_frame(&mut self, frame: &[f32]) {
        debug_assert_eq!(frame.len(), self.channels);
        self.output.extend_from_slice(frame);
    }
}

pub(crate) struct PlanarWriter<'a> { scratch: &'a mut PlanarAudioScratch }
impl<'a> PlanarWriter<'a> {
    pub(crate) fn new(scratch: &'a mut PlanarAudioScratch) -> Self { Self { scratch } }
    pub(crate) fn channel_mut(&mut self, channel: usize) -> &mut [f32] { self.scratch.channel_mut(channel) }
}

#[cfg(test)]
mod tests {
    use super::{InterleavedWriter, PlanarAudioScratch, PlanarWriter};

    #[test]
    fn planar_scratch_is_contiguous_and_reusable() {
        let mut scratch = PlanarAudioScratch::default();
        scratch.resize(2, 4).unwrap();
        let capacity = scratch.data.capacity();
        {
            let mut writer = PlanarWriter::new(&mut scratch);
            writer.channel_mut(0).copy_from_slice(&[1.0, 2.0, 3.0, 4.0]);
            writer.channel_mut(1).copy_from_slice(&[5.0, 6.0, 7.0, 8.0]);
        }
        assert_eq!(scratch.channel(0), &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(scratch.channel(1), &[5.0, 6.0, 7.0, 8.0]);
        scratch.resize(1, 2).unwrap();
        assert!(scratch.data.capacity() >= capacity);
    }

    #[test]
    fn interleaved_writer_appends_complete_frames() {
        let mut output = Vec::new();
        let mut writer = InterleavedWriter::new(&mut output, 2);
        writer.reserve_frames(2);
        writer.extend_frame(&[0.25, -0.25]);
        writer.extend_frame(&[0.5, -0.5]);
        assert_eq!(output, [0.25, -0.25, 0.5, -0.5]);
    }
}
