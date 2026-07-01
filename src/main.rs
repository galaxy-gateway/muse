//! muse — a TUI music explorer. `muse [dir]` (defaults to ".").

mod app;
mod archive;
mod audio;
mod beat;
mod color;
mod config;
mod dircache;
mod effects;
mod event;
mod media;
mod metacache;
mod model;
mod particles;
mod spectrum;
mod ui;
mod util;

use std::io::{self, Write, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::unbounded;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;
use crate::event::{AppEvent, spawn_dir_stats, spawn_index, spawn_input, spawn_ticks};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // Hidden headless smoke test: `muse --probe <file>` decodes + reads tags.
    if args.get(1).map(|s| s == "--probe").unwrap_or(false) {
        return probe(args.get(2).map(PathBuf::from));
    }

    let dir = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    if !dir.is_dir() {
        eprintln!("muse: not a directory: {}", dir.display());
        std::process::exit(1);
    }

    // Route panics (incl. background decode threads) to a logfile rather than
    // stderr: a recoverable decode panic must not corrupt the alternate-screen
    // TUI. The audio threads catch_unwind and keep running; this just records
    // what happened for debugging.
    install_panic_logger();

    let (tx, rx) = unbounded::<AppEvent>();
    let mut app = App::new(&dir, tx.clone())?;

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    // Any-motion mouse tracking (1003) so hover works without a button held.
    let _ = write!(stdout(), "\x1b[?1003h");
    let _ = stdout().flush();

    // Detect terminal graphics support (Kitty/iTerm2/Sixel) for crisp album art,
    // falling back to Unicode half-blocks. Queried before the input thread starts
    // so the terminal's response on stdin isn't swallowed by it.
    app.set_picker(ratatui_image::picker::Picker::from_query_stdio().ok());

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    spawn_input(tx.clone());
    spawn_ticks(tx.clone());
    spawn_index(dir.clone(), tx.clone());
    // Fill in each top-level dir's recursive stats off-thread so the tree
    // populates progressively behind the already-visible UI.
    spawn_dir_stats(app.tree.pending_paths(), tx.clone());

    let res = run(&mut terminal, &mut app, rx);

    // Persist the session (now-playing track, position, volume, loop, cursor)
    // so the next launch resumes where this one left off.
    app.save_state();

    let _ = write!(terminal.backend_mut(), "\x1b[?1003l");
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    res
}

/// Send panic output to the muse logfile instead of stderr, so a background
/// decoder panic (which the audio threads catch and recover from) cannot scribble
/// over the TUI's alternate screen.
fn install_panic_logger() {
    use std::io::Write as _;
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = crate::config::log_path() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(f, "panic: {info}");
            }
        }
    }));
}

fn probe(path: Option<PathBuf>) -> Result<()> {
    let path = path.ok_or_else(|| anyhow::anyhow!("usage: muse --probe <file>"))?;
    let reg = crate::media::Registry::new();
    let prov = reg
        .for_path(&path)
        .ok_or_else(|| anyhow::anyhow!("unsupported file type"))?;
    let meta = prov.metadata(&path);
    println!(
        "provider={} title={:?} artist={:?} genre={:?} dur={:.1}s fields={:?}",
        prov.id(),
        meta.title,
        meta.artist,
        meta.genre,
        meta.duration,
        meta.fields
    );
    let bins = crate::audio::waveform_bins(&path, 64)?;
    let peak = bins.iter().fold(0f32, |m, &(lo, hi)| m.max(hi.max(-lo)));
    println!("waveform bins={} peak={:.3}", bins.len(), peak);
    Ok(())
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    rx: crossbeam_channel::Receiver<AppEvent>,
) -> Result<()> {
    terminal.draw(|f| ui::draw(f, app))?;
    let mut last_draw = Instant::now();
    let min_frame = Duration::from_millis(16);

    while let Ok(ev) = rx.recv() {
        let is_tick = matches!(ev, AppEvent::Tick);
        app.handle(ev);
        if app.should_quit {
            break;
        }
        // Coalesce ticks: redraw at most ~60fps; always redraw on non-tick events.
        if !is_tick || last_draw.elapsed() >= min_frame {
            terminal.draw(|f| ui::draw(f, app))?;
            last_draw = Instant::now();
        }
    }
    Ok(())
}
