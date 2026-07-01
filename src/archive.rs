//! In-memory archive VFS. Lists and reads audio entries from zip / tar /
//! tar.gz / tar.bz2 / 7z containers **without** extracting anything to the
//! user's disk: an entry is decompressed into process memory on demand and
//! cached, and `close` wipes an archive's memory when it is collapsed in the UI.
//!
//! The rest of muse addresses an entry by a *virtual path* — an ordinary
//! `PathBuf` of the form `<archive-on-disk>/<inner/entry.flac>`. Because the
//! archive prefix is a real file, `Path::extension` / `file_name` still work, so
//! the registry, the tree label, and the inspector treat a virtual path exactly
//! like a disk file. Only this module, the tree, and the decode/tag leaves need
//! to know the difference.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::media::Registry;

/// Decompressed entry bytes kept in memory across the app; prevents a 1.7GB zip
/// from caching all 1.7GB forever. LRU eviction on archive browse.
const ARCHIVE_BYTES_CAP: usize = 256 * 1024 * 1024;

/// One file inside an archive (audio entries only, after filtering).
#[derive(Clone)]
pub struct ArchiveEntry {
    /// Inner path within the archive, '/'-separated (e.g. `disc1/01 - Track.flac`).
    pub inner: String,
    /// Uncompressed size in bytes (0 if the format doesn't report it).
    pub size: u64,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    SevenZ,
}

/// Whether `path` names an archive container muse can open (by extension).
pub fn is_archive(path: &Path) -> bool {
    kind_of(path).is_some()
}

fn kind_of(path: &Path) -> Option<Kind> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Kind::TarGz)
    } else if name.ends_with(".tar.bz2") || name.ends_with(".tbz2") || name.ends_with(".tbz") {
        Some(Kind::TarBz2)
    } else if name.ends_with(".tar") {
        Some(Kind::Tar)
    } else if name.ends_with(".zip") {
        Some(Kind::Zip)
    } else if name.ends_with(".7z") {
        Some(Kind::SevenZ)
    } else {
        None
    }
}

/// Resolve a virtual path into `(archive-on-disk, inner-entry)`, or `None` for a
/// plain disk file or an archive container itself. Walks ancestors deepest-first
/// and returns the deepest ancestor that is an existing archive file on disk.
/// Nested archives are not supported (an inner `.zip` isn't a file on disk).
pub fn split_virtual(path: &Path) -> Option<(PathBuf, String)> {
    // skip(1): the entry is never the container itself.
    for anc in path.ancestors().skip(1) {
        if anc.as_os_str().is_empty() {
            break;
        }
        if is_archive(anc) && anc.is_file() {
            let inner = path.strip_prefix(anc).ok()?;
            let s = inner.to_string_lossy().replace('\\', "/");
            if s.is_empty() {
                return None;
            }
            return Some((anc.to_path_buf(), s));
        }
    }
    None
}

struct Store {
    /// Audio-entry listing per archive (cheap; just names + sizes).
    index: HashMap<PathBuf, Arc<Vec<ArchiveEntry>>>,
    /// Decompressed entry bytes per (archive, inner) — the heavy memory.
    bytes: HashMap<(PathBuf, String), Arc<Vec<u8>>>,
    /// LRU insertion order for `bytes` (oldest first), for bounded eviction.
    bytes_order: Vec<(PathBuf, String)>,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| {
    Mutex::new(Store {
        index: HashMap::new(),
        bytes: HashMap::new(),
        bytes_order: Vec::new(),
    })
});

/// The audio entries inside `archive`, cached. Empty on any read error.
pub fn list_audio(archive: &Path) -> Arc<Vec<ArchiveEntry>> {
    let key = archive.to_path_buf();
    if let Some(v) = STORE.lock().unwrap().index.get(&key) {
        return v.clone();
    }
    let reg = Registry::new();
    let audio: Vec<ArchiveEntry> = read_index(archive)
        .unwrap_or_default()
        .into_iter()
        .filter(|e| reg.is_supported(Path::new(&e.inner)))
        .collect();
    let arc = Arc::new(audio);
    STORE.lock().unwrap().index.insert(key, arc.clone());
    arc
}

