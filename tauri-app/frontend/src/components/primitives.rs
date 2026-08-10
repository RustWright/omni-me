//! Shared UI primitives — the app's building blocks.
//!
//! Before this, every card/button/banner/tile was an inline Tailwind class
//! string copy-pasted across `pages/*.rs` and drifted apart (4+ primary-button
//! variants, card radius `lg` vs `xl`, surface opacity `/30`–`/60`–full, border
//! `white/5` vs `white/10`). These primitives are the single source of truth so
//! the whole app reads as one system. They consume the design tokens from
//! `tailwind.config.js` / `input.css` (`obsidian-*`, `success/warn/error`,
//! `rounded-card`, `shadow-card`) — change a token, every primitive follows.
//!
//! Each takes an optional `class` appended last, so a call site can still tune
//! spacing/width without forking the base look.

use dioxus::prelude::*;

/// Canonical form-input class. `date_field.rs` / `account_input.rs` still carry
/// their own near-identical copies (folded here when those get migrated); new
/// inputs should use this or the [`TextInput`] wrapper.
pub const INPUT_CLASS: &str = "w-full px-3 py-2 bg-obsidian-sidebar border border-obsidian-border/10 rounded-md text-obsidian-text text-sm outline-none focus:border-obsidian-accent";

// ── Surfaces ────────────────────────────────────────────────────────────────

/// Elevated content surface — the one canonical card. `interactive` adds a
/// hover lift + pointer for cards that are themselves click targets.
#[component]
pub fn Card(
    #[props(default = String::new())] class: String,
    #[props(default = false)] interactive: bool,
    children: Element,
) -> Element {
    let hover = if interactive {
        " transition-shadow hover:shadow-card-hover cursor-pointer"
    } else {
        ""
    };
    rsx! {
        div { class: "bg-obsidian-surface border border-obsidian-border/10 rounded-card shadow-card p-4{hover} {class}",
            {children}
        }
    }
}

/// Page title block: accent H1 + optional subtitle on the left, action slot
/// (`children`) right-aligned. Matches the established
/// `text-2xl font-bold text-obsidian-accent` page-title treatment.
#[component]
pub fn PageHeader(
    title: String,
    #[props(default = String::new())] subtitle: String,
    children: Element,
) -> Element {
    rsx! {
        div { class: "flex items-start justify-between gap-4 mb-4",
            div {
                h1 { class: "text-2xl font-bold text-obsidian-accent", "{title}" }
                if !subtitle.is_empty() {
                    p { class: "mt-0.5 text-sm text-obsidian-text-muted", "{subtitle}" }
                }
            }
            div { class: "flex items-center gap-2 shrink-0", {children} }
        }
    }
}

/// A labeled sub-section: small uppercase caption + content.
#[component]
pub fn Section(
    title: String,
    #[props(default = String::new())] class: String,
    children: Element,
) -> Element {
    rsx! {
        section { class: "{class}",
            h2 { class: "mb-2 text-xs font-semibold uppercase tracking-wide text-obsidian-text-muted",
                "{title}"
            }
            {children}
        }
    }
}

/// Small form-field caption.
#[component]
pub fn FieldLabel(text: String, #[props(default = String::new())] class: String) -> Element {
    rsx! {
        label { class: "block mb-1 text-xs font-medium text-obsidian-text-muted {class}", "{text}" }
    }
}

// ── Button ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonVariant {
    /// Accent-filled — the one primary action per view.
    #[default]
    Primary,
    /// Bordered surface — secondary actions.
    Secondary,
    /// No fill — tertiary / toolbar actions.
    Ghost,
    /// Destructive — delete/remove.
    Danger,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
}

/// The one button. Settled primary treatment: accent fill + white label +
/// semibold + `rounded-md`, hover dims the fill.
#[component]
pub fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default = false)] disabled: bool,
    /// Stretch to the container width.
    #[props(default = false)]
    full: bool,
    /// Render as `type=submit` (default `button`).
    #[props(default = false)]
    submit: bool,
    #[props(default = String::new())] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let base = "inline-flex items-center justify-center gap-1.5 rounded-md font-semibold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-obsidian-accent/60 disabled:opacity-50 disabled:cursor-not-allowed";
    let sizing = match size {
        ButtonSize::Sm => "px-3 py-1.5 text-xs",
        ButtonSize::Md => "px-4 py-2 text-sm",
    };
    let tone = match variant {
        ButtonVariant::Primary => "bg-obsidian-accent text-white hover:bg-obsidian-accent/90",
        ButtonVariant::Secondary => {
            "bg-obsidian-surface text-obsidian-text border border-obsidian-border/10 hover:bg-white/5"
        }
        ButtonVariant::Ghost => "text-obsidian-text-muted hover:text-obsidian-text hover:bg-white/5",
        ButtonVariant::Danger => "bg-error/15 text-error border border-error/30 hover:bg-error/25",
    };
    let width = if full { "w-full" } else { "" };
    rsx! {
        button {
            r#type: if submit { "submit" } else { "button" },
            class: "{base} {sizing} {tone} {width} {class}",
            disabled,
            onclick: move |e| {
                if let Some(h) = &onclick {
                    h.call(e);
                }
            },
            {children}
        }
    }
}

