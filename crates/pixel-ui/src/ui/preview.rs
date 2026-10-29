//! Anteprima dell'animazione: sezione del dock e modale a schermo intero.

use eframe::egui;
use pixel_core::{Grid, Project};

use crate::app::PixelApp;
use crate::ui::{ACCENT, CANVAS_BG, CHECKER_A, CHECKER_B, WEAK_TEXT};

/// Lato massimo del player nella sezione del dock.
const PREVIEW_SIZE: f32 = 236.0;

/// Stato dell'anteprima: riproduzione, testina del tempo e texture
/// ricalcolata solo quando serve.
#[derive(Default)]
pub struct PreviewState {
    pub playing: bool,
    /// Tempo trascorso dall'inizio del ciclo, in millisecondi.
    playhead_ms: f32,
    shown_frame: usize,
    built_frame: Option<usize>,
    built_rev: u64,
    texture: Option<egui::TextureHandle>,
}

impl PreviewState {
    /// Alterna riproduzione e pausa.
    pub fn toggle(&mut self) {
        self.playing = !self.playing;
        if !self.playing {
            self.playhead_ms = 0.0;
            self.shown_frame = 0;
        }
    }

    /// Aggiorna la testina del tempo e sceglie quale frame mostrare.
    pub fn advance(&mut self, ctx: &egui::Context, project: Option<&Project>) {
        if !self.playing {
            self.playhead_ms = 0.0;
            if let Some(project) = project {
                self.shown_frame = project.active_frame_index();
            }
            return;
        }
        let Some(project) = project else {
            self.playing = false;
            return;
        };
        let dt_ms = ctx.input(|i| i.stable_dt) * 1000.0;
        self.playhead_ms += dt_ms;
        let total_ms: f32 = project.frames().iter().map(|f| f.duration_ms()).sum();
        if total_ms > 0.0 {
            self.playhead_ms %= total_ms;
        }
        self.shown_frame = frame_at_time(project, self.playhead_ms);
    }

    /// Ultimo frame disegnato (per i contatori).
    pub const fn shown_frame(&self) -> usize {
        self.shown_frame
    }
}

/// Frame visibile a `playhead_ms`, sommando le durate dei frame.
fn frame_at_time(project: &Project, mut playhead_ms: f32) -> usize {
    for (index, frame) in project.frames().iter().enumerate() {
        playhead_ms -= frame.duration_ms();
        if playhead_ms < 0.0 {
            return index;
        }
    }
    project.frame_count().saturating_sub(1)
}

/// Contenuto della sezione "Anteprima" del dock: player grande, fps e
/// play/pausa. La striscia dei frame resta nella sezione "Frame".
pub fn preview_content(ui: &mut egui::Ui, app: &mut PixelApp) {
    let shown = app.preview.shown_frame;
    let rev = app.content_rev();
    let ctx = ui.ctx().clone();
    let project = app.project.as_ref();
    ensure_preview_texture(&ctx, &mut app.preview, project, shown, rev);
    let preview_texture = app.preview.texture.clone();
    let playing = app.preview.playing;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let label = if playing { "Pausa" } else { "Riproduci" };
        if ui.button(label).clicked() {
            app.preview.toggle();
        }
        let mut fps = app.project.as_ref().map_or(12.0, |p| p.fps());
        let fps_edit = ui.add(
            egui::DragValue::new(&mut fps)
                .range(1.0..=60.0)
                .suffix(" fps"),
        );
        if fps_edit.changed()
            && let Some(project) = app.project.as_mut()
        {
            project.set_fps(fps);
            app.mark_dirty();
        }
    });
    ui.add_space(6.0);

    if let Some(tex) = preview_texture {
        let dims = tex.size_vec2();
        let scale = (PREVIEW_SIZE / dims.max_elem()).min(3.0);
        let size = dims * scale;
        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());
        draw_player_backdrop(ui, rect);
        ui.painter().image(
            tex.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    let frame_count = app.project.as_ref().map_or(0, Project::frame_count);
    let stato = if playing { "in riproduzione" } else { "attivo" };
    ui.label(
        egui::RichText::new(format!(
            "frame {}/{} — {stato}",
            app.preview.shown_frame(),
            frame_count
        ))
        .small()
        .color(WEAK_TEXT),
    );
}

