//! Every amount the app shows, and the eye that hides them.
//!
//! Design and the cases text masking cannot catch: `docs/src/privacy-eye.md`.

use dioxus::prelude::*;

/// What a hidden amount shows instead of its digits.
pub const MASK: &str = "••••";

/// How far the eye reaches once it is on. Each level includes the one before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaskScope {
    /// Amounts the app formats itself: balances, totals, postings, inputs.
    Amounts,
    /// Also money written in text: chat, email bodies, subjects, descriptions.
    Text,
    /// Also documents and photos, covered until tapped.
    Everything,
}

impl MaskScope {
    pub const ALL: [MaskScope; 3] = [MaskScope::Amounts, MaskScope::Text, MaskScope::Everything];

    pub fn key(self) -> &'static str {
        match self {
            MaskScope::Amounts => "amounts",
            MaskScope::Text => "text",
            MaskScope::Everything => "everything",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Privacy {
    hidden: bool,
    scope: MaskScope,
}

const HIDDEN_KEY: &str = "omni.privacy.hidden";
const SCOPE_KEY: &str = "omni.privacy.scope";

// First read from localStorage, synchronously, so the first frame is already
// masked. The continuity store's copy then overrides it at boot, before any page
// mounts: the WebView flushes localStorage seconds late, so a kill can lose it.
static PRIVACY: GlobalSignal<Privacy> = Signal::global(load);

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn load() -> Privacy {
    let get = |k: &str| storage().and_then(|s| s.get_item(k).ok().flatten());
    Privacy {
        hidden: get(HIDDEN_KEY).as_deref() == Some("1"),
        scope: get(SCOPE_KEY)
            .as_deref()
            .and_then(MaskScope::from_key)
            .unwrap_or(MaskScope::Everything),
    }
}

/// The eye as the continuity store writes it to disk.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PersistedPrivacy {
    pub hidden: bool,
    pub scope: String,
}

pub fn persisted() -> PersistedPrivacy {
    let p = *PRIVACY.peek();
    PersistedPrivacy {
        hidden: p.hidden,
        scope: p.scope.key().to_string(),
    }
}

/// Apply the disk copy at boot. It wins over `localStorage`, which can lag.
pub fn restore(saved: PersistedPrivacy) {
    let scope = MaskScope::from_key(&saved.scope).unwrap_or(MaskScope::Everything);
    let next = Privacy {
        hidden: saved.hidden,
        scope,
    };
    if *PRIVACY.peek() != next {
        *PRIVACY.write() = next;
    }
    save(HIDDEN_KEY, if saved.hidden { "1" } else { "0" });
    save(SCOPE_KEY, scope.key());
}

fn save(key: &str, value: &str) {
    if let Some(s) = storage() {
        let _ = s.set_item(key, value);
    }
}

/// Whether amounts are hidden. Subscribes the calling component.
pub fn hidden() -> bool {
    PRIVACY.read().hidden
}

pub fn toggle_hidden() {
    let next = !PRIVACY.peek().hidden;
    PRIVACY.write().hidden = next;
    save(HIDDEN_KEY, if next { "1" } else { "0" });
}

pub fn scope() -> MaskScope {
    PRIVACY.read().scope
}

pub fn set_scope(scope: MaskScope) {
    PRIVACY.write().scope = scope;
    save(SCOPE_KEY, scope.key());
}

fn hides(level: MaskScope) -> bool {
    let p = *PRIVACY.read();
    p.hidden && p.scope >= level
}

/// Whether documents and photos are covered right now.
pub fn covers_documents() -> bool {
    hides(MaskScope::Everything)
}

/// A stringified decimal and its commodity, formatted: "1,234.50 CAD".
pub fn format_money(amount: &str, commodity: &str) -> String {
    if hides(MaskScope::Amounts) {
        return format!("{MASK} {commodity}");
    }
    format_amount(amount, commodity)
}

/// An amount shown as it arrived, "-12.5 CAD", for rows that never padded.
pub fn amount_as_is(amount: &str, commodity: &str) -> String {
    if hides(MaskScope::Amounts) {
        return format!("{MASK} {commodity}");
    }
    format!("{amount} {commodity}")
}

/// A bare figure with no commodity beside it, such as a budget target.
pub fn figure(value: &str) -> String {
    if hides(MaskScope::Amounts) {
        return MASK.to_string();
    }
    value.to_string()
}

/// Free text with any money in it masked, at the Text level and above.
pub fn text(s: &str) -> String {
    if hides(MaskScope::Text) {
        mask_text(s)
    } else {
        s.to_string()
    }
}

/// One spreadsheet cell. A lone decimal ("-45.2") is an amount in a statement
/// export even without a currency beside it, so it is masked outright.
pub fn cell(s: &str) -> String {
    if !hides(MaskScope::Text) {
        return s.to_string();
    }
    if is_bare_decimal(s.trim()) {
        MASK.to_string()
    } else {
        mask_text(s)
    }
}

fn is_bare_decimal(s: &str) -> bool {
    let s = s.trim_start_matches(['-', '+', '(']).trim_end_matches(')');
    let s = s.trim_start_matches(SYMBOLS);
    s.matches('.').count() == 1
        && s.starts_with(|c: char| c.is_ascii_digit())
        && s.ends_with(|c: char| c.is_ascii_digit())
        && s.chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

/// Inline style for an amount input: shows dots, still edits normally.
pub fn input_style() -> &'static str {
    if hides(MaskScope::Amounts) {
        "-webkit-text-security: disc;"
    } else {
        ""
    }
}

/// Inline style for a free-text input whose value holds money.
pub fn text_input_style(value: &str) -> &'static str {
    if hides(MaskScope::Text) && mask_text(value) != value {
        "-webkit-text-security: disc;"
    } else {
        ""
    }
}

