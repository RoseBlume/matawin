fn main() {
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .expect("Failed to run rustc");

    let version = String::from_utf8(rustc.stdout).unwrap();

    if version.contains("nightly") {
        println!("cargo:rustc-cfg=nightly");
    }
}