/// Anteprima a schermo intero (modale): niente menu n pannelli, il
/// player occupa tutto lo schermo e `Esc`/click chiude (gestito in app).
pub fn fullscreen_preview(app: &mut PixelApp, root: &mut egui::Ui) {
    let ctx = root.ctx().clone();
    let shown = app.preview.shown_frame;
    let rev = app.content_rev();
    let project = app.project.as_ref();
    ensure_preview_texture(&ctx, &mut app.preview, project, shown, rev);
    let preview_texture = app.preview.texture.clone();
    let playing = app.preview.playing;

    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(CANVAS_BG))
        .show(root, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(28.0);
                if let Some(tex) = preview_texture {
                    let available = ui.available_size();
                    let dims = tex.size_vec2();
                    let scale = (available.x * 0.92 / dims.x)
                        .min(available.y * 0.80 / dims.y);
                    let size = dims * scale;
                    let (rect, _response) =
                        ui.allocate_exact_size(size, egui::Sense::hover());
                    draw_player_backdrop(ui, rect);
                    ui.painter().image(
                        tex.id(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.weak("nessun progetto da mostrare");
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let label = if playing { "Pausa" } else { "Riproduci" };
                    if ui.button(label).clicked() {
                        app.preview.toggle();
                    }
                    if ui.button("Esci (Esc)").clicked() {
                        app.fullscreen_preview = false;
                    }
                });
                ui.label(
                    egui::RichText::new(format!(
                        "frame {}/{}",
                        app.preview.shown_frame(),
                        app.project.as_ref().map_or(0, Project::frame_count)
                    ))
                    .color(WEAK_TEXT),
                );
            });
        });
}

/// Ricrea la texture dell'anteprima solo se cambia il frame mostrato, il
/// contenuto dei pixel (`rev`) o il progetto stesso.
fn ensure_preview_texture(
    ctx: &egui::Context,
    state: &mut PreviewState,
    project: Option<&Project>,
    shown: usize,
    rev: u64,
) {
    let Some(frame) = project.and_then(|p| p.frame(shown)) else {
        state.texture = None;
        state.built_frame = None;
        return;
    };
    if state.built_frame == Some(shown) && state.built_rev == rev && state.texture.is_some() {
        return;
    }
    let grid = frame.grid();
    let size = [
        usize::try_from(grid.width()).unwrap_or(0),
        usize::try_from(grid.height()).unwrap_or(0),
    ];
    let bytes: Vec<u8> = grid.pixels().iter().flat_map(|c| c.channels()).collect();
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &bytes);
    // Stesso id: aggiorna la texture GPU esistente senza allocarne una nuova.
    match state.texture.as_mut() {
        Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
        None => {
            let name = format!("preview_frame_{shown}");
            state.texture = Some(ctx.load_texture(name, image, egui::TextureOptions::NEAREST));
        }
    }
    state.built_frame = Some(shown);
    state.built_rev = rev;
}

/// Thumbnail di un frame: sfondo scuro, contorno pi corposo se attivo.
pub fn draw_frame_thumb(ui: &egui::Ui, rect: egui::Rect, grid: &Grid, is_active: bool) {
    let painter = ui.painter();
    let bg = if is_active {
        ACCENT.gamma_multiply(0.20)
    } else {
        CHECKER_B
    };
    painter.rect_filled(rect, 4.0, bg);
    let stroke = egui::Stroke::new(
        if is_active { 2.0 } else { 0.5 },
        if is_active { ACCENT } else { crate::ui::GRID_LINE },
    );
    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Middle);
    let (w, h) = grid.dimensions();
    let cell = egui::vec2(rect.width() / w as f32, rect.height() / h as f32);
    for (index, color) in grid.pixels().iter().enumerate() {
        if color.is_transparent() {
            continue;
        }
        let x = index % w as usize;
        let y = index / w as usize;
        let cell_rect = egui::Rect::from_min_size(
            rect.min + egui::vec2(x as f32, y as f32) * cell,
            cell,
        );
        let [r, g, b, a] = color.channels();
        painter.rect_filled(cell_rect, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
    }
}

/// Sfondo del player: scacchiera neutra sotto i pixel trasparenti.
fn draw_player_backdrop(ui: &egui::Ui, rect: egui::Rect) {
    let cell = 8.0;
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            let color = if (x + y) % 2 == 0 { CHECKER_A } else { CHECKER_B };
            let piece = egui::Rect::from_min_size(
                rect.min + egui::vec2(x as f32, y as f32) * cell,
                egui::vec2(cell, cell),
            );
            ui.painter().rect_filled(piece, 0.0, color);
        }
    }
}
