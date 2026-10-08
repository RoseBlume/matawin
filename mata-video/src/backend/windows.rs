use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
    time::Duration,
};

use windows::{
    core::PCWSTR,
    Win32::{
        Media::MediaFoundation::{
            IMFMediaBuffer,
            IMFMediaType,
            IMFSourceReader,
            MFCreateMediaType,
            MFCreateSourceReaderFromURL,
            MFShutdown,
            MFStartup,
            MFMediaType_Video,
            MFVideoFormat_ARGB32,
            MFSTARTUP_FULL,
            MF_VERSION,
            MF_MT_DEFAULT_STRIDE,
            MF_MT_FRAME_RATE,
            MF_MT_FRAME_SIZE,
            MF_MT_MAJOR_TYPE,
            MF_MT_SUBTYPE,
            MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
            MF_SOURCE_READERF_ENDOFSTREAM,
            MF_SOURCE_READER_ALL_STREAMS,
            MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        },
        System::Com::{
            CoInitializeEx,
            CoUninitialize,
            COINIT_MULTITHREADED,
        },
    },
};

use crate::{
    PixelFormat,
    VideoError,
    VideoFrame,
    VideoInfo,
};

const HUNDRED_NANOSECONDS_PER_SECOND: u64 = 10_000_000;

pub(crate) struct PlatformDecoder {
    reader: IMFSourceReader,
    info: VideoInfo,
    stride: i32,
    com_initialized: bool,
    mf_initialized: bool,
    ended: bool,
}

impl PlatformDecoder {
    pub(crate) fn open(path: &Path) -> Result<Self, VideoError> {
        if !path.exists() {
            return Err(VideoError::new(format!(
                "video file does not exist: {}",
                path.display(),
            )));
        }

        let mut com_initialized = false;
        let mut mf_initialized = false;

        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_ok() {
                com_initialized = true;
            } else if hr.0 != windows::Win32::Foundation::RPC_E_CHANGED_MODE.0 {
                return Err(VideoError::new(format!(
                    "failed to initialize COM: {hr:?}",
                )));
            }

            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                if com_initialized {
                    CoUninitialize();
                }
                return Err(VideoError::new(format!(
                    "failed to initialize Media Foundation: {error}",
                )));
            }
            mf_initialized = true;

            let wide_path = wide_null(path.as_os_str());
            let reader = match MFCreateSourceReaderFromURL(
                PCWSTR(wide_path.as_ptr()),
                None,
            ) {
                Ok(reader) => reader,
                Err(error) => {
                    cleanup_runtime(com_initialized, mf_initialized);
                    return Err(VideoError::new(format!(
                        "failed to open video with Media Foundation: {error}",
                    )));
                }
            };

            let all_streams = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
            let video_stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

            if let Err(error) = reader.SetStreamSelection(all_streams, false) {
                cleanup_runtime(com_initialized, mf_initialized);
                return Err(VideoError::new(format!(
                    "failed to disable Media Foundation streams: {error}",
                )));
            }

            if let Err(error) = reader.SetStreamSelection(video_stream, true) {
                cleanup_runtime(com_initialized, mf_initialized);
                return Err(VideoError::new(format!(
                    "failed to select the video stream: {error}",
                )));
            }

            let output_type = match create_argb32_media_type() {
                Ok(media_type) => media_type,
                Err(error) => {
                    cleanup_runtime(com_initialized, mf_initialized);
                    return Err(error);
                }
            };

            if let Err(error) = reader.SetCurrentMediaType(
                video_stream,
                None,
                &output_type,
            ) {
                cleanup_runtime(com_initialized, mf_initialized);
                return Err(VideoError::new(format!(
                    "failed to request ARGB32 output from Media Foundation: {error}",
                )));
            }

            let current_type = match reader.GetCurrentMediaType(video_stream) {
                Ok(media_type) => media_type,
                Err(error) => {
                    cleanup_runtime(com_initialized, mf_initialized);
                    return Err(VideoError::new(format!(
                        "failed to read Media Foundation video format: {error}",
                    )));
                }
            };

            let (info, stride) = match layout_from_media_type(&current_type) {
                Ok(layout) => layout,
                Err(error) => {
                    cleanup_runtime(com_initialized, mf_initialized);
                    return Err(error);
                }
            };

