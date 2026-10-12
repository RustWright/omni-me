//! The fixed list of retention categories an archived email is filed under.
//!
//! One per document, picked by the reader from this list and nothing else (his
//! ruling R4-6, 2026-10-11), so a timer can be set per category. The category is
//! its own field rather than a free tag; the archive shows it as `retain:<key>`.
//! Rationale and the analysis behind the list: `docs/src/archive.md`.

/// The prefix of the tag a category is shown as.
pub const RETAIN_TAG_PREFIX: &str = "retain:";

/// One category: the key stored, a label for screens, and the definition the
/// reader is given.
pub struct RetentionClass {
    pub key: &'static str,
    pub label: &'static str,
    pub definition: &'static str,
}

/// ⛔ `receipt` comes first and its definition carries the one rule that must
/// not be lost: a charge makes it a receipt whatever else the email is about.
pub const CLASSES: &[RetentionClass] = &[
    RetentionClass {
        key: "receipt",
        label: "Receipts and invoices",
        definition: "shows an amount the owner was charged or paid: a receipt, invoice, \
                     order confirmation with a total, or a delivery notice that also shows \
                     the charge. Any email showing a charged amount is a receipt, whatever \
                     else it is about.",
    },
    RetentionClass {
        key: "statement",
        label: "Statements, tax, payslips, bills",
        definition: "a bank or investment statement, a tax document, a payslip, or a bill \
                     with an amount due.",
    },
    RetentionClass {
        key: "banking_notice",
        label: "Banking notices",
        definition: "a notice about money moving or an account, with no charge shown: money \
                     received, a transfer, interest paid, a statement being ready.",
    },
    RetentionClass {
        key: "delivery_update",
        label: "Delivery updates",
        definition: "shipping or delivery status with no amount charged: shipped, out for \
                     delivery, delivered.",
    },
    RetentionClass {
        key: "login_code",
        label: "Sign-in links and codes",
        definition: "a one-time code, sign-in link, verification email or password reset.",
    },
    RetentionClass {
        key: "security_alert",
        label: "Security alerts",
        definition: "a new sign-in, a new device, a changed password or another account \
                     security warning.",
    },
    RetentionClass {
        key: "policy_notice",
        label: "Terms and policy notices",
        definition: "changes to terms of service, privacy policy, fees or agreements.",
    },
    RetentionClass {
        key: "promo",
        label: "Promotions and newsletters",
        definition: "marketing, offers, sales, newsletters, webinars or event invitations \
                     from a company.",
    },
    RetentionClass {
        key: "social",
        label: "Social notifications",
        definition: "notifications from a social network: likes, follows, friend \
                     suggestions, activity summaries.",
    },
    RetentionClass {
        key: "dev_notification",
        label: "Developer notifications",
        definition: "automated software messages: build or CI results, monitoring, site \
                     audits, repository activity.",
    },
    RetentionClass {
        key: "personal",
        label: "Personal correspondence",
        definition: "written by a person to the owner, or a document of lasting personal \
                     value: a letter, a contract, an application, a certificate.",
    },
    RetentionClass {
        key: "other",
        label: "Other",
        definition: "none of the above.",
    },
];

/// Whether `key` is on the list.
pub fn is_class(key: &str) -> bool {
    CLASSES.iter().any(|c| c.key == key)
}

/// The tag a category is shown as.
pub fn retain_tag(key: &str) -> String {
    format!("{RETAIN_TAG_PREFIX}{key}")
}

/// The keys, for a JSON schema `enum`.
pub fn keys() -> Vec<&'static str> {
    CLASSES.iter().map(|c| c.key).collect()
}

/// The list as prompt text, one line per category.
pub fn prompt_list() -> String {
    CLASSES
        .iter()
        .map(|c| format!("- {}: {}", c.key, c.definition))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_tag_safe() {
        let mut seen = std::collections::BTreeSet::new();
        for c in CLASSES {
            assert!(seen.insert(c.key), "duplicate {}", c.key);
            assert!(
                c.key.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'),
                "{} would not survive tag normalisation",
                c.key
            );
            assert!(crate::events::Tag::normalize(&retain_tag(c.key)).is_ok());
        }
    }

    #[test]
    fn a_charge_is_named_as_the_receipt_rule() {
        assert_eq!(CLASSES[0].key, "receipt");
        assert!(CLASSES[0].definition.contains("whatever"));
    }
}
