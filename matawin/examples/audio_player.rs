//! Playlist audio player for files in `workspace_manifest_dir/audio`.
//!
//! The playlist is discovered recursively and sorted by path. Playback uses
//! Mata's existing streamed [`Audio`] backend, while the visible transport
//! controls are MataWin software widgets.

mod support;

use std::{
    error::Error,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use matawin::{
    widgets::{Anchors, Audio, Button, Rect, TextField, WidgetEvent, WidgetExt, WidgetWindow},
    Window,
};

use support::{collect_files, workspace_manifest_dir};

/// Volume change applied by the volume buttons.
///
/// Visibility: **private to the example binary**.
const VOLUME_STEP: f32 = 0.05;
/// Number of seconds moved by the seek buttons.
///
/// Visibility: **private to the example binary**.
const SEEK_SECONDS: f64 = 10.0;

/// Playlist state and the one currently active streamed audio source.
///
/// Only one track is open at a time, which prevents an arbitrarily large
/// playlist from retaining decoded audio or playback state in memory.
///
/// Visibility: **private to the example binary**.
struct PlaylistPlayer {
    /// Sorted audio files discovered in the workspace's `audio` directory.
    tracks: Vec<PathBuf>,
    /// Index of the currently selected track.
    index: usize,
    /// Audio widget/source for the current track.
    current: Audio,
    /// Volume preserved when switching between tracks.
    volume: f32,
}

impl PlaylistPlayer {
    /// Opens the first playlist entry without starting playback.
    ///
    /// Visibility: **private to the example binary**.
    fn new(tracks: Vec<PathBuf>) -> Result<Self, Box<dyn Error>> {
        let current = Audio::from_file(&tracks[0], false, false)?;
        let volume = 0.70;
        current.set_volume(volume);
        current.set_looping(false);
        Ok(Self {
            tracks,
            index: 0,
            current,
            volume,
        })
    }

    /// Returns the currently selected path.
    ///
    /// Visibility: **private to the example binary**.
    fn current_path(&self) -> &Path {
        &self.tracks[self.index]
    }

    /// Returns a display-friendly filename for the selected track.
    ///
    /// Visibility: **private to the example binary**.
    fn current_name(&self) -> String {
        self.current_path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unnamed track")
            .to_owned()
    }

    /// Replaces the active source with `index` and optionally starts the new
    /// source immediately.
    ///
    /// Visibility: **private to the example binary**.
    fn open(&mut self, index: usize, autoplay: bool) -> Result<(), Box<dyn Error>> {
        self.current.stop();
        let next = Audio::from_file(&self.tracks[index], autoplay, false)?;
        next.set_volume(self.volume);
        next.set_looping(false);
        self.current = next;
        self.index = index;
        Ok(())
    }

    /// Selects the next track, wrapping to the beginning.
    ///
    /// Visibility: **private to the example binary**.
    fn next(&mut self, autoplay: bool) -> Result<(), Box<dyn Error>> {
        let next = (self.index + 1) % self.tracks.len();
        self.open(next, autoplay)
    }

    /// Selects the previous track, wrapping to the end.
    ///
    /// Visibility: **private to the example binary**.
    fn previous(&mut self, autoplay: bool) -> Result<(), Box<dyn Error>> {
        let previous = if self.index == 0 {
            self.tracks.len() - 1
        } else {
            self.index - 1
        };
        self.open(previous, autoplay)
    }

    /// Toggles playback of the current source.
    ///
    /// Visibility: **private to the example binary**.
    fn toggle(&self) {
        self.current.source().toggle_playback();
    }

    /// Stops playback and returns the current track to its stopped state.
    ///
    /// Visibility: **private to the example binary**.
    fn stop(&self) {
        self.current.stop();
    }

    /// Seeks by a signed number of seconds while clamping to the known track
    /// duration when one is available.
    ///
    /// Visibility: **private to the example binary**.
    fn seek_by(&self, delta: f64) {
        let source = self.current.source();
        let state = source.state();
        let mut target = (state.position_seconds + delta).max(0.0);
        if let Some(duration) = state.duration_seconds {
            target = target.min(duration);
        }
        source.seek(target);
    }

    /// Changes and stores playlist volume, preserving it across track changes.
    ///
    /// Visibility: **private to the example binary**.
    fn change_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        self.current.set_volume(self.volume);
    }
}

/// Returns whether `path` has a common audio-file extension supported by Mata
/// audio decoders/playback builds.
///
/// Visibility: **private to the example binary**.
fn is_audio_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(
        extension.to_ascii_lowercase().as_str(),
        "aac"
            | "aif"
            | "aiff"
            | "flac"
            | "m4a"
            | "mp3"
            | "oga"
            | "ogg"
            | "opus"
            | "wav"
    )
}

/// Formats a non-negative second value as `m:ss`.
///
/// Visibility: **private to the example binary**.
fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Refreshes the read-only track and playback status fields.
///
/// Visibility: **private to the example binary**.
fn refresh_status(player: &PlaylistPlayer, title: &TextField, status: &TextField) {
    let source = player.current.source();
    let state = source.state();
    let title_text = format!(
        "Track {} / {}  -  {}",
        player.index + 1,
        player.tracks.len(),
        player.current_name()
    );
    if title.text() != title_text {
        title.set_text(&title_text);
    }

    let duration = state
        .duration_seconds
        .map(format_time)
        .unwrap_or_else(|| "--:--".to_owned());
    let playback = if state.playing { "Playing" } else { "Paused" };
    let status_text = format!(
        "{}    {} / {}    Volume: {:.0}%",
        playback,
        format_time(state.position_seconds),
        duration,
        state.volume * 100.0
    );
    if status.text() != status_text {
        status.set_text(&status_text);
    }
}

