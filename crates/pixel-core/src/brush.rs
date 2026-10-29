//! Impronta del pennello sulla griglia: penna dura, morbida e gomma.
//!
//! Il tratto in sé vive in `pixel-ui` (interpolazione, stato del drag);
//! qui c'è solo il "colpo" su una cella e sulla sua corona di vicini:
//!
//! - **penna dura**: un solo pixel, sostituito col colore del pennello;
//! - **penna morbida**: il centro prende il colore pieno, i 4 vicini
//!   ortogonali lo stesso colore con alpha × [`SOFT_ORTHO`] e le 4
//!   diagonali con alpha × [`SOFT_DIAG`]; un vicino con alpha maggiore
//!   del falloff resta com'era, così il tocco non mangia il dettaglio
//!   già disegnato;
//! - **gomma**: il centro diventa trasparente; nella variante morbida i
//!   vicini vengono solo attenuati (alpha × [`ERASER_SOFT_ORTHO`] e
//!   × [`ERASER_SOFT_DIAG`]), tracciando un bordo sfumato.

use crate::color::Rgba;
use crate::grid::Grid;

/// Alpha dei vicini ortogonali in penna morbida (es. 255 → 179 ≈ 0.7).
pub const SOFT_ORTHO: f32 = 0.7;
/// Alpha delle diagonali in penna morbida.
pub const SOFT_DIAG: f32 = 0.4;
/// Gomma morbida: alpha residua dei vicini ortogonali.
pub const ERASER_SOFT_ORTHO: f32 = 0.3;
/// Gomma morbida: alpha residua delle diagonali.
pub const ERASER_SOFT_DIAG: f32 = 0.6;

/// Posizione di un vicino espresso come (dx, dy) in celle.
const ORTHO: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const DIAG: [(i32, i32); 4] = [(-1, -1), (1, -1), (-1, 1), (1, 1)];

/// Impronta della penna in `(x, y)`.
pub fn stamp(grid: &mut Grid, x: u32, y: u32, color: Rgba, soft: bool) {
    if !soft {
        grid.set_pixel(x, y, color);
        return;
    }
    // Centro: sostituzione diretta, come la penna dura.
    grid.set_pixel(x, y, color);
    let ortho = scaled_alpha(color, SOFT_ORTHO);
    let diag = scaled_alpha(color, SOFT_DIAG);
    for (nx, ny) in offsets(x, y, &ORTHO) {
        touch_if_stronger(grid, nx, ny, ortho);
    }
    for (nx, ny) in offsets(x, y, &DIAG) {
        touch_if_stronger(grid, nx, ny, diag);
    }
}

/// Cancello in `(x, y)`: il centro diventa completamente trasparente;
/// con `soft` anche i vicini vengono solo attenuati.
pub fn erase(grid: &mut Grid, x: u32, y: u32, soft: bool) {
    grid.set_pixel(x, y, Rgba::TRANSPARENT);
    if !soft {
        return;
    }
    for (nx, ny) in offsets(x, y, &ORTHO) {
        multiply_alpha(grid, nx, ny, ERASER_SOFT_ORTHO);
    }
    for (nx, ny) in offsets(x, y, &DIAG) {
        multiply_alpha(grid, nx, ny, ERASER_SOFT_DIAG);
    }
}

/// Vicini in-bounds di (x, y) per gli offset forniti.
fn offsets(x: u32, y: u32, list: &[(i32, i32)]) -> Vec<(u32, u32)> {
    list.iter()
        .filter_map(|(dx, dy)| {
            let nx = i64::from(x) + i64::from(*dx);
            let ny = i64::from(y) + i64::from(*dy);
            (nx >= 0 && ny >= 0).then(|| (nx as u32, ny as u32))
        })
        .collect()
}

/// Scrive `candidate` solo se l'alpha presente è più debole.
fn touch_if_stronger(grid: &mut Grid, x: u32, y: u32, candidate: Rgba) {
    if let Some(existing) = grid.get_pixel(x, y)
        && existing.alpha() < candidate.alpha()
    {
        grid.set_pixel(x, y, candidate);
    }
}

