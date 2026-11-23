pub mod account_input;
pub mod attachment_viewer;
pub mod date_field;
pub mod editor;
pub mod feedback;
pub mod month_grid;
pub mod search_pick;
// `icon`'s `allow(dead_code)` exists only because nav.rs and tag_editor.rs still hand-draw six
// of its glyphs; adopt `Icon` there and remove it, since it also hides newly dead glyphs.
// `primitives` must never get one, so the compiler names the next unused item.
#[allow(dead_code)]
pub mod icon;
pub mod nav;
pub mod plan_card;
pub mod primitives;
pub mod privacy_eye;
pub mod proposal_card;
pub mod splash;
pub mod sync_status;
pub mod tag_editor;
