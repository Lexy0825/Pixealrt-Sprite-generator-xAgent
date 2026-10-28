//! Persistenza del progetto su disco: file JSON "versione 1', salvataggio
//! atomico (prima la scrittura del file temporaneo, poi il rename).
//!
//! Il file JSON è la single source of truth condivisa da editor e server
//! MCP: il campo `version` rende possibili le future migrazioni di formato.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{error::PixelError, project::Project};

/// La versione corrente del formato su disco. Qualsiasi progetto salvato
/// con una versione superiore viene rifiutato con
/// [`PixelError::UnsupportedVersion`]; versioni inferiori consentiranno le
/// migrazioni incrementali (oggi esiste solo la 1).
pub const FORMAT_VERSION: u32 = 1;

/// Estensione convenzionale per i file progetto.
pub const FILE_EXTENSION: &str = "pixproj";

/// Busta serializzata: version + progetto intero.
#[derive(Debug, Serialize, Deserialize)]
struct ProjectFile {
    version: u32,
    project: Project,
}

/// Salva il progetto in `path` con write-then-rename atomico.
///
/// Su tutti i sistemi (incluso Windows) `fs::rename` sostituisce il file di
/// destinazione: un crash a metà lascia intatto il vecchio file.
pub fn save(project: &Project, path: impl AsRef<Path>) -> Result<(), PixelError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| PixelError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    let wrapper = ProjectFile {
        version: FORMAT_VERSION,
        project: project.clone(),
    };
    let json = serde_json::to_string_pretty(&wrapper).map_err(|source| PixelError::Io {
        path: path.to_owned(),
        source: std::io::Error::other(source),
    })?;

    // Il file temporaneo vive nella stessa directory così il rename è
    // intrinsecamente atomico (stesso volume).
    let tmp = sibling_temp_path(path);
    fs::write(&tmp, json.as_bytes()).map_err(|source| PixelError::Io {
        path: tmp.clone(),
        source,
    })?;
    fs::rename(&tmp, path).map_err(|source| PixelError::Io {
        path: path.to_owned(),
        source,
    })
}

/// Carica il progetto da `path`, verificando la versione del formato.
pub fn load(path: impl AsRef<Path>) -> Result<Project, PixelError> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|source| PixelError::Io {
        path: path.to_owned(),
        source,
    })?;
    parse(&bytes)
}

/// Sonda minimale: legge solo `version` così un file di una versione futura
/// può essere rifiutato con l'errore tipizzato anche se il resto non decodifica.
#[derive(Deserialize)]
struct VersionProbe {
    version: Option<u32>,
}

fn json_err(e: serde_json::Error) -> PixelError {
    PixelError::Io {
        path: PathBuf::from("(json in memoria)"),
        source: std::io::Error::other(e),
    }
}

/// Parsing da byte già letti: separato per renderlo testabile senza toccare
/// il disco.
///
/// Prima si valida la versione del formato, poi si deserializza il progetto.
pub fn parse(bytes: &[u8]) -> Result<Project, PixelError> {
    let probe: VersionProbe = serde_json::from_slice(bytes).map_err(json_err)?;
    match probe.version {
        Some(version) if version == FORMAT_VERSION => {
            let wrapper: ProjectFile = serde_json::from_slice(bytes).map_err(json_err)?;
            Ok(wrapper.project)
        }
        found => Err(PixelError::UnsupportedVersion {
            found: found.unwrap_or_default(),
            supported: FORMAT_VERSION,
        }),
    }
}

/// Percorso del file temporaneo accanto al file di destinazione.
fn sibling_temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".tmp");
    path.with_file_name(name)
}

use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pixel-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_load_roundtrip_preserves_everything() {
        let dir = tempdir();
        let path = dir.join("demo.pixproj");
        let mut project = Project::new_with_fps("sprite", 6, 4, 24.0).unwrap();
        project.set_active_frame(0).unwrap();
        project.set_pixel(1, 0, Rgba::rgb(255, 0, 0));
        project.set_pixel(0, 1, Rgba::rgba(0, 255, 0, 7));
        project.add_frame(true);
        project.set_name("sprite ruotato");

        save(&project, &path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.name(), "sprite ruotato");
        assert_eq!(project, loaded);
        // Il file temporaneo è stato consumato dal rename.
        assert!(!dir.join("demo.pixproj.tmp").exists());
    }

    #[test]
    fn future_version_is_rejected() {
        let json = br#"{"version": 99}"#;
        let err = parse(json).unwrap_err();
        assert!(matches!(
            err,
            PixelError::UnsupportedVersion {
                found: 99,
                supported: 1
            }
        ));
    }
}
