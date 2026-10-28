//! Colore RGBA su 8 bit per canale.

use serde::{Deserialize, Serialize};

/// Colore RGBA con 8 bit per canale, contenuto in `[u8; 4]` (`[r, g, b, a]`).
///
/// `alpha == 0` significa **completamente trasparente**: le griglie nascono
/// senza alcun colore di sfondo predefinito (né bianco né nero); la
/// trasparenza è codificata solo nel canale alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// Pixel completamente trasparente: stato iniziale di ogni griglia nuova.
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);

    /// Colore opaco a partire dai soli canali RGB (alpha = 255).
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, u8::MAX])
    }

    /// Colore RGBA esplicito, incluso l'alpha.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    /// I quattro canali `[r, g, b, a]`.
    #[must_use]
    pub const fn channels(self) -> [u8; 4] {
        self.0
    }

    /// Valore del canale alpha (`0` = trasparente, `255` = opaco).
    #[must_use]
    pub const fn alpha(self) -> u8 {
        self.0[3]
    }

    /// `true` se il pixel è completamente trasparente.
    #[must_use]
    pub const fn is_transparent(self) -> bool {
        self.0[3] == 0
    }
}

impl From<[u8; 4]> for Rgba {
    fn from(channels: [u8; 4]) -> Self {
        Self(channels)
    }
}

impl From<Rgba> for [u8; 4] {
    fn from(color: Rgba) -> Self {
        color.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_fully_transparent() {
        assert_eq!(Rgba::default(), Rgba::TRANSPARENT);
        assert!(Rgba::default().is_transparent());
        assert_eq!(Rgba::default().alpha(), 0);
        // Nessun canale RGB viene "sporcato" da uno sfondo bianco.
        assert_eq!(Rgba::default().channels(), [0, 0, 0, 0]);
    }

    #[test]
    fn rgb_helper_is_opaque() {
        assert_eq!(Rgba::rgb(10, 20, 30).channels(), [10, 20, 30, 255]);
        assert_eq!(Rgba::rgba(10, 20, 30, 16).channels(), [10, 20, 30, 16]);
    }
}
