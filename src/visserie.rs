//! Visserie métrique ISO (filet à 60°, pas gros, à droite) en maillages fermés, portée
//! de `moteur/visserie.js` de GDA : 1 unité = 1 mm, axe Y.
//!
//! Vis : la face d'appui de la tête est en y = 0, la tige descend jusqu'à y = −longueur
//! (fraisée : tête comprise). Écrou et rondelle sont posés sur y = 0, la tige est centrée.
//!
//! Chaque pièce est une suite d'anneaux de `segments` sommets parcourant son profil.
//! Méthode Soustraction : un corps lisse chanfreiné moins un outil de coupe hélicoïdal
//! d'un seul tenant. Méthode Anneaux : le filet échantillonné directement.

use crate::booleens::{self, Op};
use crate::maillage::{defaut, Face, Maillage, P3};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    Vis,
    Ecrou,
    Rondelle,
    Tige,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tete {
    Hexagonale,
    Cylindrique,
    Fraisee,
}

/// Cotes en mm d'une taille : d diamètre, p pas gros ; s sur plats (tête H et écrou), k
/// hauteur de tête H (ISO 4017), m hauteur d'écrou (ISO 4032) ; dk_c, k_c, s_c tête
/// cylindrique et son six pans creux (ISO 4762) ; dk_f, s_f tête fraisée et son six pans
/// creux (ISO 10642, prolongée pour M2, M2,5 et M24) ; d1, d2, h rondelle (ISO 7089).
#[derive(Clone, Copy, Debug)]
pub struct Cotes {
    pub d: f64,
    pub p: f64,
    pub s: f64,
    pub k: f64,
    pub m: f64,
    pub dk_c: f64,
    pub k_c: f64,
    pub s_c: f64,
    pub dk_f: f64,
    pub s_f: f64,
    pub d1: f64,
    pub d2: f64,
    pub h: f64,
}

macro_rules! cotes {
    ($d:expr, $p:expr, $s:expr, $k:expr, $m:expr, $dk_c:expr, $k_c:expr, $s_c:expr, $dk_f:expr, $s_f:expr, $d1:expr, $d2:expr, $h:expr) => {
        Cotes { d: $d, p: $p, s: $s, k: $k, m: $m, dk_c: $dk_c, k_c: $k_c, s_c: $s_c, dk_f: $dk_f, s_f: $s_f, d1: $d1, d2: $d2, h: $h }
    };
}

pub const TAILLES: [(&str, Cotes); 12] = [
    ("M2", cotes!(2.0, 0.4, 4.0, 1.4, 1.6, 3.8, 2.0, 1.5, 4.4, 1.3, 2.2, 5.0, 0.3)),
    ("M2,5", cotes!(2.5, 0.45, 5.0, 1.7, 2.0, 4.5, 2.5, 2.0, 5.5, 1.5, 2.7, 6.0, 0.5)),
    ("M3", cotes!(3.0, 0.5, 5.5, 2.0, 2.4, 5.5, 3.0, 2.5, 6.72, 2.0, 3.2, 7.0, 0.5)),
    ("M4", cotes!(4.0, 0.7, 7.0, 2.8, 3.2, 7.0, 4.0, 3.0, 8.96, 2.5, 4.3, 9.0, 0.8)),
    ("M5", cotes!(5.0, 0.8, 8.0, 3.5, 4.7, 8.5, 5.0, 4.0, 11.2, 3.0, 5.3, 10.0, 1.0)),
    ("M6", cotes!(6.0, 1.0, 10.0, 4.0, 5.2, 10.0, 6.0, 5.0, 13.44, 4.0, 6.4, 12.0, 1.6)),
    ("M8", cotes!(8.0, 1.25, 13.0, 5.3, 6.8, 13.0, 8.0, 6.0, 17.92, 5.0, 8.4, 16.0, 1.6)),
    ("M10", cotes!(10.0, 1.5, 16.0, 6.4, 8.4, 16.0, 10.0, 8.0, 22.4, 6.0, 10.5, 20.0, 2.0)),
    ("M12", cotes!(12.0, 1.75, 18.0, 7.5, 10.8, 18.0, 12.0, 10.0, 26.88, 8.0, 13.0, 24.0, 2.5)),
    ("M16", cotes!(16.0, 2.0, 24.0, 10.0, 14.8, 24.0, 16.0, 14.0, 33.6, 10.0, 17.0, 30.0, 3.0)),
    ("M20", cotes!(20.0, 2.5, 30.0, 12.5, 18.0, 30.0, 20.0, 17.0, 40.32, 12.0, 21.0, 37.0, 3.0)),
    ("M24", cotes!(24.0, 3.0, 36.0, 15.0, 21.5, 36.0, 24.0, 19.0, 48.0, 14.0, 25.0, 44.0, 4.0)),
];

