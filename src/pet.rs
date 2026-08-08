//! The mood pet: a small deterministic state machine over context-window
//! usage, 5h pace, rate-limit resets, and recent-activity, rendered as a
//! cycling frame set so it visibly moves while Claude is actively working.
//!
//! Kaomoji and box-drawing characters only — no emoji. Emoji render at
//! different widths across terminals (single vs. double cell), which would
//! make the line's width jitter between refreshes.

use crate::git::{hash_path, sanitize_key};
use crate::theme::Role;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No context/rate-limit data yet — start of session.
    Fresh,
    Calm,
    /// Calm-range usage, but renders are arriving in quick succession —
    /// Claude is actively working right now.
    Working,
    Worried,
    Stressed,
    Overheated,
    /// A rate-limit window reset very recently.
    Celebrating,
}

/// Everything the state machine needs, pre-derived by the caller so this
/// module doesn't have to know about rate-limit window lengths or JSON
/// shapes — just the numbers.
pub struct PetInputs<'a> {
    pub ctx_pct: Option<f64>,
    pub five_h_used: Option<f64>,
    pub five_h_elapsed: Option<f64>,
    /// True when a rate-limit window's `resets_at` fell within the last
    /// couple of minutes.
    pub just_reset: bool,
    pub exceeds_200k: bool,
    pub now_sec: f64,
    pub session_id: Option<&'a str>,
    pub cwd: Option<&'a Path>,
}

/// "Worst" reflects the worse of context-window usage and how far the 5h bar
/// is running ahead of its own pace marker (never how far *behind*, since
/// being behind pace isn't something to be stressed about).
fn worst_pct(inputs: &PetInputs) -> f64 {
    let five_h_ahead = match (inputs.five_h_used, inputs.five_h_elapsed) {
        (Some(u), Some(e)) => (u - e).max(0.0),
        _ => 0.0,
    };
    inputs.ctx_pct.unwrap_or(0.0).max(five_h_ahead)
}

pub fn compute_state(inputs: &PetInputs) -> State {
    if inputs.just_reset {
        return State::Celebrating;
    }

    let worst = worst_pct(inputs);
    if inputs.exceeds_200k || worst >= 70.0 {
        return State::Overheated;
    }
    if worst >= 40.0 {
        return State::Stressed;
    }
    if inputs.ctx_pct.is_none() && inputs.five_h_used.is_none() {
        return State::Fresh;
    }
    if worst >= 15.0 {
        return State::Worried;
    }
    if is_active(inputs.session_id, inputs.cwd) {
        return State::Working;
    }
    State::Calm
}

pub fn role_for_state(state: State) -> Role {
    match state {
        State::Fresh | State::Calm | State::Working | State::Celebrating => Role::Ok,
        State::Worried => Role::Warn,
        State::Stressed | State::Overheated => Role::Hot,
    }
}

const UNICODE_FRAMES: &[(State, &[&str])] = &[
    (State::Fresh, &["(o_o)?"]),
    (
        State::Calm,
        &[
            "(\u{b7}\u{1d17}\u{b7})",
            "(-\u{1d17}-)",
            "(\u{b7}\u{1d17}\u{b7})",
            "(\u{b7}\u{1d17}\u{b7})",
        ],
    ),
    (
        State::Working,
        &[
            "(\u{b7}\u{1d17}\u{b7})\u{ff89}",
            "\u{ff89}(\u{b7}\u{1d17}\u{b7})",
        ],
    ),
    (
        State::Worried,
        &["(\u{b7}\u{fe4f}\u{b7})", "(\u{b7}\u{fe4f}\u{b7});"],
    ),
    (State::Stressed, &["(>_<)", "(>_<)'"]),
    (State::Overheated, &["(x_x)", "(x_x)~"]),
    (
        State::Celebrating,
        &["\u{2727}(\u{2022}\u{1d17}\u{2022})\u{2727}"],
    ),
];

