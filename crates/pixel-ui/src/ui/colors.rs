//! Pannello "Colori": selettore a ruota + strumenti penna/gomma.
//!
//! Composizione:
//! - **ruota della tinta**: anello esterno per scegliere l'hue;
//! - **disco di saturazione**: il raggio è l'intensità del colore alla
//!   tinta corrente (centro desaturato, bordo saturaione piena);
//! - **slider di luminosità e opacità**: la seconda è la trasparenza del
//!   pennello (alpha 0 = invisibile);
//! - **palette del progetto**: colori distinti già presenti nei frame;
//! - **recenti**: gli ultimi colori impugnati in questa sessione;
//! - **penna/gomma + tratto morbido**: lo strumento che il canvas userà
//!   col pulsante sinistro (il destro resta gomma rapida).
//!
//! Lo stato HSV vive in `PixelApp::picker`: sopravvive ai pannelli e i
//! click su palette/recenti lo riallineano via `PixelApp::set_brush`.

use eframe::egui;
use pixel_core::Rgba;

use crate::app::{BrushMode, PixelApp};
use crate::ui::{ACCENT, GRID_LINE, TEXT};

/// Lato del widget ruota (anello + disco), in pixel.
const WHEEL_SIZE: f32 = 150.0;
/// Spessore dell'anello della tinta, come quota del raggio.
const RING_WIDTH: f32 = 0.34;
/// Lato di uno swatch (palette e recenti).
const SWATCH: f32 = 22.0;
/// Risoluzione dell'anello della tinta.
const RING_SEGMENTS: usize = 72;
/// Fasce concentriche del disco di saturazione.
const DISC_RINGS: usize = 6;
/// Raggi per fascia del disco.
const DISC_RAYS: usize = 24;

/// Stato del selettore: HSV + alpha, allineato con `PixelApp::brush`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorPickerState {
    /// Tinta in [0,1).
    pub hue: f32,
    /// Saturazione in [0,1].
    pub sat: f32,
    /// Luminosità (value) in [0,1].
    pub val: f32,
    /// Opacità in [0,1]: 0 = pennello completamente trasparente.
    pub alpha: f32,
}

impl ColorPickerState {
    /// Riallinea HSV e alpha a un colore RGB dato (palette, recenti).
    pub fn load_from(&mut self, color: Rgba) {
        let [r, g, b, a] = color.channels();
        (self.hue, self.sat, self.val) = rgb_to_hsv(r, g, b);
        self.alpha = f32::from(a) / 255.0;
    }

    /// Colore del pennello risultante dallo stato corrente.
    #[must_use]
    pub fn brush_color(self) -> Rgba {
        let (r, g, b) = hsv_to_rgb(self.hue, self.sat, self.val);
        let a = (self.alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        Rgba::rgba(r, g, b, a)
    }
}

/// Contenuto della sezione "Colori" del dock laterale.
pub fn colors_content(ui: &mut egui::Ui, app: &mut PixelApp) {
    ui.add_space(8.0);

            // Ruota e slider modificano lo stato del selettore; alla fine
            // il pennello adotta il colore risultante.
            let mut changed = wheel(ui, app);
            changed |= sliders(ui, app);
            if changed {
                app.brush = app.picker.brush_color();
            }

            ui.add_space(10.0);
            ui.strong("Palette del progetto");
            palette_section(ui, app);

            ui.add_space(8.0);
            ui.weak("Recenti");
            let recents = app.recent_colors.clone();
            if let Some(color) = swatch_row_pick(ui, &recents, app.brush) {
                app.set_brush(color);
            }

            ui.add_space(10.0);
            ui.strong("Pennello");
            ui.add_space(4.0);
            tool_row(ui, app);
            ui.checkbox(&mut app.brush_soft, "Morbido").on_hover_text(
                "Penna: centro a opacità piena, lati attenuati (0.7, diagonali 0.4).\n\
                 Gomma: centro cancellato, vicini attenuati.",
            );

            ui.add_space(6.0);
            let [r, g, b, a] = app.brush.channels();
            ui.monospace(
                egui::RichText::new(format!(
                    "#{r:02X}{g:02X}{b:02X} · {a:03}/255"
                ))
                .weak(),
            );
}

/// Ruota della tinta + disco di saturazione. Modifica `app.picker` in
/// reazione a click e drag; `true` se qualcosa è cambiato.
fn wheel(ui: &mut egui::Ui, app: &mut PixelApp) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(WHEEL_SIZE, WHEEL_SIZE),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter().clone();
    draw_wheel(&painter, rect, app.picker);

