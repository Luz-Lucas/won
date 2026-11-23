//! Central de aplicativos: catálogo curado instalado via winget.

pub mod catalog;
pub mod parse;
pub mod runner;
pub mod winget;

pub use catalog::{App, AppCatalog, AppCategory, category_label, validate};
pub use parse::{MIN_WINGET, describe_exit, parse_export, parse_upgrade_table};
pub use runner::{CommandRunner, Output, SystemRunner};
pub use winget::{AppState, Availability, ItemOutcome, PackageManager, Winget};
