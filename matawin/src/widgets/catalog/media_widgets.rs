//! Audio/video catalog widgets and optional playback controls.

use std::{ path::{ Path, PathBuf }, sync::{ Arc, Mutex } };

use crate::widgets::{ media::AudioSource, WidgetError };
use super::{
    base::{ Widget, WidgetExt },
    buttons::ToggleButton,
    types::Orientation,
    values::{ Scale, VolumeButton },
};

#[derive(Debug, Clone)]
/// Stores the state and data for audio.
///
/// Visibility: **public**.
pub struct Audio {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores source.
    ///
    /// Visibility: **private**.
    source: AudioSource,
    /// Stores controls.
    ///
    /// Visibility: **private**.
    controls: Arc<Mutex<Option<MediaControls>>>,
}
impl Audio {
    /// Performs the from file operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the from file operation. Visibility: public."]
    pub fn from_file(
        path: impl AsRef<Path>,
        autoplay: bool,
        show_controls: bool
    ) -> Result<Self, WidgetError> {
        let source = AudioSource::open(path, autoplay).map_err(WidgetError)?;
        let controls = if show_controls { Some(MediaControls::new(source.clone())) } else { None };
        Ok(Self { widget: Widget::new(), source, controls: Arc::new(Mutex::new(controls)) })
    }
    /// Performs the source operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the source operation. Visibility: public."]
    pub fn source(&self) -> AudioSource {
        self.source.clone()
    }
    /// Performs the play operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the play operation. Visibility: public."]
    pub fn play(&self) {
        self.source.play()
    }
    #[doc = "Implements the pause operation. Visibility: public."]
    pub fn pause(&self) {
        self.source.pause()
    }
    #[doc = "Implements the stop operation. Visibility: public."]
    pub fn stop(&self) {
        self.source.stop()
    }
    /// Performs the seek operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the seek operation. Visibility: public."]
    pub fn seek(&self, seconds: f64) {
        self.source.seek(seconds)
    }
    #[doc = "Sets volume. Visibility: public."]
    pub fn set_volume(&self, v: f32) {
        self.source.set_volume(v)
    }
    #[doc = "Sets looping. Visibility: public."]
    pub fn set_looping(&self, v: bool) {
        self.source.set_looping(v)
    }
    /// Sets controls visible.
    ///
    /// Visibility: **public**.
    #[doc = "Sets controls visible. Visibility: public."]
    pub fn set_controls_visible(&self, visible: bool) {
        let mut c = self.controls.lock().unwrap();
        if visible && c.is_none() {
            *c = Some(MediaControls::new(self.source.clone()));
        } else if !visible {
            *c = None;
        }
    }
    /// Performs the controls operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the controls operation. Visibility: public."]
    pub fn controls(&self) -> Option<MediaControls> {
        self.controls.lock().unwrap().clone()
    }
}
impl WidgetExt for Audio {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

/// Optional media control strip for an [`AudioSource`].
#[derive(Debug, Clone)]
pub struct MediaControls {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores source.
    ///
    /// Visibility: **private**.
    source: AudioSource,
    /// Stores play pause.
    ///
    /// Visibility: **private**.
    play_pause: ToggleButton,
    /// Stores seek.
    ///
    /// Visibility: **private**.
    seek: Scale,
    /// Stores volume button.
    ///
    /// Visibility: **private**.
    volume_button: VolumeButton,
    /// Stores show volume.
    ///
    /// Visibility: **private**.
    show_volume: Arc<Mutex<bool>>,
    /// Stores show seek.
    ///
    /// Visibility: **private**.
    show_seek: Arc<Mutex<bool>>,
}
impl MediaControls {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new(source: AudioSource) -> Self {
        let controls = Self {
            widget: Widget::new(),
            source,
            play_pause: ToggleButton::new("Play / Pause"),
            seek: Scale::new(Orientation::Horizontal, 0.0, 1.0, 0.0),
            volume_button: VolumeButton::new(0.0, 1.0, 1.0),
            show_volume: Arc::new(Mutex::new(true)),
            show_seek: Arc::new(Mutex::new(true)),
        };
        controls.sync();
        controls
    }
    /// Performs the play operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the play operation. Visibility: public."]
    pub fn play(&self) {
        self.source.play();
        self.sync()
    }
    /// Performs the pause operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the pause operation. Visibility: public."]
    pub fn pause(&self) {
        self.source.pause();
        self.sync()
    }
    /// Performs the toggle operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the toggle operation. Visibility: public."]
    pub fn toggle(&self) {
        self.source.toggle_playback();
        self.sync()
    }
    /// Performs the stop operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the stop operation. Visibility: public."]
    pub fn stop(&self) {
        self.source.stop();
        self.sync()
    }
    /// Performs the seek operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the seek operation. Visibility: public."]
    pub fn seek(&self, seconds: f64) {
        self.source.seek(seconds);
        self.sync()
    }
    /// Sets volume.
    ///
    /// Visibility: **public**.
    #[doc = "Sets volume. Visibility: public."]
    pub fn set_volume(&self, v: f32) {
        self.source.set_volume(v);
        self.volume_button.set_value(v as f64)
    }
    /// Performs the position operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the position operation. Visibility: public."]
    pub fn position(&self) -> f64 {
        self.source.position()
    }
    /// Performs the duration operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the duration operation. Visibility: public."]
    pub fn duration(&self) -> Option<f64> {
        self.source.duration()
    }
    /// Performs the volume operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the volume operation. Visibility: public."]
    pub fn volume(&self) -> f32 {
        self.source.volume()
    }
    /// Performs the play pause button operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the play pause button operation. Visibility: public."]
    pub fn play_pause_button(&self) -> ToggleButton {
        self.play_pause.clone()
    }
    /// Performs the seek scale operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the seek scale operation. Visibility: public."]
    pub fn seek_scale(&self) -> Scale {
        self.seek.clone()
    }
    /// Performs the volume button operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the volume button operation. Visibility: public."]
    pub fn volume_button(&self) -> VolumeButton {
        self.volume_button.clone()
    }
    /// Sets show volume.
    ///
    /// Visibility: **public**.
    #[doc = "Sets show volume. Visibility: public."]
    pub fn set_show_volume(&self, v: bool) {
        *self.show_volume.lock().unwrap() = v;
        self.volume_button.set_visible(v)
    }
    /// Sets show seek.
    ///
    /// Visibility: **public**.
    #[doc = "Sets show seek. Visibility: public."]
    pub fn set_show_seek(&self, v: bool) {
        *self.show_seek.lock().unwrap() = v;
        self.seek.set_visible(v)
    }
    /// Performs the seek fraction operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the seek fraction operation. Visibility: public."]
    pub fn seek_fraction(&self) -> f64 {
        match self.duration() {
            Some(d) if d > 0.0 => (self.position() / d).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }
    /// Performs the seek to fraction operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the seek to fraction operation. Visibility: public."]
    pub fn seek_to_fraction(&self, fraction: f64) {
        if let Some(duration) = self.duration() {
            self.seek(duration * fraction.clamp(0.0, 1.0));
        }
    }
    /// Synchronizes the composite play/pause, normalized seek, and volume
    /// controls from the source's current shared state.
    #[doc = "Implements the sync operation. Visibility: public."]
    pub fn sync(&self) {
        let state = self.source.state();
        self.play_pause.set_active(state.playing);
        self.volume_button.set_value(state.volume as f64);
        let fraction = match state.duration_seconds {
            Some(d) if d > 0.0 => (state.position_seconds / d).clamp(0.0, 1.0),
            _ => 0.0,
        };
        self.seek.set_value(fraction);
    }
}
impl WidgetExt for MediaControls {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}

/// GTK-compatible video shell. MataWin currently keeps video visual decoding
/// separate, but an optional streamed audio track can be attached and managed
/// with the same `MediaControls` API.
#[derive(Debug, Clone)]
/// Stores the state and data for video.
///
/// Visibility: **public**.
pub struct Video {
    /// Stores widget.
    ///
    /// Visibility: **private**.
    widget: Widget,
    /// Stores file.
    ///
    /// Visibility: **private**.
    file: Arc<Mutex<Option<PathBuf>>>,
    /// Stores audio.
    ///
    /// Visibility: **private**.
    audio: Arc<Mutex<Option<AudioSource>>>,
    /// Stores controls.
    ///
    /// Visibility: **private**.
    controls: Arc<Mutex<Option<MediaControls>>>,
    /// Stores autoplay.
    ///
    /// Visibility: **private**.
    autoplay: Arc<Mutex<bool>>,
    /// Stores looping.
    ///
    /// Visibility: **private**.
    looping: Arc<Mutex<bool>>,
}
impl Default for Video {
    #[doc = "Creates the default value. Visibility: private/internal."]
    fn default() -> Self {
        Self::new()
    }
}
impl Video {
    /// Creates a new instance.
    ///
    /// Visibility: **public**.
    #[doc = "Creates a new value. Visibility: public."]
    pub fn new() -> Self {
        Self {
            widget: Widget::new(),
            file: Arc::new(Mutex::new(None)),
            audio: Arc::new(Mutex::new(None)),
            controls: Arc::new(Mutex::new(None)),
            autoplay: Arc::new(Mutex::new(false)),
            looping: Arc::new(Mutex::new(false)),
        }
    }
    /// Sets file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets file. Visibility: public."]
    pub fn set_file(&self, path: impl AsRef<Path>) {
        *self.file.lock().unwrap() = Some(path.as_ref().to_path_buf());
    }
    /// Sets audio file.
    ///
    /// Visibility: **public**.
    #[doc = "Sets audio file. Visibility: public."]
    pub fn set_audio_file(
        &self,
        path: impl AsRef<Path>,
        show_controls: bool
    ) -> Result<(), WidgetError> {
        let source = AudioSource::open(path, *self.autoplay.lock().unwrap()).map_err(WidgetError)?;
        source.set_looping(*self.looping.lock().unwrap());
        *self.audio.lock().unwrap() = Some(source.clone());
        *self.controls.lock().unwrap() = if show_controls {
            Some(MediaControls::new(source))
        } else {
            None
        };
        Ok(())
    }
    /// Sets autoplay.
    ///
    /// Visibility: **public**.
    #[doc = "Sets autoplay. Visibility: public."]
    pub fn set_autoplay(&self, v: bool) {
        *self.autoplay.lock().unwrap() = v;
    }
    #[doc = "Sets looping. Visibility: public."]
    pub fn set_looping(&self, v: bool) {
        *self.looping.lock().unwrap() = v;
        if let Some(a) = self.audio.lock().unwrap().as_ref() {
            a.set_looping(v)
        }
    }
    /// Performs the controls operation.
    ///
    /// Visibility: **public**.
    #[doc = "Implements the controls operation. Visibility: public."]
    pub fn controls(&self) -> Option<MediaControls> {
        self.controls.lock().unwrap().clone()
    }
}
impl WidgetExt for Video {
    #[doc = "Implements the widget operation. Visibility: private/internal."]
    fn widget(&self) -> &Widget {
        &self.widget
    }
}
