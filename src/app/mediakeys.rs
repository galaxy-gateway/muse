//! OS media-key integration (souvlaki): handle now-playing control events and
//! push track metadata / play-state to the system now-playing display.

use crossbeam_channel::Sender;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig, SeekDirection,
};

use super::App;
use crate::audio::TransportCmd;
use crate::event::AppEvent;

impl App {
    /// Handle an OS media-key / now-playing control event.
    pub(super) fn on_media(&mut self, e: MediaControlEvent) {
        match e {
            MediaControlEvent::Play => self.engine.send(TransportCmd::Play),
            MediaControlEvent::Pause => self.engine.send(TransportCmd::Pause),
            MediaControlEvent::Toggle => self.engine.send(TransportCmd::Toggle),
            MediaControlEvent::Stop => self.engine.send(TransportCmd::Pause),
            MediaControlEvent::Next => self.play_relative(1),
            MediaControlEvent::Previous => self.play_relative(-1),
            MediaControlEvent::Seek(SeekDirection::Forward) => self.seek_rel(5.0),
            MediaControlEvent::Seek(SeekDirection::Backward) => self.seek_rel(-5.0),
            MediaControlEvent::SeekBy(dir, dur) => {
                let s = dur.as_secs_f64();
                let d = if matches!(dir, SeekDirection::Backward) {
                    -s
                } else {
                    s
                };
                self.seek_rel(d);
            }
            MediaControlEvent::SetPosition(pos) => self.seek_to_secs(pos.0.as_secs_f64()),
            MediaControlEvent::Quit => self.should_quit = true,
            _ => {}
        }
    }

    /// Push the now-playing track's tags to the OS now-playing display.
    pub(super) fn push_media_metadata(&mut self) {
        let Some(p) = self.now_playing.clone() else {
            return;
        };
        let meta = self.meta_cache.get(&p);
        let title = meta
            .filter(|m| !m.title.is_empty())
            .map(|m| m.title.clone())
            .unwrap_or_else(|| {
                p.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let artist = meta.map(|m| m.artist.clone()).unwrap_or_default();
        let album = meta.map(|m| m.album.clone()).unwrap_or_default();
        let duration = meta
            .map(|m| m.duration)
            .filter(|d| *d > 0.0)
            .map(std::time::Duration::from_secs_f64);
        if let Some(controls) = self.media.as_mut() {
            let _ = controls.set_metadata(MediaMetadata {
                title: Some(&title),
                artist: (!artist.is_empty()).then_some(&artist),
                album: (!album.is_empty()).then_some(&album),
                duration,
                cover_url: None,
            });
        }
    }

    /// Keep the OS play/pause indicator in sync with the engine.
    pub(super) fn sync_media_playback(&mut self) {
        let playing = self.engine.is_playing();
        if playing == self.media_playing {
            return;
        }
        self.media_playing = playing;
        if let Some(controls) = self.media.as_mut() {
            let state = if playing {
                MediaPlayback::Playing { progress: None }
            } else {
                MediaPlayback::Paused { progress: None }
            };
            let _ = controls.set_playback(state);
        }
    }
}

/// Best-effort OS media-control setup. Returns `None` (and the app keeps
/// working) if the platform integration is unavailable.
pub(super) fn init_media(tx: &Sender<AppEvent>) -> Option<MediaControls> {
    let config = PlatformConfig {
        dbus_name: "muse",
        display_name: "muse",
        hwnd: console_hwnd(),
    };
    let mut controls = MediaControls::new(config).ok()?;
    let tx = tx.clone();
    controls
        .attach(move |event| {
            let _ = tx.send(AppEvent::Media(event));
        })
        .ok()?;
    Some(controls)
}

/// Windows SMTC needs a window handle; a console app can borrow the console's.
#[cfg(target_os = "windows")]
fn console_hwnd() -> Option<*mut std::ffi::c_void> {
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }
    let h = unsafe { GetConsoleWindow() };
    (!h.is_null()).then_some(h)
}

#[cfg(not(target_os = "windows"))]
fn console_hwnd() -> Option<*mut std::ffi::c_void> {
    None
}
