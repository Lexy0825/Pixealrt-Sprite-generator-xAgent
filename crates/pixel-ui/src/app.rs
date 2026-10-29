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
use pixel_core::{brush, PixelError, Project, Rgba, project_json};

use crate::ui;

/// Attesa prima che le modifiche del canvas finiscano su disco.
const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(400);
/// Quanto resta visibile l'avviso di "aggiornato dall'esterno".
const EXTERNAL_NOTICE: Duration = Duration::from_secs(4);
/// Massimo di colori ricordati nella palette "recenti".
pub const RECENT_LIMIT: usize = 16;
/// Massimo di swatch estratti dal progetto per la palette.
const PALETTE_LIMIT: usize = 64;

/// Strumento attivo: la gomma cancella, la penna dipinge con `brush`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushMode {
    /// Penna: dipinge del colore `brush` (falloff se `brush_soft`).
    #[default]
    Pen,
    /// Gomma: rende trasparente il pixel (attenua i vicini se `brush_soft`).
    Eraser,
}

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
    /// Penna o gomma; la scelta incide sul pulsante sinistro del mouse.
    pub brush_mode: BrushMode,
    /// Tratto morbido: falloff sui vicini (penna) e attenuazione (gomma).
    pub brush_soft: bool,
    /// Colori usati di recente (testa = più recente), sessione corrente.
    pub recent_colors: Vec<Rgba>,
    /// Stato del selettore a ruota (hue/sat/val/alpha) sincronizzato col brush.
    pub picker: ui::colors::ColorPickerState,
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

    /// Cache della palette del progetto: `(content_rev, colori)`.
    palette_cache: Option<(u64, Vec<Rgba>)>,

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
    pub fn new(cc: &eframe::CreationContext<'_>, startup_path: Option<PathBuf>) -> Self {
        ui::apply_theme(&cc.egui_ctx);
        // Se un file progetto è stato passato da CLI si apre quello:
        // è il canale per condividere subito lo stato con il server MCP.
        let (project, path, load_error) = match &startup_path {
            Some(file) => match project_json::load(file) {
                Ok(project) => (Some(project), Some(file.clone()), None),
                Err(err) => {
                    eprintln!("apertura file avviato fallita: {err}");
                    (Project::new("progetto senza nome", 32, 32).ok(), None, Some(err))
                }
            },
            // Senza file: progetto pronto; la griglia nuova è tutta
            // trasparente e la scacchiera dice visivamente "qui si disegna".
            None => (Project::new("progetto senza nome", 32, 32).ok(), None, None),
        };
        if let Some(err) = load_error {
            eprintln!("progetto di partenza: {err}");
        }
        let mut app = Self {
            project,
            path,
            dirty: false,
            save_deadline: None,
            watched_mtime: None,
            external_notice_until: None,
            brush: Rgba::rgb(232, 90, 62),
            brush_mode: BrushMode::Pen,
            brush_soft: true,
            recent_colors: Vec::new(),
            picker: ui::colors::ColorPickerState::default(),
            zoom: 14.0,
            pan: egui::Vec2::ZERO,
            show_checker: true,
            show_grid: true,
            new_dialog: NewDialog::default(),
            ai_instructions_open: false,
            last_paint_cell: None,
            preview: ui::preview::PreviewState::default(),
            palette_cache: None,
            texture: None,
            texture_frame: None,
            texture_rev: 0,
            content_rev: 0,
        };
        app.sync_watch();
        // Il selettore parte allineato al colore iniziale del pennello.
        app.picker.load_from(app.brush);
        app
    }

    /// Cambia il colore del pennello (palette, recenti, cursore esagonale):
    /// tiene il selettore a ruota allineato ai canali RGB.
    pub fn set_brush(&mut self, color: Rgba) {
        self.brush = color;
        self.picker.load_from(color);
    }

    /// Inserisce un colore in testa ai recenti senza duplicati consecutivi.
    pub fn push_recent(&mut self, color: Rgba) {
        if color.is_transparent() {
            return;
        }
        self.recent_colors.retain(|&c| c != color);
        self.recent_colors.insert(0, color);
        self.recent_colors.truncate(RECENT_LIMIT);
    }

    /// Colori distinti (alpha > 0) di *tutti* i frame del progetto, in
    /// ordine di scoperta. Cache legata a `content_rev`: non riscatta
    /// nulla se i pixel non cambiano.
    pub fn project_palette(&mut self) -> Vec<Rgba> {
        let rev = self.content_rev;
        if let Some((cached_rev, cached)) = &self.palette_cache {
            if *cached_rev == rev {
                return cached.clone();
            }
        }
        let mut colors: Vec<Rgba> = Vec::new();
        if let Some(project) = &self.project {
            for frame in project.frames() {
                for color in frame.grid().pixels() {
                    if !color.is_transparent()
                        && !colors.contains(color)
                        && colors.len() < PALETTE_LIMIT
                    {
                        colors.push(*color);
                    }
                }
            }
        }
        self.palette_cache = Some((rev, colors.clone()));
        colors
    }
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
    ///
    /// `erase` distingue gomma da penna; l'impronta (un pixel o falloff sui
    /// vicini) segue `brush_soft` e vive in `pixel-core::brush`.
    pub fn paint_line(&mut self, cell: (u32, u32), erase: bool) {
        let (x1, y1) = cell;
        let (x0, y0) = self.last_paint_cell.unwrap_or(cell);
        let dx = i32::from(x1 as i32) - i32::from(x0 as i32);
        let dy = i32::from(y1 as i32) - i32::from(y0 as i32);
        let denom = (stroke_steps(dx, dy) as i32).max(1);
        // Colori e opzioni letti qui fuori dal loop, per non collidere col
        // prestito mutabile del progetto.
        let brush = self.brush;
        let soft = self.brush_soft;
        if !erase && self.last_paint_cell.is_none() {
            self.push_recent(brush);
        }
        // Dalle coordinate u32 ai pass interpolati, riquadrato in-bounds.
        let Some(project) = self.project.as_mut() else {
            return;
        };
        for step in 0..=stroke_steps(dx, dy) {
            let x = x0 as i32 + dx * step as i32 / denom;
            let y = y0 as i32 + dy * step as i32 / denom;
            if x >= 0 && y >= 0 {
                let grid = project.active_frame_mut().grid_mut();
                if erase {
                    brush::erase(grid, x as u32, y as u32, soft);
                } else {
                    brush::stamp(grid, x as u32, y as u32, brush, soft);
                }
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
        // Il file è diverso da come l'abbiamo lasciate: prima salviamo le
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
        // Il pannello Colori docking a sinistra dell'anteprima; anche lui
        // è un laterale, quindi prima del canvas (CentralPanel finale).
        ui::colors::show(self, root);
        ui::canvas::show(self, root, &ctx);

        // L'animazione in riproduzione vuole repaint a 60 Hz; in pausa
        // manteniamo comunque un tick lento (500 ms): serve a far girare il
        // polling del file condiviso anche quando l'utente non tocca nulla —
        // senza tick, egui inattivo non esegue pass e l'AI sembrerebbe muta.
        ctx.request_repaint_after(if self.preview.playing {
            Duration::from_secs_f64(1.0 / 60.0)
        } else {
            Duration::from_millis(500)
        });
    }
}

fn log_error(err: &PixelError) {
    eprintln!("errore: {err}");
}

fn log_done(path: &std::path::Path) {
    eprintln!("esportato: {}", path.display());
}

/// Numero di passi del tratteggio per la differenza (dx, dy) fra due celle.
///
/// Con `unsigned_abs`: `(-1) as u32` sarebbe `u32::MAX` e un click fermo
/// trasformerebbe il loop in miliardi d'iterazioni (congelamento UI).
fn stroke_steps(dx: i32, dy: i32) -> u32 {
    dx.unsigned_abs().max(dy.unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::stroke_steps;

    /// Un click fermo (delta 0) e un trascinamento normale: pochissimi
    /// passi. Con il bug storico dello scroll-as-u32 qui si sarebbero
    /// visti 4 294 967 295 passi (loop infinito-praticamente) e la
    /// prova pratica era una finestra "Non risponde".
    #[test]
    fn stroke_steps_never_explodes() {
        assert_eq!(stroke_steps(0, 0), 0); // click fermo
        assert_eq!(stroke_steps(3, -4), 4); // tratto corto in diagonale
        assert_eq!(stroke_steps(-60, 40), 60); // attraversamento di mezza griglia
    }
}