/// The formatting behind [`format_money`], without the mask.
///
/// Pads to two decimal places and keeps longer fractions verbatim (BTC).
pub fn format_amount(amount: &str, commodity: &str) -> String {
    let (sign, magnitude) = match amount.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", amount),
    };
    let (int_part, frac_part) = match magnitude.split_once('.') {
        Some((i, f)) => (i, f.to_string()),
        None => (magnitude, String::new()),
    };
    let frac_padded = if frac_part.len() < 2 {
        format!("{frac_part:0<2}")
    } else {
        frac_part
    };
    let int_grouped = group_thousands(int_part);
    format!("{sign}{int_grouped}.{frac_padded} {commodity}")
}

fn group_thousands(int_part: &str) -> String {
    let bytes = int_part.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

const SYMBOLS: &[char] = &[
    '$', '€', '£', '¥', '₹', '₦', '₩', '₽', '₺', '₱', '₪', '฿', '¢',
];

const CODES: &[&str] = &[
    "CAD", "USD", "EUR", "GBP", "AED", "AUD", "JPY", "CHF", "NGN", "CNY", "INR", "MXN", "NZD",
    "HKD", "SGD", "SEK", "NOK", "DKK", "ZAR", "BRL", "KRW", "BTC", "ETH",
];

const WORDS: &[&str] = &["dollar", "dollars", "buck", "bucks", "cent", "cents"];

/// Mask every number in `s` that reads as money.
///
/// A number counts when a currency symbol or code sits beside it, a money word
/// follows it, or it has exactly two decimal places. A bare integer with none of
/// those ("spent 412 on food") is not caught; the docs page lists such cases.
pub fn mask_text(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        let Some((end, decimals)) = number_at(&c, i) else {
            out.push(c[i]);
            i += 1;
            continue;
        };
        // Dotted chains are versions, dates or IPs, never money.
        let dotted = c.get(end) == Some(&'.') && c.get(end + 1).is_some_and(char::is_ascii_digit);
        if dotted {
            let mut j = end;
            while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
                j += 1;
            }
            out.extend(&c[i..j]);
            i = j;
            continue;
        }
        if is_money(&c, i, end, decimals) {
            out.push_str(MASK);
        } else {
            out.extend(&c[i..end]);
        }
        i = end;
    }
    out
}

/// A number starting at `i`, sign included: (end, decimal digits).
fn number_at(c: &[char], i: usize) -> Option<(usize, usize)> {
    let signed = matches!(c[i], '-' | '+' | '−');
    let first = if signed { i + 1 } else { i };
    if !c.get(first).is_some_and(char::is_ascii_digit) {
        return None;
    }
    // Part of a word or a longer number ("A123", "v2", "1.2.3") is not a start.
    if let Some(&p) = i.checked_sub(1).and_then(|k| c.get(k))
        && (p.is_alphanumeric() || p == '.' || p == '_' || p == ',')
    {
        return None;
    }
    let mut j = first;
    while j < c.len() && c[j].is_ascii_digit() {
        j += 1;
    }
    while c.get(j) == Some(&',')
        && (1..=3).all(|k| c.get(j + k).is_some_and(char::is_ascii_digit))
        && !c.get(j + 4).is_some_and(char::is_ascii_digit)
    {
        j += 4;
    }
    let mut decimals = 0;
    if c.get(j) == Some(&'.') && c.get(j + 1).is_some_and(char::is_ascii_digit) {
        j += 1;
        while j < c.len() && c[j].is_ascii_digit() {
            j += 1;
            decimals += 1;
        }
    }
    // Glued to letters ("3rd", "45px") it is not a standalone amount, unless the
    // letters are a currency code ("45CAD").
    if c.get(j).is_some_and(|ch| ch.is_alphabetic()) && word_at(c, j).is_none_or(|w| !is_code(&w)) {
        return None;
    }
    Some((j, decimals))
}

