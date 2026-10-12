//! A transaction dated after today never enters the ledger from review (R4-15,
//! his ruling 2026-10-12). A misread date is fixed in place; a real upcoming
//! payment waits until it has happened. One rule for every path that commits.

use chrono::NaiveDate;

/// Why `date` cannot be committed on `today`, or `None` when it can.
pub fn refusal(date: NaiveDate, today: NaiveDate) -> Option<String> {
    (date > today).then(|| {
        let day = date.format("%b %-d");
        let year = if chrono::Datelike::year(&date) == chrono::Datelike::year(&today) {
            String::new()
        } else {
            format!(", {}", date.format("%Y"))
        };
        format!("Dated {day}{year}. Fix the date, or commit it from {day}{year}.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn today_and_earlier_commit() {
        assert_eq!(refusal(d("2026-10-12"), d("2026-10-12")), None);
        assert_eq!(refusal(d("2026-01-01"), d("2026-10-12")), None);
    }

    #[test]
    fn a_later_date_is_refused_with_the_date_named() {
        assert_eq!(
            refusal(d("2026-10-15"), d("2026-10-12")).as_deref(),
            Some("Dated Oct 15. Fix the date, or commit it from Oct 15.")
        );
        assert_eq!(
            refusal(d("2028-02-01"), d("2026-10-12")).as_deref(),
            Some("Dated Feb 1, 2028. Fix the date, or commit it from Feb 1, 2028.")
        );
    }
}
