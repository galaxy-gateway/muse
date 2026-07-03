//! Media-type seam. The core never names "audio" directly — it asks the registry
//! for the provider that `supports(path)`. Audio is the only provider today;
//! image/video providers can be added later without touching the UI or app core.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Ordered, typed-ish metadata so unrelated media can expose different fields
/// while the inspector / future diff view stay generic.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration: f64,
    /// Extra display rows (codec, bitrate, sample rate, ...).
    pub fields: Vec<(String, String)>,
}

pub trait MediaProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn supports(&self, path: &Path) -> bool;
    fn metadata(&self, path: &Path) -> Meta;
    /// Whether this media can be played (audio: yes; image later: no).
    #[allow(dead_code)] // consumed by the player UI in a later phase
    fn is_playable(&self) -> bool;
}

const AUDIO_EXTS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "alac", "wv",
];

pub struct AudioProvider;

impl MediaProvider for AudioProvider {
    fn id(&self) -> &'static str {
        "audio"
    }

    fn supports(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn metadata(&self, path: &Path) -> Meta {
        read_audio_meta(path)
    }

    fn is_playable(&self) -> bool {
        true
    }
}

pub struct Registry {
    providers: Vec<Box<dyn MediaProvider>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            providers: vec![Box::new(AudioProvider)],
        }
    }

    pub fn for_path(&self, path: &Path) -> Option<&dyn MediaProvider> {
        self.providers
            .iter()
            .find(|p| p.supports(path))
            .map(|b| b.as_ref())
    }

    pub fn is_supported(&self, path: &Path) -> bool {
        self.for_path(path).is_some()
    }

    /// A playlist file (m3u/m3u8): shown in the tree and loadable, but not
    /// itself decodable media.
    pub fn is_playlist(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| matches!(e.to_ascii_lowercase().as_str(), "m3u" | "m3u8"))
            .unwrap_or(false)
    }

    /// Whether the tree should show this path: playable media or a playlist.
    pub fn is_visible(&self, path: &Path) -> bool {
        self.is_supported(path) || self.is_playlist(path)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn read_audio_meta(path: &Path) -> Meta {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::{Accessor, ItemKey};

    let mut meta = Meta::default();
    // In-archive entries read their tags from the in-memory bytes (probed for
    // type) rather than the disk path, which doesn't exist as a real file.
    let tagged = if let Some((arc, inner)) = crate::archive::split_virtual(path) {
        let opt = crate::archive::read(&arc, &inner).and_then(|bytes| {
            lofty::probe::Probe::new(std::io::Cursor::new(bytes.as_slice()))
                .guess_file_type()
                .ok()
                .and_then(|p| p.read().ok())
        });
        match opt {
            Some(t) => t,
            None => return meta,
        }
    } else {
        match lofty::read_from_path(path) {
            Ok(t) => t,
            Err(_) => return meta,
        }
    };
    let props = tagged.properties();
    meta.duration = props.duration().as_secs_f64();

    // Musical tags first (present-only), then technical details.
    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        meta.title = tag.title().map(|c| c.to_string()).unwrap_or_default();
        meta.artist = tag.artist().map(|c| c.to_string()).unwrap_or_default();
        meta.album = tag.album().map(|c| c.to_string()).unwrap_or_default();
        meta.genre = tag.genre().map(|c| c.to_string()).unwrap_or_default();

        // "n" or "n/total" for track and disc.
        let counted = |n: Option<u32>, total: Option<u32>| -> String {
            match (n, total) {
                (Some(n), Some(t)) => format!("{n}/{t}"),
                (Some(n), None) => n.to_string(),
                _ => String::new(),
            }
        };
        add_field(
            &mut meta.fields,
            "track",
            counted(tag.track(), tag.track_total()),
        );
        add_field(
            &mut meta.fields,
            "disc",
            counted(tag.disk(), tag.disk_total()),
        );
        let s = |k: ItemKey| tag.get_string(k).unwrap_or("").to_string();
        add_field(&mut meta.fields, "year", s(ItemKey::Year));
        add_field(&mut meta.fields, "album artist", s(ItemKey::AlbumArtist));
        add_field(&mut meta.fields, "bpm", s(ItemKey::IntegerBpm));
        add_field(&mut meta.fields, "key", s(ItemKey::InitialKey));
        add_field(&mut meta.fields, "gain", s(ItemKey::ReplayGainTrackGain));
    }

    add_field(&mut meta.fields, "format", codec_name(tagged.file_type()));
    if let Some(sr) = props.sample_rate() {
        add_field(&mut meta.fields, "sample rate", format!("{sr} Hz"));
    }
    if let Some(bd) = props.bit_depth() {
        add_field(&mut meta.fields, "bit depth", format!("{bd}-bit"));
    }
    if let Some(br) = props.audio_bitrate() {
        add_field(&mut meta.fields, "bitrate", format!("{br} kbps"));
    }
    if let Some(ch) = props.channels() {
        add_field(&mut meta.fields, "channels", ch.to_string());
    }
    meta
}

