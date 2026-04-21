//! Duration display helpers shared by the Add/Edit routine item forms.
//!
//! Durations are stored as a whole number of minutes (`estimated_duration_min`
//! in the event payload). The UI lets the user pick a value + unit (min/hour),
//! so we need two conversions:
//!   - entry-side: combine (value, unit) → minutes, deterministic
//!   - display-side: split a stored minute count back into (value, unit) for
//!     re-editing or friendly display, which has a real UX choice baked in

pub const UNIT_MIN: &str = "min";
pub const UNIT_HOUR: &str = "hour";

/// Combine a user-entered value + unit into total minutes.
///
/// Deterministic: `hour` multiplies by 60, anything else is treated as minutes.
/// Overflow-safe via `saturating_mul`.
pub fn to_minutes(value: u32, unit: &str) -> u32 {
    match unit {
        UNIT_HOUR => value.saturating_mul(60),
        _ => value,
    }
}

/// Split stored `total_minutes` into a (value, unit) pair for display.
///
/// Design choice: when should we present minutes as "2 hours" vs "120 min"?
/// The caller uses this to pre-fill the number input + unit dropdown when
/// editing an existing item, so the split determines what unit the user sees
/// first and whether tiny edits round-trip cleanly.
pub fn split_minutes_for_display(total_minutes: u32) -> (u32, &'static str) {
    match total_minutes {
        0 => (0, UNIT_MIN),
        tm if tm % 60 == 0 => (tm / 60, UNIT_HOUR),
        tm => (tm, UNIT_MIN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_minutes_hour_multiplies_by_sixty() {
        assert_eq!(to_minutes(1, UNIT_HOUR), 60);
        assert_eq!(to_minutes(2, UNIT_HOUR), 120);
    }

    #[test]
    fn to_minutes_minutes_passthrough() {
        assert_eq!(to_minutes(5, UNIT_MIN), 5);
        assert_eq!(to_minutes(45, UNIT_MIN), 45);
    }

    #[test]
    fn to_minutes_saturates_instead_of_overflowing() {
        // Guard: a hostile numeric paste shouldn't wrap silently.
        assert_eq!(to_minutes(u32::MAX, UNIT_HOUR), u32::MAX);
    }

    #[test]
    fn stored_legacy_minutes_over_sixty_split_to_hours() {
        assert_eq!(split_minutes_for_display(120), (2, UNIT_HOUR));
        assert_eq!(split_minutes_for_display(60), (1, UNIT_HOUR));
        assert_eq!(split_minutes_for_display(90), (90, UNIT_MIN));
    }

    #[test]
    fn zero_roundtrips() {
        assert_eq!(
            (0, UNIT_MIN),
            split_minutes_for_display(to_minutes(0, UNIT_MIN))
        );
    }

    #[test]
    fn whole_hours_roundtrip() {
        assert_eq!(
            (3, UNIT_HOUR),
            split_minutes_for_display(to_minutes(3, UNIT_HOUR))
        );
    }

    #[test]
    fn minutes_less_than_sixty_roundtrip() {
        assert_eq!(
            (37, UNIT_MIN),
            split_minutes_for_display(to_minutes(37, UNIT_MIN))
        );
    }

    #[test]
    fn minutes_more_than_sixty_roundtrip() {
        assert_eq!(
            (69, UNIT_MIN),
            split_minutes_for_display(to_minutes(69, UNIT_MIN))
        );
    }
}
