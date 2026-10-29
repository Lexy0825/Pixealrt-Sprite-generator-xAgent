//! Nav superiore degli strumenti, subito sotto la barra dei menu.
//!
//! Le icone sono disegnate con il painter (niente font emoji: il riso
//! su Windows/svg). Da sinistra: brand -> Home, penna, gomma, tratto
//! morbido, selettore rapido dei colori (popup recenti + palette),
//! zoom/griglia/scacchiera, riproduzione e anteprima a schermo intero.

use eframe::egui;
use pixel_core::Rgba;

use crate::app::{BrushMode, PixelApp, Screen};
use crate::ui::canvas::{ZOOM_MAX, ZOOM_MIN};
use crate::ui::{ACCENT, CHECKER_B, GRID_LINE, PANEL_BG, TEXT};

/// Lato di un pulsante a icona.
const ICON: f32 = 28.0;

pub fn show(app: &mut PixelApp, root: &mut egui::Ui, ctx: &egui::Context) {
    egui::Panel::top("tool_bar").show(root, |ui| {
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            // Brand: clic che torna alla homepage.
            if ui
                .add(egui::Button::new(egui::RichText::new("Pixel Studio").small()))
                .clicked()
            {
                go_home(app);
            }
            ui.separator();

            // ——— strumenti di disegno —————————————————————————————————
            if icon_button(ui, app.brush_mode == BrushMode::Pen, "Penna", pen_icon)
                .clicked()
            {
                app.brush_mode = BrushMode::Pen;
            }
            if icon_button(
                ui,
                app.brush_mode == BrushMode::Eraser,
                "Gomma",
                eraser_icon,
            )
            .clicked()
            {
                app.brush_mode = BrushMode::Eraser;
            }
            // Tratto morbido: toggle a puntino.
            if icon_button(
                ui,
                app.brush_soft,
                "Tratto morbido: centro pieno, lati attenuati",
                soft_icon,
            )
            .clicked()
            {
                app.brush_soft = !app.brush_soft;
            }

            // Selettore rapido: quadretto del colore corrente, apre i recenti.
            quick_colors(app, ui, ctx);

            ui.separator();

            // ——— vista ————————————————————————————————————————————
            if text_button(ui, "+", "Zoom +").clicked() {
                app.zoom = (app.zoom * 1.25).min(ZOOM_MAX);
            }
            if text_button(ui, "-", "Zoom -").clicked() {
                app.zoom = (app.zoom / 1.25).max(ZOOM_MIN);
            }
            if text_button(ui, "Adatta", "Adatta il progetto alla finestra").clicked() {
                fit_view(app);
            }
            if icon_button(ui, app.show_grid, "Griglia fra i pixel", grid_icon).clicked() {
                app.show_grid = !app.show_grid;
            }
            if icon_button(
                ui,
                app.show_checker,
                "Scacchiera sotto la trasparenza",
                checker_icon,
            )
            .clicked()
            {
                app.show_checker = !app.show_checker;
            }

            ui.separator();

            // ——— animazione ———————————————————————————————————————
            if app.preview.playing {
                if icon_button(ui, false, "Pausa", pause_icon).clicked() {
                    app.preview.toggle();
                }
            } else if icon_button(ui, false, "Riproduci", play_icon).clicked() {
                app.preview.toggle();
            }
            if text_button(ui, "Full", "Anteprima a schermo intero").clicked() {
                app.fullscreen_preview = true;
            }
        });
    });
}

pub fn go_home(app: &mut PixelApp) {
    app.screen = Screen::Home;
}

/// Pulsante quadrato a icona disegnata; `active` lo evidenzia d'accento.
fn icon_button(
    ui: &mut egui::Ui,
    active: bool,
    tooltip: &str,
    icon: fn(&egui::Painter, egui::Rect),
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ICON, ICON), egui::Sense::click());
    let bg = if active {
        ACCENT.gamma_multiply(0.25)
    } else if response.hovered() {
        CHECKER_B
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 4.0, bg);
    let stroke = if active {
        egui::Stroke::new(1.4, ACCENT)
    } else {
        egui::Stroke::NONE
    };
    ui.painter().rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Middle);
    icon(ui.painter(), rect.shrink2(egui::vec2(7.0, 6.0)));
    response.on_hover_text(tooltip)
}

/// Pulsante testuale compatto (zoom, adatta, full).
fn text_button(ui: &mut egui::Ui, label: &str, tooltip: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(label).small())).on_hover_text(tooltip)
}

// ——— icone disegnate a mano (vector, niente font) ——————————————————

/// Penna: segmento diagonale con punta.
fn pen_icon(p: &egui::Painter, r: egui::Rect) {
    p.line_segment(
        [egui::pos2(r.left(), r.bottom()), egui::pos2(r.right(), r.top())],
        egui::Stroke::new(2.4, TEXT),
    );
    p.circle_filled(egui::pos2(r.left(), r.bottom()), 2.0, TEXT);
}

/// Gomma: parallelogramma con mezza divisione.
fn eraser_icon(p: &egui::Painter, r: egui::Rect) {
    let body = egui::Rect::from_center_size(
        r.center(),
        egui::vec2(r.width() * 0.72, r.height() * 0.46),
    );
    p.rect_filled(body, 2.0, TEXT.gamma_multiply(0.9));
    p.line_segment(
        [body.left_center(), body.right_center()],
        egui::Stroke::new(1.6, PANEL_BG),
    );
}

/// Morbido: puntino d'accento con alone.
fn soft_icon(p: &egui::Painter, r: egui::Rect) {
    p.circle_filled(r.center(), 3.2, TEXT.gamma_multiply(0.45));
    p.circle_filled(r.center(), 1.6, ACCENT);
}