/// Push `(key, value)` only when `value` is non-empty, so absent tags don't
/// render as blank rows.
fn add_field(fields: &mut Vec<(String, String)>, key: &str, value: String) {
    if !value.is_empty() {
        fields.push((key.to_string(), value));
    }
}

/// Friendly codec label for a lofty `FileType` (e.g. `Mpeg` -> `MP3`).
fn codec_name(ft: lofty::file::FileType) -> String {
    use lofty::file::FileType;
    match ft {
        FileType::Mpeg => "MP3",
        FileType::Flac => "FLAC",
        FileType::Mp4 => "MP4/AAC",
        FileType::Opus => "Opus",
        FileType::Vorbis => "Vorbis",
        FileType::Wav => "WAV",
        FileType::Aiff => "AIFF",
        FileType::Aac => "AAC",
        FileType::Ape => "APE",
        FileType::WavPack => "WavPack",
        FileType::Speex => "Speex",
        other => return format!("{other:?}"),
    }
    .to_string()
}

/// Encoded cover-art bytes (JPEG/PNG/...) for `path`: the first embedded
/// picture if present, otherwise a `cover.jpg`/`folder.png`/... image sitting
/// in the track's directory. The caller decodes them.
pub fn cover_art_bytes(path: &Path) -> Option<Vec<u8>> {
    embedded_cover(path).or_else(|| folder_cover(path))
}

fn embedded_cover(path: &Path) -> Option<Vec<u8>> {
    use lofty::file::TaggedFileExt;

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let pic = tag.pictures().first()?;
    Some(pic.data().to_vec())
}

/// Common sidecar cover-image stems and extensions, checked in the track's dir.
const COVER_STEMS: &[&str] = &["cover", "folder", "front", "album", "albumart", "art"];
const COVER_EXTS: &[&str] = &["jpg", "jpeg", "png"];

fn folder_cover(path: &Path) -> Option<Vec<u8>> {
    let dir = path.parent()?;
    // Fast path: exact lowercase names in priority order.
    for stem in COVER_STEMS {
        for ext in COVER_EXTS {
            let p = dir.join(format!("{stem}.{ext}"));
            if p.is_file() {
                return std::fs::read(p).ok();
            }
        }
    }
    // Fallback: case-insensitive scan (e.g. "Cover.JPG", "Folder.Jpeg"),
    // keeping the highest-priority stem.
    let mut best: Option<(usize, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        let Some(stem) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let stem = stem.to_ascii_lowercase();
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if COVER_EXTS.contains(&ext.as_str())
            && let Some(rank) = COVER_STEMS.iter().position(|s| *s == stem)
            && best.as_ref().is_none_or(|(r, _)| rank < *r)
        {
            best = Some((rank, p));
        }
    }
    best.and_then(|(_, p)| std::fs::read(p).ok())
}

/// An 8-bit RGB triple (kept UI-toolkit-agnostic on purpose).
pub type Rgb8 = (u8, u8, u8);

/// Track lyrics: timestamped when a source provides sync, plain otherwise.
pub enum Lyrics {
    /// (seconds, line) pairs sorted by time.
    Synced(Vec<(f64, String)>),
    Plain(Vec<String>),
}

/// Load lyrics for a track: an `.lrc` sidecar next to the file first, then an
/// embedded lyrics tag. Sidecar/tag text containing `[mm:ss..]` stamps parses
/// as synced; anything else is plain lines. `None` when nothing exists.
pub fn lyrics_for(path: &Path) -> Option<Lyrics> {
    for ext in ["lrc", "LRC"] {
        let side = path.with_extension(ext);
        if let Ok(text) = std::fs::read_to_string(&side) {
            return Some(parse_lyrics(&text));
        }
    }
    let tagged = lofty::read_from_path(path).ok()?;
    use lofty::file::TaggedFileExt;
    use lofty::tag::ItemKey;
    let text = tagged
        .primary_tag()
        .or_else(|| tagged.first_tag())
        .and_then(|t| t.get_string(ItemKey::Lyrics))
        .filter(|s| !s.trim().is_empty())?
        .to_string();
    Some(parse_lyrics(&text))
}

