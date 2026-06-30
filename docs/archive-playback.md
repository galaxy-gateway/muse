# In-archive audio playback

Play audio that lives inside `.zip`, `.tar`, `.tar.gz`/`.tgz`, `.tar.bz2`/`.tbz2`,
and `.7z` archives as if each archive were a folder in the tree — without ever
writing the contained audio to the user's disk. Decompressed bytes live only in
process memory and are wiped when the archive is collapsed (closed) in the UI.

RAR is intentionally out of scope here (its format has no pure-Rust decoder); it
is tracked as a follow-up.

## The core idea: virtual paths

muse keys everything — `meta_cache`, `wave_cache`, `queue`, `now_playing`,
session persistence, shuffle, the tree — off a single `PathBuf`. Rather than
introduce a parallel "source" type through all of that, an archive entry is
addressed by a **virtual path** that is an ordinary `PathBuf`:

```
/music/Album.zip/disc1/01 - Track.flac
└──── archive (real file) ────┘└── inner entry ──┘
```

Because `Album.zip` is a real file on disk, the prefix is real and the suffix is
the entry inside it. `PathBuf::extension()` / `file_name()` still return
`flac` / `01 - Track.flac`, so `Registry::supports`, the UI name, and the
selection panel all work unchanged. The whole app carries these paths around
opaquely; only three I/O leaves and the tree need to know the difference.

`archive::split_virtual(path) -> Option<(archive_path, inner)>` resolves one by
walking the path's ancestors and returning the deepest ancestor that is an
existing archive file on disk, plus the remainder as the inner entry string. It
returns `None` for a plain disk file or for an archive container itself (no
inner component). Nested archives (an archive inside an archive) are not
supported.

## The in-memory store (`src/archive.rs`)

A process-global `LazyLock<Mutex<ArchiveStore>>` so the decode, loader, and
waveform background threads — which only ever receive a `PathBuf` — can resolve
bytes without an `App` reference.

- `list_audio(archive)` → cached `Arc<Vec<ArchiveEntry>>` of the audio entries
  (read from the archive's directory/headers; cheap for zip/7z, a scan for tar).
- `read(archive, inner)` → cached `Arc<Vec<u8>>` of one entry, decompressed into
  memory on first request (the waveform pass and the decode pass share the cache,
  so each entry decompresses once).
- `close(archive)` → drops the index and **all** cached entry bytes for that
  archive. Called when an archive node is collapsed, satisfying "wipe that
  memory out." Also dropped naturally on process exit.

`MemSource` adapts an `Arc<Vec<u8>>` into a symphonia `MediaSource` (seekable,
known length) for decoding; lofty reads tags from a `Cursor` over the same bytes.

## Tree integration (`src/model.rs`, `src/app/nav.rs`)

- `Node` gains `is_archive`. An archive file is shown as an expandable node
  (like a folder) with its audio-entry count.
- `dir_stats` counts an archive's audio entries toward its parent directory, so a
  folder that contains only archives is not pruned as "music-less."
- Expanding an archive node lists it from the store and builds the full inner
  subtree at once (entries are already known): virtual inner directories become
  ordinary `is_dir` nodes whose children are pre-populated, so descending into
  them touches no disk.
- Collapsing an archive node calls `store.close(archive)` and resets the node to
  unscanned, so re-expanding re-lists and re-reads from scratch.

## I/O leaves (`src/audio.rs`, `src/media.rs`)

`load_track`, `waveform_bins`, and `read_audio_meta` first try
`split_virtual`; if the path is virtual they decode/read tags from the store's
in-memory bytes instead of `File::open`. The ffmpeg fallback pipes the bytes via
stdin (`-i pipe:0`) so it works for archive entries too.

## Known limitations (v1)

- Session resume skips an in-archive now-playing track (its virtual path fails
  `Path::exists()` on restart). Tracked for follow-up.
- The fuzzy index (`spawn_index`) walks the real filesystem only, so in-archive
  tracks are not searchable from the filter yet.
- No hard cap on cached decompressed bytes; memory is bounded by closing the
  archive. A size-based eviction policy can be added later.
- Listing tar/tar.gz/tar.bz2 entries scans the whole stream (no random access);
  the result is cached, so the cost is paid once per open.
</content>
</invoke>
