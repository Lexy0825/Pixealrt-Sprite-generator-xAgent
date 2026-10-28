//! Stato dell'applicazione e ciclo di aggiornamento.
//!
//! Il file progetto `*.pixproj` è la single source of truth condivisa col
//! server MCP:
//! - le modifiche fatte qui vengono scritte su disco con debounce (400 ms);
//! - l'mtime del file viene controllato a ogni frame: se è cambiato (dicasi
//!   "l'AI ha scritto"), il progetto viene ricaricato e l'utente vede un
//!   avviso discreto sul canvas.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use pixel_core::{PixelError, Project, Rgba, project_json};

use crate::ui;

/// Attesa prima che le modifiche del canvas finiscano su disco.
const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(400);
/// Quanto resta visibile l'avviso di "aggiornato dall'esterno".
const EXTERNAL_NOTICE: Duration = Duration::from_secs(4);

/// Finestra modale "Nuovo progetto".
#[derive(Debug, Default)]
pub struct NewDialog {
    pub open: bool,
    pub name: String,
    pub width: String,
    pub height: String,
}

pub struct PixelApp {
    pub project: Option<Project>,
    pub path: Option<PathBuf>,

    /// Modifiche non ancora scritte su disco.
    dirty: bool,
    save_deadline: Option<Instant>,
    /// mtime del file come visto l'ultima volta: se cambia, qualcuno
    /// (di solito il server MCP) ha modificato il progetto fuori.
    watched_mtime: Option<SystemTime>,
    /// Fin di validità dell'avviso "progetto aggiornato dall'agente AI".
    external_notice_until: Option<Instant>,

    // disegno / vista
    pub brush: Rgba,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub show_checker: bool,
    pub show_grid: bool,

    // dialoghi e UI
    pub new_dialog: NewDialog,
    pub ai_instructions_open: bool,

    /// Ultima cella toccata nel tratto corrente, per non lasciare buchi
    /// nei trascinamenti veloci (interpolazione in linea retta).
    pub last_paint_cell: Option<(u32, u32)>,

    pub preview: ui::preview::PreviewState,

    /// Texture del canvas (frame attivo), ricostruita solo quando i pixel
    /// cambiano o cambia il frame selezionato.
    texture: Option<egui::TextureHandle>,
    texture_frame: Option<usize>,
    texture_rev: u64,
    /// Contatore di ogni modifica ai pixel: invalida le texture.
    content_rev: u64,
}

