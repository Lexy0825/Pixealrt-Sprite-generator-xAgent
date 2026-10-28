//! Griglia di pixel con trasparenza completa (nessun colore di sfondo).

use serde::{Deserialize, Serialize};

use crate::{color::Rgba, error::PixelError};

/// Griglia `width × height` di pixel disposti in row-major
/// (`index = y * width + x`).
///
/// Una griglia appena creata è **tutta trasparente**: nessuna cella ha un
/// colore di sfondo, la trasparenza è totale e si visualizza come
/// scacchiera nell'editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    width: u32,
    height: u32,
    pixels: Vec<Rgba>,
}

impl Grid {
    /// Nuova griglia inizializzata a trasparente su tutta la superficie.
    ///
    /// Restituisce [`PixelError::InvalidDimensions`] se una delle due
    /// dimensioni è zero o se `width * height` supera lo spazio
    /// indirizzabile.
    pub fn new(width: u32, height: u32) -> Result<Self, PixelError> {
        if width == 0 || height == 0 {
            return Err(PixelError::InvalidDimensions { width, height });
        }
        let len = checked_len(width, height).ok_or(PixelError::InvalidDimensions { width, height })?;
        Ok(Self {
            width,
            height,
            pixels: vec![Rgba::TRANSPARENT; len],
        })
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// `(width, height)` della griglia.
    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Coordinate in-bounds?
    #[must_use]
    pub const fn in_bounds(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// Imposta un pixel. Restituisce `false` se le coordinate sono fuori dai
    /// limiti della griglia (nessun panic, nessun write parziale).
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        let index = self.index(x, y);
        self.pixels[index] = color;
        true
    }

    /// Legge un pixel; `None` se fuori dai limiti.
    #[must_use]
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Rgba> {
        self.in_bounds(x, y)
            .then(|| self.pixels[self.index(x, y)])
    }

    /// Riempie il rettangolo **inclusivo** `[(x0, y0), (x1, y1)]`.
    ///
    /// Le coordinate fuori dai limiti vengono troncate al rettangolo
    /// visibile: non si scrive mai una cella inesistente.
    pub fn fill_rect(&mut self, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba) {
        let (x_start, x_end) = (x0.min(x1), x0.max(x1).min(self.width.saturating_sub(1)));
        let (y_start, y_end) = (y0.min(y1), y0.max(y1).min(self.height.saturating_sub(1)));
        // Un rettangolo che comincia oltre i limiti non scrive nulla.
        if x0.min(x1) >= self.width || y0.min(y1) >= self.height {
            return;
        }
        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let index = self.index(x, y);
                self.pixels[index] = color;
            }
        }
    }

    /// Pixel in row-major, adatti a costruire texture e PNG.
    #[must_use]
    pub fn pixels(&self) -> &[Rgba] {
        &self.pixels
    }

    /// Indice row-major di una coordinata (da usare solo in-bounds).
    const fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }
}

/// Lunghezza `width × height` in celle; `None` su overflow.
fn checked_len(width: u32, height: u32) -> Option<usize> {
    (width as usize).checked_mul(height as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_is_fully_transparent() {
        let grid = Grid::new(4, 3).unwrap();
        assert_eq!(grid.dimensions(), (4, 3));
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(grid.get_pixel(x, y), Some(Rgba::TRANSPARENT));
            }
        }
        assert!(grid.pixels().iter().all(|c| c.is_transparent()));
    }

    #[test]
    fn zero_dimensions_are_rejected() {
        assert!(matches!(
            Grid::new(0, 4),
            Err(PixelError::InvalidDimensions { width: 0, height: 4 })
        ));
        assert!(matches!(
            Grid::new(4, 0),
            Err(PixelError::InvalidDimensions { width: 4, height: 0 })
        ));
    }

    #[test]
    fn set_get_and_bounds() {
        let mut grid = Grid::new(2, 2).unwrap();
        assert!(!grid.set_pixel(5, 0, Rgba::rgb(1, 2, 3)));
        assert!(grid.set_pixel(1, 1, Rgba::rgb(255, 0, 0)));
        assert_eq!(grid.get_pixel(1, 1), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(grid.get_pixel(2, 0), None);
    }

    #[test]
    fn fill_rect_clamps_to_visible_area() {
        let mut grid = Grid::new(4, 4).unwrap();
        // Rettangolo che sfora a destra e in basso: non deve panare mai.
        grid.fill_rect(2, 2, 10, 10, Rgba::rgb(0, 200, 0));
        assert_eq!(grid.get_pixel(3, 3), Some(Rgba::rgb(0, 200, 0)));
        assert_eq!(grid.get_pixel(1, 2), Some(Rgba::TRANSPARENT));
        // Rettangolo completamente fuori griglia: no-op.
        grid.fill_rect(10, 10, 20, 20, Rgba::rgb(1, 1, 1));
        assert!(grid.get_pixel(0, 0).unwrap().is_transparent());
        // Alpha 0 riempie a trasparenza: la "gomma" è un colore come un altro.
        grid.fill_rect(0, 0, 1, 1, Rgba::TRANSPARENT);
        assert!(grid.get_pixel(0, 0).unwrap().is_transparent());
    }
}
