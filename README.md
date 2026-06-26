# muse

A terminal music explorer. Browse a directory tree of audio, play it, and *see*
it — a high-fidelity braille oscilloscope synced to the DAC plus a static
waveform overview, in an elegant TUI.

```
muse            # explore the current directory
muse ~/Music    # explore a directory
```

## Status: MVP (Phase 0–1)

- Lazy recursive file tree (dirs scanned on expand; dotfiles, non-media, and
  directories holding no music anywhere below them are hidden)
- Audio playback via symphonia → cpal, with an ffmpeg fallback decoder
- **Live oscilloscope** (braille canvas, sample-synced via a `triple_buffer` tap
  in the audio callback) with cyclable presets (`v`): line, fast, mirror, dots,
  bars, and a stereo XY / Lissajous vectorscope — each bundles a render style,
  signal mode, time-window, and auto-gain setting
- **Static waveform** overview (background peak-bin analysis, cached) with a
  playhead
- `selection` panel (above the file tree) follows the cursor; a separate
  `now playing` panel tracks the engine's current track
- Read-only metadata panel (title / artist / album / genre / codec) via lofty
- Transport bar: play/pause, seek, volume, progress
- Fuzzy finder (`/`): searches a background-built index of every track (never
  blocks the UI); flat ranked results, `⏎` applies, `esc` resets to the tree
- Color themes (`t`): `midnight` plus pride-flag palettes (pride, trans, bi,
  lesbian, pan, nonbinary, ace) and a `prismatic` mode with rainbow borders that
  shift over time
- Remembers the last scope preset and theme between launches (TOML at the
  platform config dir)

Architecture leaves a `MediaProvider` seam (`src/media.rs`) so image/video
preview + compare can be added later without touching the core.

## Keys

| key | action |
|---|---|
| `j`/`k` or `↑`/`↓` | move cursor |
| `l`/`→` | expand dir |
| `h`/`←` | collapse / jump to parent |
| `g`/`G` | top / bottom |
| `⏎` | expand dir or play file |
| `space` / `p` | play / pause |
| `,` / `.` | seek −5s / +5s |
| `-` / `+` | volume |
| `v` / `V` | cycle scope preset (forward / back) |
| `t` / `T` | cycle color theme (forward / back) |
| `n` / `p` | next / previous track |
| `r` | loop mode (off / all / one) |
| `/` | fuzzy find (⏎ apply · esc reset) |
| `?` / `q` | help / quit |

## Build

```
cargo build --release
./target/release/muse ~/Music
```

Requires `ffmpeg` on PATH only for the fallback decoder (HE-AAC / odd formats).

## Layout

```
src/
  main.rs    terminal setup/teardown, event loop, --probe smoke test
  app.rs     App state + key handling (single mutation surface)
  audio.rs   decode (symphonia/ffmpeg) -> rtrb -> cpal; triple_buffer scope tap
  media.rs   MediaProvider/Meta seam + audio provider + lofty tags
  model.rs   lazy flattened file tree
  ui.rs      all rendering (tree, inspector, scope, waveform, transport)
  event.rs   input/tick/worker event bus
  config.rs  theme
  util.rs    formatting
```

## Roadmap

P2 move + undo · P3 auto-sort (dry-run) + tag write · P4 A/B compare ·
P5 image/video providers (proving the media seam).
