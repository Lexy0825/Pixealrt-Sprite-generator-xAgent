//! Canvas centrale: disegno, scacchiera di trasparenza, zoom e pan.

use eframe::egui;
use pixel_core::Rgba;

use crate::app::PixelApp;
use crate::ui::{ACCENT, CANVAS_BG, CHECKER_A, CHECKER_B, GRID_LINE, TEXT};

/// Zoom compreso fra 1 (1:1) e 32 ingrandimenti.
pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 32.0;
/// Sopra questa superficie (in celle) la scacchiera non viene disegnata:
/// protezione per progetti molto grandi.
const CHECKER_CELL_LIMIT: usize = 65_536;

pub fn show(app: &mut PixelApp, root: &mut egui::Ui, ctx: &egui::Context) {
    // CentralPanel per ultimo: copre lo spazio non occupato dai pannelli.
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(CANVAS_BG))
        .show(root, |ui| {
            let Some((width, height)) = app.project.as_ref().map(|p| (p.width(), p.height()))
            else {
                ui.centered_and_justified(|ui| {
                    ui.weak("Nessun progetto aperto: File → Nuovo…");
                });
                return;
            };

            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter().with_clip_rect(rect);

            // ——— zoom con la rotella, ancorato al pixel sotto il cursore ———————
            let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
            if response.hovered() && scroll != 0.0 {
                let new_zoom = (app.zoom * (1.0 + scroll * 0.002).clamp(0.75, 1.33))
                    .clamp(ZOOM_MIN, ZOOM_MAX);
                if let Some(pointer) = response.hover_pos() {
                    // Mantieni fermo il pixel che sta sotto il cursore.
                    let origin = view_origin(rect, width, height, app.zoom, app.pan);
                    let cell = ((pointer - origin) / app.zoom).floor();
                    let new_origin = pointer - cell * new_zoom;
                    let new_scaled = egui::vec2(width as f32 * new_zoom, height as f32 * new_zoom);
                    app.pan = new_origin + new_scaled * 0.5 - rect.center();
                }
                app.zoom = new_zoom;
            }

            // ——— pan col tasto centrale del mouse —————————————————————————————
            if response.hovered()
                && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle))
            {
                app.pan += ctx.input(|i| i.pointer.delta());
            }

            // ——— scacchiera sotto i pixel trasparenti ————————————————————————
            let zoom = app.zoom;
            let origin = view_origin(rect, width, height, zoom, app.pan);
            let cell_size = egui::vec2(zoom, zoom);
            if app.show_checker
                && (width as usize) * (height as usize) <= CHECKER_CELL_LIMIT
            {
                for y in 0..height {
                    for x in 0..width {
                        let color = if (x + y) % 2 == 0 {
                            CHECKER_A
                        } else {
                            CHECKER_B
                        };
                        painter.rect_filled(cell_rect(origin, cell_size, x, y), 0.0, color);
                    }
                }
            }

            // ——— il progetto corrente come texture con alpha reale ————————————
            let texture_id = app.canvas_texture(ctx).map(|t| t.id());
            if let Some(id) = texture_id {
                let image_rect = egui::Rect::from_min_size(
                    origin,
                    egui::vec2(width as f32 * zoom, height as f32 * zoom),
                );
                painter.image(
                    id,
                    image_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }

            // ——— reticolo fra i pixel, a zoom utile ————————————————————————————
            if app.show_grid && zoom >= 6.0 {
                let stroke = egui::Stroke::new(1.0, GRID_LINE);
                for x in 0..=width {
                    let px = origin.x + x as f32 * zoom;
                    painter.line_segment(
                        [egui::pos2(px, origin.y), egui::pos2(px, origin.y + height as f32 * zoom)],
                        stroke,
                    );
                }
                for y in 0..=height {
                    let py = origin.y + y as f32 * zoom;
                    painter.line_segment(
                        [egui::pos2(origin.x, py), egui::pos2(origin.x + width as f32 * zoom, py)],
                        stroke,
                    );
                }
            }

            // ——— disegno: tasto sinistro dipinge, destro cancella ——————————————
            let pixel = response
                .hover_pos()
                .map(|pos| ((pos - origin) / zoom).floor())
                .filter(|cell| cell.x >= 0.0 && cell.y >= 0.0)
                .map(|cell| (cell.x as u32, cell.y as u32));
            let pixel = pixel.filter(|(x, y)| *x < width && *y < height);

            // Evidenzia il pixel sotto il cursore.
            if let Some((x, y)) = pixel {
                painter.rect_stroke(
                    cell_rect(origin, cell_size, x, y),
                    0.0,
                    egui::Stroke::new(1.5, ACCENT),
                    egui::StrokeKind::Inside,
                );
            }

            let painting = response.dragged_by(egui::PointerButton::Primary)
                || response.clicked_by(egui::PointerButton::Primary);
            let erasing = ctx.input(|i| i.pointer.secondary_down());
            if painting {
                let color = if erasing { Rgba::TRANSPARENT } else { app.brush };
                if let Some(cell) = pixel {
                    app.paint_line(cell, color);
                }
            } else {
                app.last_paint_cell = None;
            }

            // ——— avviso "aggiornato dall'agente AI" quando il file cambia ————
            if app.has_external_notice() {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(12.0, 12.0),
                        egui::vec2(238.0, 26.0),
                    ),
                    3.0,
                    ACCENT.gamma_multiply(0.22),
                );
                painter.text(
                    rect.min + egui::vec2(20.0, 25.0),
                    egui::Align2::LEFT_CENTER,
                    "Progetto aggiornato dall'agente AI",
                    egui::FontId::proportional(13.0),
                    TEXT,
                );
            }
        });
}

/// Angolo in alto a sinistra della griglia, centrata nel rettangolo
/// disponibile più l'offset di pan dell'utente.
fn view_origin(
    rect: egui::Rect,
    width: u32,
    height: u32,
    zoom: f32,
    pan: egui::Vec2,
) -> egui::Pos2 {
    let scaled = egui::vec2(width as f32 * zoom, height as f32 * zoom);
    rect.center() - scaled * 0.5 + pan
}

/// Riquadro di schermo di una cella della griglia.
fn cell_rect(origin: egui::Pos2, cell: egui::Vec2, x: u32, y: u32) -> egui::Rect {
    egui::Rect::from_min_size(
        origin + egui::vec2(x as f32, y as f32) * cell,
        cell,
    )
}
