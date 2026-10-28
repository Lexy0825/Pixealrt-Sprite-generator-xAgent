//! Errori tipizzati del crate, restituiti da tutte le operazioni fallibili.

use std::io;
use std::path::PathBuf;

/// Errore di `pixel-core`.
#[derive(Debug, thiserror::Error)]
pub enum PixelError {
    /// Errore di I/O (lettura, scrittura, rename) su un percorso file.
    #[error("errore di I/O su `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Il formato file su disco usa versioni successive a quelle supportate.
    #[error(
        "versione del formato file non supportata: {found} (supportata: {supported})"
    )]
    UnsupportedVersion { found: u32, supported: u32 },

    /// Dimensioni griglia o progetto inaccettabili (zero, negative overflow).
    #[error("dimensioni non valide: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    /// Indice frame fuori range rispetto ai frame correnti.
    #[error("indice frame {index} fuori range (frame presenti: {count})")]
    InvalidFrameIndex { index: usize, count: usize },
}
