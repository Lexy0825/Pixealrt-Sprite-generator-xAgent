//! Homepage: progetti recenti in griglia di card + [ + nuovo progetto ].
//!
//! Da qui si riprende il lavoro (click sulla card) o si parte da zero;
//! le stesse azioni restano a portata di menu File nell'editor.

use eframe::egui;

use crate::app::{NewDialog, PixelApp};
use crate::recents;
use crate::ui::{ACCENT, CANVAS_BG, GRID_LINE, PANEL_BG, TEXT, WEAK_TEXT};

/// Lato di una card in griglia.
const CARD_W: f32 = 176.0;
const CARD_H: f32 = 196.0;

pub fn show(app: &mut PixelApp, root: &mut egui::Ui, ctx: &egui::Context) {
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(CANVAS_BG).inner_margin(egui::Margin::symmetric(0, 0)))
        .show(root, |ui| {
            ui.add_space(26.0);

            // ——— intestazione: brand + azione principale ————————————————
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.heading("Pixel Studio");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(32.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("+  nuovo progetto").strong(),
                            ),
                        )
                        .on_hover_text("Crea un progetto con nome e dimensioni scelte")
                        .clicked()
                    {
                        app.new_dialog = NewDialog::default();
                        app.new_dialog.open = true;
                    }
                    if ui.button("Apri…").clicked() {
                        app.action_open();
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(32.0);
                ui.label(
                    egui::RichText::new("Riprendi un progetto o creane uno nuovo")
                        .color(WEAK_TEXT),
                );
            });
            ui.add_space(18.0);

            // ——— griglia delle card dei recenti ——————————————————————————"7
            if app.home_entries.is_empty() {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Nessun progetto recente: crea il primo con «+ nuovo progetto»").color(WEAK_TEXT));
                return;
            }
            let entries = app.home_entries.clone();
            let columns = ((ui.available_width() - 32.0) / (CARD_W + 18.0)).floor().max(1.0) as usize;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("recenti_grid")
                    .spacing([18.0, 18.0])
                    .num_columns(columns.max(1))
                    .show(ui, |ui| {
                        let mut action = None;
                        for (index, entry) in entries.iter().enumerate() {
                            let act = card(ui, app, entry);
                            if act.is_some() {
                                action = act.map(|a| (entry, a));
                            }
                            if (index + 1) % columns == 0 {
                                ui.end_row();
                            }
                        }
                        ui.end_row();
                        if let Some((entry, act)) = action {
                            match act {
                                CardAction::Open => {
                                    let path = entry.path.clone();
                                    if !app.open_project_from_disk(std::path::Path::new(&path)) {
                                        // Il file non esiste pi: lo toglie dai recenti.
                                        recents::remove(&path);
                                        app.home_entries = recents::load();
                                        app.home_thumbs.clear();
                                    }
                                }
                                CardAction::Forget => {
                                    recents::remove(&entry.path);
                                    app.home_entries = recents::load();
                                    app.home_thumbs.clear();
                                }
                            }
                        }
                    });
            });
        });

    show_new_dialog(app, ctx);
}

/// Azione possibile su una card dei recenti.
enum CardAction {
    /// Riprendi il progetto.
    Open,
    /// Togli la voce (il file su disco non viene toccato).
    Forget,
}