/// Riduce l'alpha del pixel esistente del fattore dato (canali RGB invariati).
fn multiply_alpha(grid: &mut Grid, x: u32, y: u32, factor: f32) {
    if let Some(existing) = grid.get_pixel(x, y) {
        let [r, g, b, a] = existing.channels();
        let faded = Rgba::rgba(r, g, b, scale_byte(a, factor));
        grid.set_pixel(x, y, faded);
    }
}

/// `color` con alpha ridotta del fattore; RGB invariati.
fn scaled_alpha(color: Rgba, factor: f32) -> Rgba {
    let [r, g, b, a] = color.channels();
    Rgba::rgba(r, g, b, scale_byte(a, factor))
}

/// Scala un canale u8 del fattore, portando in range senza fare overflow.
fn scale_byte(value: u8, factor: f32) -> u8 {
    (f32::from(value) * factor).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;

    const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    fn grid3() -> Grid {
        Grid::new(3, 3).expect("griglia 3×3 valida")
    }

    #[test]
    fn hard_stamp_touches_one_pixel() {
        let mut grid = grid3();
        stamp(&mut grid, 1, 1, WHITE, false);
        assert_eq!(grid.get_pixel(1, 1), Some(WHITE));
        assert!(grid.get_pixel(0, 1).unwrap().is_transparent());
        assert!(grid.get_pixel(1, 0).unwrap().is_transparent());
    }

    /// Il caso dell'utente: click bianco a piena opacità, i 4 lati a 0.7.
    #[test]
    fn soft_stamp_center_full_and_sides_dimmed() {
        let mut grid = grid3();
        stamp(&mut grid, 1, 1, WHITE, true);
        assert_eq!(grid.get_pixel(1, 1), Some(WHITE)); // alpha 1.0
        for (x, y) in [(0, 1), (2, 1), (1, 0), (1, 2)] {
            assert_eq!(grid.get_pixel(x, y), Some(Rgba::rgba(255, 255, 255, 179)));
        }
        // Le diagonali del falloff sono più tenui.
        for (x, y) in [(0, 0), (2, 0), (0, 2), (2, 2)] {
            assert_eq!(grid.get_pixel(x, y), Some(Rgba::rgba(255, 255, 255, 102)));
        }
    }

    /// Il falloff non distrugge il dettaglio già disegnato.
    #[test]
    fn soft_stamp_keeps_stronger_existing_neighbors() {
        let mut grid = grid3();
        let strong = Rgba::rgba(10, 200, 30, 220);
        stamp(&mut grid, 0, 1, strong, false); // pixel forte a fianco
        stamp(&mut grid, 1, 1, WHITE, true);
        assert_eq!(grid.get_pixel(0, 1), Some(strong));
        assert_eq!(grid.get_pixel(2, 1), Some(Rgba::rgba(255, 255, 255, 179)));
    }

    #[test]
    fn hard_erase_clears_one_pixel_only() {
        let mut grid = grid3();
        stamp(&mut grid, 1, 1, WHITE, false);
        erase(&mut grid, 1, 1, false);
        assert!(grid.get_pixel(1, 1).unwrap().is_transparent());
        assert!(grid.get_pixel(0, 1).unwrap().is_transparent());
    }

    #[test]
    fn soft_erase_fades_neighbors_not_center() {
        let mut grid = grid3();
        stamp(&mut grid, 1, 1, WHITE, true); // 9 pixel bordati
        erase(&mut grid, 1, 1, true); // gomma morbida al centro
        assert!(grid.get_pixel(1, 1).unwrap().is_transparent());
        // I lati erano 179: attenuati al 30%
        assert_eq!(grid.get_pixel(0, 1), Some(Rgba::rgba(255, 255, 255, 54)));
        // Le diagonali erano 102: attenuate al 60%
        assert_eq!(grid.get_pixel(0, 0), Some(Rgba::rgba(255, 255, 255, 61)));
    }

    #[test]
    fn stamp_is_bounds_checked() {
        let mut grid = grid3();
        stamp(&mut grid, 0, 0, WHITE, true); // angolo: nessun panic
        assert_eq!(grid.get_pixel(0, 0), Some(WHITE));
        erase(&mut grid, 2, 2, true); // angolo opposto: nessun panic
        assert!(grid.get_pixel(2, 2).unwrap().is_transparent());
    }
}
