//! Small formatting helpers.

pub fn fmt_time(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00".into();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}")
}

/// `M:SS.mmm` — seconds with millisecond precision, for the live now-playing
/// readout. The fractional part ticks each redraw as playback advances.
pub fn fmt_time_precise(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00.000".into();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    let ms = (((secs - total as f64) * 1000.0).round() as u64).min(999); // 0..=999
    format!("{m}:{s:02}.{ms:03}")
}

/// Case-insensitive subsequence fuzzy match. `Some(score)` if every char of
/// `needle` appears in order within `haystack`; lower score = tighter/earlier
/// match (contiguous, near the front). `None` if it doesn't match.
pub fn fuzzy_score(needle: &str, haystack: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().collect();
    let mut hi = 0usize;
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    for nc in needle.chars() {
        let mut found = None;
        while hi < hay.len() {
            if hay[hi].eq_ignore_ascii_case(&nc) {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let idx = found?;
        score += match last {
            Some(lm) => (idx - lm) as i32, // gap since previous match (1 = contiguous)
            None => idx as i32,            // distance from the front
        };
        last = Some(idx);
        hi = idx + 1;
    }
    Some(score)
}

/// Cheap deterministic hash (two seeds -> pseudo-random u32). Used for flicker
/// and particle jitter without an RNG dependency.
pub fn noise(a: u32, b: u32) -> u32 {
    let mut x = a
        .wrapping_mul(374_761_393)
        .wrapping_add(b.wrapping_mul(668_265_263));
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    x ^ (x >> 16)
}

/// Human-readable byte size, e.g. 4.2 MB, 512 KB, 900 B.
pub fn fmt_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}

/// Ask libmalloc to return freed-but-cached dirty pages to the OS. This is what
/// drains the hundreds of MB of MALLOC_LARGE (empty) regions left after rapid
/// track cycling (see docs/rapid-cycle-memory.md) in ~ms instead of ~35s.
/// Costs a few ms (walks all malloc zones) — call from a background thread,
/// never the UI, decode, or audio-callback thread.
/// Returns the number of bytes libmalloc reports it released.
#[cfg(target_os = "macos")]
pub fn malloc_pressure_relief() -> usize {
    unsafe extern "C" {
        // malloc/malloc.h: size_t malloc_zone_pressure_relief(malloc_zone_t*, size_t);
        // NULL zone = all zones, goal 0 = release everything possible.
        fn malloc_zone_pressure_relief(zone: *mut std::ffi::c_void, goal: usize) -> usize;
    }
    unsafe { malloc_zone_pressure_relief(std::ptr::null_mut(), 0) }
}
