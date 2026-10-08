# PCM copy audit (P58/P59)

- PCM/WAV/AIFF decode directly into `AudioFrameBuffer`; there is no intermediate owned `AudioFrame` on the `decode_into` path.
- MPEG writes the low-level decoder output directly into retained `AudioFrameBuffer` storage.
- FLAC and Vorbis still have legacy codec-internal owned vectors, but `AudioFrameBuffer::replace_samples` transfers ownership without copying; those internal vectors are future codec-local optimization targets.
- Playback swaps the complete decoded allocation into voice staging when staging is empty and gives the old staging allocation back to the decoder buffer. The fallback append path copies only when already-buffered source frames must be preserved.
- `next_frame()` intentionally remains an allocating compatibility wrapper.
