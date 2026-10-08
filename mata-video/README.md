# mata-video

`mata-video` is a streaming video decoder with one direct backend dependency per
supported operating system:

- **Windows:** `windows` + Media Foundation Source Reader.
- **Linux:** `gstreamer` + GStreamer decodebin/appsink.

There are no other direct Rust dependencies. In particular, Linux does **not**
depend directly on `gstreamer-app`, `glib`, `serde`, `thiserror`, or a codec
crate. GStreamer's installed plugins decide which codecs and containers are
available.

## Output format

Every frame contains `Vec<u32>` pixels in numeric `0xAARRGGBB` form. This is
compatible with the packed-pixel shape used by the Mata image code without
making `mata-video` depend on `mataimage`.

## Example

```rust
use mata_video::VideoDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = VideoDecoder::open("video.mp4")?;
    println!("{:?}", decoder.info());

    while let Some(frame) = decoder.next_frame()? {
        println!("{}x{} @ {:?}", frame.width, frame.height, frame.timestamp);
    }

    Ok(())
}
```

Run the included example with:

```text
cargo run --example decode -- path/to/video.mp4
```

## Linux runtime requirements

The Rust crate uses only the `gstreamer` Rust binding directly, but the system
must provide GStreamer and whichever runtime plugins are needed for the target
container/codec. The pipeline asks GStreamer to decode and convert video to
BGRA, then receives samples from `appsink` through its action signals.

## Design

- Streaming decode; the entire video is never loaded into RAM.
- A maximum of two decoded GStreamer frames are queued ahead on Linux.
- Media Foundation decodes synchronously one sample at a time on Windows.
- Video-only API: audio is intentionally left to `mata-audio` / `mata-playback`.
- No codec-specific Rust dependencies are used.

Linux Dependencies:

```sh
sudo apt-get install libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly \
gstreamer1.0-libav libgstrtspserver-1.0-dev libges-1.0-dev
```
