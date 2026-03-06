//! Time-based speed profile scheduler.
//!
//! Runs a background task that checks the active speed profile every 30 seconds
//! and adjusts the `GLOBAL_LIMITER` accordingly.  When no profile matches the
//! current time, the base `speed_limit_kbps` from settings is used.

use chrono::{Timelike, Datelike};
use crate::settings::{self, SpeedProfile};
use crate::speed_limiter::GLOBAL_LIMITER;

/// Start the speed-profile scheduler.  Should be called once during app setup.
/// The task runs forever in a tokio spawn and re-reads settings on every tick
/// (so profile changes are picked up immediately, no restart needed).
pub fn start_speed_profile_scheduler() {
    tokio::spawn(async move {
        loop {
            apply_current_profile();
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

/// Evaluate the current time against configured profiles and set the limiter.
fn apply_current_profile() {
    let settings = settings::load_settings();

    if !settings.speed_profiles_enabled || settings.speed_profiles.is_empty() {
        // No profiles — use the base limit
        GLOBAL_LIMITER.set_limit(settings.speed_limit_kbps * 1024);
        return;
    }

    let now = chrono::Local::now();
    let current_minutes = now.hour() as u16 * 60 + now.minute() as u16;
    // chrono weekday: Mon=0 .. Sun=6
    let current_day = now.weekday().num_days_from_monday() as u8;

    // Find the first matching profile (priority = order in the list)
    for profile in &settings.speed_profiles {
        if profile_matches(profile, current_minutes, current_day) {
            GLOBAL_LIMITER.set_limit(profile.speed_limit_kbps * 1024);
            return;
        }
    }

    // No profile matched — use base limit
    GLOBAL_LIMITER.set_limit(settings.speed_limit_kbps * 1024);
}

/// Check whether a profile is active at the given time.
fn profile_matches(profile: &SpeedProfile, current_minutes: u16, current_day: u8) -> bool {
    // Day filter
    if !profile.days.is_empty() && !profile.days.contains(&current_day) {
        return false;
    }

    // Parse HH:MM → minutes-since-midnight
    let start = parse_hhmm(&profile.start_time);
    let end = parse_hhmm(&profile.end_time);

    match (start, end) {
        (Some(s), Some(e)) => {
            if s <= e {
                // Normal range: e.g. 09:00–17:00
                current_minutes >= s && current_minutes < e
            } else {
                // Overnight range: e.g. 22:00–06:00
                current_minutes >= s || current_minutes < e
            }
        }
        _ => false, // Malformed times — skip
    }
}

fn parse_hhmm(s: &str) -> Option<u16> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 2 {
        let h: u16 = parts[0].parse().ok()?;
        let m: u16 = parts[1].parse().ok()?;
        if h < 24 && m < 60 {
            return Some(h * 60 + m);
        }
    }
    None
}

/// Public wrapper for profile matching, used by lib.rs commands.
pub fn profile_matches_pub(profile: &SpeedProfile, current_minutes: u16, current_day: u8) -> bool {
    profile_matches(profile, current_minutes, current_day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hhmm() {
        assert_eq!(parse_hhmm("09:00"), Some(540));
        assert_eq!(parse_hhmm("17:30"), Some(1050));
        assert_eq!(parse_hhmm("00:00"), Some(0));
        assert_eq!(parse_hhmm("23:59"), Some(1439));
        assert_eq!(parse_hhmm("24:00"), None);
        assert_eq!(parse_hhmm("abc"), None);
    }

    #[test]
    fn test_profile_matches_normal_range() {
        let profile = SpeedProfile {
            name: "Work".to_string(),
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            speed_limit_kbps: 500,
            days: vec![],
        };
        assert!(profile_matches(&profile, 540, 0));   // 09:00 Mon
        assert!(profile_matches(&profile, 720, 3));   // 12:00 Thu
        assert!(!profile_matches(&profile, 1020, 0)); // 17:00 Mon (end exclusive)
        assert!(!profile_matches(&profile, 480, 0));  // 08:00 Mon
    }

    #[test]
    fn test_profile_matches_overnight() {
        let profile = SpeedProfile {
            name: "Night".to_string(),
            start_time: "22:00".to_string(),
            end_time: "06:00".to_string(),
            speed_limit_kbps: 0,
            days: vec![],
        };
        assert!(profile_matches(&profile, 1380, 0));  // 23:00
        assert!(profile_matches(&profile, 0, 0));     // 00:00
        assert!(profile_matches(&profile, 300, 0));   // 05:00
        assert!(!profile_matches(&profile, 360, 0));  // 06:00
        assert!(!profile_matches(&profile, 720, 0));  // 12:00
    }

    #[test]
    fn test_profile_day_filter() {
        let profile = SpeedProfile {
            name: "Weekday".to_string(),
            start_time: "09:00".to_string(),
            end_time: "17:00".to_string(),
            speed_limit_kbps: 200,
            days: vec![0, 1, 2, 3, 4], // Mon-Fri
        };
        assert!(profile_matches(&profile, 600, 0));   // Mon 10:00
        assert!(!profile_matches(&profile, 600, 5));  // Sat 10:00
        assert!(!profile_matches(&profile, 600, 6));  // Sun 10:00
    }
}