// ── Banner ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
pub enum BannerKind {
    #[default]
    Info,
    Success,
    Warn,
    Error,
}

/// Inline status message — tinted surface + matching border/text.
#[component]
pub fn Banner(
    #[props(default)] kind: BannerKind,
    #[props(default = String::new())] class: String,
    children: Element,
) -> Element {
    let tone = match kind {
        BannerKind::Info => "bg-obsidian-accent/10 border-obsidian-accent/25 text-obsidian-accent",
        BannerKind::Success => "bg-success/10 border-success/25 text-success",
        BannerKind::Warn => "bg-warn/10 border-warn/25 text-warn",
        BannerKind::Error => "bg-error/10 border-error/25 text-error",
    };
    rsx! {
        div { class: "flex items-start gap-2 px-3 py-2 rounded-md border text-sm {tone} {class}",
            {children}
        }
    }
}

// ── StatTile ────────────────────────────────────────────────────────────────

/// Direction of a stat's delta — colors the delta line.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum Trend {
    #[default]
    Flat,
    Up,
    Down,
}

/// Headline metric: caption + big tabular value + optional colored delta.
/// The Overview surface's net-worth / balance tiles.
#[component]
pub fn StatTile(
    label: String,
    value: String,
    #[props(default = String::new())] delta: String,
    #[props(default)] trend: Trend,
    #[props(default = String::new())] class: String,
) -> Element {
    let delta_class = match trend {
        Trend::Up => "text-success",
        Trend::Down => "text-error",
        Trend::Flat => "text-obsidian-text-muted",
    };
    rsx! {
        div { class: "bg-obsidian-surface border border-obsidian-border/10 rounded-card shadow-card p-4 {class}",
            p { class: "text-xs font-medium uppercase tracking-wide text-obsidian-text-muted", "{label}" }
            p { class: "mt-1 text-2xl font-bold text-obsidian-text tabular-nums", "{value}" }
            if !delta.is_empty() {
                p { class: "mt-0.5 text-xs font-medium {delta_class}", "{delta}" }
            }
        }
    }
}

// ── SegmentedNav ────────────────────────────────────────────────────────────

/// Pill-style segmented control — the finances sub-nav (Overview · Ledger ·
/// Analyze) and any other small mutually-exclusive switch. `items` is
/// `(key, label)`; the active key is filled with the accent.
#[component]
pub fn SegmentedNav(
    items: Vec<(String, String)>,
    active: String,
    on_select: EventHandler<String>,
    #[props(default = String::new())] class: String,
) -> Element {
    rsx! {
        div { class: "inline-flex items-center gap-1 p-1 rounded-lg bg-obsidian-sidebar {class}",
            for (key , label) in items.iter() {
                button {
                    key: "{key}",
                    r#type: "button",
                    class: if key == &active {
                        "px-3 py-1.5 text-sm font-medium rounded-md bg-obsidian-accent text-white"
                    } else {
                        "px-3 py-1.5 text-sm font-medium rounded-md text-obsidian-text-muted hover:text-obsidian-text hover:bg-white/5"
                    },
                    onclick: {
                        let k = key.clone();
                        move |_| on_select.call(k.clone())
                    },
                    "{label}"
                }
            }
        }
    }
}

/// Simple controlled text input using the canonical [`INPUT_CLASS`].
#[component]
pub fn TextInput(
    value: String,
    on_input: EventHandler<String>,
    #[props(default = String::new())] placeholder: String,
    #[props(default = false)] disabled: bool,
    #[props(default = INPUT_CLASS.to_string())] class: String,
) -> Element {
    rsx! {
        input {
            r#type: "text",
            class: "{class}",
            placeholder: "{placeholder}",
            value: "{value}",
            autocomplete: "off",
            disabled,
            oninput: move |e| on_input.call(e.value()),
        }
    }
}
