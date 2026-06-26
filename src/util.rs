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

/// `M:SS.ffffffffffff` — seconds carried to 12 fractional digits (picoseconds),
/// for the live now-playing readout. The low digits only update at the audio
/// frame rate, but the fractional part visibly ticks every redraw.
pub fn fmt_time_precise(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "0:00.000000000000".into();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    let frac = secs - total as f64; // 0.0..1.0
    let pico = (frac * 1e12).round() as u64; // picoseconds within the second
    format!("{m}:{s:02}.{pico:012}")
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
