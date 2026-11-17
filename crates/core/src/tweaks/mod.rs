//! Motor de ajustes do Windows: modelo, catálogo embutido, backend e estado.

pub mod backend;
pub mod catalog;
pub mod model;
pub mod state;

pub use backend::{MemoryBackend, Snapshot, SystemBackend, target_of};
pub use catalog::{ALLOWED_ROOTS, Catalog, FORBIDDEN_SERVICES, HIGH_PERFORMANCE_SCHEME, validate};
pub use model::{Action, Effect, Evidence, Group, Hive, RegValue, StartType, Tweak};
pub use state::{Change, TweakStatus, TweakView, view};
