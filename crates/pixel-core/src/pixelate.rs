//! Pixellatore: riduce un'immagine qualunque alla griglia del progetto.
//!
//! L'immagine sorgente non viene mai deformata: viene adattata uniforme-
//! mente per stare tutta dentro la griglia (fit) e centrata; dello spazio
//! fuori (letterbox) la griglia non tocca nulla. Ogni cella della griglia
//! copre una regione continua dell'immagine: un ciclo a matrice percorre
//! le celle e per ciascuna determina il colore dominante:
//!
//! - solo i pixel visibili (alpha >= 128) partecipano al conteggio;
//! - i canali RGB sono quantizzati a 4 bit (bucket 16x16x16) e vince il
//!   bucket con pi occorrenze; il colore la media dei pixel del bucket,
//!   alpha compresa (i bordi anti-aliased non spezzano la cella);
//! - una cella senza pixel visibili va a trasparente.

use std::collections::HashMap;
use std::path::Path;

use crate::{color::Rgba, error::PixelError, grid::Grid};

/// Alpha minima perch un pixel della sorgente conti come visibile.
const OPAQUE_MIN: u8 = 128;

/// Esito della pixelazione, per l'editor e i tool MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelateSummary {
    /// Celle della griglia a cui  stato assegnato un colore dominante.
    pub cells_filled: usize,
    /// Celle coperte dall'immagine ma senza pixel visibili (trasparenti).
    pub cells_transparent: usize,
}

/// Adatta un'immagine sulla griglia e ne estrae i colori dominanti.
///
/// Scala uniforme `min(grid/img, 1)`: il fit riduce se l'immagine  pi
/// grande della griglia, ma non ingrandisce mai: un'immagine pi piccola
/// resta 1 cella = 1 pixel, centrata.
pub fn pixelate_grid(grid: &mut Grid, img: &image::RgbaImage) -> PixelateSummary {
    let (gw, gh) = grid.dimensions();
    let (iw, ih) = (img.width(), img.height());
    if iw == 0 || ih == 0 {
        return PixelateSummary { cells_filled: 0, cells_transparent: 0 };
    }
    let scale = (gw as f32 / iw as f32)
        .min(gh as f32 / ih as f32)
        .min(1.0);
    let off_x = (gw as f32 - iw as f32 * scale).max(0.0) * 0.5;
    let off_y = (gh as f32 - ih as f32 * scale).max(0.0) * 0.5;

    let mut filled = 0;
    let mut transparent = 0;
    for cy in 0..gh {
        for cx in 0..gw {
            // Regione sorgente della cella, in pixel immagine: [lo..hi).
            let lo_x = ((cx as f32 - off_x) / scale).max(0.0);
            let hi_x = ((cx as f32 + 1.0 - off_x) / scale).min(iw as f32);
            let lo_y = ((cy as f32 - off_y) / scale).max(0.0);
            let hi_y = ((cy as f32 + 1.0 - off_y) / scale).min(ih as f32);
            // Cella interamente fuori dall'immagine: letterbox, non toccarla.
            if hi_x <= lo_x || hi_y <= lo_y {
                continue;
            }
            let x0 = lo_x.floor() as u32;
            let x1 = (hi_x.ceil() as u32).min(iw).max(x0 + 1);
            let y0 = lo_y.floor() as u32;
            let y1 = (hi_y.ceil() as u32).min(ih).max(y0 + 1);

            match dominant_color(img, x0, y0, x1, y1) {
                Some(color) => {
                    grid.set_pixel(cx, cy, color);
                    filled += 1;
                }
                None => {
                    grid.set_pixel(cx, cy, Rgba::TRANSPARENT);
                    transparent += 1;
                }
            }
        }
    }
    PixelateSummary { cells_filled: filled, cells_transparent: transparent }
}