const ASCII_FRAMES: &[(State, &[&str])] = &[
    (State::Fresh, &[":o"]),
    (State::Calm, &[":)", ":)"]),
    (State::Working, &[":)", ":]"]),
    (State::Worried, &[":/", ":/"]),
    (State::Stressed, &[">:(", ">:|"]),
    (State::Overheated, &["X_X", "x_x"]),
    (State::Celebrating, &["\\o/"]),
];

fn frames_for(state: State, nerd: bool) -> &'static [&'static str] {
    let table = if nerd { UNICODE_FRAMES } else { ASCII_FRAMES };
    table
        .iter()
        .find(|(s, _)| *s == state)
        .map(|(_, frames)| *frames)
        .unwrap_or(&[":)"])
}

/// Picks the current animation frame. Claude Code redraws the status line
/// roughly every 300ms while a session is active, so a ~400ms frame period
/// advances visibly during real usage without flickering faster than the
/// refresh rate can show.
fn frame(state: State, nerd: bool, animate: bool, now_sec: f64) -> &'static str {
    let frames = frames_for(state, nerd);
    if !animate || frames.len() <= 1 {
        return frames[0];
    }
    let idx = ((now_sec / 0.4) as u64 as usize) % frames.len();
    frames[idx]
}

/// Renders the pet for the given inputs: `(text, role)`, where `role`
/// selects the theme color the caller should paint it with.
pub fn render(inputs: &PetInputs, nerd: bool, animate: bool) -> (String, Role) {
    let state = compute_state(inputs);
    (
        frame(state, nerd, animate, inputs.now_sec).to_string(),
        role_for_state(state),
    )
}

const ACTIVITY_TTL: Duration = Duration::from_secs(2);

/// True if a render for this session+cwd happened within `ACTIVITY_TTL`,
/// meaning Claude Code is actively redrawing the status line right now
/// (idle sessions redraw far less often). Mirrors the temp-file mtime
/// caching idiom `git.rs` already uses for the dirty-flag cache, sharing its
/// `hash_path`/`sanitize_key` helpers so the two caches use the same keying
/// scheme without duplicating it.
fn is_active(session_id: Option<&str>, cwd: Option<&Path>) -> bool {
    let path = activity_cache_path(session_id, cwd);
    let was_recent = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .map(
            |modified| match SystemTime::now().duration_since(modified) {
                Ok(age) => age < ACTIVITY_TTL,
                // `modified` reads as later than "now": either a filesystem/clock
                // granularity mismatch right after a fresh write, or a genuinely
                // future timestamp. Either way this is not a stale file.
                Err(_) => true,
            },
        )
        .unwrap_or(false);
    let _ = std::fs::write(&path, b"");
    was_recent
}

