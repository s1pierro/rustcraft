//! Couleurs de l'éditeur, réglables comme dans GDA (`ED_COULEURS`) : celles que
//! l'utilisateur a choisies, et les autres déduites des siennes ou d'une valeur par défaut.

use crate::draw::Color;
use std::cell::Cell;

/// Une couleur réglable. L'ordre est celui de `LIGNES`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum C {
    Accent,
    Relief,
    Fond,
    Surface,
    Pastilles,
    Bordures,
    Texte,
    Secondaire,
    FondCentre,
    FondBord,
    Grille,
    AxeX,
    AxeY,
    AxeZ,
    Poignee,
    Curseur,
    Reticule,
    Face,
    Envers,
    Arete,
    Sommet,
    Couture,
    FaceChoisie,
    AreteChoisie,
    SommetChoisi,
    Numero,
    Objet,
    ObjetChoisi,
    ObjetActif,
    ContourChoisi,
    Origine,
}

/// Valeur par défaut : fixe, celle d'une couleur précédente, ou un mélange avec du blanc.
#[derive(Clone, Copy)]
pub enum Defaut {
    Hex(u32),
    De(C),
    VersBlanc(C, f32),
}
use Defaut::{De, Hex, VersBlanc};

pub struct Ligne {
    /// Nom dans `reglages.txt`, le même que dans GDA.
    pub cle: &'static str,
    pub c: C,
    pub libelle: &'static str,
    pub groupe: &'static str,
    pub defaut: Defaut,
}

const fn l(cle: &'static str, c: C, libelle: &'static str, groupe: &'static str, defaut: Defaut) -> Ligne {
    Ligne { cle, c, libelle, groupe, defaut }
}

pub const NB: usize = 31;
pub const LIGNES: [Ligne; NB] = [
    l("accent", C::Accent, "Accent", "Interface", Hex(0x2a1fd1)),
    l("relief", C::Relief, "Relief", "Interface", Hex(0xff5a36)),
    l("fond", C::Fond, "Fond", "Interface", Hex(0xf1f0f6)),
    l("surface", C::Surface, "Panneaux et boutons", "Interface", Hex(0xffffff)),
    l("pastilles", C::Pastilles, "Pastilles", "Interface", Hex(0xffffff)),
    l("bordures", C::Bordures, "Bordures", "Interface", Hex(0xd9d7e4)),
    l("texte", C::Texte, "Texte", "Interface", Hex(0x14121f)),
    l("secondaire", C::Secondaire, "Texte secondaire", "Interface", Hex(0x6b6880)),
    l("fond-centre", C::FondCentre, "Fond (centre)", "Vue 3D", De(C::Surface)),
    l("fond-bord", C::FondBord, "Fond (bords)", "Vue 3D", De(C::Fond)),
    l("grille", C::Grille, "Grille", "Vue 3D", De(C::Secondaire)),
    l("axe-x", C::AxeX, "Axe X", "Vue 3D", Hex(0xd03b3b)),
    l("axe-y", C::AxeY, "Axe Y", "Vue 3D", Hex(0x2f9a3a)),
    l("axe-z", C::AxeZ, "Axe Z", "Vue 3D", Hex(0x3b62d6)),
    l("poignee", C::Poignee, "Centre de la poignée", "Vue 3D", De(C::Accent)),
    l("curseur", C::Curseur, "Curseur 3D", "Vue 3D", De(C::Relief)),
    l("reticule", C::Reticule, "Réticule du curseur", "Vue 3D", De(C::Texte)),
    l("face", C::Face, "Faces", "Géométrie", Hex(0xd4d0e2)),
    l("envers", C::Envers, "Envers des faces", "Géométrie", De(C::Relief)),
    l("arete", C::Arete, "Arêtes", "Géométrie", De(C::Texte)),
    l("sommet", C::Sommet, "Sommets", "Géométrie", De(C::Texte)),
    l("couture", C::Couture, "Coutures", "Géométrie", Hex(0xe0392b)),
    l("face-choisie", C::FaceChoisie, "Faces", "Sélection (géométrie)", VersBlanc(C::Accent, 0.35)),
    l("arete-choisie", C::AreteChoisie, "Arêtes", "Sélection (géométrie)", De(C::Accent)),
    l("sommet-choisi", C::SommetChoisi, "Sommets", "Sélection (géométrie)", De(C::Accent)),
    l("numero", C::Numero, "Numéros d'ordre", "Sélection (géométrie)", De(C::Accent)),
    l("objet", C::Objet, "Objets", "Objets (mode Scène)", De(C::Face)),
    l("objet-choisi", C::ObjetChoisi, "Objets choisis", "Objets (mode Scène)", VersBlanc(C::Accent, 0.6)),
    l("objet-actif", C::ObjetActif, "Objet actif", "Objets (mode Scène)", VersBlanc(C::Accent, 0.3)),
    l("contour-choisi", C::ContourChoisi, "Silhouette des objets choisis", "Objets (mode Scène)", De(C::Accent)),
    l("origine", C::Origine, "Origines", "Objets (mode Scène)", De(C::Relief)),
];

