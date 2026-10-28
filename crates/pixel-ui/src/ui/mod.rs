//! Moduli dell'interfaccia e tema visivo dell'app.

pub mod canvas;
pub mod menu_bar;
pub mod preview;

use eframe::egui;

// ——— tema "officina": superfici carbone-prugna, un solo accento ambra ——
// Il resto resta neutro: la scacchiera e il progetto sono i protagonisti;
// l'accento segna solo selezione, stato live e cursore.

/// Accento (selezione, avvisi live, cursore).
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0xE8, 0xB4, 0x50);
/// Sfondo intorno al canvas.
pub const CANVAS_BG: egui::Color32 = egui::Color32::from_rgb(0x1B, 0x18, 0x21);
/// Sfondo di pannelli e finestre.
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0x25, 0x21, 0x2E);
/// Colore del testo principale.
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(0xEC, 0xE9, 0xF0);
/// Testo attenuato (etichette secondarie).
pub const WEAK_TEXT: egui::Color32 = egui::Color32::from_rgb(0xA8, 0xA1, 0xB8);
/// Le due tinte della scacchiera di trasparenza (neutre, non bianche).
pub const CHECKER_A: egui::Color32 = egui::Color32::from_rgb(0x3A, 0x36, 0x45);
pub const CHECKER_B: egui::Color32 = egui::Color32::from_rgb(0x2D, 0x29, 0x37);
/// Reticolo fra i pixel (visibile a zoom alto).
pub const GRID_LINE: egui::Color32 = egui::Color32::from_rgb(0x51, 0x4B, 0x5F);

/// Applica il tema visivo al contesto egui (una volta sola, all'avvio).
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = egui::Style::default();
    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL_BG;
    visuals.window_fill = PANEL_BG;
    visuals.extreme_bg_color = CANVAS_BG;
    visuals.faint_bg_color = CHECKER_B;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.30);
    visuals.selection.stroke = egui::Stroke::new(1.5, ACCENT);
    ctx.set_global_style(style);
}