/// Griglia: quadrato a incroci.
fn grid_icon(p: &egui::Painter, r: egui::Rect) {
    let s = egui::Stroke::new(1.4, TEXT);
    p.line_segment([egui::pos2(r.center().x, r.top()), egui::pos2(r.center().x, r.bottom())], s);
    p.line_segment([egui::pos2(r.left(), r.center().y), egui::pos2(r.right(), r.center().y)], s);
}

/// Scacchiera: quattro quadretti alterni.
fn checker_icon(p: &egui::Painter, r: egui::Rect) {
    let half = r.center() - r.min;
    for (dx, dy, fill) in [
        (0.0, 0.0, true),
        (half.x, 0.0, false),
        (0.0, half.y, false),
        (half.x, half.y, true),
    ] {
        if fill {
            let cell = egui::Rect::from_min_size(r.min + egui::vec2(dx, dy), half);
            p.rect_filled(cell, 0.0, TEXT.gamma_multiply(0.8));
        }
    }
}

/// Play: triangolo.
fn play_icon(p: &egui::Painter, r: egui::Rect) {
    let c = r.center();
    let h = r.height() * 0.5;
    p.add(egui::Shape::convex_polygon(
        vec![egui::pos2(c.x - h * 0.6, c.y - h), egui::pos2(c.x - h * 0.6, c.y + h), egui::pos2(c.x + h * 0.9, c.y)],
        TEXT,
        egui::Stroke::NONE,
    ));
}

/// Pausa: due barrette.
fn pause_icon(p: &egui::Painter, r: egui::Rect) {
    let c = r.center();
    for dx in [-3.0_f32, 3.0] {
        p.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(c.x + dx - 1.6, r.top()),
                egui::vec2(3.2, r.height()),
            ),
            1.0,
            TEXT,
        );
    }
}

// ——— selettore rapido dei colori ——————————————————————————————————

/// Pulsantone col colore corrente; apre un popup snello con i recenti e
/// la palette del progetto (il pennellino grande resta nel dock Colori).
fn quick_colors(app: &mut PixelApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, ICON), egui::Sense::click());
    let painter = ui.painter();
    let [r, g, b, a] = app.brush.channels();
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
    let stroke = if app.quick_colors_open {
        egui::Stroke::new(1.6, ACCENT)
    } else {
        egui::Stroke::new(1.0, GRID_LINE)
    };
    painter.rect_stroke(rect, 4.0, stroke, egui::StrokeKind::Middle);
    if response.clicked() {
        app.quick_colors_open = !app.quick_colors_open;
    }
    let popup_anchor = response.rect.left_bottom() + egui::vec2(0.0, 4.0);
    response.on_hover_text("Colori recenti e palette del progetto");

    if !app.quick_colors_open {
        return;
    }
    let popup_id = egui::Id::unique("quick_colors_popup");
    egui::Area::new(popup_id)
        .order(egui::Order::Foreground)
        .fixed_pos(popup_anchor)
        .show(ctx, |ui| {
            egui::Frame::default()
                .fill(PANEL_BG)
                .stroke(egui::Stroke::new(1.0, GRID_LINE))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_min_width(236.0);
                    ui.strong("Recenti");
                    ui.add_space(2.0);
                    let recents = app.recent_colors.clone();
                    if recents.is_empty() {
                        ui.weak("nessun colore ancora usato");
                    } else if let Some(color) = swatch_grid(ui, &recents, app.brush) {
                        app.set_brush(color);
                    }
                    ui.add_space(6.0);
                    ui.strong("Palette del progetto");
                    let palette = app.palette_colors();
                    if palette.is_empty() {
                        ui.weak("disegna per riempirla");
                    } else if let Some(color) = swatch_grid(ui, &palette, app.brush) {
                        app.set_brush(color);
                    }
                    ui.add_space(4.0);
                    if ui
                        .add(egui::Button::new(egui::RichText::new("chiudi").small()))
                        .clicked()
                    {
                        app.quick_colors_open = false;
                    }
                });
        });
}

/// Griglia di swatch (auto 8 per riga); il colore cliccato torna come
/// `Some`. Il colore corrente porta il bordo d'accento.
fn swatch_grid(ui: &mut egui::Ui, colors: &[Rgba], current: Rgba) -> Option<Rgba> {
    const SWATCH: f32 = 20.0;
    const GAP: f32 = 3.0;
    let per_row = ((ui.available_width() / (SWATCH + GAP)).floor() as usize).max(6);
    let mut picked = None;
    for chunk in colors.chunks(per_row) {
        ui.horizontal(|ui| {
            for &color in chunk {
                let (rect, resp) = ui
                    .allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::click());
                let [r, g, b, a] = color.channels();
                ui.painter()
                    .rect_filled(rect, 3.0, egui::Color32::from_rgba_unmultiplied(r, g, b, a));
                let stroke = if color == current {
                    egui::Stroke::new(1.6, ACCENT)
                } else {
                    egui::Stroke::new(0.8, GRID_LINE)
                };
                ui.painter().rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
                if resp.clicked() {
                    picked = Some(color);
                }
            }
        });
    }
    picked
}

/// "Adatta": lo zoom degna il tutto nella superficie del canvas.
fn fit_view(app: &mut PixelApp) {
    let Some((width, height)) = app.project.as_ref().map(|p| (p.width(), p.height())) else {
        return;
    };
    let Some(view) = app.canvas_view_rect else {
        app.zoom = ZOOM_MIN; // il canvas non  ancora stato disegnato
        app.pan = egui::Vec2::ZERO;
        return;
    };
    app.zoom = (view.width() / width as f32)
        .min(view.height() / height as f32)
        .clamp(ZOOM_MIN, ZOOM_MAX);
    app.pan = egui::Vec2::ZERO;
}
