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
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

fn read_audio_meta(path: &Path) -> Meta {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;

    let mut meta = Meta::default();
    let tagged = match lofty::read_from_path(path) {
        Ok(t) => t,
        Err(_) => return meta,
    };
    let props = tagged.properties();
    meta.duration = props.duration().as_secs_f64();
    if let Some(sr) = props.sample_rate() {
        meta.fields.push(("sample rate".into(), format!("{sr} Hz")));
    }
    if let Some(br) = props.audio_bitrate() {
        meta.fields.push(("bitrate".into(), format!("{br} kbps")));
    }
    if let Some(ch) = props.channels() {
        meta.fields.push(("channels".into(), ch.to_string()));
    }
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());
    if let Some(tag) = tag {
        meta.title = tag.title().map(|c| c.to_string()).unwrap_or_default();
        meta.artist = tag.artist().map(|c| c.to_string()).unwrap_or_default();
        meta.album = tag.album().map(|c| c.to_string()).unwrap_or_default();
        meta.genre = tag.genre().map(|c| c.to_string()).unwrap_or_default();
    }
    meta
}
