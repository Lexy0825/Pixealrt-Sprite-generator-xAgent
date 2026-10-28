//! Barra dei menu in alto e finestre di dialogo collegate.

use eframe::egui;

use crate::app::PixelApp;
use crate::ui::canvas::{ZOOM_MAX, ZOOM_MIN};
use crate::ui::WEAK_TEXT;

pub fn show(app: &mut PixelApp, root: &mut egui::Ui, ctx: &egui::Context) {
    egui::Panel::top("menu_bar").show(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            show_file_menu(app, ui, ctx);
            show_view_menu(app, ui);
            show_animation_menu(app, ui);
            show_ai_menu(app, ui);

            // Nome progetto a destra, con punto se ci sono modifiche pendenti.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(project) = app.project.as_ref() {
                    let marker = if app.is_dirty() { " •" } else { "" };
                    ui.label(
                        egui::RichText::new(format!("{}{marker}", project.name()))
                            .small()
                            .color(WEAK_TEXT),
                    );
                }
            });
        });
    });

    show_new_project_dialog(app, ctx);
    show_ai_instructions(ctx, app.ai_instructions_open);

    // Tasti rapidi globali.
    let (kbd_new, kbd_open, kbd_save) = ctx.input(|i| {
        let command = i.modifiers.command;
        (
            command && i.key_pressed(egui::Key::N),
            command && i.key_pressed(egui::Key::O),
            command && i.key_pressed(egui::Key::S),
        )
    });
    if kbd_open {
        app.action_open();
    }
    if kbd_save {
        app.save_if_path();
    }
    if kbd_new {
        app.new_dialog.open = true;
    }
}

fn show_file_menu(app: &mut PixelApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.menu_button("File", |ui| {
        if ui.button("Nuovo…").clicked() {
            app.new_dialog.open = true;
        }
        if ui.button("Apri…").clicked() {
            app.action_open();
        }
        ui.separator();
        if ui.button("Salva").clicked() {
            app.save_if_path();
        }
        if ui.button("Salva come…").clicked() {
            app.action_save_as();
        }
        ui.separator();
        if ui.button("Esporta PNG (frame attivo)…").clicked() {
            app.action_export_png();
        }
        if ui.button("Esporta ZIP (tutti i frame)…").clicked() {
            app.action_export_zip();
        }
        ui.separator();
        if ui.button("Esci").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

fn show_view_menu(app: &mut PixelApp, ui: &mut egui::Ui) {
    ui.menu_button("Aspetto", |ui| {
        ui.checkbox(&mut app.show_checker, "Scacchiera di trasparenza");
        ui.checkbox(&mut app.show_grid, "Griglia fra i pixel");
        ui.separator();
        if ui.button("Zoom +").clicked() {
            app.zoom = (app.zoom * 1.25).min(ZOOM_MAX);
        }
        if ui.button("Zoom −").clicked() {
            app.zoom = (app.zoom / 1.25).max(ZOOM_MIN);
        }
        if ui.button("Ripristina vista").clicked() {
            app.zoom = 14.0;
            app.pan = egui::Vec2::ZERO;
        }
    });
}

fn show_animation_menu(app: &mut PixelApp, ui: &mut egui::Ui) {
    ui.menu_button("Animazione", |ui| {
        let label = if app.preview.playing { "Pausa" } else { "Riproduci" };
        if ui.button(label).clicked() {
            app.preview.toggle();
        }
        ui.separator();
        let mut fps = app.project.as_ref().map_or(12.0, |p| p.fps());
        let fps_edit = ui.add(
            egui::DragValue::new(&mut fps)
                .range(10.0..=60.0)
                .suffix(" fps"),
        );
        if fps_edit.changed()
            && let Some(project) = app.project.as_mut()
        {
            project.set_fps(fps);
            app.mark_dirty();
        }
        ui.separator();
        // Aggiungi / duplica / elimina frame, con il progetto mutato una
        // volta sola per bottone.
        if ui.button("Aggiungi frame vuoto").clicked() {
            if let Some(project) = app.project.as_mut() {
                let _index = project.add_frame(false);
                app.mark_dirty();
            }
        }
        if ui.button("Duplica frame attivo").clicked() {
            if let Some(project) = app.project.as_mut() {
                let _index = project.add_frame(true);
                app.mark_dirty();
            }
        }
        if ui.button("Elimina frame attivo").clicked() {
            if let Some(project) = app.project.as_mut() {
                let index = project.active_frame_index();
                if project.remove_frame(index) {
                    app.mark_dirty();
                }
            }
        }
        ui.separator();
        if ui.button("Frame precedente").clicked() {
            if let Some(project) = app.project.as_mut()
                && project.active_frame_index() > 0
            {
                let index = project.active_frame_index() - 1;
                let _ = project.set_active_frame(index);
                app.mark_dirty();
            }
        }
        if ui.button("Frame successivo").clicked() {
            if let Some(project) = app.project.as_mut()
                && project.active_frame_index() + 1 < project.frame_count()
            {
                let index = project.active_frame_index() + 1;
                let _ = project.set_active_frame(index);
                app.mark_dirty();
            }
        }
    });
}

fn show_ai_menu(app: &mut PixelApp, ui: &mut egui::Ui) {
    ui.menu_button("AI Agent", |ui| {
        ui.label(
            egui::RichText::new(match &app.path {
                Some(path) => format!("file condiviso: {}", path.display()),
                None => "progetto non ancora salvato su file: usa Salva come…".to_owned(),
            })
            .weak(),
        );
        ui.separator();
        if ui.button("Come collegare l'agente AI…").clicked() {
            app.ai_instructions_open = true;
        }
    });
}

/// Finestra modale per creare un progetto nuovo con dimensioni scelte.
fn show_new_project_dialog(app: &mut PixelApp, ctx: &egui::Context) {
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
                    numeric_field(ui, &mut app.new_dialog.width, 1, 256);
                    ui.end_row();
                    ui.label("Altezza");
                    numeric_field(ui, &mut app.new_dialog.height, 1, 256);
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

/// Finestra con le istruzioni per collegare il server MCP a un client AI.
fn show_ai_instructions(ctx: &egui::Context, mut open: bool) {
    egui::Window::new("Collega l'agente AI")
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.label("Il server `pixel-mcp` parla MCP su stdio: il client AI lo avvia come processo figlio e ottiene i tool per creare progetti, disegnare, leggere lo stato ed esportare.");
            ui.add_space(8.0);
            ui.label("1. Compila il server:");
            code_block(ui, "cargo run --release -p pixel-mcp");
            ui.add_space(4.0);
            ui.label("2. Aggiungi al file di configurazione MCP del client (esempio):");
            code_block(
                ui,
                r#"{
  "mcpServers": {
    "pixel-studio": {
      "command": "pixel-mcp",
      "args": []
    }
  }
}"#,
            );
            ui.add_space(4.0);
            ui.label("3. Riavvia il client AI: compariranno i tool `create_project`, `set_pixel`, `fill_rect`, `get_canvas`, `export_zip` e gli altri.");
        });
}

/// Blocco di testo monospazio (ed è già copiabile, come ogni campo egui).
fn code_block(ui: &mut egui::Ui, text: &str) {
    let mut editable = text.to_owned();
    ui.add(
        egui::TextEdit::multiline(&mut editable)
            .font(egui::TextStyle::Monospace)
            .desired_rows(text.lines().count() + 1)
            .clip_text(true),
    );
}
