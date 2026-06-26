# muse

A terminal music explorer. Browse a directory tree of audio, play it, and *see*
it — a high-fidelity braille oscilloscope synced to the DAC plus a static
waveform overview, in an elegant TUI.

```
muse            # explore the current directory
muse ~/Music    # explore a directory
```

## Status: MVP (Phase 0–1)

- Lazy recursive file tree (dirs scanned on expand; dotfiles + non-media hidden)
- Audio playback via symphonia → cpal, with an ffmpeg fallback decoder
- **Live oscilloscope** (braille canvas, sample-synced via a `triple_buffer` tap
  in the audio callback)
- **Static waveform** overview (background peak-bin analysis, cached) with a
  playhead
- Read-only metadata panel (title / artist / album / genre / codec) via lofty
- Transport bar: play/pause, seek, volume, progress

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
| `/` | filter (esc clears) |
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
