//! Persisted settings.
//!
//! Everything lives under `%USERPROFILE%\.burntfern\`:
//!
//! ```text
//! .burntfern\
//!   config.json          # shared across every burntfern app
//!   slate\
//!     settings.json      # Slate-specific
//!     sets.json          # saved tab sets
//!     logs\  backups\
//! ```
//!
//! Values cascade **baked defaults <- shared `config.json` <- `slate\settings.json`**.
//! Writes are atomic (temp + rename); a corrupt file is moved to `backups\` and
//! the app falls back to defaults instead of crashing.

mod load;
mod save;
mod schema;

pub use load::{ensure, load, load_sets};
pub use save::{save, save_sets};
pub use schema::{SavedSet, SavedSets, SavedTab, Settings, SCHEMA_VERSION};
