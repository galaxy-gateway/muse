//! Unified event bus. Input, tick, and background workers all post `AppEvent`s
//! to a single crossbeam channel the UI thread drains.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;

pub enum AppEvent {
    Input(crossterm::event::KeyEvent),
    Mouse(crossterm::event::MouseEvent),
    Tick,
    /// Background waveform result: (path, generation token, bins).
    Wave(PathBuf, u64, Vec<(f32, f32)>),
    /// Background album-art result: (path, decoded cover or `None` if absent).
    Art(PathBuf, Option<image::RgbImage>),
    /// Full recursive media-file index for fuzzy filtering (built off-thread).
    Index(Vec<PathBuf>),
    /// OS media-key / now-playing control event (souvlaki).
    Media(souvlaki::MediaControlEvent),
}

/// Walk the whole tree once in the background and post every supported media
/// file as a flat index, so fuzzy filtering never blocks the UI thread.
pub fn spawn_index(root: PathBuf, tx: Sender<AppEvent>) {
    thread::spawn(move || {
        let reg = crate::media::Registry::new();
        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(&root)
            .standard_filters(false) // don't consult .gitignore for a music library
            .hidden(true) // but still skip dotfiles/dirs
            .build();
        for entry in walker.flatten() {
            let p = entry.path();
            if p.is_file() && reg.is_supported(p) {
                files.push(p.to_path_buf());
            }
        }
        let _ = tx.send(AppEvent::Index(files));
    });
}

/// Blocking key reader thread.
pub fn spawn_input(tx: Sender<AppEvent>) {
    thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key)) => {
                    if key.kind == crossterm::event::KeyEventKind::Press
                        && tx.send(AppEvent::Input(key)).is_err()
                    {
                        break;
                    }
                }
                Ok(crossterm::event::Event::Mouse(m)) => {
                    if tx.send(AppEvent::Mouse(m)).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

/// ~60Hz tick to drive the live scope + playhead.
pub fn spawn_ticks(tx: Sender<AppEvent>) {
    thread::spawn(move || {
        loop {
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(16));
        }
    });
}
