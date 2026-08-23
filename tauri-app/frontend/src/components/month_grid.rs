//! Shared month-grid cell builder for the calendar UIs — the journal
//! `CalendarDrawer` and the [`crate::components::date_field::DateField`] popover
//! both render the same 6×7 grid, so the layout logic lives here once.

use chrono::{Datelike, NaiveDate};

/// One cell in a month grid. `in_current_month` is false for the spillover days
/// that pad the first/last weeks, so the renderer can grey them out.
#[derive(Clone, Debug)]
pub struct MonthCell {
    pub date: NaiveDate,
    pub in_current_month: bool,
}

/// Build the list of calendar cells for a month view.
///
/// The grid is always 6 rows × 7 cols (42 cells) so its height stays constant
/// as the user navigates months. Six rows are needed to cover every layout —
/// a 5-row grid truncates the last 1-2 days of long months that start late in
/// the week (e.g., a 31-day month starting Sunday). Week starts on Monday
/// (matches Obsidian's daily-note plugin default).
///
/// Cells outside the anchor's month carry `in_current_month: false` so the
/// renderer can grey them out (spillover style) instead of showing blanks.
///
/// `anchor` is expected to be the first day of the target month.
pub fn build_month_cells(anchor: NaiveDate) -> Vec<MonthCell> {
    let anchor_month = anchor.month();
    let start_date = anchor - chrono::Days::new(anchor.weekday().num_days_from_monday() as u64);
    std::iter::successors(Some(start_date), |d| d.succ_opt())
        .take(42)
        .map(|date| MonthCell {
            date,
            in_current_month: date.month() == anchor_month,
        })
        .collect()
}

/// Step `anchor` (first-of-month) to the first day of the previous month.
pub fn prev_month(anchor: NaiveDate) -> NaiveDate {
    let (y, m) = if anchor.month() == 1 {
        (anchor.year() - 1, 12)
    } else {
        (anchor.year(), anchor.month() - 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

/// Step `anchor` (first-of-month) to the first day of the next month.
pub fn next_month(anchor: NaiveDate) -> NaiveDate {
    let (y, m) = if anchor.month() == 12 {
        (anchor.year() + 1, 1)
    } else {
        (anchor.year(), anchor.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks in the grid contract: 42 cells (6 rows × 7 cols), Monday-first.
    #[test]
    fn build_month_cells_returns_42_monday_first_cells() {
        // February 2026: Feb 1 falls on a Sunday.
        let anchor = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
        let cells = build_month_cells(anchor);
        assert_eq!(cells.len(), 42, "always 6 full weeks");

        // First row should start on a Monday (Jan 26, 2026 is a Monday).
        assert_eq!(cells[0].date, NaiveDate::from_ymd_opt(2026, 1, 26).unwrap());
        assert!(!cells[0].in_current_month);

        // Feb 1 (Sunday) should be cell index 6.
        assert_eq!(cells[6].date, anchor);
        assert!(cells[6].in_current_month);
    }

    #[test]
    fn month_stepping_wraps_years() {
        let jan = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert_eq!(prev_month(jan), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
        let dec = NaiveDate::from_ymd_opt(2026, 12, 1).unwrap();
        assert_eq!(next_month(dec), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }
}
