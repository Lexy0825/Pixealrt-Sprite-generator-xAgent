//! `pixel-core` — logica dati pura per la creazione di pixel-art e tileset.
//!
//! Il crate è indipendente dalla GUI e dal protocollo MCP: espone solo i
//! tipi da condividere tra [editor (pixel-ui)] e [server MCP (pixel-mcp)]:
//!
//! - [`color::Rgba`]: colore RGBA a 8 bit per canale; `alpha == 0` è
//!   *trasparenza totale*, nessun colore di sfondo predefinito.
//! - [`grid::Grid`]: griglia `width × height`, tutto trasparente di default,
//!   con conversione bounds-checked.
//! - [`frame::Frame`]: un fotogramma (griglia + durata in millisecondi).
//! - [`brush`]: impronta penna dura/morbida e gomma sulla griglia,
//! - [`project::Project`]: progetto multi-frame (nome, dimensioni, fps,
//!   frame attivo).
//! - [`project_json`]: persistenza su disco come JSON versione 1, con
//!   salvataggio atomico (write-then-rename).
//! - [`export`]: esportazione in PNG (frame singolo) e in ZIP (frame
//!   multipli + `metadata.json`), e PNG in memoria per il tool MCP
//!   `get_canvas`.
//!
//! Tutti i tipi sono `Send + Sync`-friendly e serializzabili con `serde`.

pub mod brush;
pub mod color;
pub mod error;
pub mod export;
pub mod frame;
pub mod grid;
pub mod project;
pub mod project_json;

pub use color::Rgba;
pub use error::PixelError;
pub use frame::Frame;
pub use grid::Grid;
pub use project::Project;