    let mut changed = false;
    if response.clicked() || response.dragged() {
        let Some(pointer) = response.interact_pointer_pos() else {
            return false;
        };
        let center = rect.center();
        let delta = pointer - center;
        let radius = delta.length();
        let outer = WHEEL_SIZE * 0.5;
        let inner = outer * (1.0 - RING_WIDTH);
        if radius >= inner - 3.0 {
            // Anello: l'angolo del puntatore (atan2, y in basso come egui)
            // diventa tinta, con lo stesso orientamento della mesh.
            let angle = f32::atan2(delta.y, delta.x);
            let hue = (angle / std::f32::consts::TAU + 1.0).fract();
            if (hue - app.picker.hue).abs() > 1e-4 {
                app.picker.hue = hue;
                changed = true;
            }
        } else {
            // Disco: il raggio diventa saturazione.
            let sat = (radius / inner.max(1.0)).clamp(0.0, 1.0);
            if (sat - app.picker.sat).abs() > 1e-4 {
                app.picker.sat = sat;
                changed = true;
            }
        }
    }
    changed
}

/// Disegna l'anello di tinta, il disco di saturazione e i cursori della
/// selezione corrente.
fn draw_wheel(painter: &egui::Painter, rect: egui::Rect, picker: ColorPickerState) {
    let center = rect.center();
    let outer = WHEEL_SIZE * 0.5;
    let inner = outer * (1.0 - RING_WIDTH);

    // ——— anello della tinta: mesh fra due circonferenze concentriche ———
    let mut ring = egui::Mesh::default();
    for k in 0..=RING_SEGMENTS {
        let hue = k as f32 / RING_SEGMENTS as f32;
        let dir = angle_dir(hue);
        // Tinta pura: saturation e luminosità al massimo.
        let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
        let color = egui::Color32::from_rgb(r, g, b);
        ring.colored_vertex(center + dir * outer, color);
        ring.colored_vertex(center + dir * inner, color);
        if k < RING_SEGMENTS {
            let a = (k * 2) as u32;
            let next = ((k + 1) * 2) as u32;
            ring.add_triangle(a, next, a + 1);
            ring.add_triangle(a + 1, next, next + 1);
        }
    }
    painter.add(ring);

    // ——— disco di saturazione alla tinta/luminosità correnti ———————————
    let disc_radius = (inner - 3.0).max(10.0);
    let mut disc = egui::Mesh::default();
    let color_at = |sat: f32| {
        let (r, g, b) = hsv_to_rgb(picker.hue, sat, picker.val);
        egui::Color32::from_rgb(r, g, b)
    };
    // Vertice centrale (sat 0) + fasce concentriche con raggio crescente.
    let center_idx = disc.vertices.len();
    disc.colored_vertex(center, color_at(0.0));
    let mut bases = vec![center_idx as u32];
    for ring in 1..=DISC_RINGS {
        let radius = disc_radius * ring as f32 / DISC_RINGS as f32;
        let sat = ring as f32 / DISC_RINGS as f32;
        bases.push(disc.vertices.len() as u32);
        for ray in 0..DISC_RAYS {
            let angle = ray as f32 / DISC_RAYS as f32 * std::f32::consts::TAU;
            let pos = center + egui::vec2(angle.cos(), angle.sin()) * radius;
            disc.colored_vertex(pos, color_at(sat));
        }
    }
    // Ventaglio: dal centro alla prima fascia, poi quad fra fasce
    // adiacenti. Le basi dei vertici sono nella `bases`, così gli indici
    // restano sempre in range (epaint la valida: fuori range = panic).
    for ray in 0..DISC_RAYS {
        let next = ((ray + 1) % DISC_RAYS) as u32;
        disc.add_triangle(bases[0], bases[1] + ray as u32, bases[1] + next);
        for ring in 1..DISC_RINGS {
            let prev_base = bases[ring];
            let this_base = bases[ring + 1];
            disc.add_triangle(
                prev_base + ray as u32,
                this_base + ray as u32,
                this_base + next,
            );
            disc.add_triangle(prev_base + ray as u32, this_base + next, prev_base + next);
        }
    }
    painter.add(disc);

    // ——— cursori: anellino sull'anello (tinta), cerchio nel disco (sat) ——
    let hue = (picker.hue + 1.0).fract();
    let ring_cursor = center + angle_dir(hue) * (inner + outer) * 0.5;
    painter.circle_stroke(ring_cursor, 5.0, egui::Stroke::new(1.5, TEXT));
    let sat_cursor = center + angle_dir(hue) * picker.sat * disc_radius;
    let contrast = if picker.val > 0.5 && picker.sat < 0.6 {
        egui::Color32::BLACK
    } else {
        TEXT
    };
    painter.circle_stroke(sat_cursor, 4.0, egui::Stroke::new(1.5, contrast));
}

