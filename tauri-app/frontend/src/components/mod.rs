pub mod account_input;
pub mod date_field;
pub mod editor;
pub mod month_grid;
// Design-system primitives. Live and widely consumed: Button, Banner,
// PageHeader, Card, IconButton, SegmentedNav, TextInput / INPUT_CLASS, and most
// of the Icon set.
//
// Genuinely unconsumed — the *only* thing the two `allow(dead_code)` attributes
// below are for: `Section`, `StatTile`, `FieldLabel`, `Trend`, and the icon
// glyphs still drawn as inline SVG in nav.rs / tag_editor.rs (Menu, Link,
// ChartBar, Wallet, ArrowUp, ArrowDown).
//
// Trip-wire: delete each `allow(dead_code)` the moment its list empties — while
// it sits here it also hides any *newly* dead primitive. Re-check with a
// call-site grep rather than trusting this comment. The previous version of it
// was wrong in both directions at once: it claimed `Section` was live (it has
// never had a caller) and that Card / PageHeader / TextInput were awaiting one
// (they have 8 / 20 / 2).
#[allow(dead_code)]
pub mod icon;
pub mod nav;
#[allow(dead_code)]
pub mod primitives;
pub mod sync_status;
pub mod tag_editor;
