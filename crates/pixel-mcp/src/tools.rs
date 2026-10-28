//! I tool MCP: ogni funzione è un tool JSON-RPC visibile all'agente AI.
//!
//! Ogni tool opera su un **file progetto** `*.pixproj` (JSON v1 di
//! `pixel-core`): caricamento → mutazione → salvataggio atomico. Il file su
//! disco è la single source of truth condivisa con l'editor `pixel-ui`, che
//! lo ricarica automaticamente.
//!
//! Secondo la API di rmcp 3.8, ogni tool riceve i suoi argomenti nel
//! wrapper [`Parameters`] con uno struct `Deserialize + JsonSchema` (lo
//! schema JSON generato descrive i parametri al client AI); nei test le
//! funzioni sono richiamabili direttamente, senza transport stdio.

use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use pixel_core::{PixelError, Project, Rgba, export as core_export, project_json};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ErrorData as McpError, schemars, tool, tool_router};

/// Servizio MCP senza stato: lo stato completo vive nei file progetto.
///
/// `Clone` è richiesto da `rmcp::serve` (il servizio gira in task dedicati).
#[derive(Clone, Default)]
pub struct PixelService;

impl PixelService {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Colore RGBA 8 bit per canale, come parametro MCP. L'alpha è opzionale e
/// predefinito a 255 (opaco): disegnare è il caso comune.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Alpha 0..255; `0` significa trasparenza totale (a tutti gli effetti
    /// la "gomma").
    #[serde(default = "default_alpha")]
    pub a: u8,
}

fn default_alpha() -> u8 {
    u8::MAX
}

impl From<Color> for Rgba {
    fn from(color: Color) -> Self {
        Rgba::rgba(color.r, color.g, color.b, color.a)
    }
}

// ——— helper comuni (errori, I/O progetto) ——————————————————————————————————

fn mcp_error(err: PixelError) -> McpError {
    McpError::internal_error(err.to_string(), None)
}

fn load_project(path: &str) -> Result<Project, McpError> {
    project_json::load(Path::new(path)).map_err(mcp_error)
}

fn save_project(project: &Project, path: &str) -> Result<(), McpError> {
    project_json::save(project, Path::new(path)).map_err(mcp_error)
}

fn describe(project: &Project, path: &str) -> String {
    format!(
        "progetto `{}` {}×{}, {} frame, frame attivo {}, fps {:.1}, file `{}`",
        project.name(),
        project.width(),
        project.height(),
        project.frame_count(),
        project.active_frame_index(),
        project.fps(),
        path,
    )
}

