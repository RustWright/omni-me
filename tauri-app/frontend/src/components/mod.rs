pub mod account_input;
pub mod date_field;
pub mod editor;
pub mod month_grid;
// `icon` keeps an `allow(dead_code)` for exactly one reason: six glyphs
// (Menu, Link, ChartBar, Wallet, ArrowUp, ArrowDown) exist in the set while
// nav.rs and tag_editor.rs still hand-draw the same shapes as inline SVG. That
// is duplication, not spare capacity — the fix is to adopt `Icon` at those
// call sites, after which this attribute must go. While it sits here it also
// hides any *newly* dead glyph.
//
// `primitives` deliberately has NO such attribute: every item in it is
// consumed, and it must stay that way so the compiler reports the next
// unconsumed one instead of a comment claiming there are none. Four items
// (`Section`, `StatTile`, `FieldLabel`, `Trend`) were removed on 2026-08-28
// having never had a caller — the design-system refactor that would have
// adopted them ran to completion across all five pages without doing so, and
// the Overview surface `StatTile` was written for shipped with richer cards
// instead. Verify with a call-site grep, never by trusting a comment: the
// version of this note that preceded the removal was wrong in both directions
// at once, claiming `Section` was live and that Card / PageHeader / TextInput
// were still awaiting a first caller.
#[allow(dead_code)]
pub mod icon;
pub mod nav;
pub mod primitives;
pub mod splash;
pub mod sync_status;
pub mod tag_editor;