impl PixelApp {
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        ui::apply_theme(&cc.egui_ctx);
        // Si apre con un progetto pronto; la griglia nuova è tutta
        // trasparente e la scacchiera dice visivamente "qui si disegna".
        let project = Project::new("progetto senza nome", 32, 32).ok();
        Self {
            project,
            path: None,
            dirty: false,
            save_deadline: None,
            watched_mtime: None,
            external_notice_until: None,
            brush: Rgba::rgb(232, 90, 62),
            zoom: 14.0,
            pan: egui::Vec2::ZERO,
            show_checker: true,
            show_grid: true,
            new_dialog: NewDialog::default(),
            ai_instructions_open: false,
            last_paint_cell: None,
            preview: ui::preview::PreviewState::default(),
            texture: None,
            texture_frame: None,
            texture_rev: 0,
            content_rev: 0,
        }
    }

    // ——— mutazione + autosave —————————————————————————————————————————————

    /// Registra una modifica ai pixel: invalida la texture e avvia il
    /// debounce per il salvataggio automatico.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.content_rev = self.content_rev.wrapping_add(1);
        self.save_deadline = Some(Instant::now() + AUTOSAVE_DEBOUNCE);
    }

    /// `true` se ci sono modifiche non ancora scritte su disco.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Contatore delle modifiche ai pixel: invalida le texture cache.
    #[must_use]
    pub const fn content_rev(&self) -> u64 {
        self.content_rev
    }

    /// Disegna un tratto interpolando in linea retta dal punto precedente
    /// a `cell`: un trascinamento veloce non lascia buchi nel tratteggio.
    pub fn paint_line(&mut self, cell: (u32, u32), color: Rgba) {
        let (x1, y1) = cell;
        let (x0, y0) = self.last_paint_cell.unwrap_or(cell);
        let dx = i32::from(x1 as i32) - i32::from(x0 as i32);
        let dy = i32::from(y1 as i32) - i32::from(y0 as i32);
        let steps = dx.abs().max(dy.abs()).min(u32::MAX as i32) as u32;
        // Dalle coordinate u32 ai pass interpolati, riquadrato in-bounds.
        let Some(project) = self.project.as_mut() else {
            return;
        };
        for step in 0..=steps {
            let x = x0 as i32 + dx * step as i32 / (steps as i32).max(1);
            let y = y0 as i32 + dy * step as i32 / (steps as i32).max(1);
            if x >= 0 && y >= 0 {
                project.set_pixel(x as u32, y as u32, color);
            }
        }
        self.last_paint_cell = Some(cell);
        self.mark_dirty();
    }

    /// Salvataggio immediato sul file corrente, se ce n'è uno. Restituisce
    /// `true` se scritto.
    pub fn save_if_path(&mut self) -> bool {
        let Some(path) = self.path.clone() else {
            return false;
        };
        if self.save_now_to(&path) {
            self.dirty = false;
            self.save_deadline = None;
            self.sync_watch();
            true
        } else {
            false
        }
    }

    /// Salvataggio su un percorso esplicito (usato da "Salva come").
    pub fn save_now_to(&mut self, path: &std::path::Path) -> bool {
        let Some(project) = &self.project else {
            return false;
        };
        match project_json::save(project, path) {
            Ok(()) => true,
            Err(err) => {
                log_error(&err);
                false
            }
        }
    }

    /// Allinea l'mtime osservato con quello attuale del file, così le
    /// scritture fatte da noi non risultano "esterne".
    pub fn sync_watch(&mut self) {
        self.watched_mtime = self
            .path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
    }

    /// Controlla l'mtime: se il file è cambiato (server MCP), salva
    /// prima le modifiche pendenti e poi adotta il nuovo contenuto.
    fn poll_shared_file(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Ok(mtime) = fs::metadata(&path).and_then(|m| m.modified()) else {
            return; // file momentaneamente assente (rename atomico in corso)
        };
        if Some(mtime) == self.watched_mtime {
            return;
        }
        // Il file è diverso da come l'abbiamo lasciato: prima salviamo le
        // modifiche pendenti, poi il nuovo contenuto viene adottato.
        if self.dirty && self.save_if_path() {
            self.sync_watch();
            return;
        }
        match project_json::load(&path) {
            Ok(project) => {
                self.project = Some(project);
                self.dirty = false;
                self.content_rev = self.content_rev.wrapping_add(1);
                self.texture_frame = None; // forza il reload della texture
                self.external_notice_until = Some(Instant::now() + EXTERNAL_NOTICE);
                self.sync_watch();
            }
            Err(err) => log_error(&err),
        }
    }

    // ——— azioni (menu File) ———————————————————————————————————————————————

    pub fn action_new(&mut self, name: String, width: u32, height: u32) {
        match Project::new(name, width, height) {
            Ok(project) => {
                self.project = Some(project);
                self.path = None;
                self.dirty = false;
                self.content_rev = self.content_rev.wrapping_add(1);
                self.texture_frame = None;
                self.pan = egui::Vec2::ZERO;
                self.zoom = 14.0;
                self.sync_watch();
            }
            Err(err) => log_error(&err),
        }
    }

    pub fn action_open(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Progetti Pixel Studio", &[project_json::FILE_EXTENSION])
            .pick_file()
        else {
            return;
        };
        match project_json::load(&file) {
            Ok(project) => {
                self.project = Some(project);
                self.path = Some(file);
                self.dirty = false;
                self.content_rev = self.content_rev.wrapping_add(1);
                self.texture_frame = None;
                self.pan = egui::Vec2::ZERO;
                self.sync_watch();
            }
            Err(err) => log_error(&err),
        }
    }

    pub fn action_save_as(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Progetti Pixel Studio", &[project_json::FILE_EXTENSION])
            .set_file_name("progetto.pixproj")
            .save_file()
        else {
            return;
        };
        self.path = Some(file);
        self.save_if_path();
    }

    /// Esporta il frame attivo come PNG senza sfondo.
    pub fn action_export_png(&mut self) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Immagini PNG", &["png"])
            .set_file_name("frame.png")
            .save_file()
        else {
            return;
        };
        let index = project.active_frame_index();
        match pixel_core::export::export_png(&project, index, &file) {
            Ok(()) => log_done(&file),
            Err(err) => log_error(&err),
        }
    }

    /// Esporta tutti i frame come ZIP (uno PNG per frame + metadata.json).
    pub fn action_export_zip(&mut self) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Archivi ZIP", &["zip"])
            .set_file_name("frames.zip")
            .save_file()
        else {
            return;
        };
        match pixel_core::export::export_zip(&project, &file) {
            Ok(()) => log_done(&file),
            Err(err) => log_error(&err),
        }
    }

    // ——— texture del canvas ————————————————————————————————————————————————

    /// Texture del frame attivo, ricostruita solo se pixel o frame cambiano.
    pub fn canvas_texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        let project = self.project.as_ref()?;
        let frame = project.active_frame_index();
        let stale = self.texture.is_none()
            || self.texture_frame != Some(frame)
            || self.texture_rev != self.content_rev;
        if stale {
            let grid = project.active_frame().grid();
            let size = [
                usize::try_from(grid.width()).unwrap_or(0),
                usize::try_from(grid.height()).unwrap_or(0),
            ];
            let bytes: Vec<u8> = grid.pixels().iter().flat_map(|c| c.channels()).collect();
            let image = egui::ColorImage::from_rgba_unmultiplied(size, &bytes);
            let name = format!("canvas_frame_{frame}");
            let handle = match self.texture.take() {
                Some(mut tex) => {
                    // Stesso id: aggiorna la GPU texture esistentew senza
                    // sprecare una nuova risorsa a ogni modifica.
                    tex.set(image, egui::TextureOptions::NEAREST);
                    tex
                }
                None => ctx.load_texture(name, image, egui::TextureOptions::NEAREST),
            };
            self.texture = Some(handle);
            self.texture_frame = Some(frame);
            self.texture_rev = self.content_rev;
        }
        self.texture.as_ref()
    }

    fn advance_external_notice(&mut self) {
        if let Some(until) = self.external_notice_until
            && Instant::now() >= until
        {
            self.external_notice_until = None;
        }
    }

    /// `true` mentre l'avviso "modificato dall'agente AI" è visibile.
    #[must_use]
    pub fn has_external_notice(&self) -> bool {
        self.external_notice_until.is_some()
    }
}

