//! Composants tactiles : pastilles, pilules, onglets, interrupteurs, curseurs,
//! listes défilantes. Mode immédiat : chaque image redécrit l'interface.

use crate::icones::Icone;
use crate::theme::{coul, BLANC, C};
use crate::draw::{Color, DrawList, Rect};
use crate::text::Text;
use crate::ui::{sub_id, Id, Sense, Ui};
use std::collections::HashMap;

const SHADOW: Color = [0x1B, 0x1B, 0x40, 60];

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    List,
    Sliders,
    Help,
    Dossier,
    Contexte,
    Os,
    Cle,
    Globe,
    Gauche,
    Droite,
    Annuler,
    Retablir,
}

/// Défilement de chaque zone : position et hauteur du contenu.
pub type Scrolls = HashMap<Id, (f32, f32)>;

pub struct Cx<'a> {
    pub ui: &'a mut Ui,
    pub text: &'a mut Text,
    pub dl: &'a mut DrawList,
    pub dp: f32,
}

/// Colonne de composants dans une zone défilante.
pub struct Body {
    pub x: f32,
    pub w: f32,
    pub y: f32,
    top: f32,
    id: Id,
    rect: Rect,
}

impl Cx<'_> {
    fn label(&mut self, x: f32, cy: f32, s: &str, size: f32, color: Color) {
        let px = (size * self.dp).round();
        let base = cy + self.text.center_offset(px);
        self.text.draw(self.dl, x, base, s, px, color);
    }

    pub fn text_at(&mut self, x: f32, cy: f32, s: &str, size: f32, color: Color) {
        self.label(x, cy, s, size, color);
    }

    /// Texte calé à droite sur `right`.
    pub fn text_right(&mut self, right: f32, cy: f32, s: &str, size: f32, color: Color) {
        let w = self.text.measure(s, (size * self.dp).round());
        self.label(right - w, cy, s, size, color);
    }

    fn centered(&mut self, r: Rect, s: &str, size: f32, color: Color) {
        let w = self.text.measure(s, (size * self.dp).round());
        self.label(r.cx() - w / 2.0, r.cy(), s, size, color);
    }

    pub fn clip(&mut self, c: Option<Rect>) {
        self.dl.set_clip(c);
        self.ui.set_clip(c);
    }

    pub fn icon(&mut self, r: Rect, icon: Icon, color: Color) {
        let (cx, cy, u) = (r.cx(), r.cy(), r.w.min(r.h) / 24.0);
        let t = 2.2 * u;
        let p = |x: f32, y: f32| [cx + x * u, cy + y * u];
        match icon {
            Icon::List => {
                for y in [-6.0, 0.0, 6.0] {
                    self.dl.rrect(Rect::new(cx - 8.5 * u - 1.3 * u, cy + (y - 1.3) * u, 2.6 * u, 2.6 * u), 1.3 * u, color);
                    self.dl.line(p(-4.0, y), p(9.0, y), t, color);
                }
            }
            Icon::Sliders => {
                for (y, k) in [(-6.0, 3.0), (0.0, -3.5), (6.0, 2.0)] {
                    self.dl.line(p(-9.0, y), p(9.0, y), 1.6 * u, color);
                    self.dl.rrect(Rect::new(cx + (k - 2.5) * u, cy + (y - 2.5) * u, 5.0 * u, 5.0 * u), 2.5 * u, color);
                }
            }
            Icon::Help => self.centered(r, "?", 0.85 * r.h / self.dp, color),
            Icon::Dossier => {
                let l = [(-9.0, 7.0), (-9.0, -6.5), (-3.0, -6.5), (-0.5, -3.0), (9.0, -3.0), (9.0, 7.0), (-9.0, 7.0)];
                for c in l.windows(2) {
                    self.dl.line(p(c[0].0, c[0].1), p(c[1].0, c[1].1), t, color);
                }
            }
            Icon::Cle => self.icone(r, &crate::icones::MODIFS, color),
            Icon::Os => self.icone(r, &crate::icones::OS, color),
            Icon::Globe => self.icone(r, &crate::icones::GLOBE, color),
            Icon::Contexte => {
                // Un cube vu en coin : ce que l'on a sous la main
                let l = [(0.0, -9.0), (8.0, -4.5), (8.0, 4.5), (0.0, 9.0), (-8.0, 4.5), (-8.0, -4.5), (0.0, -9.0)];
                for c in l.windows(2) {
                    self.dl.line(p(c[0].0, c[0].1), p(c[1].0, c[1].1), t, color);
                }
                for c in [(-8.0, -4.5), (8.0, -4.5), (0.0, 9.0)] {
                    self.dl.line(p(0.0, 0.0), p(c.0, c.1), t, color);
                }
            }
            Icon::Gauche | Icon::Droite => {
                let s = if icon == Icon::Gauche { 1.0 } else { -1.0 };
                self.dl.line(p(s * 4.0, -8.0), p(s * -4.0, 0.0), t, color);
                self.dl.line(p(s * -4.0, 0.0), p(s * 4.0, 8.0), t, color);
            }
            Icon::Annuler | Icon::Retablir => {
                // Demi-tour : un arc, et sa pointe en haut du côté du départ
                let s = if icon == Icon::Annuler { 1.0 } else { -1.0 };
                let arc = |k: f32| {
                    let a = (k / 10.0 - 0.5) * std::f32::consts::PI;
                    (s * (1.0 + 6.5 * a.cos()), 0.5 + 6.5 * a.sin())
                };
                for k in 0..10 {
                    let (a, b) = (arc(k as f32), arc(k as f32 + 1.0));
                    self.dl.line(p(a.0, a.1), p(b.0, b.1), t, color);
                }
                self.dl.line(p(s * 1.0, -6.0), p(s * -7.0, -6.0), t, color);
                self.dl.line(p(s * -7.0, -6.0), p(s * -3.5, -9.5), t, color);
                self.dl.line(p(s * -7.0, -6.0), p(s * -3.5, -2.5), t, color);
                self.dl.line(p(s * 1.0, 7.0), p(s * -6.0, 7.0), t, color);
            }
        }
    }

    /// Bouton rond flottant.
    pub fn pastille(&mut self, id: Id, r: Rect, icon: Icon, active: bool) -> bool {
        let clicked = self.ui.click(id, r, None);
        let pressed = self.ui.pressed(id);
        let s = 8.0 * self.dp;
        self.dl.shadow(Rect::new(r.x, r.y + 2.0 * self.dp, r.w, r.h), r.w / 2.0, s, SHADOW);
        let bg = if active { coul(C::Accent) } else if pressed { coul(C::Fond) } else { coul(C::Pastilles) };
        self.dl.rrect(r, r.w / 2.0, bg);
        self.icon(r.inset(r.w * 0.16), icon, if active { BLANC } else { coul(C::Accent) });
        clicked
    }

    /// Pastille sans effet pour l'instant : elle garde sa place et le doigt.
    pub fn pastille_eteinte(&mut self, id: Id, r: Rect, icon: Icon) {
        self.ui.region_add(id, r, Sense::Sink);
        self.dl.rrect(r, r.w / 2.0, coul(C::Fond));
        self.icon(r.inset(r.w * 0.16), icon, coul(C::Bordures));
    }

    /// Pilule flottante, sur l'espace de travail.
    pub fn pill(&mut self, id: Id, r: Rect, label: &str, active: bool) -> bool {
        self.pill_teinte(id, r, label, active, coul(C::Accent))
    }

    /// Pilule dont la couleur active porte un sens : un axe, par exemple.
    pub fn pill_teinte(&mut self, id: Id, r: Rect, label: &str, active: bool, couleur: Color) -> bool {
        let clicked = self.ui.click(id, r, None);
        let pressed = self.ui.pressed(id);
        self.dl.shadow(Rect::new(r.x, r.y + 2.0 * self.dp, r.w, r.h), r.h / 2.0, 6.0 * self.dp, SHADOW);
        self.dl.rrect(r, r.h / 2.0, if active { couleur } else if pressed { coul(C::Fond) } else { coul(C::Surface) });
        self.centered(r, label, 15.0, if active { BLANC } else { coul(C::Texte) });
        clicked
    }

    /// Pilule à pictogramme, centré, ou suivi de son libellé.
    pub fn pill_icone(&mut self, id: Id, r: Rect, icone: &Icone, label: Option<&str>, active: bool, couleur: Color) -> bool {
        let clicked = self.ui.click(id, r, None);
        let pressed = self.ui.pressed(id);
        self.dl.shadow(Rect::new(r.x, r.y + 2.0 * self.dp, r.w, r.h), r.h / 2.0, 6.0 * self.dp, SHADOW);
        self.dl.rrect(r, r.h / 2.0, if active { couleur } else if pressed { coul(C::Fond) } else { coul(C::Surface) });
        // Le pictogramme suit la pilule quand elle rétrécit
        let c = (26.0 * self.dp).min(r.h * 0.6);
        let encre = if active { BLANC } else { coul(C::Texte) };
        let x = if label.is_some() { r.x + 14.0 * self.dp } else { r.cx() - c / 2.0 };
        self.icone(Rect::new(x, r.cy() - c / 2.0, c, c), icone, encre);
        if let Some(l) = label {
            self.text_at(x + c + 6.0 * self.dp, r.cy(), l, 15.0, encre);
        }
        clicked
    }

    /// Pictogramme de GDA, dans le carré `r`.
    pub fn icone(&mut self, r: Rect, icone: &Icone, color: Color) {
        let u = r.w.min(r.h) / 24.0;
        let p = |v: [f32; 2]| [r.x + v[0] * u, r.y + v[1] * u];
        for (e, t) in icone.traits {
            for c in t.windows(2) {
                self.dl.line(p(c[0]), p(c[1]), e * u, color);
            }
        }
        for [x, y, rayon] in icone.disques {
            let (c, q) = (p([*x, *y]), rayon * u);
            self.dl.rrect(Rect::new(c[0] - q, c[1] - q, 2.0 * q, 2.0 * q), q, color);
        }
    }

    /// Glyphe, s'il existe sur cet appareil, et libellé.
    fn glyphe_visible(&mut self, glyphe: &str) -> bool {
        glyphe.chars().all(|c| self.text.possede(c))
    }

    pub fn largeur_operation(&mut self, glyphe: &str, label: &str) -> f32 {
        let g = if self.glyphe_visible(glyphe) { self.text.measure(glyphe, (18.0 * self.dp).round()) + 7.0 * self.dp } else { 0.0 };
        g + self.text.measure(label, (14.5 * self.dp).round()) + 32.0 * self.dp
    }

    /// Pilule de la barre rapide. Un glissement passe à la zone `scroll`.
    pub fn pill_operation(&mut self, id: Id, r: Rect, glyphe: &str, label: &str, scroll: Id) -> bool {
        let clicked = self.ui.click(id, r, Some(scroll));
        let pressed = self.ui.pressed(id);
        self.dl.shadow(Rect::new(r.x, r.y + 2.0 * self.dp, r.w, r.h), r.h / 2.0, 6.0 * self.dp, SHADOW);
        self.dl.rrect(r, r.h / 2.0, if pressed { coul(C::Fond) } else { coul(C::Surface) });
        let mut x = r.x + 16.0 * self.dp;
        if self.glyphe_visible(glyphe) {
            self.text_at(x, r.cy(), glyphe, 18.0, coul(C::Accent));
            x += self.text.measure(glyphe, (18.0 * self.dp).round()) + 7.0 * self.dp;
        }
        self.text_at(x, r.cy(), label, 14.5, coul(C::Texte));
        clicked
    }

    pub fn pill_icone_width(&mut self, label: &str) -> f32 {
        self.text.measure(label, (15.0 * self.dp).round()) + (14.0 + 26.0 + 6.0 + 18.0) * self.dp
    }

    pub fn pill_width(&mut self, label: &str) -> f32 {
        self.text.measure(label, (15.0 * self.dp).round()) + 36.0 * self.dp
    }

    /// Rangée d'onglets dans l'en-tête d'un panneau. Renvoie l'onglet choisi.
    pub fn tabs(&mut self, id: Id, r: Rect, labels: &[&str], selected: usize) -> usize {
        let (mut x, mut result) = (r.x, selected);
        for (i, label) in labels.iter().enumerate() {
            let w = self.text.measure(label, (15.0 * self.dp).round()) + 32.0 * self.dp;
            let t = Rect::new(x, r.y, w, r.h);
            if self.ui.click(sub_id(id, i), t, None) {
                result = i;
            }
            let on = i == selected;
            self.dl.rrect(t, r.h / 2.0, if on { coul(C::Accent) } else { coul(C::Fond) });
            self.centered(t, label, 15.0, if on { BLANC } else { coul(C::Texte) });
            x += w + 6.0 * self.dp;
        }
        result
    }

    pub fn body(&mut self, id: Id, rect: Rect, scrolls: &Scrolls) -> Body {
        self.ui.region_add(id, rect, Sense::Scroll);
        let delta = self.ui.take_scroll(id);
        let (off, content) = scrolls.get(&id).copied().unwrap_or((0.0, 0.0));
        let off = (off - delta).clamp(0.0, (content - rect.h).max(0.0));
        self.clip(Some(rect));
        let pad = 14.0 * self.dp;
        Body { x: rect.x + pad, w: rect.w - 2.0 * pad, y: rect.y + 8.0 * self.dp - off, top: rect.y - off, id, rect }
    }

    pub fn body_end(&mut self, b: Body, scrolls: &mut Scrolls) {
        self.clip(None);
        let content = b.y - b.top + 12.0 * self.dp;
        let off = b.rect.y - b.top;
        scrolls.insert(b.id, (off, content));
        if content > b.rect.h {
            let h = (b.rect.h * b.rect.h / content).max(32.0 * self.dp);
            let y = b.rect.y + off / (content - b.rect.h) * (b.rect.h - h);
            self.dl.rrect(Rect::new(b.rect.right() - 6.0 * self.dp, y, 3.0 * self.dp, h), 1.5 * self.dp, [0x6C, 0x6C, 0x80, 120]);
        }
    }

    pub fn section(&mut self, b: &mut Body, title: &str) {
        b.y += 10.0 * self.dp;
        self.label(b.x, b.y + 12.0 * self.dp, title, 12.5, coul(C::Secondaire));
        b.y += 28.0 * self.dp;
    }

    pub fn paragraph(&mut self, b: &mut Body, s: &str) {
        let px = (15.0 * self.dp).round();
        for line in self.text.wrap(s, px, b.w) {
            self.label(b.x, b.y + 11.0 * self.dp, &line, 15.0, coul(C::Texte));
            b.y += 22.0 * self.dp;
        }
        b.y += 8.0 * self.dp;
    }

    /// Ligne de liste bord à bord. Renvoie vrai à l'appui.
    pub fn row(&mut self, b: &mut Body, id: Id, label: &str, swatch: Option<Color>, selected: bool) -> bool {
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 48.0 * self.dp);
        let clicked = self.ui.click(id, r, Some(b.id));
        if self.ui.pressed(id) || selected {
            self.dl.rrect(r.inset(2.0 * self.dp), 10.0 * self.dp, coul(C::Fond));
        }
        let mut x = b.x;
        if let Some(c) = swatch {
            let s = 22.0 * self.dp;
            self.dl.rrect(Rect::new(x, r.cy() - s / 2.0, s, s), 6.0 * self.dp, c);
            x += s + 12.0 * self.dp;
        }
        self.label(x, r.cy(), label, 16.0, if selected { coul(C::Accent) } else { coul(C::Texte) });
        b.y += r.h;
        clicked
    }

    /// Ligne d'un arbre : décalée selon son `niveau`, un chevron en pastille s'il y a
    /// quelque chose à plier (`plie` : `Some(vrai)` replié), le nom et un lien à droite.
    /// `selected` surligne la ligne, `choisi` colore seulement son nom ; `long` : la
    /// ligne accepte l'appui long. Rend l'appui sur la ligne, sur le chevron, sur le
    /// lien, et l'appui long sur la ligne.
    #[allow(clippy::too_many_arguments)]
    pub fn ligne_arbre(&mut self, b: &mut Body, cle: Id, chevron: Id, label: &str, niveau: usize, plie: Option<bool>, selected: bool, choisi: bool, lien: Option<(Id, &str)>, long: bool) -> (bool, bool, bool, bool) {
        let d = self.dp;
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 44.0 * d);
        let x0 = b.x + niveau as f32 * 16.0 * d;
        let z_chevron = Rect::new(x0 - 6.0 * d, r.y, 36.0 * d, r.h);
        let droite = r.right();
        let z_lien = lien.map(|(_, t)| {
            let large = self.text.measure(t, (15.0 * d).round()) + 2.0 * (b.rect.right() - (b.x + b.w)).max(12.0 * d);
            Rect::new(droite - large, r.y, large, r.h)
        });
        let debut = if plie.is_some() { z_chevron.right() } else { r.x };
        let z_nom = Rect::new(debut, r.y, z_lien.map_or(droite, |z| z.x) - debut, r.h);
        let ligne = self.ui.click(cle, z_nom, Some(b.id));
        let chev = plie.is_some() && self.ui.click(chevron, z_chevron, Some(b.id));
        let tenu = long && self.ui.long_press(cle);
        let appui = match (lien, z_lien) {
            (Some((lid, _)), Some(z)) => self.ui.click(lid, z, Some(b.id)),
            _ => false,
        };
        if self.ui.pressed(cle) || selected {
            self.dl.rrect(r.inset(2.0 * d), 10.0 * d, coul(C::Fond));
        }
        let (cx_, cy) = (x0 + 12.0 * d, r.cy());
        if let Some(p) = plie {
            let s = 24.0 * d;
            let fond = if self.ui.pressed(chevron) { coul(C::Bordures) } else if selected { coul(C::Surface) } else { coul(C::Fond) };
            self.dl.rrect(Rect::new(cx_ - s / 2.0, cy - s / 2.0, s, s), s / 2.0, fond);
            let (e, c) = (4.0 * d, coul(C::Texte));
            if p {
                self.dl.line([cx_ - e / 2.0, cy - e], [cx_ + e / 2.0, cy], 2.0 * d, c);
                self.dl.line([cx_ + e / 2.0, cy], [cx_ - e / 2.0, cy + e], 2.0 * d, c);
            } else {
                self.dl.line([cx_ - e, cy - e / 2.0], [cx_, cy + e / 2.0], 2.0 * d, c);
                self.dl.line([cx_, cy + e / 2.0], [cx_ + e, cy - e / 2.0], 2.0 * d, c);
            }
        }
        self.label(x0 + 30.0 * d, cy, label, 16.0, if selected || choisi { coul(C::Accent) } else { coul(C::Texte) });
        if let Some((lid, t)) = lien {
            let c = coul(C::Accent);
            let c = if self.ui.pressed(lid) { [c[0], c[1], c[2], 150] } else { c };
            self.text_right(b.x + b.w, cy, t, 15.0, c);
        }
        b.y += r.h;
        (ligne, chev, appui, tenu)
    }

    /// Ligne de liste avec un lien à droite, dans son surlignage : rend l'appui sur la
    /// ligne et l'appui sur le lien.
    pub fn row_lien(&mut self, b: &mut Body, id: Id, label: &str, selected: bool, lien: Option<(Id, &str)>) -> (bool, bool) {
        let Some((lid, texte)) = lien else { return (self.row(b, id, label, None, selected), false) };
        let d = self.dp;
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 48.0 * d);
        let large = self.text.measure(texte, (15.0 * d).round()) + 2.0 * (b.rect.right() - (b.x + b.w)).max(12.0 * d);
        let z_lien = Rect::new(r.right() - large, r.y, large, r.h);
        let z_nom = Rect::new(r.x, r.y, z_lien.x - r.x, r.h);
        let (ligne, appui) = (self.ui.click(id, z_nom, Some(b.id)), self.ui.click(lid, z_lien, Some(b.id)));
        if self.ui.pressed(id) || selected {
            self.dl.rrect(r.inset(2.0 * d), 10.0 * d, coul(C::Fond));
        }
        self.label(b.x, r.cy(), label, 16.0, if selected { coul(C::Accent) } else { coul(C::Texte) });
        let c = coul(C::Accent);
        let c = if self.ui.pressed(lid) { [c[0], c[1], c[2], 150] } else { c };
        self.text_right(b.x + b.w, r.cy(), texte, 15.0, c);
        b.y += r.h;
        (ligne, appui)
    }

    /// Légende discrète à gauche, et un lien à droite sur la même ligne s'il y en a un.
    /// Renvoie vrai à l'appui sur le lien.
    pub fn legende_lien(&mut self, b: &mut Body, texte: &str, lien: Option<(Id, &str)>) -> bool {
        let d = self.dp;
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 40.0 * d);
        self.label(b.x, r.cy(), texte, 15.0, coul(C::Secondaire));
        let mut appui = false;
        if let Some((lid, t)) = lien {
            let large = self.text.measure(t, (15.0 * d).round()) + 2.0 * (r.right() - (b.x + b.w)).max(12.0 * d);
            appui = self.ui.click(lid, Rect::new(r.right() - large, r.y, large, r.h), Some(b.id));
            let c = coul(C::Accent);
            let c = if self.ui.pressed(lid) { [c[0], c[1], c[2], 150] } else { c };
            self.text_right(b.x + b.w, r.cy(), t, 15.0, c);
        }
        b.y += r.h;
        appui
    }

    /// Lien en fin de liste, aligné à droite : un texte de la couleur d'accent, sans
    /// fond. Renvoie vrai à l'appui.
    pub fn lien(&mut self, b: &mut Body, id: Id, label: &str) -> bool {
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 40.0 * self.dp);
        let clicked = self.ui.click(id, r, Some(b.id));
        let c = coul(C::Accent);
        let c = if self.ui.pressed(id) { [c[0], c[1], c[2], 150] } else { c };
        self.text_right(b.x + b.w, r.cy(), label, 15.0, c);
        b.y += r.h;
        clicked
    }

    /// En-tête d'un set : son chevron et son nom, qui le plient, sa visibilité et son
    /// menu. Rend l'appui sur chacun.
    pub fn tete_set(&mut self, b: &mut Body, cle: Id, nom: &str, n: usize, replie: bool, visible: bool) -> (bool, bool, bool) {
        let d = self.dp;
        let r = Rect::new(b.rect.x, b.y, b.rect.w, 48.0 * d);
        let (w, h, m) = (46.0 * d, 26.0 * d, 44.0 * d);
        let z_menu = Rect::new(r.right() - m, r.y, m, r.h);
        let track = Rect::new(z_menu.x - w, r.cy() - h / 2.0, w, h);
        let z_oeil = Rect::new(track.x - 6.0 * d, r.y, w + 6.0 * d, r.h);
        let z_nom = Rect::new(r.x, r.y, z_oeil.x - r.x, r.h);
        let (plier, oeil, menu) = (self.ui.click(sub_id(cle, 0), z_nom, Some(b.id)), self.ui.click(sub_id(cle, 1), z_oeil, Some(b.id)), self.ui.click(sub_id(cle, 2), z_menu, Some(b.id)));
        if self.ui.pressed(sub_id(cle, 0)) {
            self.dl.rrect(z_nom.inset(2.0 * d), 10.0 * d, coul(C::Fond));
        }
        let c = if visible { coul(C::Texte) } else { coul(C::Secondaire) };
        let (x, y, e) = (b.x + 6.0 * d, r.cy(), 5.0 * d);
        if replie {
            self.dl.line([x - e / 2.0, y - e], [x + e / 2.0, y], 2.0 * d, c);
            self.dl.line([x + e / 2.0, y], [x - e / 2.0, y + e], 2.0 * d, c);
        } else {
            self.dl.line([x - e, y - e / 2.0], [x, y + e / 2.0], 2.0 * d, c);
            self.dl.line([x, y + e / 2.0], [x + e, y - e / 2.0], 2.0 * d, c);
        }
        let x = b.x + 22.0 * d;
        // Le nombre d'objets reste lisible : c'est le nom qui se coupe
        let place = ((z_nom.right() - x) / (9.0 * d)) as usize;
        let suffixe = format!("  {n}");
        let garde = place.saturating_sub(suffixe.chars().count() + 1).max(4);
        let court: String = if nom.chars().count() > garde { nom.chars().take(garde).chain(['…']).collect() } else { nom.to_string() };
        self.label(x, r.cy(), &(court + &suffixe), 16.0, c);
        self.dl.rrect(track, h / 2.0, if visible { coul(C::Accent) } else { coul(C::Bordures) });
        let k = h - 6.0 * d;
        let kx = if visible { track.right() - k - 3.0 * d } else { track.x + 3.0 * d };
        self.dl.rrect(Rect::new(kx, track.y + 3.0 * d, k, k), k / 2.0, BLANC);
        let s = 4.0 * d;
        for i in 0..3 {
            let px = z_menu.cx() + (i as f32 - 1.0) * 8.0 * d;
            self.dl.rrect(Rect::new(px - s / 2.0, r.cy() - s / 2.0, s, s), s / 2.0, coul(C::Texte));
        }
        b.y += r.h;
        (plier, oeil, menu)
    }

    pub fn toggle(&mut self, b: &mut Body, id: Id, label: &str, value: &mut bool) {
        let r = Rect::new(b.x, b.y, b.w, 48.0 * self.dp);
        if self.ui.click(id, r, Some(b.id)) {
            *value = !*value;
        }
        self.label(b.x, r.cy(), label, 16.0, coul(C::Texte));
        let (w, h) = (46.0 * self.dp, 26.0 * self.dp);
        let track = Rect::new(r.right() - w, r.cy() - h / 2.0, w, h);
        self.dl.rrect(track, h / 2.0, if *value { coul(C::Accent) } else { coul(C::Bordures) });
        let k = h - 6.0 * self.dp;
        let kx = if *value { track.right() - k - 3.0 * self.dp } else { track.x + 3.0 * self.dp };
        self.dl.rrect(Rect::new(kx, track.y + 3.0 * self.dp, k, k), k / 2.0, BLANC);
        b.y += r.h;
    }

    pub fn slider(&mut self, b: &mut Body, id: Id, label: &str, shown: &str, value: &mut f32, lo: f32, hi: f32) {
        self.slider_borne(b, id, label, shown, value, lo, hi, None, true);
    }

    /// Curseur dont `plage` marque la partie permise sur la piste ; grisé et immobile
    /// quand il n'est pas `libre`.
    #[allow(clippy::too_many_arguments)]
    pub fn slider_borne(&mut self, b: &mut Body, id: Id, label: &str, shown: &str, value: &mut f32, lo: f32, hi: f32, plage: Option<(f32, f32)>, libre: bool) {
        let r = Rect::new(b.x, b.y, b.w, 64.0 * self.dp);
        self.label(b.x, r.y + 16.0 * self.dp, label, 16.0, if libre { coul(C::Texte) } else { coul(C::Secondaire) });
        self.text_right(r.right(), r.y + 16.0 * self.dp, shown, 14.0, coul(C::Secondaire));
        let track_y = r.y + 44.0 * self.dp;
        let knob = 11.0 * self.dp;
        let (x0, x1) = (r.x + knob, r.right() - knob);
        if let Some(d) = self.ui.drag(id, r).filter(|_| libre) {
            *value = lo + ((d.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * (hi - lo);
        }
        let t = 4.0 * self.dp;
        let ax = |v: f32| x0 + (v.clamp(lo, hi) - lo) / (hi - lo) * (x1 - x0);
        let at = ax(*value);
        let c = if libre { coul(C::Accent) } else { coul(C::Secondaire) };
        self.dl.rrect(Rect::new(x0, track_y - t / 2.0, x1 - x0, t), t / 2.0, coul(C::Bordures));
        if let Some((a, z)) = plage {
            // La plage permise, plus épaisse et pâle, avec l'écart au zéro par-dessus
            let (a, z, e) = (ax(a), ax(z), 3.0 * t);
            self.dl.rrect(Rect::new(a, track_y - e / 2.0, (z - a).max(e), e), e / 2.0, [c[0], c[1], c[2], 60]);
            let o = ax(0.0);
            self.dl.rrect(Rect::new(o.min(at), track_y - t / 2.0, (at - o).abs(), t), t / 2.0, c);
        } else {
            self.dl.rrect(Rect::new(x0, track_y - t / 2.0, at - x0, t), t / 2.0, c);
        }
        let grow = if self.ui.pressed(id) && libre { 2.0 * self.dp } else { 0.0 };
        self.dl.rrect(Rect::new(at - knob - grow, track_y - knob - grow, 2.0 * (knob + grow), 2.0 * (knob + grow)), knob + grow, c);
        b.y += r.h;
    }

    /// Ligne d'une limite : son nom dans la couleur de l'axe, ses bornes en deux cadres
    /// quand il y en a, et son état dans une pastille qui le change. Rend l'appui sur
    /// l'état, sur le minimum et sur le maximum.
    pub fn ligne_limite(&mut self, b: &mut Body, cle: Id, label: &str, couleur: Color, etat: &str, bornes: Option<(&str, &str)>) -> (bool, bool, bool) {
        let d = self.dp;
        let r = Rect::new(b.x, b.y, b.w, 48.0 * d);
        self.label(b.x, r.cy(), label, 16.0, couleur);
        let (ie, imin, imax) = (sub_id(cle, 0), sub_id(cle, 1), sub_id(cle, 2));
        let we = 80.0 * d;
        let z = Rect::new(r.right() - we, r.y + 8.0 * d, we, 32.0 * d);
        let e = self.ui.click(ie, z, Some(b.id));
        let libre = etat == "Libre";
        self.dl.rrect(z, z.h / 2.0, if self.ui.pressed(ie) { coul(C::Bordures) } else if libre { coul(C::Fond) } else { couleur });
        self.centered(z, etat, 14.0, if libre { coul(C::Texte) } else { BLANC });
        let (mut mn, mut mx) = (false, false);
        if let Some((a, c)) = bornes {
            let wc = 60.0 * d;
            for (k, (i, v)) in [(imin, a), (imax, c)].into_iter().enumerate() {
                let f = Rect::new(z.x - (2 - k) as f32 * (wc + 6.0 * d), r.y + 6.0 * d, wc, 36.0 * d);
                let t = self.ui.click(i, f, Some(b.id));
                self.dl.rrect(f, 10.0 * d, if self.ui.pressed(i) { coul(C::Bordures) } else { coul(C::Fond) });
                self.centered(f, v, 14.0, coul(C::Texte));
                if k == 0 { mn = t } else { mx = t }
            }
        }
        b.y += r.h;
        (e, mn, mx)
    }

    /// Ligne d'une couleur : son nom, son retour au défaut si elle est choisie, son
    /// échantillon. Rend l'appui sur la ligne et celui sur le retour.
    pub fn ligne_couleur(&mut self, b: &mut Body, id: Id, raz: Id, label: &str, couleur: Color, choisie: bool) -> (bool, bool) {
        let r = Rect::new(b.x, b.y, b.w, 48.0 * self.dp);
        let ouvre = self.ui.click(id, r, Some(b.id));
        self.label(b.x, r.cy(), label, 16.0, coul(C::Texte));
        let e = Rect::new(r.right() - 60.0 * self.dp, r.y + 8.0 * self.dp, 60.0 * self.dp, 32.0 * self.dp);
        self.dl.rrect(e, 8.0 * self.dp, coul(C::Bordures));
        self.dl.rrect(e.inset(1.5 * self.dp), 7.0 * self.dp, couleur);
        let mut retour = false;
        if choisie {
            let z = Rect::new(e.x - 44.0 * self.dp, r.y + 2.0 * self.dp, 44.0 * self.dp, 44.0 * self.dp);
            retour = self.ui.click(raz, z, Some(b.id));
            self.icon(z.inset(11.0 * self.dp), Icon::Annuler, coul(C::Secondaire));
        }
        b.y += r.h;
        (ouvre && !retour, retour)
    }

    /// Ligne d'une valeur chiffrée : le libellé, puis la valeur dans son cadre.
    pub fn champ(&mut self, b: &mut Body, id: Id, label: &str, couleur: Color, valeur: &str) -> bool {
        let r = Rect::new(b.x, b.y, b.w, 48.0 * self.dp);
        let clicked = self.ui.click(id, r, Some(b.id));
        self.label(b.x, r.cy(), label, 16.0, couleur);
        let w = (r.w * 0.62).max(120.0 * self.dp).min(r.w - 24.0 * self.dp);
        let cadre = Rect::new(r.right() - w, r.y + 4.0 * self.dp, w, 40.0 * self.dp);
        self.dl.rrect(cadre, 10.0 * self.dp, if self.ui.pressed(id) { coul(C::Bordures) } else { coul(C::Fond) });
        self.text_right(cadre.right() - 12.0 * self.dp, cadre.cy(), valeur, 15.0, coul(C::Texte));
        b.y += r.h;
        clicked
    }

    /// Rangée de boutons de même largeur : (libellé, possible). Rend celui touché.
    /// Ligne de temps : graduée en secondes, les clés en losanges, la tête de lecture.
    /// Rend l'instant sous le doigt qui la touche ou la glisse.
    pub fn ligne_temps(&mut self, b: &mut Body, id: Id, duree: f32, temps: f32, cles: &[f32], marque: Option<f32>) -> Option<f32> {
        let d = self.dp;
        let r = Rect::new(b.x, b.y, b.w, 72.0 * d);
        let (x0, x1) = (r.x + 10.0 * d, r.right() - 10.0 * d);
        let ax = |t: f32| x0 + (t / duree.max(1e-6)).clamp(0.0, 1.0) * (x1 - x0);
        let mut rendu = None;
        if let Some(p) = self.ui.drag(id, r) {
            rendu = Some(((p.x - x0) / (x1 - x0)).clamp(0.0, 1.0) * duree);
        }
        let piste = Rect::new(x0, r.y + 22.0 * d, x1 - x0, 28.0 * d);
        self.dl.rrect(piste, 8.0 * d, coul(C::Fond));
        // Une graduation par seconde, plus fine par dixième quand la place le permet
        let n = duree.floor() as usize;
        let dixiemes = (x1 - x0) / duree.max(1e-6) > 60.0 * d;
        for k in 0..=(n * 10) {
            let t = k as f32 / 10.0;
            if t > duree + 1e-4 || (!dixiemes && k % 10 != 0) {
                continue;
            }
            let x = ax(t);
            let haut = if k % 10 == 0 { 10.0 } else { 4.0 } * d;
            self.dl.line([x, piste.bottom()], [x, piste.bottom() - haut], 1.0 * d, coul(C::Bordures));
            if k % 10 == 0 {
                self.centered(Rect::new(x - 20.0 * d, piste.bottom() + 2.0 * d, 40.0 * d, 18.0 * d), &format!("{}", k / 10), 12.0, coul(C::Secondaire));
            }
        }
        for &t in cles {
            let (x, y, e) = (ax(t), piste.cy(), 7.0 * d);
            let c = if marque.is_some_and(|m| (m - t).abs() < 1e-4) { coul(C::Accent) } else { coul(C::Texte) };
            self.dl.line([x - e, y], [x, y - e], 2.0 * d, c);
            self.dl.line([x, y - e], [x + e, y], 2.0 * d, c);
            self.dl.line([x + e, y], [x, y + e], 2.0 * d, c);
            self.dl.line([x, y + e], [x - e, y], 2.0 * d, c);
        }
        let x = ax(temps);
        self.dl.rrect(Rect::new(x - 1.5 * d, r.y + 14.0 * d, 3.0 * d, 44.0 * d), 1.5 * d, coul(C::Accent));
        self.dl.rrect(Rect::new(x - 6.0 * d, r.y + 8.0 * d, 12.0 * d, 12.0 * d), 6.0 * d, coul(C::Accent));
        b.y += r.h;
        rendu
    }

    pub fn rangee(&mut self, b: &mut Body, cle: Id, l: &[(&str, bool)]) -> Option<usize> {
        let (ecart, h) = (6.0 * self.dp, 44.0 * self.dp);
        let w = (b.w - ecart * (l.len() - 1) as f32) / l.len() as f32;
        let mut touche = None;
        for (k, (label, possible)) in l.iter().enumerate() {
            let r = Rect::new(b.x + (w + ecart) * k as f32, b.y + 4.0 * self.dp, w, h);
            let i = sub_id(cle, k);
            if *possible && self.ui.click(i, r, Some(b.id)) {
                touche = Some(k);
            }
            self.dl.rrect(r, h / 2.0, if *possible && self.ui.pressed(i) { coul(C::Bordures) } else { coul(C::Fond) });
            self.centered(r, label, 15.0, if *possible { coul(C::Accent) } else { coul(C::Secondaire) });
        }
        b.y += 52.0 * self.dp;
        touche
    }

    /// Choix exclusif en pilules, `par_ligne` sur chaque ligne. Rend celui touché.
    pub fn choix(&mut self, b: &mut Body, cle: Id, l: &[&str], choisi: Option<usize>, par_ligne: usize) -> Option<usize> {
        let c: Vec<(&str, bool)> = l.iter().enumerate().map(|(k, x)| (*x, choisi == Some(k))).collect();
        self.bascules(b, cle, &c, par_ligne)
    }

    /// Pilules allumées ou non, `par_ligne` sur chaque ligne. Rend celle touchée.
    pub fn bascules(&mut self, b: &mut Body, cle: Id, l: &[(&str, bool)], par_ligne: usize) -> Option<usize> {
        let (ecart, h) = (6.0 * self.dp, 40.0 * self.dp);
        let w = (b.w - ecart * (par_ligne - 1) as f32) / par_ligne as f32;
        let mut touche = None;
        for (k, (label, on)) in l.iter().enumerate() {
            let r = Rect::new(b.x + (w + ecart) * (k % par_ligne) as f32, b.y + 4.0 * self.dp + (h + ecart) * (k / par_ligne) as f32, w, h);
            if self.ui.click(sub_id(cle, k), r, Some(b.id)) {
                touche = Some(k);
            }
            self.dl.rrect(r, h / 2.0, if *on { coul(C::Accent) } else { coul(C::Fond) });
            self.centered(r, label, 14.0, if *on { BLANC } else { coul(C::Texte) });
        }
        b.y += (h + ecart) * l.len().div_ceil(par_ligne) as f32 + 4.0 * self.dp;
        touche
    }

    /// Bouton de panneau, grisé quand l'action n'est pas possible.
    pub fn bouton(&mut self, b: &mut Body, id: Id, label: &str, possible: bool) -> bool {
        if possible {
            return self.button(b, id, label);
        }
        let r = Rect::new(b.x, b.y + 4.0 * self.dp, b.w, 44.0 * self.dp);
        self.dl.rrect(r, r.h / 2.0, coul(C::Fond));
        self.centered(r, label, 15.0, coul(C::Secondaire));
        b.y += 52.0 * self.dp;
        false
    }

    pub fn button(&mut self, b: &mut Body, id: Id, label: &str) -> bool {
        let r = Rect::new(b.x, b.y + 4.0 * self.dp, b.w, 44.0 * self.dp);
        let clicked = self.ui.click(id, r, Some(b.id));
        self.dl.rrect(r, r.h / 2.0, if self.ui.pressed(id) { coul(C::Bordures) } else { coul(C::Fond) });
        self.centered(r, label, 15.0, coul(C::Accent));
        b.y += 52.0 * self.dp;
        clicked
    }
}
