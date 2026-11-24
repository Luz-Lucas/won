//! Temas (§9.5): um arquivo `theme.toml` que junta ajustes nativos, o papel
//! de parede da Oficina e a cor de destaque. Aplicar passa pelo motor de
//! ajustes (diário primeiro) e vira um único item no histórico de temas.

pub mod apply;
pub mod builtin;
pub mod model;

pub use apply::{
    Applied, Env, Preview, PreviewItem, Record, WallpaperCtx, apply, history, preview, undo_last,
};
pub use model::{Accent, Parsed, Theme, WallpaperRef, parse, to_toml};
