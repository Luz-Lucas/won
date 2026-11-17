//! Motor de ajustes do Windows: modelo, catálogo embutido e validação.

pub mod catalog;
pub mod model;

pub use catalog::{ALLOWED_ROOTS, Catalog, FORBIDDEN_SERVICES, HIGH_PERFORMANCE_SCHEME, validate};
pub use model::{Action, Effect, Evidence, Group, Hive, RegValue, StartType, Tweak};
