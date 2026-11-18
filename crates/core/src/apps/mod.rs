//! Central de aplicativos: catálogo curado instalado via winget.

pub mod catalog;

pub use catalog::{App, AppCatalog, AppCategory, category_label, validate};
