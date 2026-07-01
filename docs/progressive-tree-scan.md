# Progressive tree scan (instant TUI + progress bar)

## Problem

`App::new` built the whole file tree synchronously before the terminal was even
set up, so a large library hung on a blank screen at launch. The tree is "lazy"
(dirs scanned on expand), but the **root** scan called `dir_stats` on every
top-level dir, and `dir_stats` **recurses the entire subtree** to count media
files (needed to prune music-less dirs and show `N tracks`). So opening the root
walked the whole library up front.

## Fix: shallow scan + streamed stats

1. **Shallow root scan** (`TreeModel::scan_shallow`, used by `TreeModel::new`).
   Reads only the root's immediate children — one `read_dir`, no recursion. Each
   dir/archive becomes a `pending` node (shown, not yet counted). The TUI renders
   immediately.
2. **Background stats worker** (`event::spawn_dir_stats`, spawned from `main`).
   Computes each top-level entry's recursive `dir_stats` (or archive audio count)
   off-thread and streams `AppEvent::DirStats { path, count, size, scanned,
   total }` back.
3. **Progressive apply** (`App::handle`). Each result calls
   `TreeModel::apply_stats` (records count/size, clears `pending`),
   `rebuild_visible` (now that stats are in, a music-less dir is pruned), and
   `reselect_after_scan` (keeps the cursor on the same path across the reflow).

`scan_done`/`scan_total` on `App` drive a progress bar; `App::scanning()` is true
until `scan_done == scan_total`.

## Deferred pruning

`push_visible` hides an expandable node only once it is **done and empty**
(`!pending && count == 0`). While `pending` it stays visible, so dirs appear
right away and music-less ones quietly drop out as their counts arrive. Pending
dirs render a `…` in place of the `N tracks` meta.

## UI

While `App::scanning()`, the tree panel's bottom border shows a spinner, a
rotating tongue-in-cheek quip (`SCAN_QUIPS`), a text progress bar, and
`done/total` (`ui/tree.rs::scan_hint`). It disappears when the scan completes and
the normal key hint returns.

## Notes

- Expanding a still-`pending` dir triggers the normal (full, recursive) `scan`
  for that one subtree on demand; the background result later reconciles its
  count. Slight duplicate work, harmless.
- The fuzzy-search index (`spawn_index`) is unchanged — it already walked the
  tree off-thread.
- On-expand scanning of deep dirs is still synchronous; only the startup root
  walk was the launch blocker.
