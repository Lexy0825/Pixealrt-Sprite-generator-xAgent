//! Stato dell'applicazione e ciclo di disegno.
//!
//! Due schermate:
//! - `Screen::Home` (all'avvio): i progetti recenti e `[ + nuovo ]`;
//! - `Screen::Editor`: canvas con zoom/pan, nav degli strumenti e dock
//!   laterale a sezioni (Frame, Colori, Anteprima).
//!
//! Il file progetto `*.pixproj` resta la single source of truth condivisa
//! col server MCP: le modifiche locali vanno su disco con debounce
//! (400 ms) e l'mtime del file viene controllato a ogni frame; se  stato
//! scritto dall'esterno (l'AI) il progetto viene ricaricato e l'utente
//! vede un avviso discreto sul canvas.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui;
use pixel_core::{PixelError, Project, Rgba, brush, pixelate, project_json};

use crate::{recents, ui};

/// Attesa prima che le modifiche del canvas finiscano su disco.
const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(400);
/// Quanto resta visibile l'avviso di "aggiornato dall'esterno".
const EXTERNAL_NOTICE: Duration = Duration::from_secs(4);
/// Massimo di colori ricordati in "Recenti" (palette di sessione).
pub const RECENT_LIMIT: usize = 16;
/// Schermata corrente dell'app (homepage o editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// Homepage: progetti recenti + creazione nuova.
    Home,
    /// Editor: canvas, strumenti, dock.
    Editor,
}