/// Runs the playlist audio player example.
///
/// Visibility: **private to the example binary**.
fn main() -> Result<(), Box<dyn Error>> {
    let audio_dir = workspace_manifest_dir().join("assets_ignored").join("audio");
    let tracks = collect_files(&audio_dir, is_audio_file)?;
    if tracks.is_empty() {
        return Err(format!("no supported audio files were found in {}", audio_dir.display()).into());
    }

    let native_window = Window::new("MataWin audio playlist", (920, 250));
    let window = WidgetWindow::from_window(native_window)?;
    let mut player = PlaylistPlayer::new(tracks)?;
    let size: (u32, u32) = window.size();
    // Space on either side of screen = 16px
    // Space between buttons = 8px
    // width of buttons = size.0 - (56px + (16px * 2)
    // next x = width of previous + 8px + 16px
    let button_width: u32 = (size.0 - 56 - (16 * 2)) / 8;
    let mut x: i32 = 16;
    let previous = Button::new(&window, Rect::new(x, 18, button_width, 40), "Previous")?;
    x += (button_width as i32) + 8;
    let play_pause = Button::new(&window, Rect::new(x, 18, button_width, 40), "Play/Pause")?;
    x += (button_width as i32) + 8;
    let next = Button::new(&window, Rect::new(x, 18, button_width, 40), "Next")?;
    x += (button_width as i32) + 8;
    let stop = Button::new(&window, Rect::new(x, 18, button_width, 40), "Stop")?;
    x += (button_width as i32) + 8;
    let seek_back = Button::new(&window, Rect::new(x, 18, button_width, 40), "-10 sec")?;
    x += (button_width as i32) + 8;
    let seek_forward = Button::new(&window, Rect::new(x, 18, button_width, 40), "+10 sec")?;
    x += (button_width as i32) + 8;
    let quieter = Button::new(&window, Rect::new(x, 18, button_width, 40), "Vol -")?;
    x += (button_width as i32) + 8;
    let louder = Button::new(&window, Rect::new(x, 18, button_width, 40), "Vol +")?;
    // let seek_forward = Button::new(&window, Rect::new(484, 18, 82, 40), "+10 sec")?;
    // let quieter = Button::new(&window, Rect::new(574, 18, 82, 40), "Vol -")?;
    // let louder = Button::new(&window, Rect::new(664, 18, 82, 40), "Vol +")?;

    let title = TextField::new(&window, Rect::new(16, 78, 888, 42), "")?;
    let status = TextField::new(&window, Rect::new(16, 132, 888, 42), "")?;
    title.set_enabled(false);
    status.set_enabled(false);
    title.set_anchors(Anchors::TOP_HORIZONTAL);
    status.set_anchors(Anchors::TOP_HORIZONTAL);

    for button in [
        &previous,
        &play_pause,
        &next,
        &stop,
        &seek_back,
        &seek_forward,
        &quieter,
        &louder,
    ] {
        button.set_anchors(Anchors::TOP_LEFT);
    }

    refresh_status(&player, &title, &status);

    let mut events = Vec::with_capacity(24);
    let mut last_status_refresh = Instant::now() - Duration::from_secs(1);

    loop {
        window.poll_events_into(&mut events);

        for event in &events {
            match event {
                WidgetEvent::Resized(width, height) => {
                    let button_width: u32 = (width - 56 - (16 * 2)) / 8;
                    let mut x: i32 = 16;
                    for button in [
                        &previous,
                        &play_pause,
                        &next,
                        &stop,
                        &seek_back,
                        &seek_forward,
                        &quieter,
                        &louder,
                    ] {
                        button.set_bounds(Rect::new(x, 18, button_width, 40));
                        x += (button_width as i32) + 8;
                    }
                    title.set_bounds(Rect::new(16, 78, width - 32, 42));
                    status.set_bounds(Rect::new(16, 132, width - 32, 42));
                }
                WidgetEvent::Close => {
                    player.stop();
                    return Ok(());
                }
                WidgetEvent::ButtonClicked(id) if *id == previous.id() => {
                    let autoplay = player.current.source().state().playing;
                    player.previous(autoplay)?;
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == play_pause.id() => {
                    player.toggle();
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == next.id() => {
                    let autoplay = player.current.source().state().playing;
                    player.next(autoplay)?;
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == stop.id() => {
                    player.stop();
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == seek_back.id() => {
                    player.seek_by(-SEEK_SECONDS);
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == seek_forward.id() => {
                    player.seek_by(SEEK_SECONDS);
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == quieter.id() => {
                    player.change_volume(-VOLUME_STEP);
                    refresh_status(&player, &title, &status);
                }
                WidgetEvent::ButtonClicked(id) if *id == louder.id() => {
                    player.change_volume(VOLUME_STEP);
                    refresh_status(&player, &title, &status);
                }
                _ => {}
            }
        }

        // Advance to the next playlist entry after the decoder reaches the end.
        // The next track autoplays because completion implies the current track
        // had already been playing.
        let state = player.current.source().state();
        if let Some(duration) = state.duration_seconds {
            if !state.playing
                && duration > 0.0
                && state.position_seconds >= (duration - 0.05).max(0.0)
            {
                player.next(true)?;
                refresh_status(&player, &title, &status);
            }
        }

        if last_status_refresh.elapsed() >= Duration::from_millis(200) {
            refresh_status(&player, &title, &status);
            last_status_refresh = Instant::now();
        }

        window.present();
        thread::sleep(Duration::from_millis(8));
    }
}
