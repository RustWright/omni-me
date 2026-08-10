pub mod account_input;
pub mod date_field;
pub mod editor;
// Design-system foundation (Stage B). Landed ahead of its consumers — the
// finances Overview/Ledger/Analyze surfaces (Stage C) are the first sites to use
// the full primitive/icon set, so some variants read as dead code until then.
// Drop these `allow`s as the surfaces adopt them.
#[allow(dead_code)]
pub mod icon;
pub mod nav;
#[allow(dead_code)]
pub mod primitives;
pub mod sync_status;
pub mod tag_editor;
