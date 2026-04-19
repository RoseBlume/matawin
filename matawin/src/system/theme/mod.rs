use std::process::Command;

pub fn system_theme_color() -> u32 {
    #[cfg(target_os = "windows")]
    {
        windows_theme_color().unwrap_or(0x000000)
    }

    #[cfg(target_os = "linux")]
    {
        linux_theme_color().unwrap_or(0x000000)
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        0x000000
    }
}


#[cfg(target_os = "windows")]
fn windows_theme_color() -> Option<u32> {

    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Output contains something like:
    // AppsUseLightTheme    REG_DWORD    0x0
    if stdout.contains("0x0") {
        Some(0x000000) // dark
    } else if stdout.contains("0x1") {
        Some(0xFFFFFF) // light
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_theme_color() -> Option<u32> {

    // Try GNOME (most common)
    if let Ok(output) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();

        if stdout.contains("dark") {
            return Some(0x000000);
        }
        if stdout.contains("light") {
            return Some(0xFFFFFF);
        }
    }

    // Fallback: GTK_THEME env var
    if let Ok(theme) = std::env::var("GTK_THEME") {
        let theme = theme.to_lowercase();
        if theme.contains("dark") {
            return Some(0x000000);
        } else {
            return Some(0xFFFFFF);
        }
    }

    None
}