//! Loja: catálogo de cursores, pacotes de ícones e temas. O catálogo embutido
//! (`catalog/loja/loja.zip`, gerado por `tools/loja/gerar.py`) funciona sem
//! internet; o catálogo online (`remoto.json` no repositório) traz itens extras
//! que são baixados sob demanda e conferidos por SHA-256.
//!
//! "Baixar" copia o pacote para `%LOCALAPPDATA%\WON\loja\<tipo>\<id>\`;
//! "Aplicar" aponta o registro do usuário (HKCU) para esses arquivos, guardando
//! antes o valor anterior; "Reverter" devolve exatamente o que havia.

pub mod atalhos;
pub mod catalog;
pub mod library;
pub mod sys;

pub use atalhos::{AppliedShortcuts, RevertedShortcuts};
pub use catalog::{Catalog, Fetch, HttpFetch, Item, Kind, REMOTE_URL};
pub use library::{Applied, Library, LibraryState, Reverted};
