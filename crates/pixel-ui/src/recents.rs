//! Progetti recenti: la lista vive su disco e alimenta la Homepage.
//!
//! Ogni voce tiene il percorso del `.pixproj`, nome, dimensioni, data e la
//! thumbnail del frame attivo (PNG 64x64, rigenerata a ogni salvataggio).
//! La lista resta limitata a 12 voci, senza duplicati per percorso.

use std::fs;
use std::path::{Path, PathBuf};

use pixel_core::Project;
use serde::{Deserialize, Serialize};

/// Nome emotivo della sottocartella dati dell'app.
pub const DIR_NAME: &str = "PixelStudio";
/// Massimo di progetti ricordati.
const LIMIT: usize = 12;
const THUMB_SIZE: u32 = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Percorso del file progetto.
    pub path: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Percorso della thumbnail PNG; vuoto se non generata.
    #[serde(default)]
    pub thumb: String,
    /// Modifica del progetto in secondi Unix.
    pub updated: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    entries: Vec<Entry>,
}

/// Cartella dati: APPDATA\PixelStudio su Windows, ~/.pixel-studio altrove.
#[must_use]
pub fn data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return PathBuf::from(appdata).join(DIR_NAME);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(format!(".{}", DIR_NAME.to_lowercase()));
    }
    std::env::temp_dir().join(DIR_NAME)
}

fn recent_file() -> PathBuf {
    data_dir().join("recents.json")
}

/// Hash FNV-1a: nome stabile del file thumbnail per un percorso.
fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x00000100000001b3);
    }
    hash
}

fn thumbnails_dir() -> PathBuf {
    data_dir().join("thumbs")
}

/// Corrisponde ad avere lo stesso progetto (percorso canonico).
fn same_path(a: &str, b: &Path) -> bool {
    match fs::canonicalize(b) {
        Ok(canon) => PathBuf::from(a) == canon || Path::new(a) == b,
        Err(_) => Path::new(a) == b,
    }
}

/// Legge la lista dei recenti; file assente o rotto = lista vuota.
#[must_use]
pub fn load() -> Vec<Entry> {
    let bytes = fs::read(recent_file()).ok();
    serde_json::from_slice::<RecentFile>(bytes.as_deref().unwrap_or(b"[]"))
        .map(|file| file.entries)
        .unwrap_or_default()
}

/// Scrive la lista (crea la cartella dati se manca).
fn save(entries: &[Entry]) {
    let dir = data_dir();
    let _ = fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(&RecentFile { entries: entries.to_vec() }) {
        let _ = fs::write(recent_file(), json);
    }
}

/// Genera la thumbnail PNG (64x64, fit) del frame attivo e ne ritorna il
/// percorso; `None` se la scrittura non riesce (non  un errore bloccante).
fn frame_thumbnail_path(project: &Project, project_path: &Path) -> Option<PathBuf> {
    let grid = project.active_frame().grid();
    let (w, h) = grid.dimensions();
    let mut img = image::RgbaImage::new(w, h);
    for (index, color) in grid.pixels().iter().enumerate() {
        let [r, g, b, a] = color.channels();
        img.put_pixel(
            (index % w as usize) as u32,
            (index / w as usize) as u32,
            image::Rgba([r, g, b, a]),
        );
    }
    // Fit dentro THUMB_SIZE, nearest: la pixel-art resta netta.
    let scale = (f64::from(THUMB_SIZE) / f64::from(w).max(1.0))
        .min(f64::from(THUMB_SIZE) / f64::from(h).max(1.0))
        .min(1.0);
    let out_w = ((f64::from(w) * scale).round() as u32).max(1);
    let out_h = ((f64::from(h) * scale).round() as u32).max(1);
    let thumb = image::imageops::resize(&img, out_w, out_h, image::imageops::FilterType::Nearest);
    let out_path = thumbnails_dir().join(format!("{fnv1a_path:x}.png", fnv1a_path = fnv1a(&project_path.to_string_lossy())));
    let _ = fs::create_dir_all(thumbnails_dir());
    thumb.save(&out_path).ok().map(|_| out_path)
}

/// Registra (o promuove a testa) un progetto nei recenti e aggiorna la
/// sua thumbnail. Silenzioso: i fallimenti non bloccano il salvataggio.
pub fn record(project: &Project, path: &Path) {
    let mut entries = load();
    entries.retain(|e| !same_path(&e.path, path));
    entries.insert(
        0,
        Entry {
            path: path.to_string_lossy().into_owned(),
            name: project.name().to_owned(),
            width: project.width(),
            height: project.height(),
            thumb: frame_thumbnail_path(project, path)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        },
    );
    entries.truncate(LIMIT);
    save(&entries);
}

/// `true` se il progetto  gi la testa dei recenti: evita riscritture
/// inutili ad ogni autosave.
pub fn is_front(path: &Path) -> bool {
    load().first().map(|e| same_path(&e.path, path)).unwrap_or(false)
}

/// Rimuove una voce dai recenti (per il pulsante della card).
pub fn remove(path: &str) {
    let mut entries = load();
    entries.retain(|e| e.path != path);
    save(&entries);
}

/// Data leggibile "GG mon AAAA · HH:MM" da `updated` (secondi Unix).
/// Conversione giorni-data con l'algoritmo di Howard Hinnant
/// (civil_from_days): nessuna dipendenza in pi.
#[must_use]
pub fn format_when(updated: u64) -> String {
    let days = i64::try_from(updated / 86_400).unwrap_or(0);
    let secs = updated % 86_400;
    let (hour, minute) = (secs / 3600, (secs % 3600) / 60);
    // Soviet algorithm civile di Howard Hinnant (civil_from_days):
    // z = giorni dal 1970-01-01 spostati nell'era, poi era/yoe/doy/mp.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1461 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    debug_assert!((1..=12).contains(&month), "mese fuori range: {month}");
    let year = yoe + era * 400 + i64::from(month <= 2);
    let months = ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago",
                  "set", "ott", "nov", "dic"];
    format!(
        "{day:02} {} {year} · {hour:02}:{minute:02}",
        months[(month - 1) as usize],
    )
}