/// Réglages d'une pièce. Les champs `Option` sont ceux des paramètres détaillés : lus
/// seulement avec `detail`, et remplaçant alors la valeur de la taille.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub taille: String,
    pub tete: Tete,
    pub longueur: f64,
    pub partiel: bool,
    pub segments: usize,
    pub rangs: usize,
    pub soustraction: bool,
    pub detail: bool,
    pub d: Option<f64>,
    pub p: Option<f64>,
    pub profondeur: Option<f64>,
    pub sommet: Option<f64>,
    pub gorge: Option<f64>,
    pub filetee: Option<f64>,
    pub lisse: Option<f64>,
    pub hauteur: Option<f64>,
    pub plats: Option<f64>,
    pub interieur: Option<f64>,
    pub exterieur: Option<f64>,
    pub epaisseur: Option<f64>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            taille: "M8".into(),
            tete: Tete::Hexagonale,
            longueur: 20.0,
            partiel: false,
            segments: 24,
            rangs: 8,
            soustraction: false,
            detail: false,
            d: None,
            p: None,
            profondeur: None,
            sommet: None,
            gorge: None,
            filetee: None,
            lisse: None,
            hauteur: None,
            plats: None,
            interieur: None,
            exterieur: None,
            epaisseur: None,
        }
    }
}

/// Paramètres effectifs d'une pièce (`Visserie.parametres`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parametres {
    pub d: f64,
    pub p: f64,
    pub profondeur: f64,
    pub sommet: f64,
    pub gorge: f64,
    pub filetee: f64,
    pub lisse: f64,
    pub hauteur: f64,
    pub plats: f64,
    pub interieur: f64,
    pub exterieur: f64,
    pub epaisseur: f64,
}