            Ok(Self {
                reader,
                info,
                stride,
                com_initialized,
                mf_initialized,
                ended: false,
            })
        }
    }

    #[inline]
    pub(crate) fn info(&self) -> VideoInfo {
        self.info
    }

    pub(crate) fn next_frame(&mut self) -> Result<Option<VideoFrame>, VideoError> {
        if self.ended {
            return Ok(None);
        }

        let video_stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

        loop {
            let mut stream_flags = 0u32;
            let mut timestamp_100ns = 0i64;
            let mut sample = None;

            unsafe {
                self.reader
                    .ReadSample(
                        video_stream,
                        0,
                        None,
                        Some(&mut stream_flags),
                        Some(&mut timestamp_100ns),
                        Some(&mut sample),
                    )
                    .map_err(|error| {
                        VideoError::new(format!(
                            "Media Foundation failed to decode a video sample: {error}",
                        ))
                    })?;
            }

            if stream_flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                self.ended = true;
                return Ok(None);
            }

            if stream_flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                let current_type = unsafe {
                    self.reader.GetCurrentMediaType(video_stream)
                }
                .map_err(|error| {
                    VideoError::new(format!(
                        "failed to read changed Media Foundation video format: {error}",
                    ))
                })?;
                let (info, stride) = layout_from_media_type(&current_type)?;
                self.info = info;
                self.stride = stride;
            }

            let Some(sample) = sample else {
                // Stream ticks and format-change notifications may legitimately
                // arrive without an accompanying sample.
                continue;
            };

            let sample_duration = unsafe { sample.GetSampleDuration().ok() }
                .and_then(hns_i64_to_duration);

            let contiguous = unsafe { sample.ConvertToContiguousBuffer() }
                .map_err(|error| {
                    VideoError::new(format!(
                        "failed to obtain contiguous Media Foundation frame data: {error}",
                    ))
                })?;

            let pixels = copy_media_buffer(
                &contiguous,
                self.info.width,
                self.info.height,
                self.stride,
            )?;

            return Ok(Some(VideoFrame {
                width: self.info.width,
                height: self.info.height,
                pixels,
                timestamp: hns_i64_to_duration(timestamp_100ns)
                    .unwrap_or(Duration::ZERO),
                duration: sample_duration,
            }));
        }
    }
}

impl Drop for PlatformDecoder {
    fn drop(&mut self) {
        cleanup_runtime(self.com_initialized, self.mf_initialized);
    }
}

fn cleanup_runtime(com_initialized: bool, mf_initialized: bool) {
    unsafe {
        if mf_initialized {
            let _ = MFShutdown();
        }
        if com_initialized {
            CoUninitialize();
        }
    }
}

fn create_argb32_media_type() -> Result<IMFMediaType, VideoError> {
    let media_type = unsafe { MFCreateMediaType() }.map_err(|error| {
        VideoError::new(format!("failed to create Media Foundation media type: {error}"))
    })?;

    unsafe { media_type
        .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video) }
        .map_err(|error| {
            VideoError::new(format!("failed to set Media Foundation major type: {error}"))
        })?;
    unsafe { media_type
        .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32) }
        .map_err(|error| {
            VideoError::new(format!("failed to request Media Foundation ARGB32: {error}"))
        })?;

    Ok(media_type)
}