fn activity_cache_path(session_id: Option<&str>, cwd: Option<&Path>) -> PathBuf {
    let key = sanitize_key(session_id.unwrap_or("no-session"));
    let cwd_hash = cwd.map(hash_path).unwrap_or(0);
    std::env::temp_dir().join(format!("zeek-meter-statusline-activity-{key}-{cwd_hash:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_inputs<'a>(session_id: &'a str) -> PetInputs<'a> {
        PetInputs {
            ctx_pct: None,
            five_h_used: None,
            five_h_elapsed: None,
            just_reset: false,
            exceeds_200k: false,
            now_sec: 1_000_000.0,
            session_id: Some(session_id),
            cwd: None,
        }
    }

    #[test]
    fn fresh_when_no_data() {
        let inputs = base_inputs("pet-test-fresh");
        assert_eq!(compute_state(&inputs), State::Fresh);
    }

    #[test]
    fn calm_below_15() {
        let mut inputs = base_inputs("pet-test-calm");
        inputs.ctx_pct = Some(5.0);
        assert_eq!(compute_state(&inputs), State::Calm);
    }

    #[test]
    fn worried_between_15_and_40() {
        let mut inputs = base_inputs("pet-test-worried");
        inputs.ctx_pct = Some(20.0);
        assert_eq!(compute_state(&inputs), State::Worried);
    }

    #[test]
    fn stressed_between_40_and_70() {
        let mut inputs = base_inputs("pet-test-stressed");
        inputs.ctx_pct = Some(50.0);
        assert_eq!(compute_state(&inputs), State::Stressed);
    }

    #[test]
    fn overheated_at_or_above_70() {
        let mut inputs = base_inputs("pet-test-overheated");
        inputs.ctx_pct = Some(85.0);
        assert_eq!(compute_state(&inputs), State::Overheated);
    }

    #[test]
    fn overheated_when_exceeds_200k_regardless_of_pct() {
        let mut inputs = base_inputs("pet-test-exceeds");
        inputs.ctx_pct = Some(5.0);
        inputs.exceeds_200k = true;
        assert_eq!(compute_state(&inputs), State::Overheated);
    }

    #[test]
    fn celebrating_wins_over_everything() {
        let mut inputs = base_inputs("pet-test-celebrating");
        inputs.ctx_pct = Some(90.0);
        inputs.just_reset = true;
        assert_eq!(compute_state(&inputs), State::Celebrating);
    }

    #[test]
    fn five_h_ahead_of_pace_drives_worst() {
        let mut inputs = base_inputs("pet-test-pace");
        inputs.five_h_used = Some(60.0);
        inputs.five_h_elapsed = Some(10.0); // 50 points ahead of pace -> Stressed band (40-70)
        assert_eq!(compute_state(&inputs), State::Stressed);
    }

    #[test]
    fn behind_pace_never_counts_against_worst() {
        let mut inputs = base_inputs("pet-test-behind");
        inputs.five_h_used = Some(10.0);
        inputs.five_h_elapsed = Some(60.0); // way behind pace, not stressful
                                            // worst = 0, and five_h_used being present means this isn't "Fresh"
                                            // (no data at all) — first call for this session isn't active yet.
        assert_eq!(compute_state(&inputs), State::Calm);
    }

    // `is_active`'s TTL-based caching depends on real filesystem mtime
    // precision, which varies enough across filesystems/sandboxes to make a
    // "second call reports active" assertion flaky — the same reason
    // `git.rs`'s analogous dirty-flag TTL cache isn't asserted against real
    // timing either. `compute_state`'s deterministic branches (Fresh, Calm,
    // Worried, Stressed, Overheated, Celebrating) are covered above; the one
    // property worth pinning down here is that a *fresh* cache key (never
    // touched before) always reads as inactive.
    #[test]
    fn first_call_for_a_fresh_key_is_never_active() {
        let mut inputs = base_inputs("pet-test-first-call-unique");
        inputs.ctx_pct = Some(1.0);
        assert_eq!(compute_state(&inputs), State::Calm);
    }

    #[test]
    fn ascii_fallback_frames_are_ascii_only() {
        for (_, frames) in ASCII_FRAMES {
            for f in *frames {
                assert!(f.is_ascii(), "expected ASCII frame, got {f:?}");
            }
        }
    }

    #[test]
    fn animation_cycles_through_frames() {
        let frames = frames_for(State::Calm, true);
        assert!(frames.len() > 1);
        let f0 = frame(State::Calm, true, true, 0.0);
        let f1 = frame(State::Calm, true, true, 0.4);
        assert_ne!(f0, f1);
    }

    #[test]
    fn animate_off_always_shows_first_frame() {
        let f0 = frame(State::Calm, true, false, 0.0);
        let f1 = frame(State::Calm, true, false, 999.0);
        assert_eq!(f0, f1);
    }

    #[test]
    fn role_mapping_matches_severity() {
        assert_eq!(role_for_state(State::Calm), Role::Ok);
        assert_eq!(role_for_state(State::Worried), Role::Warn);
        assert_eq!(role_for_state(State::Overheated), Role::Hot);
    }
}
