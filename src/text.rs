//! Texte : polices du système, glyphes dessinés à la demande dans un atlas.
//! Aucun appel OpenGL : le moteur de rendu envoie l'atlas quand `dirty` est levé.

use crate::draw::{Color, DrawList, Rect};
use std::collections::HashMap;

pub const ATLAS: usize = 1024;
const FONTS: [&str; 3] = ["/system/fonts/RobotoStatic-Regular.ttf", "/system/fonts/Roboto-Regular.ttf", "/system/fonts/DroidSans.ttf"];
/// Roboto n'a ni flèches ni ✕ : Android les prend là, et nous aussi.
const SYMBOLES: [&str; 2] = ["/system/fonts/NotoSansSymbols-Regular-Subsetted.ttf", "/system/fonts/NotoSansSymbols-Regular.ttf"];

#[derive(Clone, Copy)]
struct Glyph {
    rect: Rect, // relatif au point de base
    uv0: [f32; 2],
    uv1: [f32; 2],
    advance: f32,
}

pub struct Text {
    font: Option<fontdue::Font>,
    /// Chargée au premier caractère qui manque ; `Some(None)` : aucune sur cet appareil.
    symboles: Option<Option<fontdue::Font>>,
    glyphs: HashMap<(char, u32), Glyph>,
    pub atlas: Vec<u8>,
    pub dirty: bool,
    shelf: (usize, usize, usize), // x, y, hauteur de la rangée courante
}

fn premiere(chemins: &[&str]) -> Option<fontdue::Font> {
    chemins.iter().find_map(|p| {
        let bytes = std::fs::read(p).ok()?;
        fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
    })
}

impl Text {
    /// Sans police lisible, l'application reste utilisable, le texte n'est simplement pas dessiné.
    pub fn new() -> Text {
        Text { font: premiere(&FONTS), symboles: None, glyphs: HashMap::new(), atlas: vec![0; ATLAS * ATLAS], dirty: true, shelf: (1, 1, 0) }
    }

    fn glyph(&mut self, c: char, px: f32) -> Option<Glyph> {
        let key = (c, px.to_bits());
        if let Some(g) = self.glyphs.get(&key) {
            return Some(*g);
        }
        let manque = self.font.as_ref()?.lookup_glyph_index(c) == 0;
        if manque && self.symboles.is_none() {
            self.symboles = Some(premiere(&SYMBOLES));
        }
        let secours = self.symboles.iter().flatten().find(|f| manque && f.lookup_glyph_index(c) != 0);
        let (m, bitmap) = secours.or(self.font.as_ref())?.rasterize(c, px);
        let (w, h) = (m.width, m.height);
        let (mut x, mut y, mut row) = self.shelf;
        if x + w + 1 >= ATLAS {
            (x, y, row) = (1, y + row + 1, 0);
        }
        if y + h + 1 >= ATLAS {
            return None; // atlas plein : le glyphe ne s'affiche pas, rien ne plante
        }
        for line in 0..h {
            let at = (y + line) * ATLAS + x;
            self.atlas[at..at + w].copy_from_slice(&bitmap[line * w..(line + 1) * w]);
        }
        self.shelf = (x + w + 1, y, row.max(h));
        self.dirty = true;
        let t = ATLAS as f32;
        let g = Glyph {
            rect: Rect::new(m.xmin as f32, -(m.ymin as f32) - h as f32, w as f32, h as f32),
            uv0: [x as f32 / t, y as f32 / t],
            uv1: [(x + w) as f32 / t, (y + h) as f32 / t],
            advance: m.advance_width,
        };
        self.glyphs.insert(key, g);
        Some(g)
    }

    /// Le caractère s'affichera-t-il ? Charge la police de symboles s'il le faut.
    pub fn possede(&mut self, c: char) -> bool {
        let Some(f) = &self.font else { return false };
        if f.lookup_glyph_index(c) != 0 {
            return true;
        }
        if self.symboles.is_none() {
            self.symboles = Some(premiere(&SYMBOLES));
        }
        self.symboles.iter().flatten().any(|f| f.lookup_glyph_index(c) != 0)
    }

    pub fn measure(&mut self, s: &str, px: f32) -> f32 {
        s.chars().map(|c| self.glyph(c, px).map_or(px * 0.55, |g| g.advance)).sum()
    }

    /// Décalage entre le centre d'une ligne et son point de base.
    pub fn center_offset(&self, px: f32) -> f32 {
        match self.font.as_ref().and_then(|f| f.horizontal_line_metrics(px)) {
            Some(m) => (m.ascent + m.descent) / 2.0,
            None => px * 0.35,
        }
    }

    pub fn draw(&mut self, dl: &mut DrawList, x: f32, baseline: f32, s: &str, px: f32, color: Color) {
        let (mut pen, base) = (x, baseline.round());
        for c in s.chars() {
            if let Some(g) = self.glyph(c, px) {
                if g.rect.w > 0.0 {
                    let r = Rect::new((pen + g.rect.x).round(), base + g.rect.y, g.rect.w, g.rect.h);
                    dl.glyph(r, g.uv0, g.uv1, color);
                }
                pen += g.advance;
            }
        }
    }

    /// Découpe `s` en lignes de largeur `w` au plus, aux espaces.
    pub fn wrap(&mut self, s: &str, px: f32, w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in s.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if self.measure(&candidate, px) > w && !line.is_empty() {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
}

impl Default for Text {
    fn default() -> Text {
        Text::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_symboles_absents_de_roboto_viennent_de_la_police_de_secours() {
        let mut t = Text::new();
        let Some(f) = &t.font else { return }; // pas de police système ici
        if f.lookup_glyph_index('→') != 0 || !SYMBOLES.iter().any(|p| std::path::Path::new(p).exists()) {
            return;
        }
        assert!(t.symboles.is_none(), "chargée seulement au besoin");
        t.glyph('a', 16.0);
        assert!(t.symboles.is_none());
        let g = t.glyph('→', 16.0).unwrap();
        assert!(g.rect.w > 8.0 && t.symboles.as_ref().unwrap().is_some());
    }
}