/// Parse lyric text: LRC timestamp lines (`[mm:ss.xx]words`, multiple stamps
/// per line allowed) become synced; metadata tags (`[ar:...]`) are skipped;
/// text without any stamps falls back to plain lines.
pub fn parse_lyrics(text: &str) -> Lyrics {
    let mut synced: Vec<(f64, String)> = Vec::new();
    let mut any_stamp = false;
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps: Vec<f64> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let inner = &rest[1..end];
            rest = rest[end + 1..].trim_start();
            // [mm:ss] / [mm:ss.xx] — anything else ([ar:], [ti:]) is metadata.
            let mut it = inner.splitn(2, ':');
            let (Some(m), Some(s)) = (it.next(), it.next()) else {
                continue;
            };
            if let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) {
                stamps.push(m * 60.0 + s);
                any_stamp = true;
            }
        }
        for t in stamps {
            synced.push((t, rest.to_string()));
        }
    }
    if any_stamp {
        synced.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Lyrics::Synced(synced)
    } else {
        Lyrics::Plain(text.lines().map(|l| l.trim_end().to_string()).collect())
    }
}

/// Two dominant vivid colors of a decoded cover thumbnail, as (primary,
/// secondary) RGB triples. Coarse 4-bit-per-channel histogram over a
/// subsampled pixel grid, weighted by saturation x value so vivid album
/// colors outrank large gray/black backgrounds. The secondary is the
/// strongest bucket sufficiently far from the primary in hue (falling back to
/// the second-strongest overall). `None` when the art is essentially
/// colorless — callers should keep their existing palette.
pub fn dominant_colors(img: &image::RgbImage) -> Option<(Rgb8, Rgb8)> {
    #[derive(Clone, Copy, Default)]
    struct Bucket {
        weight: f32,
        n: u32,
        r: u32,
        g: u32,
        b: u32,
    }
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // ~4k samples regardless of thumbnail size.
    let step = ((w as usize * h as usize) / 4096).max(1);
    let mut buckets = vec![Bucket::default(); 4096];
    for (i, px) in img.pixels().enumerate() {
        if !i.is_multiple_of(step) {
            continue;
        }
        let [r, g, b] = px.0;
        let (maxc, minc) = (r.max(g).max(b) as f32, r.min(g).min(b) as f32);
        let sat = if maxc > 0.0 {
            (maxc - minc) / maxc
        } else {
            0.0
        };
        let val = maxc / 255.0;
        // Vividness weighting; the epsilon keeps near-monochrome art scoring.
        let weight = sat * val + 0.02 * val;
        let key = ((r as usize >> 4) << 8) | ((g as usize >> 4) << 4) | (b as usize >> 4);
        let e = &mut buckets[key];
        e.weight += weight;
        e.n += 1;
        e.r += r as u32;
        e.g += g as u32;
        e.b += b as u32;
    }
    let avg = |b: &Bucket| {
        (
            (b.r / b.n.max(1)) as u8,
            (b.g / b.n.max(1)) as u8,
            (b.b / b.n.max(1)) as u8,
        )
    };
    let hue = |(r, g, b): Rgb8| -> f32 {
        let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let (maxc, minc) = (r.max(g).max(b), r.min(g).min(b));
        let d = maxc - minc;
        if d <= f32::EPSILON {
            return -1.0; // achromatic
        }
        60.0 * if maxc == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if maxc == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        }
    };
    let primary_key = (0..buckets.len()).max_by(|&a, &b| {
        buckets[a]
            .weight
            .partial_cmp(&buckets[b].weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    })?;
    // A cover with no meaningful color energy keeps the theme's own palette.
    if buckets[primary_key].weight < 1.0 {
        return None;
    }
    let primary = avg(&buckets[primary_key]);
    let p_hue = hue(primary);
    // Secondary: strongest bucket at least ~60 degrees of hue away (or a
    // clearly different lightness for achromatic primaries).
    let far_enough = |c: Rgb8| -> bool {
        let h = hue(c);
        if p_hue < 0.0 || h < 0.0 {
            let lp = primary.0 as i32 + primary.1 as i32 + primary.2 as i32;
            let lc = c.0 as i32 + c.1 as i32 + c.2 as i32;
            return (lp - lc).abs() > 180;
        }
        let d = (h - p_hue).abs();
        d.min(360.0 - d) > 60.0
    };
    let secondary_key = (0..buckets.len())
        .filter(|&k| k != primary_key && buckets[k].n > 0 && far_enough(avg(&buckets[k])))
        .max_by(|&a, &b| {
            buckets[a]
                .weight
                .partial_cmp(&buckets[b].weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .or_else(|| {
            (0..buckets.len())
                .filter(|&k| k != primary_key && buckets[k].n > 0)
                .max_by(|&a, &b| {
                    buckets[a]
                        .weight
                        .partial_cmp(&buckets[b].weight)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    // Single-color art (one occupied bucket): the primary doubles as the
    // secondary rather than failing the whole extraction.
    let secondary = secondary_key.map_or(primary, |k| avg(&buckets[k]));
    Some((primary, secondary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_parses_synced_and_skips_metadata() {
        let text = "[ar:Someone]\n[00:01.50]first line\n[00:03][00:05.2]repeated\n";
        match parse_lyrics(text) {
            Lyrics::Synced(v) => {
                assert_eq!(v.len(), 3);
                assert!((v[0].0 - 1.5).abs() < 1e-9);
                assert_eq!(v[0].1, "first line");
                assert_eq!(v[1].1, "repeated");
                assert!((v[2].0 - 5.2).abs() < 1e-9);
            }
            _ => panic!("expected synced"),
        }
    }

    #[test]
    fn plain_text_lyrics_fall_back() {
        match parse_lyrics("just words\nno stamps") {
            Lyrics::Plain(v) => assert_eq!(v.len(), 2),
            _ => panic!("expected plain"),
        }
    }

    #[test]
    fn dominant_colors_single_hue_cover() {
        let img = image::RgbImage::from_pixel(64, 64, image::Rgb([200, 20, 20]));
        let (p, s) = dominant_colors(&img).expect("solid color should extract");
        assert!(p.0 > 150 && p.1 < 60 && p.2 < 60, "primary={p:?}");
        assert_eq!(p, s, "single-hue art doubles primary as secondary");
    }

    #[test]
    fn dominant_colors_two_hues() {
        let mut img = image::RgbImage::from_pixel(64, 64, image::Rgb([220, 30, 30]));
        for y in 0..64 {
            for x in 0..20 {
                img.put_pixel(x, y, image::Rgb([20, 80, 230]));
            }
        }
        let (p, s) = dominant_colors(&img).expect("should extract");
        // Red dominates (more area); blue is the far-hue secondary.
        assert!(p.0 > p.2, "primary should be the red family: {p:?}");
        assert!(s.2 > s.0, "secondary should be the blue family: {s:?}");
    }

    #[test]
    fn dominant_colors_black_cover_is_none() {
        let img = image::RgbImage::from_pixel(64, 64, image::Rgb([0, 0, 0]));
        assert!(dominant_colors(&img).is_none());
    }

    #[test]
    fn folder_cover_found_by_name_priority() {
        let dir = std::env::temp_dir().join("muse_cover_test_a");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Lower-priority "art.png" plus higher-priority "cover.jpg".
        std::fs::write(dir.join("art.png"), b"PNGART").unwrap();
        std::fs::write(dir.join("cover.jpg"), b"JPEGCOVER").unwrap();
        let track = dir.join("song.flac");
        std::fs::write(&track, b"not a real flac").unwrap();
        // No embedded art in the junk file -> folder fallback, cover.jpg wins.
        assert_eq!(cover_art_bytes(&track).as_deref(), Some(&b"JPEGCOVER"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_cover_case_insensitive() {
        let dir = std::env::temp_dir().join("muse_cover_test_b");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Folder.PNG"), b"CAPS").unwrap();
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"junk").unwrap();
        assert_eq!(cover_art_bytes(&track).as_deref(), Some(&b"CAPS"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_cover_returns_none() {
        let dir = std::env::temp_dir().join("muse_cover_test_c");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"junk").unwrap();
        assert!(cover_art_bytes(&track).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