// ——— schemi dei parametri dei tool ——————————————————————————————————————————

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateProjectParams {
    /// Percorso del file progetto `.pixproj` da creare.
    pub path: String,
    /// Nome del progetto; se assente: "progetto senza nome".
    #[serde(default)]
    pub name: Option<String>,
    pub width: u32,
    pub height: u32,
    /// FPS dell'anteprima; predefinito 12.
    #[serde(default)]
    pub fps: Option<f32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PathParams {
    /// Percorso del file progetto `.pixproj`.
    pub path: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddFrameParams {
    pub path: String,
    /// `true` duplica i pixel del frame attivo; `false` crea un frame vuoto.
    pub copy_active: bool,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetActiveFrameParams {
    pub path: String,
    /// Indice 0-based del frame da attivare.
    pub index: usize,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetFpsParams {
    pub path: String,
    /// FPS dell'anteprima, limitato a 10..60.
    pub fps: f32,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetPixelParams {
    pub path: String,
    pub x: u32,
    pub y: u32,
    pub color: Color,
    /// Frame 0-based da toccare; `null`/assente = frame attivo.
    #[serde(default)]
    pub frame: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RectParams {
    pub path: String,
    /// Angolo inclusivo di partenza.
    pub x0: u32,
    pub y0: u32,
    /// Angolo inclusivo di arrivo.
    pub x1: u32,
    pub y1: u32,
    #[serde(default)]
    pub frame: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FillRectParams {
    #[serde(flatten)]
    pub rect: RectParams,
    pub color: Color,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GetCanvasParams {
    pub path: String,
    /// Frame 0-based da leggere; `null`/assente = frame attivo.
    #[serde(default)]
    pub frame: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportPngParams {
    pub path: String,
    /// File PNG di destinazione.
    pub out: String,
    /// Frame 0-based da esportare; `null`/assente = frame attivo.
    #[serde(default)]
    pub frame: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ExportZipParams {
    pub path: String,
    /// File ZIP di destinazione (frame_0000.png, ... + metadata.json).
    pub out: String,
}

// ——— definizione del servizio: router + ServerHandler in una volta ———————————

#[tool_router(server_handler)]
impl PixelService {
    #[tool(description = "Crea un nuovo progetto pixel-art e lo salva su `path` (file JSON `.pixproj`). La griglia parte tutta trasparente.")]
    fn create_project(
        &self,
        Parameters(CreateProjectParams {
            path,
            name,
            width,
            height,
            fps,
        }): Parameters<CreateProjectParams>,
    ) -> Result<String, McpError> {
        let project = Project::new_with_fps(
            name.unwrap_or_else(|| "progetto senza nome".to_owned()),
            width,
            height,
            fps.unwrap_or(12.0),
        )
        .map_err(mcp_error)?;
        save_project(&project, &path)?;
        Ok(format!("creato {}", describe(&project, &path)))
    }

    #[tool(description = "Statistiche del progetto: dimensioni, numero di frame, frame attivo, fps, percorso file.")]
    fn get_project_info(
        &self,
        Parameters(PathParams { path }): Parameters<PathParams>,
    ) -> Result<String, McpError> {
        let project = load_project(&path)?;
        Ok(describe(&project, &path))
    }

    #[tool(description = "Aggiunge un frame alla sequenza dell'animazione. `copy_active=true` duplica i pixel del frame attivo.")]
    fn add_frame(
        &self,
        Parameters(AddFrameParams { path, copy_active }): Parameters<AddFrameParams>,
    ) -> Result<String, McpError> {
        let mut project = load_project(&path)?;
        let index = project.add_frame(copy_active);
        save_project(&project, &path)?;
        Ok(format!(
            "aggiunto il frame {index} (totale {}): {}",
            project.frame_count(),
            describe(&project, &path),
        ))
    }

    #[tool(description = "Imposta il frame attivo su cui disegnare (indice 0-based).")]
    fn set_active_frame(
        &self,
        Parameters(SetActiveFrameParams { path, index }): Parameters<SetActiveFrameParams>,
    ) -> Result<String, McpError> {
        let mut project = load_project(&path)?;
        project.set_active_frame(index).map_err(mcp_error)?;
        save_project(&project, &path)?;
        Ok(format!(
            "frame attivo: {index} di {} ({})",
            project.frame_count(),
            describe(&project, &path),
        ))
    }

    #[tool(description = "Imposta i fps dell'anteprima dell'animazione (10..60).")]
    fn set_fps(
        &self,
        Parameters(SetFpsParams { path, fps }): Parameters<SetFpsParams>,
    ) -> Result<String, McpError> {
        let mut project = load_project(&path)?;
        project.set_fps(fps);
        save_project(&project, &path)?;
        Ok(describe(&project, &path))
    }

    #[tool(description = "Imposta un singolo pixel. Colore con r/g/b e alpha `a` (0 = trasparente). `frame` opzionale 0-based; senza, si disegna sul frame attivo.")]
    fn set_pixel(
        &self,
        Parameters(p): Parameters<SetPixelParams>,
    ) -> Result<String, McpError> {
        let SetPixelParams {
            path,
            x,
            y,
            color,
            frame,
        } = p;
        let mut project = load_project(&path)?;
        let rgba = Rgba::from(color);
        if !project.set_pixel_on_frame(frame, x, y, rgba) {
            return Err(McpError::internal_error(
                format!(
                    "coordinate ({x}, {y}) fuori dai limiti {}x{} o frame inesistente",
                    project.width(),
                    project.height(),
                ),
                None,
            ));
        }
        save_project(&project, &path)?;
        Ok(format!(
            "pixel ({x}, {y}) = rgba{:?} sul frame {}",
            rgba.channels(),
            frame.unwrap_or(project.active_frame_index()),
        ))
    }

    #[tool(description = "Riempie un rettangolo **inclusivo** con un colore (r/g/b/a). Utile per sfondi, bande e tile grandi.")]
    fn fill_rect(&self, Parameters(p): Parameters<FillRectParams>) -> Result<String, McpError> {
        let FillRectParams {
            rect: RectParams {
                path, x0, y0, x1, y1, frame,
            },
            color,
        } = p;
        let mut project = load_project(&path)?;
        project.fill_rect(x0, y0, x1, y1, Rgba::from(color), frame);
        save_project(&project, &path)?;
        Ok(format!(
            "rettangolo ({x0}, {y0})..({x1}, {y1}) riempito sul frame {}",
            frame.unwrap_or(project.active_frame_index()),
        ))
    }

    #[tool(description = "Cancella un rettangolo **inclusivo**: l'alpha torna a 0, cioè trasparenza totale.")]
    fn erase_rect(&self, Parameters(p): Parameters<RectParams>) -> Result<String, McpError> {
        let RectParams {
            path, x0, y0, x1, y1, frame,
        } = p;
        let mut project = load_project(&path)?;
        project.fill_rect(x0, y0, x1, y1, Rgba::TRANSPARENT, frame);
        save_project(&project, &path)?;
        Ok(format!(
            "rettangolo ({x0}, {y0})..({x1}, {y1}) azzerato sul frame {}",
            frame.unwrap_or(project.active_frame_index()),
        ))
    }

    #[tool(description = "Legge la griglia di un frame come PNG in base64. Serve a *vedere* il disegno; i pixel trasparenti restano alpha 0.")]
    fn get_canvas(
        &self,
        Parameters(GetCanvasParams { path, frame }): Parameters<GetCanvasParams>,
    ) -> Result<String, McpError> {
        let project = load_project(&path)?;
        let target = frame.unwrap_or(project.active_frame_index());
        let Some(frame_obj) = project.frame(target) else {
            return Err(McpError::internal_error(
                format!(
                    "frame {target} inesistente (frame presenti: {})",
                    project.frame_count()
                ),
                None,
            ));
        };
        let png = core_export::png_bytes(frame_obj.grid()).map_err(mcp_error)?;
        Ok(format!(
            "{{\"path\": {path:?}, \"width\": {}, \"height\": {}, \"frame\": {target}, \"png_base64\": {:?}}}",
            frame_obj.grid().width(),
            frame_obj.grid().height(),
            BASE64.encode(png),
        ))
    }

    #[tool(description = "Esporta il frame `frame` (predefinito: attivo) come PNG senza sfondo nel file `out`.")]
    fn export_png(
        &self,
        Parameters(ExportPngParams { path, out, frame }): Parameters<ExportPngParams>,
    ) -> Result<String, McpError> {
        let project = load_project(&path)?;
        let index = frame.unwrap_or(project.active_frame_index());
        core_export::export_png(&project, index, Path::new(&out)).map_err(mcp_error)?;
        Ok(format!("esportato `{path}` frame {index} → `{out}`"))
    }

    #[tool(description = "Esporta tutti i frame come ZIP `out`: frame_0000.png, frame_0001.png, ... più un metadata.json con dimensioni, fps e numero di frame.")]
    fn export_zip(
        &self,
        Parameters(ExportZipParams { path, out }): Parameters<ExportZipParams>,
    ) -> Result<String, McpError> {
        let project = load_project(&path)?;
        core_export::export_zip(&project, Path::new(&out)).map_err(mcp_error)?;
        Ok(format!(
            "esportato `{path}` ({} frame) → `{out}`",
            project.frame_count(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixel_core::Rgba;
    use std::fs;
    use std::path::PathBuf;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pixel-mcp-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_set_and_read_back() {
        let service = PixelService::new();
        let dir = tempdir("crud");
        let project_path = dir.join("demo.pixproj");
        let path = project_path.display().to_string();

        let created = service
            .create_project(Parameters(CreateProjectParams {
                path: path.clone(),
                name: Some("demo".to_owned()),
                width: 4,
                height: 4,
                fps: Some(24.0),
            }))
            .unwrap();
        assert!(created.contains("demo"));

        service
            .set_pixel(Parameters(SetPixelParams {
                path: path.clone(),
                x: 2,
                y: 1,
                color: Color {
                    r: 250,
                    g: 10,
                    b: 60,
                    a: 255,
                },
                frame: None,
            }))
            .unwrap();

        // Il file su disco è la fonte: lo ricarichiamo direttamente dal core.
        let loaded = project_json::load(&project_path).unwrap();
        assert_eq!(
            loaded.active_frame().grid().get_pixel(2, 1),
            Some(Rgba::rgb(250, 10, 60))
        );

        let canvas = service
            .get_canvas(Parameters(GetCanvasParams {
                path: path.clone(),
                frame: None,
            }))
            .unwrap();
        assert!(canvas.contains("\"width\": 4"));
        assert!(canvas.contains("\"height\": 4"));
    }

    #[test]
    fn out_of_bounds_pixel_is_an_error() {
        let service = PixelService::new();
        let dir = tempdir("oob");
        let project_path = dir.join("demo.pixproj");
        let path = project_path.display().to_string();
        service
            .create_project(Parameters(CreateProjectParams {
                path: path.clone(),
                name: None,
                width: 4,
                height: 4,
                fps: None,
            }))
            .unwrap();

        let err = service
            .set_pixel(Parameters(SetPixelParams {
                path,
                x: 9,
                y: 9,
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                frame: Some(5),
            }))
            .unwrap_err();
        assert!(err.to_string().contains("fuori dai limiti"));
    }

    #[test]
    fn frames_and_exports_work_on_disk() {
        let service = PixelService::new();
        let dir = tempdir("exports");
        let project_path = dir.join("anim.pixproj");
        let path = project_path.display().to_string();
        service
            .create_project(Parameters(CreateProjectParams {
                path: path.clone(),
                name: Some("anim".to_owned()),
                width: 2,
                height: 2,
                fps: None,
            }))
            .unwrap();
        service
            .set_pixel(Parameters(SetPixelParams {
                path: path.clone(),
                x: 0,
                y: 0,
                color: Color {
                    r: 255,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                frame: None,
            }))
            .unwrap();
        service
            .add_frame(Parameters(AddFrameParams {
                path: path.clone(),
                copy_active: true,
            }))
            .unwrap();

        let png = dir.join("frame.png");
        let zip = dir.join("frames.zip");
        service
            .export_png(Parameters(ExportPngParams {
                path: path.clone(),
                out: png.display().to_string(),
                frame: Some(0),
            }))
            .unwrap();
        service
            .export_zip(Parameters(ExportZipParams {
                path: path.clone(),
                out: zip.display().to_string(),
            }))
            .unwrap();
        assert!(png.exists() && png.metadata().unwrap().len() > 0);
        assert!(zip.exists() && zip.metadata().unwrap().len() > 0);
    }

    #[test]
    fn missing_project_file_fails_cleanly() {
        let err = PixelService::new()
            .get_project_info(Parameters(PathParams {
                path: "/percorso/che/non/esiste.pixproj".to_owned(),
            }))
            .unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