/// Card di un progetto recente: thumbnail, nome, dimensioni, data.
fn card(
    ui: &mut egui::Ui,
    app: &mut PixelApp,
    entry: &recents::Entry,
) -> Option<CardAction> {
    // La response della card: click = apri; il pulsante "x" messo dopo
    // copre il suo angolo e prende priorit sul puntatore.
    let (rect, response) = ui.allocate_exact_size(egui::vec2(CARD_W, CARD_H), egui::Sense::click());
    let painter = ui.painter();

    // Container: pannello tondo con glow d'accento al passaggio.
    let hovered = rect.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default());
    painter.rect_filled(
        rect,
        6.0,
        if hovered { PANEL_BG.gamma_multiply(1.6) } else { PANEL_BG },
    );
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(if hovered { 1.4 } else { 1.0 }, if hovered { ACCENT } else { GRID_LINE }),
        egui::StrokeKind::Outside,
    );

    // Area dell'immagine (80% in alto).
    let inner = rect.shrink2(egui::vec2(6.0, 6.0));
    let image = egui::Rect::from_min_size(inner.min, egui::vec2(inner.width(), inner.height() * 0.74));
    let info = egui::Rect::from_min_size(
        egui::pos2(inner.min.x, image.max.y + 2.0),
        egui::vec2(inner.width(), inner.max.y - image.max.y - 2.0),
    );

    // Thumbnail: la texture  caricata una volta sola per percorso.
    let tex = load_thumbnail(ui, app, entry);
    if let Some(tex) = &tex {
        painter.image(tex.id(), image, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    } else {
        painter.rect_filled(image, 4.0, CANVAS_BG);
        painter.text(
            egui::pos2(image.center().x, image.center().y),
            egui::Align2::CENTER_CENTER,
            "pixel-art",
            egui::FontId::proportional(12.0),
            WEAK_TEXT,
        );
    }

    // Metadati: nome, dimensioni + data.
    painter.text(
        egui::pos2(info.min.x, info.min.y + 4.0),
        egui::Align2::LEFT_TOP,
        entry.name.clone(),
        egui::FontId::proportional(14.0),
        TEXT,
    );
    painter.text(
        egui::pos2(info.max.x, info.min.y + 4.0),
        egui::Align2::RIGHT_TOP,
        format!("{}x{}", entry.width, entry.height),
        egui::FontId::proportional(12.0),
        WEAK_TEXT,
    );
    painter.text(
        egui::pos2(info.min.x, info.max.y - 2.0),
        egui::Align2::LEFT_BOTTOM,
        recents::format_when(entry.updated),
        egui::FontId::proportional(12.0),
        WEAK_TEXT,
    );

    // Pulsante "x" (togli dai recenti) sopra la card: click = dimentica.
    let x_rect = egui::Rect::from_center_size(
        egui::pos2(rect.max.x - 12.0, rect.min.y + 12.0),
        egui::vec2(18.0, 18.0),
    );
    let x_button = ui.put(
        x_rect,
        egui::Button::new(egui::RichText::new("x").small())
            .fill(PANEL_BG.gamma_multiply(1.4))
            .stroke(egui::Stroke::new(0.8, GRID_LINE)),
    );
    if x_button.on_hover_text("Togli dai recenti").clicked() {
        return Some(CardAction::Forget);
    }
    if response.on_hover_text(format!("Apri {}", entry.name)).clicked() {
        return Some(CardAction::Open);
    }
    None
}

/// Texture della thumbnail: caricamento pigro con cache per percorso.
fn load_thumbnail(
    ui: &egui::Ui,
    app: &mut PixelApp,
    entry: &recents::Entry,
) -> Option<egui::TextureHandle> {
    if entry.thumb.is_empty() {
        return None;
    }
    if let Some(tex) = app.home_thumbs.get(&entry.thumb) {
        return Some(tex.clone());
    }
    let img = image::open(&entry.thumb).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ui
        .ctx()
        .load_texture("home_thumb", color, egui::TextureOptions::NEAREST);
    app.home_thumbs.insert(entry.thumb.clone(), tex.clone());
    Some(tex)
}

/// Finestra modale per creare un progetto nuovo con dimensioni scelte.
fn show_new_dialog(app: &mut PixelApp, ctx: &egui::Context) {
    let mut open = app.new_dialog.open;
    egui::Window::new("Nuovo progetto")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            egui::Grid::new("nuovo_progetto_campi")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Nome");
                    ui.text_edit_singleline(&mut app.new_dialog.name);
                    ui.end_row();
                    ui.label("Larghezza");
                    numeric_field(ui, &mut app.new_dialog.width, 1, 512);
                    ui.end_row();
                    ui.label("Altezza");
                    numeric_field(ui, &mut app.new_dialog.height, 1, 512);
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Crea").clicked() {
                    let width = app.new_dialog.width.trim().parse::<u32>();
                    let height = app.new_dialog.height.trim().parse::<u32>();
                    if let (Ok(width), Ok(height)) = (width, height) {
                        let name = std::mem::take(&mut app.new_dialog.name);
                        app.action_new(name, width, height);
                        app.new_dialog.open = false;
                    }
                }
                if ui.button("Annulla").clicked() {
                    app.new_dialog.open = false;
                }
            });
        });
    app.new_dialog.open = open;
}

/// Campo numerico tenuto come testo, modificato solo in cifre valide.
fn numeric_field(ui: &mut egui::Ui, text: &mut String, min: u32, max: u32) {
    let mut value = text.parse::<u32>().unwrap_or(32);
    let response = ui.add(egui::DragValue::new(&mut value).range(min..=max));
    if response.changed() {
        *text = value.to_string();
    }
}
