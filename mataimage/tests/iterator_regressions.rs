use mataimage::{DecodeScratch, Image, ImageFrame};

#[test]
fn borrowed_frame_iterator_remains_exact_size_without_allocating() {
    let image = Image {
        width: 1,
        height: 1,
        pixels: vec![0x0102_03ff],
        frames: vec![
            ImageFrame { pixels: vec![0x1112_13ff] },
            ImageFrame { pixels: vec![0x2122_23ff] },
        ],
    };

    let mut frames = image.frame_iter();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames.next(), Some(&[0x0102_03ff][..]));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames.next(), Some(&[0x1112_13ff][..]));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames.next(), Some(&[0x2122_23ff][..]));
    assert_eq!(frames.len(), 0);
    assert_eq!(frames.next(), None);
}

#[test]
fn consuming_frame_iterator_remains_exact_size_without_cloning() {
    let image = Image {
        width: 1,
        height: 1,
        pixels: vec![1],
        frames: vec![
            ImageFrame { pixels: vec![2] },
            ImageFrame { pixels: vec![3] },
        ],
    };

    let mut frames = image.into_frame_iter();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames.next(), Some(vec![1]));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames.next(), Some(vec![2]));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames.next(), Some(vec![3]));
    assert_eq!(frames.len(), 0);
}

#[test]
fn decode_scratch_retained_bytes_counts_fixed_history() {
    let scratch = DecodeScratch::new();
    assert!(scratch.retained_bytes() >= 32_768usize);
}