fn layout_from_media_type(media_type: &IMFMediaType) -> Result<(VideoInfo, i32), VideoError> {
    let frame_size = unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(|error| {
        VideoError::new(format!("Media Foundation did not report frame size: {error}"))
    })?;
    let width = (frame_size >> 32) as u32;
    let height = frame_size as u32;

    if width == 0 || height == 0 {
        return Err(VideoError::new("Media Foundation returned zero-sized video frames"));
    }

    let (frame_rate_numerator, frame_rate_denominator) = unsafe { media_type
        .GetUINT64(&MF_MT_FRAME_RATE) }
        .map(|rate| ((rate >> 32) as u32, rate as u32))
        .unwrap_or((0, 0));

    let stride = unsafe { media_type.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
        .map(|value| value as i32)
        .unwrap_or_else(|_| (width.saturating_mul(4)).min(i32::MAX as u32) as i32);

    Ok((VideoInfo {
        width,
        height,
        frame_rate_numerator,
        frame_rate_denominator,
        duration: None,
        pixel_format: PixelFormat::Argb32,
    }, stride))
}

fn copy_media_buffer(
    buffer: &IMFMediaBuffer,
    width: u32,
    height: u32,
    stride: i32,
) -> Result<Vec<u32>, VideoError> {
    let width = width as usize;
    let height = height as usize;
    let row_bytes = width
        .checked_mul(4)
        .ok_or_else(|| VideoError::new("video row size overflow"))?;
    let required = row_bytes
        .checked_mul(height)
        .ok_or_else(|| VideoError::new("video frame size overflow"))?;

    let mut raw = ptr::null_mut();
    let mut current_length = 0u32;

    unsafe {
        buffer
            .Lock(&mut raw, None, Some(&mut current_length))
            .map_err(|error| {
                VideoError::new(format!(
                    "failed to lock Media Foundation frame buffer: {error}",
                ))
            })?;
    }

    struct UnlockGuard<'a>(&'a IMFMediaBuffer);
    impl Drop for UnlockGuard<'_> {
        fn drop(&mut self) {
            unsafe {
                let _ = self.0.Unlock();
            }
        }
    }
    let _unlock = UnlockGuard(buffer);

    if raw.is_null() {
        return Err(VideoError::new("Media Foundation returned a null frame buffer"));
    }

    let byte_len = current_length as usize;
    if byte_len < required {
        return Err(VideoError::new(format!(
            "Media Foundation frame is too small: got {byte_len} bytes, expected at least {required}",
        )));
    }

    let bytes = unsafe { std::slice::from_raw_parts(raw, byte_len) };

    // MF_MT_DEFAULT_STRIDE is signed. Positive stride is top-down; a negative
    // stride denotes a bottom-up image. Use its absolute value for addressing
    // and reverse row order when necessary so callers always receive top-down
    // pixels.
    let stride_abs = if stride == i32::MIN {
        return Err(VideoError::new("Media Foundation returned an invalid frame stride"));
    } else {
        stride.unsigned_abs() as usize
    };
    let stride_abs = stride_abs.max(row_bytes);

    let span = stride_abs
        .checked_mul(height.saturating_sub(1))
        .and_then(|value| value.checked_add(row_bytes))
        .ok_or_else(|| VideoError::new("video frame stride overflow"))?;
    if byte_len < span {
        return Err(VideoError::new(format!(
            "Media Foundation frame is too small for its stride: got {byte_len} bytes, need {span}",
        )));
    }

    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| VideoError::new("video pixel count overflow"))?;
    let mut pixels = Vec::with_capacity(pixel_count);

    for y in 0..height {
        let source_y = if stride < 0 {
            height - 1 - y
        } else {
            y
        };
        let row_start = source_y
            .checked_mul(stride_abs)
            .ok_or_else(|| VideoError::new("video row offset overflow"))?;
        let row_end = row_start
            .checked_add(row_bytes)
            .ok_or_else(|| VideoError::new("video row end overflow"))?;

        if row_end > bytes.len() {
            return Err(VideoError::new("Media Foundation frame has an invalid stride"));
        }

        for pixel in bytes[row_start..row_end].chunks_exact(4) {
            // Media Foundation ARGB32 is stored as BGRA bytes on little-endian
            // Windows. Normalize alpha to opaque if a decoder leaves it zero.
            let mut value = u32::from_le_bytes([
                pixel[0], pixel[1], pixel[2], pixel[3],
            ]);
            if value >> 24 == 0 {
                value |= 0xFF00_0000;
            }
            pixels.push(value);
        }
    }

    Ok(pixels)
}

#[inline]
fn hns_i64_to_duration(value: i64) -> Option<Duration> {
    if value < 0 {
        return None;
    }

    let value = value as u64;
    let seconds = value / HUNDRED_NANOSECONDS_PER_SECOND;
    let remaining_100ns = value % HUNDRED_NANOSECONDS_PER_SECOND;
    let nanoseconds = remaining_100ns * 100;

    Some(Duration::new(seconds, nanoseconds as u32))
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(core::iter::once(0)).collect()
}
