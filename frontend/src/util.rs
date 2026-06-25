//! Small presentation helpers shared across pages (relative time, snippets).

use js_sys::Date;
use wasm_bindgen::JsValue;

/// Human relative time from a Unix-seconds timestamp: "just now", "5m", "3h",
/// "2d", then a short "Mon D" date for anything older than a week.
pub fn rel_time(unix_secs: i64) -> String {
    let now = (Date::now() / 1000.0) as i64;
    let diff = now - unix_secs;
    if diff < 60 {
        "just now".into()
    } else if diff < 3_600 {
        format!("{}m", diff / 60)
    } else if diff < 86_400 {
        format!("{}h", diff / 3_600)
    } else if diff < 7 * 86_400 {
        format!("{}d", diff / 86_400)
    } else {
        let d = Date::new(&JsValue::from_f64((unix_secs as f64) * 1000.0));
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let m = (d.get_month() as usize).min(11);
        format!("{} {}", MONTHS[m], d.get_date() as u32)
    }
}

/// Collapse whitespace and truncate to `max` chars (adds an ellipsis if cut).
pub fn snippet(s: &str, max: usize) -> String {
    let one_line: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let t: String = one_line.chars().take(max).collect();
        format!("{}\u{2026}", t.trim_end())
    }
}