/// Decompressed bytes for one entry, cached in memory. `None` on error.
pub fn read(archive: &Path, inner: &str) -> Option<Arc<Vec<u8>>> {
    let key = (archive.to_path_buf(), inner.to_string());

    // Check cache and touch LRU on hit.
    {
        let mut store = STORE.lock().unwrap();
        if let Some(b) = store.bytes.get(&key) {
            let result = b.clone();
            // Move to back (most recently used).
            if let Some(pos) = store.bytes_order.iter().position(|k| k == &key) {
                store.bytes_order.remove(pos);
                store.bytes_order.push(key.clone());
            }
            return Some(result);
        }
    }

    // Not cached; decompress (lock released during decompression).
    let data = read_entry(archive, inner)?;
    let arc = Arc::new(data);

    // Re-check and insert with LRU eviction (prevent duplicate insertion race).
    {
        let mut store = STORE.lock().unwrap();

        // Another thread may have inserted this entry while we decompressed.
        if let Some(b) = store.bytes.get(&key) {
            return Some(b.clone());
        }

        // Insert our decompressed copy.
        store.bytes.insert(key.clone(), arc.clone());
        store.bytes_order.push(key.clone());

        // Evict oldest entries if total size exceeds cap.
        loop {
            let total_size: usize = store
                .bytes_order
                .iter()
                .filter_map(|k| store.bytes.get(k).map(|v| v.len()))
                .sum();

            if total_size <= ARCHIVE_BYTES_CAP || store.bytes_order.len() <= 1 {
                break;
            }

            // Remove the oldest entry (front of order).
            let oldest_key = store.bytes_order.remove(0);
            store.bytes.remove(&oldest_key);
        }
    }

    Some(arc)
}

/// Wipe all cached memory for `archive` — its entry index and every
/// decompressed entry. Called when the archive node is collapsed in the UI so
/// "closing the archive" releases the memory, as required.
pub fn close(archive: &Path) {
    let mut s = STORE.lock().unwrap();
    s.index.remove(archive);
    s.bytes.retain(|(p, _), _| p != archive);
    s.bytes_order.retain(|(p, _)| p != archive);
}

// --- per-format listing ---------------------------------------------------

fn read_index(archive: &Path) -> Option<Vec<ArchiveEntry>> {
    match kind_of(archive)? {
        Kind::Zip => zip_index(archive),
        Kind::Tar | Kind::TarGz | Kind::TarBz2 => tar_index(archive, kind_of(archive)?),
        Kind::SevenZ => sevenz_index(archive),
    }
}

fn zip_index(archive: &Path) -> Option<Vec<ArchiveEntry>> {
    let file = File::open(archive).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        if let Ok(e) = zip.by_index(i) {
            if e.is_file() {
                out.push(ArchiveEntry {
                    inner: norm_inner(e.name()),
                    size: e.size(),
                });
            }
        }
    }
    Some(out)
}

fn tar_index(archive: &Path, kind: Kind) -> Option<Vec<ArchiveEntry>> {
    let mut tar = tar_reader(archive, kind)?;
    let mut out = Vec::new();
    for entry in tar.entries().ok()? {
        let Ok(entry) = entry else { continue };
        if entry.header().entry_type().is_file() {
            if let Ok(p) = entry.path() {
                out.push(ArchiveEntry {
                    inner: norm_inner(&p.to_string_lossy()),
                    size: entry.header().size().unwrap_or(0),
                });
            }
        }
    }
    Some(out)
}

fn sevenz_index(archive: &Path) -> Option<Vec<ArchiveEntry>> {
    let arc = sevenz_rust::Archive::open(archive).ok()?;
    let out = arc
        .files
        .iter()
        .filter(|e| e.has_stream() && !e.is_directory())
        .map(|e| ArchiveEntry {
            inner: norm_inner(e.name()),
            size: e.size(),
        })
        .collect();
    Some(out)
}

// --- per-format single-entry read -----------------------------------------

fn read_entry(archive: &Path, inner: &str) -> Option<Vec<u8>> {
    match kind_of(archive)? {
        Kind::Zip => zip_read(archive, inner),
        Kind::Tar | Kind::TarGz | Kind::TarBz2 => tar_read(archive, kind_of(archive)?, inner),
        Kind::SevenZ => sevenz_read(archive, inner),
    }
}

fn zip_read(archive: &Path, inner: &str) -> Option<Vec<u8>> {
    let file = File::open(archive).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;
    // Entry names may or may not match `norm_inner`; try the literal name first,
    // then fall back to a normalized scan.
    if let Ok(mut e) = zip.by_name(inner) {
        let mut buf = Vec::with_capacity(e.size() as usize);
        e.read_to_end(&mut buf).ok()?;
        return Some(buf);
    }
    for i in 0..zip.len() {
        let name = { zip.by_index(i).ok().map(|e| norm_inner(e.name())) };
        if name.as_deref() == Some(inner) {
            let mut e = zip.by_index(i).ok()?;
            let mut buf = Vec::with_capacity(e.size() as usize);
            e.read_to_end(&mut buf).ok()?;
            return Some(buf);
        }
    }
    None
}