/// Direzione dell'angolo, coerente con l'orientamento della mesh.
fn angle_dir(hue: f32) -> egui::Vec2 {
    let angle = hue * std::f32::consts::TAU;
    egui::vec2(angle.cos(), angle.sin())
}

/// Slider di luminosità e opacità; `true` se qualcosa è cambiato.
fn sliders(ui: &mut egui::Ui, app: &mut PixelApp) -> bool {
    let mut val = app.picker.val;
    let mut alpha = app.picker.alpha;
    let changed_val = ui
        .add(egui::Slider::new(&mut val, 0.0..=1.0).text("Luminosità"))
        .changed();
    let changed_alpha = ui
        .add(egui::Slider::new(&mut alpha, 0.0..=1.0).text("Opacità"))
        .on_hover_text("Alpha del pennello: 0% = trasparente totale.")
        .changed();
    app.picker.val = val;
    app.picker.alpha = alpha;
    changed_val || changed_alpha
}

/// File di swatch cliccabili; restituisce il colore scelto se un click è
/// avvenuto; il colore corrente è bordato d'accento.
fn swatch_row_pick(ui: &mut egui::Ui, colors: &[Rgba], current: Rgba) -> Option<Rgba> {
    // `swatch_click` usato come "click semplice": i recenti non sono
    // eliminabili col destro (nessun reale perduto: sono indicative).
    let mut picked = None;
    let per_row = ((ui.available_width() / (SWATCH + 4.0)).floor() as usize).max(3);
    for chunk in colors.chunks(per_row) {
        ui.horizontal(|ui| {
            for &color in chunk {
                if draw_swatch(ui, color, color == current) {
                    picked = Some(color);
                }
            }
        });
    }
    picked
}

/// Swatch: quadratino del colore con bordo; `true` al click.
fn draw_swatch(ui: &mut egui::Ui, color: Rgba, current: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::click());
    let [r, g, b, a] = color.channels();
    ui.painter().rect_filled(
        rect,
        3.0,
        egui::Color32::from_rgba_unmultiplied(r, g, b, a),
    );
    let stroke = if current {
        egui::Stroke::new(2.0, ACCENT)
    } else {
        egui::Stroke::new(1.0, GRID_LINE)
    };
    ui.painter()
        .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
    response
        .on_hover_text(format!(
            "#{r:02X}{g:02X}{b:02X} · opacità {:.0}%",
            f32::from(a) / 2.55
        ))
        .clicked()
}

/// Cursore tra penna e gomma: due selezioni in fila.
fn tool_row(ui: &mut egui::Ui, app: &mut PixelApp) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(app.brush_mode == BrushMode::Pen, "Penna")
            .clicked()
        {
            app.brush_mode = BrushMode::Pen;
        }
        if ui
            .selectable_label(app.brush_mode == BrushMode::Eraser, "Gomma")
            .clicked()
        {
            app.brush_mode = BrushMode::Eraser;
        }
    });
    ui.weak("tasto destro sul canvas è sempre gomma");
}

// ——— conversioni HSV ————————————————————————————————————————

