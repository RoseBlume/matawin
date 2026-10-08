#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Signature dispatch reaches every decoder enabled by `all-formats`.
    let _ = mataimage::Image::from_bytes(data);
    let _ = mataimage::ImageInfo::from_bytes(data);
});