/// Couleurs choisies par l'utilisateur, dans l'ordre de `LIGNES`.
pub type Perso = [Option<Color>; NB];

/// Encre sur l'accent, quel que soit le thème.
pub const BLANC: Color = [255, 255, 255, 255];

const fn rgb(v: u32) -> Color {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
}

fn vers_blanc(c: Color, t: f32) -> Color {
    let m = |v: u8| (v as f32 + (255.0 - v as f32) * t).round() as u8;
    [m(c[0]), m(c[1]), m(c[2]), 255]
}

fn resoudre(perso: &Perso) -> [Color; NB] {
    let mut r = [BLANC; NB];
    for (i, l) in LIGNES.iter().enumerate() {
        r[i] = perso[i].unwrap_or(match l.defaut {
            Hex(v) => rgb(v),
            De(c) => r[c as usize],
            VersBlanc(c, t) => vers_blanc(r[c as usize], t),
        });
    }
    r
}

thread_local! {
    // Toute l'interface vit sur le fil de rendu
    static PALETTE: Cell<[Color; NB]> = Cell::new(resoudre(&[None; NB]));
}

/// La couleur en vigueur.
pub fn coul(c: C) -> Color {
    PALETTE.with(|p| p.get()[c as usize])
}

/// Met en vigueur les choix de l'utilisateur.
pub fn appliquer(perso: &Perso) {
    PALETTE.with(|p| p.set(resoudre(perso)));
}

/// « #rrggbb ».
pub fn hex(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Lit « rrggbb », avec ou sans « # ».
pub fn lire_hex(s: &str) -> Option<Color> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(rgb)
}

/// Teinte (0 à 360), saturation et luminosité (0 à 100).
pub fn vers_tsl(c: Color) -> [f32; 3] {
    let [r, g, b] = [c[0], c[1], c[2]].map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let (l, e) = ((max + min) / 2.0, max - min);
    if e == 0.0 {
        return [0.0, 0.0, l * 100.0];
    }
    let s = e / (1.0 - (2.0 * l - 1.0).abs());
    let t = if max == r {
        ((g - b) / e).rem_euclid(6.0)
    } else if max == g {
        (b - r) / e + 2.0
    } else {
        (r - g) / e + 4.0
    };
    [t * 60.0, s * 100.0, l * 100.0]
}

pub fn depuis_tsl([t, s, l]: [f32; 3]) -> Color {
    let (s, l) = (s / 100.0, l / 100.0);
    let a = s * l.min(1.0 - l);
    let f = |n: f32| {
        let k = (n + t / 30.0) % 12.0;
        ((l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)) * 255.0).round() as u8
    };
    [f(0.0), f(8.0), f(4.0), 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_ordre_de_la_table_est_celui_de_l_enum_et_les_defauts_remontent() {
        for (i, l) in LIGNES.iter().enumerate() {
            assert_eq!(l.c as usize, i, "{}", l.cle);
            if let De(c) | VersBlanc(c, _) = l.defaut {
                assert!((c as usize) < i, "{} dépend d'une couleur suivante", l.cle);
            }
        }
    }

    #[test]
    fn une_couleur_choisie_gagne_celles_qui_en_dependent() {
        let mut p = [None; NB];
        p[C::Accent as usize] = Some([10, 20, 30, 255]);
        p[C::ObjetActif as usize] = Some([1, 2, 3, 255]);
        appliquer(&p);
        assert_eq!(coul(C::Poignee), [10, 20, 30, 255]);
        assert_eq!(coul(C::ObjetChoisi), vers_blanc([10, 20, 30, 255], 0.6));
        assert_eq!(coul(C::ObjetActif), [1, 2, 3, 255], "un choix n'est pas recalculé");
        appliquer(&[None; NB]);
        assert_eq!(hex(coul(C::Accent)), "#2a1fd1");
    }

    #[test]
    fn hex_aller_retour() {
        assert_eq!(lire_hex("#2A1fd1"), Some([0x2a, 0x1f, 0xd1, 255]));
        assert_eq!(lire_hex(" 2a1fd1 "), Some([0x2a, 0x1f, 0xd1, 255]));
        for s in ["", "#12345", "#1234567", "#12345g", "éééééé"] {
            assert_eq!(lire_hex(s), None, "{s}");
        }
    }

    #[test]
    fn tsl_aller_retour() {
        for c in [[0x2a, 0x1f, 0xd1, 255], [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [128, 128, 128, 255], [0, 0, 0, 255], [255, 255, 255, 255], [0xd4, 0xd0, 0xe2, 255]] {
            assert_eq!(depuis_tsl(vers_tsl(c)), c);
        }
    }
}
