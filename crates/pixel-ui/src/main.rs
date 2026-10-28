//! `pixel-ui` — editor desktop per pixel-art e tileset (eframe/egui).
//!
//! Layout: barra dei menu in alto, canvas centrale con zoom/pan e
//! scacchiera di trasparenza, pannello destro con l'anteprima live
//! dell'animazione. Il progetto corrente vive in un file JSON condiviso
//! con il server MCP: le modifiche vengono auto-salvate con debounce, i
//! cambi esterni (dell'AI) vengono ricaricati automaticamente.

mod app;
mod ui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Pixel Studio")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "pixel-studio",
        options,
        Box::new(|cc| Ok(Box::new(app::PixelApp::new(cc)))),
    )
}