/// Colore dominante dei pixel `[x0..x1) x [y0..y1)` dell'immagine.
fn dominant_color(
    img: &image::RgbaImage,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
) -> Option<Rgba> {
    // chiave = RGB quantizzato a 4 bit; valore = (conteggio, somme r/g/b/a).
    let mut buckets: HashMap<(u8, u8, u8), (u32, u32, u32, u32, u32)> =
        HashMap::new();
    for py in y0..y1 {
        for px in x0..x1 {
            let [r, g, b, a] = img.get_pixel(px, py).0;
            if a < OPAQUE_MIN {
                continue;
            }
            let entry =
                buckets.entry((r >> 4, g >> 4, b >> 4)).or_insert((0, 0, 0, 0, 0));
            *entry = (
                entry.0 + 1,
                entry.1 + u32::from(r),
                entry.2 + u32::from(g),
                entry.3 + u32::from(b),
                entry.4 + u32::from(a),
            );
        }
    }
    let (_, &(count, sr, sg, sb, sa)) = buckets.iter().max_by_key(|(_, e)| e.0)?;
    let avg = |sum: u32| ((sum as f32 / count as f32).round() as u32).clamp(0, 255) as u8;
    Some(Rgba::rgba(avg(sr), avg(sg), avg(sb), avg(sa)))
}

/// Carica un'immagine da disco e la pixella sulla griglia.
pub fn pixelate_from_path(
    grid: &mut Grid,
    path: impl AsRef<Path>,
) -> Result<PixelateSummary, PixelError> {
    let img = image::open(path)?;
    Ok(pixelate_grid(grid, &img.to_rgba8()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;

    /// Immagine 4x4: rossa in alto, blu in basso, tutto opaco.
    fn half_red_half_blue() -> image::RgbaImage {
        let mut img = image::RgbaImage::new(4, 4);
        for y in 0..2 {
            for x in 0..4 {
                img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        for y in 2..4 {
            for x in 0..4 {
                img.put_pixel(x, y, image::Rgba([0, 0, 255, 255]));
            }
        }
        img
    }

    #[test]
    fn one_cell_one_pixel_when_same_size() {
        // Immagine 4x4 su griglia 4x4: mappatura 1:1, nessun letterbox.
        let mut grid = Grid::new(4, 4).unwrap();
        let summary = pixelate_grid(&mut grid, &half_red_half_blue());
        assert_eq!(summary.cells_filled, 16);
        assert_eq!(summary.cells_transparent, 0);
        for y in 0..2 {
            for x in 0..4 {
                assert_eq!(grid.get_pixel(x, y), Some(Rgba::rgb(255, 0, 0)));
            }
        }
        for y in 2..4 {
            for x in 0..4 {
                assert_eq!(grid.get_pixel(x, y), Some(Rgba::rgb(0, 0, 255)));
            }
        }
    }

    #[test]
    fn smaller_image_is_not_upscaled_and_letterbox_is_untouched() {
        // Immagine 2x2 su griglia 8x4: scale resta 1.0 (1 cella = 1 px),
        // centrata in (3..5, 1..3); il letterbox non viene toccato.
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, image::Rgba([255, 255, 0, 255]));

        let mut grid = Grid::new(8, 4).unwrap();
        grid.set_pixel(7, 0, Rgba::rgb(9, 9, 9)); // marker nel letterbox
        let summary = pixelate_grid(&mut grid, &img);

        assert_eq!(summary.cells_filled, 4);
        assert_eq!(grid.get_pixel(7, 0), Some(Rgba::rgb(9, 9, 9)));
        let colored = grid.pixels().iter().filter(|c| !c.is_transparent()).count();
        assert_eq!(colored, 5); // 4 dell'immagine + il marker
    }

    #[test]
    fn dominant_color_wins_over_noise() {
        // Cella unica con 3 bianchi e 1 nero: vince il bianco.
        let mut img = image::RgbaImage::new(2, 2);
        for (x, y) in [(0usize, 0usize), (1, 0), (0, 1), (1, 1)] {
            img.put_pixel(x as u32, y as u32, image::Rgba([255, 255, 255, 255]));
        }
        img.put_pixel(0, 1, image::Rgba([0, 0, 0, 255]));
        let mut grid = Grid::new(1, 1).unwrap();
        let summary = pixelate_grid(&mut grid, &img);
        assert_eq!(summary.cells_filled, 1);
        assert_eq!(grid.get_pixel(0, 0), Some(Rgba::rgb(255, 255, 255)));
    }

    #[test]
    fn fully_transparent_region_maps_to_transparent() {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 0]));
        let mut grid = Grid::new(4, 4).unwrap();
        let summary = pixelate_grid(&mut grid, &img);
        assert_eq!(summary.cells_filled, 0);
        assert_eq!(summary.cells_transparent, 16);
        assert!(grid.pixels().iter().all(|c| c.is_transparent()));
    }
}
