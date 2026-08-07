//! Bar rendering, pace-marker placement, and color thresholds.

pub const BAR_WIDTH: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Green,
    Yellow,
    Red,
    Gray,
}

impl Color {
    pub fn ansi(&self) -> &'static str {
        match self {
            Color::Green => "\x1b[32m",
            Color::Yellow => "\x1b[33m",
            Color::Red => "\x1b[31m",
            Color::Gray => "\x1b[90m",
        }
    }
}

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";

pub fn clamp(n: f64, lo: f64, hi: f64) -> f64 {
    n.max(lo).min(hi)
}

/// Renders a `width`-character bar. `used_pct` fills from the left (rounded to
/// the nearest cell); `pace_idx`, if given, overlays a `|` at that cell —
/// clamped into range — regardless of fill state, so it's always visible.
pub fn build_bar(
    width: usize,
    used_pct: Option<f64>,
    pace_idx: Option<usize>,
    nerd: bool,
) -> String {
    let fill_char = if nerd { '█' } else { '#' };
    let empty_char = if nerd { '░' } else { '-' };
    let filled = match used_pct {
        Some(p) => (clamp(p, 0.0, 100.0) / 100.0 * width as f64).round() as usize,
        None => 0,
    };
    let mut chars: Vec<char> = (0..width)
        .map(|i| if i < filled { fill_char } else { empty_char })
        .collect();
    if let Some(idx) = pace_idx {
        let idx = idx.min(width.saturating_sub(1));
        chars[idx] = '|';
    }
    chars.into_iter().collect()
}

/// Pace-relative color for the 5h/7d rate-limit bars: green if usage is at or
/// under the pace the clock has set (`used% <= elapsed%`), yellow up to 15
/// points ahead of pace, red beyond that. Gray ("n/a") when either input is
/// missing.
pub fn color_for_pace(used_pct: Option<f64>, elapsed_pct: Option<f64>) -> Color {
    match (used_pct, elapsed_pct) {
        (Some(u), Some(e)) => {
            let diff = u - e;
            if diff <= 0.0 {
                Color::Green
            } else if diff <= 15.0 {
                Color::Yellow
            } else {
                Color::Red
            }
        }
        _ => Color::Gray,
    }
}

/// Flat threshold color for the context-window bar (no pace concept): green
/// under 50%, yellow 50-80%, red above 80%.
pub fn color_for_context(pct: Option<f64>) -> Color {
    match pct {
        None => Color::Gray,
        Some(p) if p < 50.0 => Color::Green,
        Some(p) if p <= 80.0 => Color::Yellow,
        _ => Color::Red,
    }
}

pub fn fmt_pct(pct: Option<f64>) -> String {
    match pct {
        None => "n/a".to_string(),
        Some(p) => format!("{}%", p.round() as i64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_and_places_marker() {
        // used=60%, pace at index 5 -> 5 filled, marker at 5, then dashes.
        assert_eq!(build_bar(10, Some(60.0), Some(5), false), "#####|----");
        // used=20%, pace at index 5 -> 2 filled, gap, marker, gap.
        assert_eq!(build_bar(10, Some(20.0), Some(5), false), "##---|----");
    }

    #[test]
    fn bar_with_no_data_is_all_empty() {
        assert_eq!(build_bar(10, None, None, false), "----------");
    }

    #[test]
    fn pace_index_clamped_into_range() {
        assert_eq!(build_bar(10, Some(0.0), Some(999), false), "---------|");
    }

    #[test]
    fn pace_colors() {
        assert_eq!(color_for_pace(Some(60.0), Some(50.0)), Color::Yellow); // 10 ahead
        assert_eq!(color_for_pace(Some(20.0), Some(50.0)), Color::Green); // behind pace
        assert_eq!(color_for_pace(Some(95.0), Some(11.0)), Color::Red); // way ahead
        assert_eq!(color_for_pace(None, Some(50.0)), Color::Gray);
    }

    #[test]
    fn context_colors() {
        assert_eq!(color_for_context(Some(30.0)), Color::Green);
        assert_eq!(color_for_context(Some(50.0)), Color::Yellow);
        assert_eq!(color_for_context(Some(80.0)), Color::Yellow);
        assert_eq!(color_for_context(Some(81.0)), Color::Red);
        assert_eq!(color_for_context(None), Color::Gray);
    }

    #[test]
    fn fmt_pct_rounds_and_handles_none() {
        assert_eq!(fmt_pct(Some(41.6)), "42%");
        assert_eq!(fmt_pct(None), "n/a");
    }
}
