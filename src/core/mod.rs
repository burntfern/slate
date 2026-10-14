pub mod group;
pub mod identity;
pub mod workspace;

pub use group::{Effect, Group, GroupSet, Tab};
pub use identity::{GroupId, TabId, WindowIdentity};
pub use workspace::{plan_restore, OpenWindow, RestoreOutcome, SavedGroup, SavedTab, Workspace};