impl eframe::App for PixelApp {
    /// Manutenzione dello stato: gira anche con la finestra nascosta,
    /// così l'autosave e il polling del file condiviso non si fermano.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self
            .save_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.save_if_path();
        }
        self.poll_shared_file();
        self.advance_external_notice();
        self.preview.advance(ctx, self.project.as_ref());
    }

    /// Disegno della UI: menu in alto, anteprima a destra, canvas al centro.
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // I pannelli si attaccano alla Root Ui ricevuta da eframe 0.37:
        // il menu in alto e l'anteprima vogliono lui stesso, il canvas
        // (CentralPanel) va chiamato per ultimo come da convenzione egui.
        // Context è un clone economico (Arc interno): così la Root Ui può
        // ancora essere passata in mutazione ai pannelli senza conflitti di borrow.
        let ctx = root.ctx().clone();
        ui::menu_bar::show(self, root, &ctx);
        ui::preview::show(self, root);
        ui::canvas::show(self, root, &ctx);

        // Con l'animazione in riproduzione il repaint è a cadenza fissa;
        // in pausa egui non ridisegna e non sprechiamo CPU.
        if self.preview.playing {
            ctx.request_repaint_after(Duration::from_secs_f64(1.0 / 60.0));
        }
    }
}

fn log_error(err: &PixelError) {
    eprintln!("errore: {err}");
}

fn log_done(path: &std::path::Path) {
    eprintln!("esportato: {}", path.display());
}