/// `Math.round` de JavaScript.
fn arrondi(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn cotes(taille: &str) -> Result<Cotes, String> {
    TAILLES.iter().find(|(n, _)| *n == taille).map(|(_, c)| *c).ok_or_else(|| format!("taille inconnue : {taille}"))
}

fn fraisee(t: &Cotes) -> f64 {
    (t.dk_f - t.d) / 2.0
}

pub fn parametres(piece: Piece, o: &Options) -> Result<Parametres, String> {
    let t = cotes(&o.taille)?;
    let mut p = Parametres::default();
    if piece == Piece::Rondelle {
        (p.interieur, p.exterieur, p.epaisseur) = (t.d1, t.d2, t.h);
    } else {
        (p.d, p.p, p.profondeur, p.sommet, p.gorge) = (t.d, t.p, arrondi(0.54127 * t.p * 1e4) / 1e4, t.p / 8.0, t.p / 4.0);
        if piece == Piece::Vis {
            let utile = o.longueur - if o.tete == Tete::Fraisee { fraisee(&t) } else { 0.0 };
            p.filetee = if o.partiel { utile.min(2.0 * t.d + 6.0) } else { utile };
            p.lisse = arrondi((utile - p.filetee) * 1e4) / 1e4;
        }
        if piece == Piece::Ecrou {
            (p.hauteur, p.plats) = (t.m, t.s);
        }
    }
    if o.detail {
        let sur = |c: &mut f64, v: Option<f64>| {
            if let Some(v) = v.filter(|v| v.is_finite()) {
                *c = v;
            }
        };
        if piece == Piece::Rondelle {
            sur(&mut p.interieur, o.interieur);
            sur(&mut p.exterieur, o.exterieur);
            sur(&mut p.epaisseur, o.epaisseur);
        } else {
            sur(&mut p.d, o.d);
            sur(&mut p.p, o.p);
            sur(&mut p.profondeur, o.profondeur);
            sur(&mut p.sommet, o.sommet);
            sur(&mut p.gorge, o.gorge);
            if piece == Piece::Vis {
                sur(&mut p.filetee, o.filetee);
                sur(&mut p.lisse, o.lisse);
            }
            if piece == Piece::Ecrou {
                sur(&mut p.hauteur, o.hauteur);
                sur(&mut p.plats, o.plats);
            }
        }
    }
    Ok(p)
}

/// Pourquoi la pièce est impossible.
pub fn probleme(piece: Piece, o: &Options) -> Option<String> {
    let (t, p) = match (cotes(&o.taille), parametres(piece, o)) {
        (Ok(t), Ok(p)) => (t, p),
        (Err(e), _) | (_, Err(e)) => return Some(e),
    };
    let dit = |s: &str| Some(s.to_string());
    if piece == Piece::Rondelle {
        return if p.interieur > 0.0 && p.exterieur > p.interieur && p.epaisseur > 0.0 { None } else { dit("diamètre intérieur à garder sous l’extérieur") };
    }
    if !(p.d > 0.0 && p.p > 0.0 && p.profondeur > 0.0 && p.sommet >= 0.0 && p.gorge >= 0.0) {
        return dit("réglages invalides");
    }
    if p.sommet + p.gorge >= p.p {
        return dit("sommet + gorge doivent rester sous le pas");
    }
    if p.profondeur >= p.d / 2.0 {
        return dit("filet plus profond que le rayon");
    }
    if piece == Piece::Vis {
        if !(p.filetee > 0.0) {
            return dit(if o.detail {
                "longueur filetée nulle"
            } else if o.tete == Tete::Fraisee {
                "plus courte que sa tête fraisée"
            } else {
                "longueur nulle"
            });
        }
        if !(p.lisse >= 0.0) {
            return dit("longueur lisse négative");
        }
        let tete = match o.tete {
            Tete::Fraisee => t.dk_f / 2.0,
            Tete::Cylindrique => t.dk_c / 2.0,
            Tete::Hexagonale => t.s / 2.0,
        };
        if p.d / 2.0 >= tete {
            return Some(format!("diamètre plus grand que la tête de la taille {}", o.taille));
        }
        if o.tete == Tete::Fraisee && t.s_f / 3f64.sqrt() >= p.d / 2.0 {
            return dit("six pans creux plus large que la tige");
        }
        if o.soustraction && p.filetee + p.lisse <= 1.1 * p.profondeur {
            return dit("trop courte pour son chanfrein");
        }
    }
    if piece == Piece::Tige && o.soustraction && o.longueur <= 2.2 * p.profondeur {
        return dit("trop courte pour ses chanfreins");
    }
    if piece == Piece::Ecrou && !(p.d / 2.0 < p.plats / 2.0 && p.hauteur > 0.0) {
        return dit("taraudage plus large que l’écrou");
    }
    if piece == Piece::Ecrou && o.soustraction && p.hauteur <= 2.3 * p.profondeur {
        return dit("trop mince pour ses chanfreins");
    }
    None
}

fn verifier(piece: Piece, o: &Options) -> Result<Parametres, String> {
    match probleme(piece, o) {
        Some(e) => Err(e),
        None => parametres(piece, o),
    }
}

/// Nombre de sommets, pour prévenir avant de construire une pièce trop lourde.
pub fn sommets(piece: Piece, o: &Options) -> usize {
    let Ok(p) = parametres(piece, o) else { return 0 };
    let n = o.segments as f64;
    if piece == Piece::Rondelle {
        return (4.0 * n) as usize;
    }
    let dy = p.p / o.rangs as f64;
    // Comme dans GDA, l'écrou passe avant la méthode
    if piece == Piece::Ecrou {
        return ((2.0 + (p.hauteur / dy).ceil() + 1.0) * n) as usize;
    }
    // Soustraction : environ huit sommets par copie du profil de l'outil
    if o.soustraction {
        let l = if piece == Piece::Tige { o.longueur } else { p.filetee };
        return ((l / p.p * n).ceil() * 8.0 + 4.0 * n) as usize;
    }
    if piece == Piece::Tige {
        return (((o.longueur / dy).ceil() + 1.0) * n + 2.0) as usize;
    }
    (((p.filetee / dy).ceil() + 8.0) * n + 2.0) as usize
}

/// Rayon de la surface filetée selon la phase u ∈ [0, p) : sommet plat (épaisseur c),
/// flanc, gorge plate (épaisseur g), flanc ; profondeur h. ISO : c = p/8, g = p/4, h = 5H/8.
fn profil(u: f64, p: f64, rmaj: f64, h: f64, c: f64, g: f64) -> f64 {
    let f = (p - c - g) / 2.0;
    if u < c {
        rmaj
    } else if u < c + f {
        rmaj - h * (u - c) / f
    } else if u < c + f + g {
        rmaj - h
    } else {
        rmaj - h + h * (u - c - f - g) / f
    }
}

/// `%` de JavaScript et de Rust : le signe du dividende.
fn modulo(y: f64, p: f64) -> f64 {
    (y % p + p) % p
}

/// Anneau fileté à la hauteur y : un rayon par sommet ; `borne` applique un chanfrein.
fn filet(y: f64, q: &Parametres, n: usize, borne: impl Fn(f64) -> f64) -> Vec<f64> {
    (0..n).map(|k| borne(profil(modulo(y + q.p * k as f64 / n as f64, q.p), q.p, q.d / 2.0, q.profondeur, q.sommet, q.gorge))).collect()
}

/// Rayon d'un hexagone de `s` sur plats, angles sur les multiples de 60°.
fn hexagone(s: f64, t: f64) -> f64 {
    let c = PI / 3.0;
    s / 2.0 / (((t % c) + c) % c - PI / 6.0).cos()
}

/// Anneaux à pas réguliers de y0 vers y1 (y1 compris), dans un sens ou dans l'autre.
fn hauteurs(y0: f64, y1: f64, dy: f64) -> Vec<f64> {
    let n = ((y1 - y0).abs() / dy - 1e-9).ceil().max(1.0) as usize;
    (0..=n).map(|j| y0 + (y1 - y0) * j as f64 / n as f64).collect()
}

/// Forme d'un anneau : rayon constant, hexagone, ou un rayon par sommet.
enum Rayon {
    Fixe(f64),
    Hexagone(f64),
    Liste(Vec<f64>),
}

enum Anneau {
    Axe(f64),
    Tour(f64, Rayon),
}

/// Maillage d'une suite d'anneaux ; `cyclique` : le dernier rejoint le premier.
fn construire(anneaux: &[Anneau], n: usize, cyclique: bool) -> Maillage {
    let mut s: Vec<P3> = Vec::new();
    // (premier sommet, sur l'axe)
    let idx: Vec<(usize, bool)> = anneaux
        .iter()
        .map(|a| match a {
            Anneau::Axe(y) => {
                s.push([0.0, *y, 0.0]);
                (s.len() - 1, true)
            }
            Anneau::Tour(y, r) => {
                let d = s.len();
                for k in 0..n {
                    let t = 2.0 * PI * k as f64 / n as f64;
                    let r = match r {
                        Rayon::Fixe(r) => *r,
                        Rayon::Hexagone(c) => hexagone(*c, t),
                        Rayon::Liste(l) => l[k],
                    };
                    s.push([r * t.cos(), *y, r * t.sin()]);
                }
                (d, false)
            }
        })
        .collect();
    let m = anneaux.len();
    let de = |i: (usize, bool), k: usize| if i.1 { i.0 } else { i.0 + k % n };
    let mut f = Vec::new();
    for i in 0..if cyclique { m } else { m - 1 } {
        let (a, b) = (idx[i], idx[(i + 1) % m]);
        for k in 0..n {
            let q = [de(a, k), de(b, k), de(b, k + 1), de(a, k + 1)];
            let s: Vec<usize> = (0..4).filter(|&j| q[j] != q[(j + 1) % 4]).map(|j| q[j]).collect();
            f.push(Face { s, m: defaut(), uv: Vec::new() });
        }
    }
    Maillage::new(s, f, Vec::new())
}

/// Volume signé (positif : faces vers l'extérieur).
pub fn volume(m: &Maillage) -> f64 {
    let mut v = 0.0;
    for f in &m.faces {
        for k in 1..f.s.len().saturating_sub(1) {
            let [a, b, c] = [m.sommets[f.s[0]], m.sommets[f.s[k]], m.sommets[f.s[k + 1]]];
            v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
        }
    }
    v / 6.0
}

/// Outil de coupe du filet : ruban hélicoïdal fermé du creux entre deux filets, du centre
/// de gorge `y0` (ou en dessous) jusqu'à ce que le profil atteigne `y1` ; n copies du
/// profil par tour ; `sortie` : sur le dernier tour, le profil s'écarte de l'axe.
pub fn outil(q: &Parametres, n: usize, y0: f64, y1: f64, sortie: bool) -> Maillage {
    let (p, r_, h) = (q.p, q.d / 2.0, q.profondeur);
    let (rf, g, pente) = (r_ - h, q.gorge, (p - q.sommet - q.gorge) / 2.0 / h);
    let wmax = ((p - q.sommet) / 2.0 + pente * 0.15 * h).min(0.49 * p);
    let w1 = wmax.max(g / 2.0);
    let r1 = rf + (w1 - g / 2.0) / pente;
    let r2 = r_.max(r1) + 0.5 * h;
    let mut axial = vec![[rf, -g / 2.0], [r1, -w1], [r2, -w1], [r2, w1], [r1, w1], [rf, g / 2.0]];
    if g <= 0.0 {
        axial.pop();
    }
    // Glissement le long de l'hélice jusqu'au plan normal à l'hélice de rayon moyen rm
    let (rm, a) = (r_ - h / 2.0, p / (2.0 * PI));
    let profil: Vec<[f64; 3]> = axial
        .iter()
        .map(|&[r, y]| {
            let mut f = 0.0;
            for _ in 0..8 {
                f -= (rm * r * f64::sin(f) - a * (y - a * f)) / (rm * r * f64::cos(f) + a * a);
            }
            [r, f, y - a * f]
        })
        .collect();
    // Centres de gorge : même hélice que la méthode par anneaux ; θ = −2πk/n
    let haut = profil.iter().map(|c| c[2]).fold(f64::NEG_INFINITY, f64::max);
    let c0 = (p + q.sommet) / 2.0 + ((y0 - (p + q.sommet) / 2.0) / p).floor() * p;
    let nf = n as f64;
    let kk = ((y1 - haut - c0) * nf / p).floor().max(1.0) as usize;
    let m = profil.len();
    let tour = if sortie { n.min(kk) } else { 0 };
    let mut s = Vec::with_capacity((kk + 1) * m);
    for k in 0..=kk {
        let t = -2.0 * PI * k as f64 / nf;
        let d = if tour > 0 && k + tour > kk { 1.08 * h * (k + tour - kk) as f64 / tour as f64 } else { 0.0 };
        for &[r, phi, y] in &profil {
            s.push([(r + d) * (phi + t).cos(), c0 + y - a * t, (r + d) * (phi + t).sin()]);
        }
    }
    let tri = |s: Vec<usize>| Face { s, m: defaut(), uv: Vec::new() };
    let mut f = Vec::with_capacity(kk * m * 2 + 2);
    for k in 0..kk {
        for i in 0..m {
            let (aa, b) = (k * m + i, k * m + (i + 1) % m);
            let (c, d) = (b + m, aa + m);
            f.push(tri(vec![aa, b, c]));
            f.push(tri(vec![aa, c, d]));
        }
    }
    f.push(tri((0..m).rev().collect()));
    f.push(tri((0..m).map(|i| kk * m + i).collect()));
    let mut r = Maillage::new(s, f, Vec::new());
    if volume(&r) < 0.0 {
        r.faces.iter_mut().for_each(|x| x.s.reverse());
    }
    r
}

fn tarauder(corps: &Maillage, q: &Parametres, n: usize, y0: f64, y1: f64, sortie: bool) -> Result<Maillage, String> {
    booleens::operation(corps, &outil(q, n, y0, y1, sortie), Op::Difference)
}

/// Rayon du bout chanfreiné : un peu sous le fond du filet, pour que l'outil n'y affleure pas.
fn pointe(q: &Parametres) -> f64 {
    q.d / 2.0 - 1.05 * q.profondeur
}

/// Vis ou boulon.
pub fn vis(o: &Options) -> Result<Maillage, String> {
    let (corps, coupe) = corps_vis(o)?;
    match coupe {
        Some((q, y0, y1)) => tarauder(&corps, &q, o.segments, y0, y1, true),
        None => Ok(corps),
    }
}

/// Corps de la vis, et ce qu'il reste à y tailler (soustraction).
fn corps_vis(o: &Options) -> Result<(Maillage, Option<(Parametres, f64, f64)>), String> {
    let (t, q) = (cotes(&o.taille)?, verifier(Piece::Vis, o)?);
    let (n, p, r_, rf) = (o.segments, q.p, q.d / 2.0, q.d / 2.0 - q.profondeur);
    let k_f = fraisee(&t);
    let haut = if o.tete == Tete::Fraisee { -k_f } else { 0.0 };
    let longueur = -haut + q.lisse + q.filetee;
    let fin = -longueur + q.filetee;
    let dy = p / o.rangs as f64;
    let mut a = vec![Anneau::Axe(-longueur)];
    if o.soustraction {
        let r0 = pointe(&q);
        a.push(Anneau::Tour(-longueur, Rayon::Fixe(r0)));
        a.push(Anneau::Tour(-longueur + r_ - r0, Rayon::Fixe(r_)));
        a.push(Anneau::Tour(haut, Rayon::Fixe(r_)));
    } else {
        // Filet, chanfreiné à 45° en bout depuis le fond du filet
        for y in hauteurs(-longueur, fin, dy) {
            a.push(Anneau::Tour(y, Rayon::Liste(filet(y, &q, n, |r| r.min(rf + (y + longueur))))));
        }
        if fin < haut - 1e-9 {
            let y = (fin + dy).min(haut);
            a.push(Anneau::Tour(y, Rayon::Fixe(r_)));
            if y < haut - 1e-9 {
                a.push(Anneau::Tour(haut, Rayon::Fixe(r_)));
            }
        }
    }
    match o.tete {
        Tete::Hexagonale => {
            a.push(Anneau::Tour(0.0, Rayon::Hexagone(t.s)));
            a.push(Anneau::Tour(t.k, Rayon::Hexagone(t.s)));
            a.push(Anneau::Axe(t.k));
        }
        Tete::Cylindrique => {
            let e = t.k_c / 2.0;
            a.push(Anneau::Tour(0.0, Rayon::Fixe(t.dk_c / 2.0)));
            a.push(Anneau::Tour(t.k_c, Rayon::Fixe(t.dk_c / 2.0)));
            a.push(Anneau::Tour(t.k_c, Rayon::Hexagone(t.s_c)));
            a.push(Anneau::Tour(t.k_c - e, Rayon::Hexagone(t.s_c)));
            a.push(Anneau::Axe(t.k_c - e));
        }
        Tete::Fraisee => {
            let e = 0.6 * k_f;
            a.push(Anneau::Tour(0.0, Rayon::Fixe(t.dk_f / 2.0)));
            a.push(Anneau::Tour(0.0, Rayon::Hexagone(t.s_f)));
            a.push(Anneau::Tour(-e, Rayon::Hexagone(t.s_f)));
            a.push(Anneau::Axe(-e));
        }
    }
    let corps = construire(&a, n, false);
    Ok((corps, o.soustraction.then_some((q, -longueur - p, fin))))
}

/// Écrou hexagonal, posé sur y = 0, taraudage chanfreiné à 45° sur ses deux faces.
pub fn ecrou(o: &Options) -> Result<Maillage, String> {
    let q = verifier(Piece::Ecrou, o)?;
    let (r_, m, n) = (q.d / 2.0, q.hauteur, o.segments);
    let hex = || Rayon::Hexagone(q.plats);
    if o.soustraction {
        // Prisme aux chanfreins coniques, plein au centre ; moins une tige lisse plus
        // longue que l'écrou, filetée par l'outil : même hélice que la vis
        let h = q.profondeur;
        let (a, b) = (r_ + 0.05 * h, r_ - 1.05 * h);
        let (c, p) = (a - b, q.p);
        let t = |y, r| Anneau::Tour(y, Rayon::Fixe(r));
        let corps = construire(&[Anneau::Axe(c), t(c, b), t(0.0, a), Anneau::Tour(0.0, hex()), Anneau::Tour(m, hex()), t(m, a), t(m - c, b), Anneau::Axe(m - c)], n, false);
        let lisse = construire(&[Anneau::Axe(-p), t(-p, r_), t(m + p, r_), Anneau::Axe(m + p)], n, false);
        let tige = tarauder(&lisse, &q, n, -2.0 * p, m + 2.0 * p, false)?;
        return booleens::operation(&corps, &tige, Op::Difference);
    }
    let mut a = vec![Anneau::Tour(0.0, hex()), Anneau::Tour(m, hex())];
    for y in hauteurs(m, 0.0, q.p / o.rangs as f64) {
        a.push(Anneau::Tour(y, Rayon::Liste(filet(y, &q, n, |r| r.max(r_ - y.min(m - y))))));
    }
    Ok(construire(&a, n, true))
}

/// Rondelle plate, posée sur y = 0.
pub fn rondelle(o: &Options) -> Result<Maillage, String> {
    let q = verifier(Piece::Rondelle, o)?;
    let (a, b, h) = (q.interieur / 2.0, q.exterieur / 2.0, q.epaisseur);
    let t = |y, r| Anneau::Tour(y, Rayon::Fixe(r));
    Ok(construire(&[t(0.0, b), t(h, b), t(h, a), t(0.0, a)], o.segments, true))
}

/// Tige filetée centrée sur l'origine, chanfreinée aux deux bouts.
pub fn tige(o: &Options) -> Result<Maillage, String> {
    let q = verifier(Piece::Tige, o)?;
    let (n, rf, l) = (o.segments, q.d / 2.0 - q.profondeur, o.longueur / 2.0);
    if o.soustraction {
        let (r_, r0) = (q.d / 2.0, pointe(&q));
        let c = r_ - r0;
        let t = |y, r| Anneau::Tour(y, Rayon::Fixe(r));
        let corps = construire(&[Anneau::Axe(-l), t(-l, r0), t(-l + c, r_), t(l - c, r_), t(l, r0), Anneau::Axe(l)], n, false);
        return tarauder(&corps, &q, n, -l - q.p, l + q.p, false);
    }
    let mut a = vec![Anneau::Axe(-l)];
    for y in hauteurs(-l, l, q.p / o.rangs as f64) {
        a.push(Anneau::Tour(y, Rayon::Liste(filet(y, &q, n, |r| r.min(rf + (y + l).min(l - y))))));
    }
    a.push(Anneau::Axe(l));
    Ok(construire(&a, n, false))
}

pub fn creer(piece: Piece, o: &Options) -> Result<Maillage, String> {
    match piece {
        Piece::Vis => vis(o),
        Piece::Ecrou => ecrou(o),
        Piece::Rondelle => rondelle(o),
        Piece::Tige => tige(o),
    }
}