fn tar_read(archive: &Path, kind: Kind, inner: &str) -> Option<Vec<u8>> {
    let mut tar = tar_reader(archive, kind)?;
    for entry in tar.entries().ok()? {
        let Ok(mut entry) = entry else { continue };
        let matches = entry
            .path()
            .map(|p| norm_inner(&p.to_string_lossy()) == inner)
            .unwrap_or(false);
        if matches {
            let mut buf = Vec::with_capacity(entry.header().size().unwrap_or(0) as usize);
            entry.read_to_end(&mut buf).ok()?;
            return Some(buf);
        }
    }
    None
}

fn sevenz_read(archive: &Path, inner: &str) -> Option<Vec<u8>> {
    let mut reader =
        sevenz_rust::SevenZReader::open(archive, sevenz_rust::Password::empty()).ok()?;
    let mut found: Option<Vec<u8>> = None;
    reader
        .for_each_entries(|entry, rd| {
            if norm_inner(entry.name()) == inner {
                let mut buf = Vec::with_capacity(entry.size() as usize);
                rd.read_to_end(&mut buf)?;
                found = Some(buf);
                return Ok(false); // stop iterating
            }
            // Drain this entry's stream so the decoder can advance to the next.
            std::io::copy(rd, &mut std::io::sink())?;
            Ok(true)
        })
        .ok()?;
    found
}

fn tar_reader(archive: &Path, kind: Kind) -> Option<tar::Archive<Box<dyn Read>>> {
    let f = File::open(archive).ok()?;
    let r: Box<dyn Read> = match kind {
        Kind::Tar => Box::new(f),
        Kind::TarGz => Box::new(flate2::read::GzDecoder::new(f)),
        Kind::TarBz2 => Box::new(bzip2::read::BzDecoder::new(f)),
        _ => return None,
    };
    Some(tar::Archive::new(r))
}

/// Normalize an archive entry name to muse's inner-path form: forward slashes,
/// no leading `./` or `/`.
fn norm_inner(name: &str) -> String {
    let s = name.replace('\\', "/");
    s.trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn kind(name: &str) -> Option<Kind> {
        kind_of(Path::new(name))
    }

    #[test]
    fn extension_classification() {
        assert!(kind("a.zip") == Some(Kind::Zip));
        assert!(kind("a.7z") == Some(Kind::SevenZ));
        assert!(kind("a.tar") == Some(Kind::Tar));
        assert!(kind("a.tar.gz") == Some(Kind::TarGz) && kind("a.tgz") == Some(Kind::TarGz));
        assert!(kind("a.tar.bz2") == Some(Kind::TarBz2) && kind("a.tbz2") == Some(Kind::TarBz2));
        // case-insensitive, and non-archives reject
        assert!(kind("A.ZIP") == Some(Kind::Zip));
        assert!(kind("song.mp3").is_none() && kind("folder").is_none());
    }

    #[test]
    fn norm_inner_strips_prefixes() {
        assert_eq!(norm_inner("./a/b.mp3"), "a/b.mp3");
        assert_eq!(norm_inner("/a/b.mp3"), "a/b.mp3");
        assert_eq!(norm_inner("a\\b.mp3"), "a/b.mp3");
    }

    #[test]
    fn split_virtual_resolves_against_real_archive() {
        // A real (non-archive-valid but present) file on disk named *.zip is
        // enough for the boundary search, which only checks extension + is_file.
        let dir = std::env::temp_dir().join(format!("muse_vfs_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("Album.zip");
        let mut f = File::create(&zip).unwrap();
        f.write_all(b"PK").unwrap();
        let plain = dir.join("loose.mp3");
        File::create(&plain).unwrap();

        let entry = zip.join("disc1/01 - Track.flac");
        assert_eq!(
            split_virtual(&entry),
            Some((zip.clone(), "disc1/01 - Track.flac".to_string()))
        );
        // The container itself is not an entry.
        assert_eq!(split_virtual(&zip), None);
        // A plain disk file is not virtual.
        assert_eq!(split_virtual(&plain), None);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