/// HSV (componenti in [0,1]) → RGB byte. Canali in sRGB, stessa
/// convenzione di `Rgba`: si scala direttamente, niente gamma extra.
#[must_use]
pub fn hsv_to_rgb(hue: f32, sat: f32, val: f32) -> (u8, u8, u8) {
    let h = (hue + 1.0).fract() * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = val * (1.0 - sat);
    let q = val * (1.0 - sat * f);
    let t = val * (1.0 - sat * (1.0 - f));
    let (r, g, b) = match i % 6 {
        0 => (val, t, p),
        1 => (q, val, p),
        2 => (p, val, t),
        3 => (p, q, val),
        4 => (t, p, val),
        _ => (val, p, q),
    };
    (
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// RGB byte → HSV (componenti [0,1]).
#[must_use]
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = f32::from(r) / 255.0;
    let g = f32::from(g) / 255.0;
    let b = f32::from(b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta) % 6.0
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let hue = (hue / 6.0 + 1.0).fract();
    let sat = if max == 0.0 { 0.0 } else { delta / max };
    (hue, sat, max)
}

// ——— palette esplicita del progetto ——————————————————————————————————

/// Azione su uno swatch della palette: `true` = rimozione (destro).
fn swatch_click(ui: &mut egui::Ui, color: Rgba, current: bool) -> Option<bool> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(SWATCH, SWATCH), egui::Sense::click());
    let [r, g, b, a] = color.channels();
    let color32 = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
    ui.painter().rect_filled(rect, 3.0, color32);
    if response.secondary_clicked() {
        return Some(true);
    }
    let stroke = if current {
        egui::Stroke::new(2.0, ACCENT)
    } else {
        egui::Stroke::new(1.0, GRID_LINE)
    };
    ui.painter()
        .rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Middle);
    let hover = response.on_hover_text(format!(
        "#{r:02X}{g:02X}{b:02X} · opacità {:.0}%
click = usa, destro = toglie dalla palette",
        f32::from(a) / 2.55
    ));
    if hover.clicked() {
        Some(false)
    } else {
        None
    }
}

/// Sezione della palette esplicita: `+` aggiunge il pennello corrente;
/// click = usa il colore, destro = lo toglie dalla palette. Solo i
/// colori espliciti finiscono qui: mai il trasparente né i grigi del
/// falloff del pennello morbido.
fn palette_section(ui: &mut egui::Ui, app: &mut PixelApp) {
    if ui
        .add(egui::Button::new(egui::RichText::new("+ colore corrente").small()))
        .on_hover_text("Aggiunge il colore attuale alla palette del progetto")
        .clicked()
    {
        app.add_to_palette();
    }
    let palette = app.palette_colors();
    if palette.is_empty() {
        ui.weak("vuota: dipingi con un centro pieno per aggiungerla");
        return;
    }
    let mut picked = None;
    let per_row = ((ui.available_width() / (SWATCH + 4.0)).floor() as usize).max(3);
    for (chunk_index, chunk) in palette.chunks(per_row).enumerate() {
        ui.horizontal(|ui| {
            for (slot, &color) in chunk.iter().enumerate() {
                let index = chunk_index * per_row + slot;
                if let Some(remove) = swatch_click(ui, color, color == app.brush) {
                    picked = Some((index, remove));
                }
            }
        });
    }
    if let Some((index, remove)) = picked {
        if remove {
            app.remove_palette_color(index);
        } else {
            app.set_brush(palette[index]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{hsv_to_rgb, rgb_to_hsv};

    #[test]
    fn hsv_primaries_map_to_rgb_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0)); // rosso
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), (0, 255, 0)); // verde
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), (0, 0, 255)); // blu
        assert_eq!(hsv_to_rgb(0.5, 0.0, 1.0), (255, 255, 255)); // bianco
        assert_eq!(hsv_to_rgb(0.9, 1.0, 0.0), (0, 0, 0)); // nero
    }

    #[test]
    fn hue_wraps_around() {
        assert_eq!(hsv_to_rgb(-0.1, 1.0, 1.0), hsv_to_rgb(0.9, 1.0, 1.0));
    }

    #[test]
    fn rgb_to_hsv_roundtrips() {
        for (r, g, b) in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (232, 90, 62)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            assert_eq!(hsv_to_rgb(h, s, v), (r, g, b), "rgb {r},{g},{b}");
        }
    }

    #[test]
    fn gray_has_zero_saturation() {
        let (_h, s, v) = rgb_to_hsv(180, 180, 180);
        assert_eq!(s, 0.0);
        assert!((v - 180.0 / 255.0).abs() < 1e-3);
    }
}