/// Sezione del dock laterale richiamabile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Frames: lista verticale + tasti rapidi.
    Frames,
    /// Colori: ruota, palette e recenti.
    Colors,
    /// Anteprima del movimento.
    Preview,
}

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

    // navegazione delle schermate
    pub screen: Screen,
    /// Sezione del dock aperta (`None` = tutto canvas).
    pub section: Option<Section>,
    /// Popup snello dei colori aperto dalla nav.
    pub quick_colors_open: bool,
    /// Anteprima a schermo intero (modal): `Esc` o click per uscire.
    pub fullscreen_preview: bool,
    /// Superficie del canvas della frame precedente: serve a "Adatta".
    pub canvas_view_rect: Option<egui::Rect>,
    /// Copia dei recenti letta quando si apre la Homepage.
    pub home_entries: Vec<recents::Entry>,
    /// Texture 64x64 dei riquadri della Homepage, per percorso di thumb.
    pub home_thumbs: HashMap<String, egui::TextureHandle>,

    // modifiche / interazione col file condiviso
    dirty: bool,
    save_deadline: Option<Instant>,
    watched_mtime: Option<SystemTime>,
    external_notice_until: Option<Instant>,
    /// Stato fullscreen della frame precedente: `ViewportCommand` una volta sola.
    was_fullscreen: bool,

    // disegno / vista
    pub brush: Rgba,
    pub brush_mode: BrushMode,
    pub brush_soft: bool,
    pub recent_colors: Vec<Rgba>,
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
        // Se un file progetto arriva da CLI si apre subito l'editor su
        // quello; altrimenti la Homepage con i recenti  la prima cosa
        // che l'utente vede.
        let (project, path) = match &startup_path {
            Some(file) => {
                let loaded = project_json::load(file);
                match loaded {
                    Ok(project) => (Some(project), Some(file.clone())),
                    Err(err) => {
                        eprintln!("apertura file da avvio fallita: {err}");
                        let fallback = Project::new("progetto senza nome", 32, 32).ok();
                        (fallback, None)
                    }
                }
            }
            None => (Project::new("progetto senza nome", 32, 32).ok(), None),
        };
        let mut app = Self {
            project,
            path,
            screen: if startup_path.is_some() { Screen::Editor } else { Screen::Home },
            section: None,
            quick_colors_open: false,
            fullscreen_preview: false,
            canvas_view_rect: None,
            home_entries: recents::load(),
            home_thumbs: HashMap::new(),
            dirty: false,
            save_deadline: None,
            watched_mtime: None,
            external_notice_until: None,
            was_fullscreen: false,
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
            texture: None,
            texture_frame: None,
            texture_rev: 0,
            content_rev: 0,
        };
        app.sync_watch();
        app.picker.load_from(app.brush);
        app
    }

    /// Cambia il colore del pennello (palette, recenti, cursore esagonale):
    /// tiene il selettore a ruota allineato ai canali RGB.
    pub fn set_brush(&mut self, color: Rgba) {
        self.brush = color;
        self.picker.load_from(color);
    }

    /// Ricorda il colore usato (senza duplicati consecutivi).
    pub fn push_recent(&mut self, color: Rgba) {
        if color.is_transparent() {
            return;
        }
        self.recent_colors.retain(|&c| c != color);
        self.recent_colors.insert(0, color);
        self.recent_colors.truncate(RECENT_LIMIT);
    }

    // ——— palette esplicita del progetto ——————————————————————————————————

    /// Colori espliciti scelti dall'utente per il progetto: solo questi
    /// compaiono nella palette, mai il trasparente o i grigi del falloff
    /// del pennello morbido.
    pub fn palette_colors(&self) -> Vec<Rgba> {
        self.project
            .as_ref()
            .map_or_else(Vec::new, |p| p.palette().to_vec())
    }

    /// Aggiunge il colore corrente alla palette esplicita (se non c' gi).
    pub fn add_to_palette(&mut self) {
        let color = self.brush;
        if self
            .project
            .as_mut()
            .is_some_and(|p| p.push_palette(color))
        {
            self.mark_dirty();
        }
    }

    /// Rimuove un swatch dalla palette esplicita.
    pub fn remove_palette_color(&mut self, index: usize) {
        if self
            .project
            .as_mut()
            .is_some_and(|p| p.remove_palette(index))
        {
            self.mark_dirty();
        }
    }

    // ——— frame: crea / duplica / elimina / sposta / seleziona ——————————————

    /// Aggiunge un frame (vuoto o copia dell'attivo) e lo seleziona.
    pub fn frame_add(&mut self, copy_active: bool) {
        if let Some(project) = self.project.as_mut() {
            project.add_frame(copy_active);
            self.mark_dirty();
        }
    }

    /// Rimuove il frame attivo; l'ultimo frame  sempre conservato.
    pub fn frame_delete_active(&mut self) {
        if let Some(project) = self.project.as_mut() {
            let index = project.active_frame_index();
            if project.remove_frame(index) {
                self.mark_dirty();
            }
        }
    }

    /// Sposta il frame attivo di `delta` posizioni (negativo = in su).
    pub fn frame_move(&mut self, delta: i64) {
        if let Some(project) = self.project.as_mut() {
            let from = project.active_frame_index();
            let to = if delta < 0 {
                from.saturating_sub(delta.unsigned_abs() as usize)
            } else {
                from + delta as usize
            };
            if project.move_frame(from, to) {
                self.mark_dirty();
            }
        }
    }

    /// Attiva un frame dalla lista verticale della sezione Frame.
    pub fn select_frame(&mut self, index: usize) {
        if self
            .project
            .as_mut()
            .is_some_and(|p| p.set_active_frame(index).is_ok())
        {
            self.mark_dirty();
        }
    }

    // ——— navigazione schermate —————————————————————————————————————————————

    /// Torna alla Homepage: ricarica i recenti e ferma l'animazione.
    pub fn go_home(&mut self) {
        self.screen = Screen::Home;
        self.home_entries = recents::load();
        self.home_thumbs.clear();
        if self.preview.playing {
            self.preview.toggle();
        }
    }

    /// Riprender un progetto dai recenti o da file: ricarica da disco e
    /// passa all'editor. `false` se il file non  pi leggibile.
    pub fn open_project_from_disk(&mut self, path: &Path) -> bool {
        match project_json::load(path) {
            Ok(project) => {
                recents::record(&project, path);
                self.project = Some(project);
                self.path = Some(path.to_owned());
                self.dirty = false;
                self.content_rev = self.content_rev.wrapping_add(1);
                self.texture_frame = None;
                self.screen = Screen::Editor;
                self.pan = egui::Vec2::ZERO;
                self.zoom = 14.0;
                self.sync_watch();
                true
            }
            Err(err) => {
                log_error(&err);
                false
            }
        }
    }

    /// Aggiorna i recenti dopo un salvataggio (silenzioso).
    fn record_recents(&self, path: &Path) {
        // Se la testa dei recenti  gi file corrente, niente scrittura:
        // l'autosave esplode di frequente e il file non cambia mai qui.
        if recents::is_front(path) {
            return;
        }
        if let Some(project) = self.project.clone() {
            recents::record(&project, path);
        }
    }

    // ——— disegno: tratto interpolato (penna o gomma) ————————————————————+

    /// Disegna un tratto interpolando in linea retta dal punto precedente
    /// a `cell`: un trascinamento veloce non lascia buchi nel tratteggio.
    ///
    /// Il colore della penna entra nella palette esplicita del progetto a
    /// inizio tratto (il falloff morbido tocca solo i vicini, il centro
    /// resta il colore scelto).
    pub fn paint_line(&mut self, cell: (u32, u32), erase: bool) {
        let (x1, y1) = cell;
        let (x0, y0) = self.last_paint_cell.unwrap_or(cell);
        let dx = i32::from(x1 as i32) - i32::from(x0 as i32);
        let dy = i32::from(y1 as i32) - i32::from(y0 as i32);
        let denom = (stroke_steps(dx, dy) as i32).max(1);
        // Colori e opzioni letti qui fuori dal loop, per non colidere col
        // prestito mutabile del progetto.
        let brush_color = self.brush;
        let soft = self.brush_soft;
        if !erase && self.last_paint_cell.is_none() {
            self.push_recent(brush_color);
        }
        let Some(project) = self.project.as_mut() else {
            return;
        };
        // Inizio tratto in penna: il colore scelto entra nella palette
        // esplicita (i grigi del falloff no e il trasparente no).
        if !erase && self.last_paint_cell.is_none() {
            project.push_palette(brush_color);
        }
        for step in 0..=stroke_steps(dx, dy) {
            let x = x0 as i32 + dx * step as i32 / denom;
            let y = y0 as i32 + dy * step as i32 / denom;
            if x >= 0 && y >= 0 {
                let grid = project.active_frame_mut().grid_mut();
                if erase {
                    brush::erase(grid, x as u32, y as u32, soft);
                } else {
                    brush::stamp(grid, x as u32, y as u32, brush_color, soft);
                }
            }
        }
        self.last_paint_cell = Some(cell);
        self.mark_dirty();
    }

    // ——— file condiviso con il server MCP ———————————————————————————————————

    /// Salvataggio immediato sul file corrente, se ce n' uno. Restituisce
    /// `true` se scritto.
    pub fn save_if_path(&mut self) -> bool {
        let Some(path) = self.path.clone() else {
            return false;
        };
        if self.save_now_to(&path) {
            self.dirty = false;
            self.save_deadline = None;
            self.sync_watch();
            self.record_recents(&path);
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

    /// Allinea l'mtime osservato con quello attuale del file, cos le
    /// scritture fatte da noi non risultano "esterne".
    pub fn sync_watch(&mut self) {
        self.watched_mtime = self
            .path
            .as_ref()
            .and_then(|p| fs::metadata(p).ok())
            .and_then(|m| m.modified().ok());
    }

    /// Controlla l'mtime: se il file  cambiato (server MCP), salva
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
        // Il file  diverso da come l'abbiamo lasciato: prima salviamo le
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

    // ——— azioni dei menu ——————————————————————————————————————————————————————

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
        self.open_project_from_disk(&file);
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

    /// Pixella un'immagine scelta da disco sul frame attivo: la griglia
    /// resta della dimensione del progetto, l'immagine  adattata dentro
    /// (colori dominanti per cella) con eventuali bande trasparenti.
    pub fn action_import_image(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Immagini", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let grid = project.active_frame_mut().grid_mut();
        match pixelate::pixelate_from_path(grid, &file) {
            Ok(summary) => {
                eprintln!(
                    "pixellato {}: {} celle colorate, {} trasparenti (letterbox/vuote)",
                    file.display(),
                    summary.cells_filled,
                    summary.cells_transparent,
                );
                self.mark_dirty();
            }
            Err(err) => log_error(&err),
        }
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
                    // Stesso id: aggiorna la texture GPU esistente senza
                    // sprecarne una nuova a ogni modifica.
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
    /// cos l'autosave e il polling del file condiviso non si fermano.
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

    /// Disegno della UI: Homepage oppure editore (menu + nav strumenti +
    /// dock a sezioni + canvas); a schermo intero resta solo l'anteprima.
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();

        // Il comando fullscreen parte una sola volta per transizione.
        if self.fullscreen_preview != self.was_fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen_preview));
            self.was_fullscreen = self.fullscreen_preview;
        }

        // Esc: esce dall'anteprima modale; nella home non fa nulla.
        if self.fullscreen_preview {
            let esc_or_click = ctx.input(|i| {
                i.key_pressed(egui::Key::Escape) || i.pointer.any_click()
            });
            if esc_or_click {
                self.fullscreen_preview = false;
            }
            ui::preview::fullscreen_preview(self, root);
        } else {
            match self.screen {
                Screen::Home => ui::home::show(self, root, &ctx),
                Screen::Editor => {
                    // Pannelli in ordine: menu, nav strumenti, dock (sezioni
                    // laterali), poi il canvas (CentralPanel: per ultimo).
                    ui::menu_bar::show(self, root, &ctx);
                    ui::topbar::show(self, root, &ctx);
                    ui::dock::show(self, root, &ctx);
                    ui::canvas::show(self, root, &ctx);
                }
            }
        }

        // L'animazione in riproduzione vuole repaint a 60 Hz; in pausa un
        // tick lento (500 ms) fa girare il polling del file condiviso
        // anche quando l'utente non tocca nulla.
        ctx.request_repaint_after(if self.preview.playing || self.fullscreen_preview {
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
