//! Un fotogramma dell'animazione: griglia + durata.

use serde::{Deserialize, Serialize};

use crate::{error::PixelError, grid::Grid};

/// Durata minima accettabile per un frame (10 ms).
const MIN_DURATION_MS: f32 = 10.0;

/// Un fotogramma: la sua griglia RGBA e la durata in millisecondi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    grid: Grid,
    /// Durata di visualizzazione del frame nel ciclo dell'animazione.
    duration_ms: f32,
}

impl Frame {
    /// Frame vuoto (tutto trasparente) con la durata indicata.
    pub fn new(width: u32, height: u32, duration_ms: f32) -> Result<Self, PixelError> {
        Ok(Self {
            grid: Grid::new(width, height)?,
            duration_ms: clamp_duration(duration_ms),
        })
    }

    /// Frame che adotta una griglia già pronta.
    #[must_use]
    pub fn from_grid(grid: Grid, duration_ms: f32) -> Self {
        Self {
            grid,
            duration_ms: clamp_duration(duration_ms),
        }
    }
    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    #[must_use]
    pub fn duration_ms(&self) -> f32 {
        self.duration_ms
    }

    /// Imposta la durata; i valori sotto [`MIN_DURATION_MS`] vengono limitati.
    pub fn set_duration_ms(&mut self, duration_ms: f32) {
        self.duration_ms = clamp_duration(duration_ms);
    }
}

fn clamp_duration(duration_ms: f32) -> f32 {
    if duration_ms < MIN_DURATION_MS || duration_ms.is_nan() {
        MIN_DURATION_MS
    } else {
        duration_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_is_clamped() {
        let mut frame = Frame::new(1, 1, 0.0).unwrap();
        assert_eq!(frame.duration_ms(), MIN_DURATION_MS);
        frame.set_duration_ms(250.0);
        assert_eq!(frame.duration_ms(), 250.0);
        frame.set_duration_ms(1.0);
        assert_eq!(frame.duration_ms(), MIN_DURATION_MS);
    }
}
