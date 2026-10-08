#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub(crate) use linux::PlatformDecoder;
#[cfg(windows)]
pub(crate) use windows::PlatformDecoder;
