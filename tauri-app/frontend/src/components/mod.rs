pub mod account_input;
pub mod date_field;
pub mod editor;
pub mod month_grid;
// Design-system foundation (Stage B), now consumed across finances (Stage C) and
// journal/notes/routines/settings (Stage D): Button, Banner, SegmentedNav, Section
// and most of the Icon set are live. A few items are deliberately published ahead
// of their first consumer — Card, StatTile, PageHeader, FieldLabel, TextInput /
// INPUT_CLASS, Trend, and the icon glyphs still living as inline SVGs in nav.rs /
// tag_editor.rs (Menu, Link, ChartBar, Wallet, ArrowUp, ArrowDown). The module-
// level `allow(dead_code)` covers those until they're adopted; drop it once every
// item is used.
#[allow(dead_code)]
pub mod icon;
pub mod nav;
#[allow(dead_code)]
pub mod primitives;
pub mod sync_status;
pub mod tag_editor;
