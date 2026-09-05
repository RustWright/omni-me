//! Tauri command handlers — the app's IPC surface.
//!
//! Two properties of this layer were reviewed and deliberately kept, so that
//! they are not repeatedly rediscovered as problems:
//!
//! **The one-line commands are not redundant wrappers.** Around a dozen
//! commands do nothing but call a `core::db::queries` function and map its
//! error to `String`. That is the minimum a `#[tauri::command]` can be: the
//! attribute is what registers the function in `generate_handler!` and makes it
//! reachable from the frontend, so the wrapper IS the feature. Deleting one
//! deletes the frontend's access to that query; "inlining" it is not possible.
//!
//! **`Result<T, String>` everywhere is the boundary's error type, not
//! laziness.** Errors cross into JavaScript, where a rich Rust error enum has
//! no representation — it would be serialised to a string on the way out
//! regardless. Typed errors stay meaningful in `core`, which is where anything
//! branches on them; by the time a value reaches this layer it is on its way to
//! a user-visible message. Trip-wire: revisit only if the frontend ever needs
//! to branch on an error *kind* rather than display it, at which point return a
//! structured payload for those specific commands rather than converting all of
//! them.

pub mod attachments;
pub mod auto_import;
pub mod budget;
pub mod extract;
pub mod feedback;
pub mod import;
pub mod journal_import;
pub mod llm;
pub mod notes;
pub mod routines;
pub mod settings;
pub mod share_intent;
pub mod sync;
pub mod timezone;
pub mod update;
pub mod workspace;

pub(crate) mod shared;
