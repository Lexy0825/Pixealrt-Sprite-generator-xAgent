//! Progetto multi-frame: lo stato condiviso tra editor e server MCP.

use serde::{Deserialize, Serialize};

use crate::{color::Rgba, error::PixelError, frame::Frame};

/// FPS predefinito dell'anteprima (usato pure come durata di default).
const DEFAULT_FPS: f32 = 12.0;

/// Progetto pixel-art/tileset: dimensioni, sequence di frame, fps e indice
/// del frame attivo.
///
/// L'API è senza stato GUI e senza protocollo: sia `pixel-ui` sia
/// `pixel-mcp` la chiamano identica, operando sul file JSON condiviso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    name: String,
    width: u32,
    height: u32,
    fps: f32,
    active_frame: usize,
    frames: Vec<Frame>,
}

impl Project {
    /// Nuovo progetto da dimensioni, con un solo frame vuoto e durata
    /// derivata da [`DEFAULT_FPS`].
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Result<Self, PixelError> {
        Self::new_with_fps(name, width, height, DEFAULT_FPS)
    }

    /// Come [`Project::new`], ma con fps esplicito.
    pub fn new_with_fps(
        name: impl Into<String>,
        width: u32,
        height: u32,
        fps: f32,
    ) -> Result<Self, PixelError> {
        let frame = Frame::new(width, height, 1000.0 / sanitize_fps(fps))?;
        Ok(Self {
            name: name.into(),
            width,
            height,
            fps: sanitize_fps(fps),
            active_frame: 0,
            frames: vec![frame],
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn fps(&self) -> f32 {
        self.fps
    }

    /// Imposta i fps dell'anteprima; i valori assurdi vengono limitati.
    pub fn set_fps(&mut self, fps: f32) {
        self.fps = sanitize_fps(fps);
    }

    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Tutti i frame, in ordine di riproduzione.
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Indice del frame attivo (quello mostrato dal canvas).
    #[must_use]
    pub const fn active_frame_index(&self) -> usize {
        self.active_frame
    }

    /// Seleziona il frame attivo, errori se fuori range.
    pub fn set_active_frame(&mut self, index: usize) -> Result<(), PixelError> {
        if index >= self.frames.len() {
            return Err(PixelError::InvalidFrameIndex {
                index,
                count: self.frames.len(),
            });
        }
        self.active_frame = index;
        Ok(())
    }

    /// Frame attivo (invariante non vuota: un progetto ha almeno 1 frame).
    #[must_use]
    pub fn active_frame(&self) -> &Frame {
        &self.frames[self.active_frame]
    }

    pub fn active_frame_mut(&mut self) -> &mut Frame {
        &mut self.frames[self.active_frame]
    }

    /// Frame all'indice indicato; `None` se fuori range.
    #[must_use]
    pub fn frame(&self, index: usize) -> Option<&Frame> {
        self.frames.get(index)
    }

    /// `frame(mut, indice)` mutabile; `None` se fuori range.
    #[must_use]
    pub fn frame_mut(&mut self, index: Option<usize>) -> Option<&mut Frame> {
        match index {
            Some(i) => self.frames.get_mut(i),
            None => self.frames.get_mut(self.active_frame),
        }
    }

    /// Aggiunge un frame: `copy_active` duplica i pixel del frame attivo,
    /// altrimenti il frame nuovo è vuoto. Restituisce il numero del frame
    /// aggiunto.
    pub fn add_frame(&mut self, copy_active: bool) -> usize {
        let duration_ms = self.frames[self.active_frame].duration_ms();
        let frame = if copy_active {
            Frame::from_grid(self.frames[self.active_frame].grid().clone(), duration_ms)
        } else {
            Frame::new(self.width, self.height, duration_ms)
                .expect("le dimensioni valide del progetto restano valide per un nuovo frame")
        };
        self.frames.push(frame);
        self.frames.len() - 1
    }

    /// Elimina un frame; il progetto mantiene sempre almeno un frame.
    /// Restituisce `false` se non eliminabile (frame unico o out-of-range).
    pub fn remove_frame(&mut self, index: usize) -> bool {
        if self.frames.len() <= 1 || index >= self.frames.len() {
            return false;
        }
        self.frames.remove(index);
        if self.active_frame >= self.frames.len() {
            self.active_frame = self.frames.len() - 1;
        } else if self.active_frame > index {
            self.active_frame -= 1;
        }
        true
    }

    /// Imposta un pixel sul frame attivo.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) -> bool {
        self.active_frame_mut().grid_mut().set_pixel(x, y, color)
    }

    /// Imposta un pixel su un frame o sull'attivo (con `frame: None`).
    pub fn set_pixel_on_frame(
        &mut self,
        frame: Option<usize>,
        x: u32,
        y: u32,
        color: Rgba,
    ) -> bool {
        self.frame_mut(frame)
            .is_some_and(|f| f.grid_mut().set_pixel(x, y, color))
    }

    /// Rettangolo inclusivo, come [`Grid::fill_rect`]; frame con `None`.
    pub fn fill_rect(
        &mut self,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        color: Rgba,
        frame: Option<usize>,
    ) -> bool {
        self.frame_mut(frame).is_some_and(|f| {
            f.grid_mut().fill_rect(x0, y0, x1, y1, color);
            true
        })
    }
}

fn sanitize_fps(fps: f32) -> f32 {
    if !(10.0..=60.0).contains(&fps) || fps.is_nan() {
        DEFAULT_FPS
    } else {
        fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_one_empty_transparent_frame() {
        let project = Project::new("demo", 8, 8).unwrap();
        assert_eq!(project.name(), "demo");
        assert_eq!(project.frame_count(), 1);
        assert_eq!(project.active_frame_index(), 0);
        assert!(project
            .active_frame()
            .grid()
            .pixels()
            .iter()
            .all(|c| c.is_transparent()));
    }

    #[test]
    fn add_and_remove_frames_keep_bounds_sane() {
        let mut project = Project::new("demo", 4, 4).unwrap();
        let i = project.add_frame(true); // frame indice 1
        assert_eq!(i, 1);
        project.set_active_frame(1).unwrap();
        assert_eq!(project.active_frame_index(), 1);
        // Il duplicato contiene i pixel disegnati sul frame sorgente.
        project.set_active_frame(0).unwrap();
        project.set_pixel(0, 0, Rgba::rgb(255, 0, 0));
        let dup = project.add_frame(true);
        assert_eq!(dup, 2);
        assert_eq!(
            project.frame(2).unwrap().grid().get_pixel(0, 0),
            Some(Rgba::rgb(255, 0, 0))
        );
        // Il progetto conserva sempre almeno un frame: quest'ultimo remove no-op.
        project.remove_frame(2);
        assert_eq!(project.frame_count(), 2);
        project.remove_frame(1);
        project.remove_frame(0);
        assert_eq!(project.frame_count(), 1);
        assert!(!project.remove_frame(0));
        assert_eq!(project.active_frame_index(), 0);
    }

    #[test]
    fn out_of_range_frame_ops_are_rejected() {
        let mut project = Project::new("demo", 4, 4).unwrap();
        assert!(matches!(
            project.set_active_frame(5),
            Err(PixelError::InvalidFrameIndex { index: 5, count: 1 })
        ));
        assert!(!project.set_pixel_on_frame(Some(7), 0, 0, Rgba::rgb(1, 1, 1)));
    }

    #[test]
    fn fps_is_sane() {
        let mut project = Project::new_with_fps("demo", 4, 4, 0.0).unwrap();
        assert_eq!(project.fps(), 12.0); // clamp per fps invalido
        project.set_fps(24.0);
        assert_eq!(project.fps(), 24.0);
    }
}
