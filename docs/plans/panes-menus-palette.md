# Plan: resizable panes, context menus, command palette

Issues: [#65](https://github.com/galaxy-gateway/muse/issues/65) (resize),
[#38](https://github.com/galaxy-gateway/muse/issues/38) (context menu),
[#36](https://github.com/galaxy-gateway/muse/issues/36) (palette).

## Design (src/app/cmd.rs, src/ui/overlay.rs)

- **One action table.** `CMDS` = (title, key hint, `Act`). `Act::Key` replays a
  normal-mode key through `on_key`, so palette and menus never duplicate
  behaviour. New command = one table row (or a new key + a row).
- **Palette**: `:` or Ctrl+P. Fuzzy-filters titles via `util::fuzzy_score`.
  Consumes keys while open (checked first in `on_key`).
- **Context menu**: right-click. Items depend on the zone (tree row, transport /
  now-playing / waveform, scope, elsewhere). Hover selects, click runs, any other
  click dismisses. Rect is computed at open (clamped to screen).
- **Dividers**: implemented separately in `src/app/panes.rs` (#67). The
  palette and menu only expose `Reset Pane Layout`, which restores its defaults.

## Not done / follow-ups
- Palette has no mouse support; menu has no submenus.
- Keybinding hints in `CMDS` are static (see #37 for configurable keys).
