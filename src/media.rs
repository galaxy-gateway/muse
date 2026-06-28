//! Media-type seam. The core never names "audio" directly — it asks the registry
//! for the provider that `supports(path)`. Audio is the only provider today;
//! image/video providers can be added later without touching the UI or app core.

use std::path::Path;

/// Ordered, typed-ish metadata so unrelated media can expose different fields
/// while the inspector / future diff view stay generic.
#[derive(Clone, Default)]
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
    let tagged = match lofty::read_from_path(path) {
        Ok(t) => t,
        Err(_) => return meta,
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
