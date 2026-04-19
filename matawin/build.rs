
#[cfg(target_os = "linux")]
fn main() {
    #[cfg(feature = "wayland")]
    {
        println!("cargo:rustc-link-lib=wayland-client");
        // println!("cargo:rustc-link-lib=wayland-protocowasdls");
    }
    #[cfg(feature = "x11")]
    {
        println!("cargo:rustc-link-lib=X11");
    }
}

#[cfg(target_os = "windows")]
fn main() {
    println!("cargo:rustc-link-lib=user32");
}