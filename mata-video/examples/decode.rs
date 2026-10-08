use mata_video::VideoDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: cargo run --example decode -- <video-file>")?;

    let mut decoder = VideoDecoder::open(path)?;
    let info = decoder.info();

    println!(
        "{}x{}, fps={:?}, duration={:?}, format={:?}",
        info.width,
        info.height,
        info.frames_per_second(),
        info.duration,
        info.pixel_format,
    );

    let mut frame_count = 0u64;
    while let Some(frame) = decoder.next_frame()? {
        frame_count += 1;
        println!(
            "frame {frame_count}: {}x{}, {} pixels, pts={:?}, duration={:?}",
            frame.width,
            frame.height,
            frame.pixels.len(),
            frame.timestamp,
            frame.duration,
        );
    }

    println!("decoded {frame_count} frames");
    Ok(())
}
