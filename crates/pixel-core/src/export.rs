//! Esportazione degli artefatti finali: PNG (frame singolo o in memoria) e
//! ZIP (tutti i frame + metadati).
//!
//! I PNG non portano alcun colore di sfondo: il canale alpha del frame è
//! copiato tale e quale nel PNG, quindi la trasparenza si conserva
//! integralmente — il pixel "vuoto" resta trasparente, non bianco.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use image::{ImageEncoder, codecs::png::PngEncoder};
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::{error::PixelError, grid::Grid, project::Project};

/// Pixel RGBA di una griglia, appiattiti in modo row-major.
fn rgba_bytes(grid: &Grid) -> Vec<u8> {
    grid.pixels()
        .iter()
        .flat_map(|c| c.channels())
        .collect()
}

/// Codifica la griglia in PNG in memoria, senza toccare il disco.
/// Serve al tool MCP `get_canvas` e all'esportazione ZIP dei frame.
pub fn png_bytes(grid: &Grid) -> Result<Vec<u8>, PixelError> {
    let (width, height) = grid.dimensions();
    let mut data = Vec::new();
    let encoder = PngEncoder::new(std::io::Cursor::new(&mut data));
    encoder
        .write_image(
            &rgba_bytes(grid),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|source| PixelError::Io {
            path: PathBuf::from("(png in memoria)"),
            source: std::io::Error::other(source),
        })?;
    Ok(data)
}

/// Garantisce che la directory di destinazione esista.
fn ensure_parent_dir(path: &Path) -> Result<(), PixelError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| PixelError::Io {
            path: parent.to_owned(),
            source,
        })?;
    }
    Ok(())
}

/// Esporta il frame `frame_index` come PNG standalone senza sfondo.
pub fn export_png(
    project: &Project,
    frame_index: usize,
    out_path: impl AsRef<Path>,
) -> Result<(), PixelError> {
    let Some(frame) = project.frame(frame_index) else {
        return Err(PixelError::InvalidFrameIndex {
            index: frame_index,
            count: project.frame_count(),
        });
    };
    let data = png_bytes(frame.grid())?;
    let out_path = out_path.as_ref();
    ensure_parent_dir(out_path)?;
    std::fs::write(out_path, data).map_err(|source| PixelError::Io {
        path: out_path.to_owned(),
        source,
    })
}

/// Esporta tutti i frame in un archivio ZIP: `frame_0000.png`,
/// `frame_0001.png`, ... più `metadata.json` (dimensioni, fps, numero di
/// frame) per chi consuma gli asset in pipeline.
pub fn export_zip(project: &Project, out_path: impl AsRef<Path>) -> Result<(), PixelError> {
    let out_path = out_path.as_ref();
    ensure_parent_dir(out_path)?;

    let file = File::create(out_path).map_err(|source| PixelError::Io {
        path: out_path.to_owned(),
        source,
    })?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let options =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (index, frame) in project.frames().iter().enumerate() {
        zip.start_file(format!("frame_{index:04}.png"), options)
            .map_err(to_io)?;
        zip.write_all(&png_bytes(frame.grid())?)
            .map_err(|source| PixelError::Io {
                path: out_path.to_owned(),
                source,
            })?;
    }

    let metadata = format!(
        "{{\"name\": {:?}, \"width\": {}, \"height\": {}, \"fps\": {:.2}, \"frame_count\": {}}}",
        project.name(),
        project.width(),
        project.height(),
        project.fps(),
        project.frame_count(),
    );
    zip.start_file("metadata.json", options).map_err(to_io)?;
    zip.write_all(metadata.as_bytes())
        .map_err(|source| PixelError::Io {
            path: out_path.to_owned(),
            source,
        })?;
    zip.finish().map_err(to_io)?;
    Ok(())
}

fn to_io(err: zip::result::ZipError) -> PixelError {
    PixelError::Io {
        path: PathBuf::from("(zip)"),
        source: std::io::Error::other(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use std::io::Read as _;

    fn notify_test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pixel-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn small_project() -> Project {
        let mut project = Project::new_with_fps("export-demo".to_owned(), 3, 2, 12.0).unwrap();
        project.set_pixel(0, 0, Rgba::rgb(255, 0, 0));
        project.set_pixel(1, 1, Rgba::rgba(0, 255, 0, 7));
        project.add_frame(true); // frame 1: uguale al frame 0
        project
    }

    #[test]
    fn png_bytes_roundtrip_through_decoder() {
        use image::GenericImageView as _;
        let project = small_project();
        let data = png_bytes(project.active_frame().grid()).unwrap();
        let decoded = image::load_from_memory(&data).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        let red = decoded.get_pixel(0, 0).0;
        assert_eq!(red, [255, 0, 0, 255]);
        let soft_green = decoded.get_pixel(1, 1).0;
        // L'alpha parziale si conserva identico: nessuno sfondo se lo mangia.
        assert_eq!(soft_green, [0, 255, 0, 7]);
        // Il pixel non toccato resta trasparente: nessuno sfondo bianco.
        let untouched = decoded.get_pixel(2, 0).0;
        assert_eq!(untouched, [0, 0, 0, 0]);
    }

    #[test]
    fn export_png_writes_file_and_honors_alpha() {
        let project = small_project();
        let out = notify_test_dir().join("frame.png");
        export_png(&project, 0, &out).unwrap();
        assert!(out.exists());
        let decoded = image::open(&out).unwrap().to_rgba8();
        assert_eq!(decoded.get_pixel(1, 1).0[3], 7);
        // Indice fuori range: errore tipizzato, niente panic.
        assert!(matches!(
            export_png(&project, 99, &out),
            Err(PixelError::InvalidFrameIndex { index: 99, .. })
        ));
    }

    #[test]
    fn export_zip_organizes_frames_and_metadata() {
        let project = small_project();
        let out = notify_test_dir().join("demo.zip");
        export_zip(&project, &out).unwrap();

        let file = File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let names: Vec<String> = zip
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        assert_eq!(names, ["frame_0000.png", "frame_0001.png", "metadata.json"]);
        let mut metadata = String::new();
        zip.by_name("metadata.json")
            .unwrap()
            .read_to_string(&mut metadata)
            .unwrap();
        assert!(metadata.contains("\"frame_count\": 2"));
        assert!(metadata.contains("\"fps\": 12"));
    }
}
