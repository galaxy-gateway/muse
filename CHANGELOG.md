# Changelog

Earlier history lives in the git log.

## 0.66.1 - 2026-11-18

### Fixed
- Replace `chunks_exact` with `as_chunks` in the audio decode paths so
  `cargo clippy -- -D warnings` passes again.

## 0.66.0 - 2026-11-18

### Added
- Right-click context menus for tree rows, transport/now-playing/waveform, and
  the scope (#38).
- Command palette on : or Ctrl+P with fuzzy filtering over 28 commands,
  including Reset Pane Layout (#36).
- Help sheet entries for the palette and context menu.

Pane dividers (drag-to-resize, #65) shipped separately in #67.