fn is_money(c: &[char], start: usize, end: usize, decimals: usize) -> bool {
    let before = skip_space_back(c, start);
    if before.is_some_and(|k| SYMBOLS.contains(&c[k])) {
        return true;
    }
    if let Some(k) = before
        && c[k].is_ascii_uppercase()
        && is_code(&word_before(c, k + 1))
    {
        return true;
    }
    let after = skip_space_fwd(c, end);
    if after.is_some_and(|k| SYMBOLS.contains(&c[k])) {
        return true;
    }
    if let Some(w) = after.and_then(|k| word_at(c, k))
        && (is_code(&w) || WORDS.contains(&w.to_lowercase().as_str()))
    {
        return true;
    }
    decimals == 2 && c.get(end) != Some(&'%')
}

/// Index of the character before `i`, past at most one space.
fn skip_space_back(c: &[char], i: usize) -> Option<usize> {
    let k = i.checked_sub(1)?;
    if c[k] == ' ' {
        k.checked_sub(1)
    } else {
        Some(k)
    }
}

fn skip_space_fwd(c: &[char], i: usize) -> Option<usize> {
    let k = if c.get(i) == Some(&' ') { i + 1 } else { i };
    (k < c.len()).then_some(k)
}

fn word_at(c: &[char], i: usize) -> Option<String> {
    let w: String = c[i..].iter().take_while(|ch| ch.is_alphabetic()).collect();
    (!w.is_empty()).then_some(w)
}

/// The run of letters ending just before `end`.
fn word_before(c: &[char], end: usize) -> String {
    let mut k = end;
    while k > 0 && c[k - 1].is_alphabetic() {
        k -= 1;
    }
    c[k..end].iter().collect()
}

fn is_code(w: &str) -> bool {
    CODES.contains(&w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_amount_pads_fractional_to_two_digits() {
        assert_eq!(format_amount("5", "CAD"), "5.00 CAD");
        assert_eq!(format_amount("5.4", "CAD"), "5.40 CAD");
        assert_eq!(format_amount("5.40", "CAD"), "5.40 CAD");
    }

    #[test]
    fn format_amount_preserves_extra_precision_for_crypto() {
        assert_eq!(format_amount("0.00123456", "BTC"), "0.00123456 BTC");
    }

    #[test]
    fn format_amount_adds_thousands_separators() {
        assert_eq!(format_amount("1234.5", "CAD"), "1,234.50 CAD");
        assert_eq!(format_amount("1234567.89", "CAD"), "1,234,567.89 CAD");
        assert_eq!(format_amount("999.99", "CAD"), "999.99 CAD");
    }

    #[test]
    fn format_amount_handles_negative_amounts() {
        assert_eq!(format_amount("-1450.18", "CAD"), "-1,450.18 CAD");
        assert_eq!(format_amount("-1.5", "CAD"), "-1.50 CAD");
    }

    #[test]
    fn masks_symbol_amounts() {
        assert_eq!(
            mask_text("You spent $412.80 on food"),
            "You spent $•••• on food"
        );
        assert_eq!(mask_text("$45"), "$••••");
        assert_eq!(mask_text("-$1,450.18 owed"), "-$•••• owed");
        assert_eq!(mask_text("€ 12"), "€ ••••");
        assert_eq!(mask_text("12€"), "••••€");
    }

    #[test]
    fn masks_code_amounts_either_side() {
        assert_eq!(mask_text("412.80 CAD"), "•••• CAD");
        assert_eq!(mask_text("CAD 1,200"), "CAD ••••");
        assert_eq!(mask_text("paid 1200 USD."), "paid •••• USD.");
        assert_eq!(mask_text("45CAD"), "••••CAD");
    }

    #[test]
    fn masks_two_decimal_figures_and_money_words() {
        assert_eq!(mask_text("total -1,450.18"), "total ••••");
        assert_eq!(mask_text("rent was 1800 dollars"), "rent was •••• dollars");
        assert_eq!(mask_text("(45.20)"), "(••••)");
    }

    #[test]
    fn a_lone_decimal_cell_is_an_amount() {
        for s in ["-45.2", "1,234.5", "(12.00)", "$3.5"] {
            assert!(is_bare_decimal(s), "{s}");
        }
        for s in ["2026-11-19", "1234", "1.2.6", "Groceries", ""] {
            assert!(!is_bare_decimal(s), "{s}");
        }
    }

    #[test]
    fn leaves_numbers_that_are_not_money() {
        for s in [
            "version 1.2.6",
            "on 2026-11-19 at 10:30",
            "12.50% of budget",
            "16,000 events",
            "3 transactions",
            "item 1234",
            "A1234.56",
            "192.168.1.10",
            "ruling 107",
        ] {
            assert_eq!(mask_text(s), s, "{s}");
        }
    }
}
