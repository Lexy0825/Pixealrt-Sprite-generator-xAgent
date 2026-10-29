//! Dock destro a sezioni: schede laterali richiamabili a tendina.
//!
//! Una colonna di schede sempre visibile sul bordo destro
//! apre la sezione corrispondente sul pannello laterale: "Frame", "Colori",
//! "Anteprima". Ogni sezione si chiude con il pulsante X o rieffettando un
//! click sulla sua scheda: il canvas si allarga a tutta finestra.

use eframe::egui;

use crate::app::{PixelApp, Section};
use crate::ui::preview;
use crate::ui::{ACCENT, CHECKER_B, WEAK_TEXT};

/// Larghezza della striscia delle schede laterali.
const RAIL_WIDTH: f32 = 30.0;
/// Larghezza iniziale del pannello di sezione.
const SECTION_SIZE: f32 = 262.0;

pub fn show(app: &mut PixelApp, root: &mut egui::Ui, _ctx: &egui::Context) {
    // 1) Striscia delle schede.
    egui::Panel::right("dock_rail")
        .exact_size(RAIL_WIDTH)
        .show(root, |ui| {
            ui.add_space(6.0);
            for section in [Section::Frames, Section::Colors, Section::Preview] {
                rail_tab(ui, app, section);
                ui.add_space(6.0);
            }
        });

    // 2) Pannello della sezione aperta (se c'e').
    if let Some(section) = app.section {
        let title = match section {
            Section::Frames => "Frame",
            Section::Colors => "Colori",
            Section::Preview => "Anteprima",
        };
        egui::Panel::right("dock_section")
            .default_size(SECTION_SIZE)
            .resizable(true)
            .min_size(200.0)
            .show(root, |ui| {
                header(ui, title, app);
                ui.separator();
                match section {
                    Section::Frames => frames_content(ui, app),
                    Section::Colors => crate::ui::colors::colors_content(ui, app),
                    Section::Preview => preview::preview_content(ui, app),
                }
            });
    }
}

/// Scheda verticale della striscia: click apre (o chiude, se gia' aperta).
fn rail_tab(ui: &mut egui::Ui, app: &mut PixelApp, section: Section) {
    let open = app.section == Some(section);
    let label = match section {
        Section::Frames => "Frame",
        Section::Colors => "Color",
        Section::Preview => "Prev",
    };
    let tooltip = match section {
        Section::Frames => "Frame e animazione",
        Section::Colors => "Selezione colori",
        Section::Preview => "Anteprima in movimento",
    };
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(RAIL_WIDTH - 6.0, 60.0),
        egui::Sense::click(),
    );
    let bg = if open {
        ACCENT.gamma_multiply(0.25)
    } else if response.hovered() {
        CHECKER_B
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 4.0, bg);
    // Etichetta verticale: un carattere per riga.
    let color = if open { ACCENT } else { WEAK_TEXT };
    let chars: Vec<char> = label.chars().collect();
    let step = 12.0;
    let start_y = rect.center().y - (chars.len() as f32 - 1.0) * step * 0.5;
    for (index, ch) in chars.iter().enumerate() {
        ui.painter().text(
            egui::pos2(rect.center().x, start_y + index as f32 * step),
            egui::Align2::CENTER_CENTER,
            ch.to_string(),
            egui::FontId::proportional(12.0),
            color,
        );
    }
    if response.clicked() {
        app.section = if open { None } else { Some(section) };
    }
    response.on_hover_text(finalize(tooltip, open));
}

/// Testo del tooltip dinamico (apertura/chiusura).
fn finalize(tooltip: &str, open: bool) -> String {
    if open {
        format!("{tooltip} - clic per chiudere")
    } else {
        format!("{tooltip} - clic per aprire")
    }
}

/// Intestazione della sezione con pulsante di chiusura.
fn header(ui: &mut egui::Ui, title: &str, app: &mut PixelApp) {
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new(egui::RichText::new("x").small()))
                .on_hover_text("Chiudi la sezione")
                .clicked()
            {
                app.section = None;
            }
        });
    });
}

// ——— sezione Frame: tasti rapidi + lista verticale ———————————————

/// Lato della thumbnail nella lista dei frame.
const FRAME_THUMB: f32 = 104.0;

/// Contenuto della sezione Frame: comandi rapidi e file dei frame
/// verticale, una riga per frame, thumbnail grande e numero a fianco.
fn frames_content(ui: &mut egui::Ui, app: &mut PixelApp) {
    // ——— tasti rapidi sul frame attivo ————————————————————————
    ui.horizontal_wrapped(|ui| {
        if ui.add(egui::Button::new("+ nuovo")).on_hover_text("Aggiunge un frame vuoto").clicked() {
            app.frame_add(false);
        }
        if ui.add(egui::Button::new("+ duplica")).on_hover_text("Duplica il frame attivo").clicked() {
            app.frame_add(true);
        }
        if ui.add(egui::Button::new("- elimina")).on_hover_text("Elimina il frame attivo").clicked() {
            app.frame_delete_active();
        }
    });
    ui.horizontal(|ui| {
        if ui.add(egui::Button::new("prima")).on_hover_text("Sposta il frame attivo indietro di uno").clicked() {
            app.frame_move(-1);
        }
        if ui.add(egui::Button::new("dopo")).on_hover_text("Sposta il frame attivo avanti di uno").clicked() {
            app.frame_move(1);
        }
        let playing = if app.preview.playing { "in riproduzione" } else { "in pausa" };
        ui.weak(egui::RichText::new(format!("{} frame, {}", app.project.as_ref().map_or(0, |p| p.frame_count()), playing)).small());
    });
    ui.separator();

    // ——— lista verticale: una riga per frame ————————————————————
    let active = app.project.as_ref().map_or(0, |p| p.active_frame_index());
    let project = app.project.clone();
    if let Some(project) = project {
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for (index, frame) in project.frames().iter().enumerate() {
                frame_row(ui, app, index, frame, index == active);
                ui.add_space(4.0);
            }
        });
    } else {
        ui.weak("nessun progetto aperto");
    }
}

/// Riga di un frame: numero, thumbnail cliccabile, durata e mini-stat.
fn frame_row(
    ui: &mut egui::Ui,
    app: &mut PixelApp,
    index: usize,
    frame: &pixel_core::Frame,
    active: bool,
) {
    ui.horizontal(|ui| {
        // Numero in colonna a sinistra.
        ui.label(
            egui::RichText::new(index.to_string()).strong().color(
                if active { ACCENT } else { WEAK_TEXT },
            ),
        );
        // Thumbnail: click = seleziona.
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(FRAME_THUMB, FRAME_THUMB),
            egui::Sense::click(),
        );
        preview::draw_frame_thumb(ui, rect, frame.grid(), active);
        if response.clicked() {
            app.select_frame(index);
        }
        response.on_hover_text(format!("frame {index}: clic per attivarlo"));
    });
}
