//! Édition en mode Scène, sans interface : pose des objets, pivots, annulation.
//! Portage de `_poser`, `_pivot`, `_centre`, `_euler` de GDA, vérifié face à
//! l'original par `tests/differentiel.rs`.

use crate::json::J;
use crate::maillage::{Maillage, P3};
use crate::projet::{Materiau, Objet, Set};
use std::sync::Arc;

pub type Q = [f64; 4];

/// Étapes d'annulation conservées.
pub const HISTOIRE: usize = 60;

pub fn qmul(a: Q, b: Q) -> Q {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        ax * bw + aw * bx + ay * bz - az * by,
        ay * bw + aw * by + az * bx - ax * bz,
        az * bw + aw * bz + ax * by - ay * bx,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn qnorm(q: Q) -> Q {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l == 0.0 { [0.0, 0.0, 0.0, 1.0] } else { [q[0] / l, q[1] / l, q[2] / l, q[3] / l] }
}

pub fn qinv(q: Q) -> Q {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Rotation d'angle `a` autour de l'axe unitaire `u`.
pub fn qaxe(u: P3, a: f64) -> Q {
    let s = (a / 2.0).sin();
    [u[0] * s, u[1] * s, u[2] * s, (a / 2.0).cos()]
}

pub fn qappliquer(q: Q, v: P3) -> P3 {
    let [x, y, z, w] = q;
    let t = [2.0 * (y * v[2] - z * v[1]), 2.0 * (z * v[0] - x * v[2]), 2.0 * (x * v[1] - y * v[0])];
    [
        v[0] + w * t[0] + y * t[2] - z * t[1],
        v[1] + w * t[1] + z * t[0] - x * t[2],
        v[2] + w * t[2] + x * t[1] - y * t[0],
    ]
}

fn q_matrice(q: Q) -> [f64; 16] {
    crate::maillage::composer([0.0; 3], q, [1.0; 3])
}

/// Si la rotation est faite de quarts de tour : l'axe `n` de l'objet est parallèle
/// à l'axe `a[n]` du monde. `None` si l'objet est de biais.
pub fn axes_alignes(q: Q) -> Option<[usize; 3]> {
    let m = q_matrice(q);
    let mut a = [0; 3];
    for n in 0..3 {
        a[n] = (0..3).find(|c| (m[4 * n + c].abs() - 1.0).abs() < 1e-6)?;
    }
    Some(a)
}

/// Angles en degrés, ordre XYZ.
pub fn euler(q: Q) -> [f64; 3] {
    let m = q_matrice(q);
    let (m11, m12, m13, m22, m23, m32, m33) = (m[0], m[4], m[8], m[5], m[9], m[6], m[10]);
    let y = m13.clamp(-1.0, 1.0).asin();
    let (x, z) = if m13.abs() < 0.9999999 { ((-m23).atan2(m33), (-m12).atan2(m11)) } else { (m32.atan2(m22), 0.0) };
    [x, y, z].map(|v| v.to_degrees() + 0.0)
}

pub fn depuis_euler(d: [f64; 3]) -> Q {
    let [a, b, c] = d.map(|v| v.to_radians() / 2.0);
    let (c1, c2, c3, s1, s2, s3) = (a.cos(), b.cos(), c.cos(), a.sin(), b.sin(), c.sin());
    [s1 * c2 * c3 + c1 * s2 * s3, c1 * s2 * c3 - s1 * c2 * s3, c1 * c2 * s3 + s1 * s2 * c3, c1 * c2 * c3 - s1 * s2 * s3]
}

/// État de départ d'un objet transformé.
#[derive(Clone, Debug)]
pub struct Depart {
    pub p: P3,
    pub q: Q,
    pub e: P3,
    pub m: Arc<Maillage>,
}

pub fn depart(objets: &[Objet], indices: &[usize]) -> Vec<Depart> {
    indices.iter().map(|&k| { let o = &objets[k]; Depart { p: o.position, q: o.rotation, e: o.echelle, m: o.maillage.clone() } }).collect()
}

/// Un objet lié, emporté par son armature choisie : leurs états de départ.
#[derive(Clone, Copy, Debug)]
pub struct Suivi {
    objet: usize,
    armature: usize,
    o: (P3, Q, P3),
    a: (P3, Q, f64),
}

/// Objets emportés par les armatures de `choix` ; ceux qui sont choisis bougent déjà.
pub fn suivis(objets: &[Objet], choix: &[usize]) -> Vec<Suivi> {
    let mut l = Vec::new();
    for &armature in choix {
        let a = &objets[armature];
        if a.armature.is_none() {
            continue;
        }
        for (objet, o) in objets.iter().enumerate() {
            if !choix.contains(&objet) && o.liaison.as_ref().is_some_and(|x| x.armature == a.id) {
                l.push(Suivi { objet, armature, o: (o.position, o.rotation, o.echelle), a: (a.position, a.rotation, a.echelle[0]) });
            }
        }
    }
    l
}

/// Replace les objets suivis : leur place par rapport à l'armature ne change pas.
pub fn suivre(objets: &mut [Objet], l: &[Suivi]) {
    for s in l {
        let a = &objets[s.armature];
        let (p, q, e) = (a.position, a.rotation, a.echelle[0]);
        let ((op, oq, oe), (dp, dq, de)) = (s.o, s.a);
        let ri = qinv(qnorm(dq));
        let r = qappliquer(q, qappliquer(ri, [0, 1, 2].map(|k| (op[k] - dp[k]) / de)));
        let o = &mut objets[s.objet];
        o.position = [0, 1, 2].map(|k| p[k] + e * r[k]);
        o.rotation = qnorm(qmul(q, qmul(ri, oq)));
        o.echelle = oe.map(|x| x * e / de);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Transfo {
    Translation(P3),
    Rotation { q: Q, c: P3 },
    Echelle { s: P3, c: P3 },
}

/// Un point, transformé.
pub fn transformer(v: P3, t: Transfo) -> P3 {
    match t {
        Transfo::Translation(d) => [v[0] + d[0], v[1] + d[1], v[2] + d[2]],
        Transfo::Rotation { q, c } => {
            let r = qappliquer(q, [v[0] - c[0], v[1] - c[1], v[2] - c[2]]);
            [r[0] + c[0], r[1] + c[1], r[2] + c[2]]
        }
        Transfo::Echelle { s, c } => [0, 1, 2].map(|k| (v[k] - c[k]) * s[k] + c[k]),
    }
}

/// Place les objets `indices` : leur état de départ, transformé.
pub fn poser(objets: &mut [Objet], indices: &[usize], departs: &[Depart], t: Transfo) {
    for (&k, e) in indices.iter().zip(departs) {
        let v = e.p;
        let o = &mut objets[k];
        match t {
            Transfo::Translation(d) => o.position = [v[0] + d[0], v[1] + d[1], v[2] + d[2]],
            Transfo::Rotation { q, c } => {
                let r = qappliquer(q, [v[0] - c[0], v[1] - c[1], v[2] - c[2]]);
                o.position = [r[0] + c[0], r[1] + c[1], r[2] + c[2]];
                o.rotation = qnorm(qmul(q, e.q));
            }
            Transfo::Echelle { s, c } => {
                o.position = [0, 1, 2].map(|n| (v[n] - c[n]) * s[n] + c[n]);
                if o.armature.is_some() {
                    // Échelle uniforme : le facteur le plus marqué des trois
                    let f = s.into_iter().reduce(|a, b| if (b - 1.0).abs() > (a - 1.0).abs() { b } else { a }).unwrap();
                    o.echelle = e.e.map(|x| x * f);
                } else if let Some(a) = axes_alignes(e.q) {
                    o.echelle = [0, 1, 2].map(|n| e.e[n] * s[a[n]]);
                } else {
                    // De biais : l'échelle du monde passe dans le maillage
                    let (r, ri) = (e.q, qinv(qnorm(e.q)));
                    let mut m = (*e.m).clone();
                    for p in &mut m.sommets {
                        let w = qappliquer(r, [p[0] * e.e[0], p[1] * e.e[1], p[2] * e.e[2]]);
                        let w = qappliquer(ri, [w[0] * s[0], w[1] * s[1], w[2] * s[2]]);
                        *p = [w[0] / e.e[0], w[1] / e.e[1], w[2] / e.e[2]];
                    }
                    o.maillage = Arc::new(m);
                }
            }
        }
    }
}

/// Longueur des axes de la poignée à l'écran, en dp.
pub const POIGNEE: f64 = 90.0;
/// Distance de capture d'une poignée sous le doigt, en dp.
pub const CAPTURE: f64 = 32.0;
const SEGMENTS: usize = 48;
const BOUT_FLECHE: f64 = 0.92;
const BOUT_ECHELLE: f64 = 0.9;
pub const RAYON_VUE: f64 = 1.22;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Translation,
    Rotation,
    Echelle,
}

/// Partie de la poignée tenue : l'origine, un axe, ou l'anneau de vue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Prise {
    O,
    Axe(usize),
    V,
}

/// Pas des transformations, réglables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pas {
    pub actif: bool,
    /// `None` : la moitié d'un carreau de la grille.
    pub translation: Option<f64>,
    pub angle: f64,
    pub echelle: f64,
}

impl Default for Pas {
    fn default() -> Pas {
        Pas { actif: false, translation: None, angle: 15.0, echelle: 0.1 }
    }
}

fn sous(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scal(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unitaire(a: P3) -> P3 {
    let l = scal(a, a).sqrt();
    if l == 0.0 { a } else { a.map(|v| v / l) }
}

fn axe(k: usize) -> P3 {
    let mut u = [0.0; 3];
    u[k] = 1.0;
    u
}

/// Caméra vue de l'écran : pixels relatifs à l'espace de travail.
#[derive(Clone, Copy, Debug)]
pub struct Vue {
    pub oeil: P3,
    pub droite: P3,
    pub haut: P3,
    pub dos: P3,
    pub largeur: f64,
    pub hauteur: f64,
    /// Tangente de la moitié du champ vertical.
    pub tan: f64,
    pub pres: f64,
}

impl Vue {
    fn profondeur(&self, p: P3) -> f64 {
        -scal(sous(p, self.oeil), self.dos)
    }

    /// Taille d'un pixel à la profondeur de `p`.
    pub fn pixel(&self, p: P3) -> f64 {
        2.0 * self.profondeur(p).max(self.pres) * self.tan / self.hauteur.max(1.0)
    }

    /// Pixel de `p`, `None` s'il est derrière la caméra.
    pub fn ecran(&self, p: P3) -> Option<[f64; 2]> {
        let z = self.profondeur(p);
        if z < self.pres {
            return None;
        }
        let v = sous(p, self.oeil);
        let k = self.hauteur / (2.0 * z * self.tan);
        Some([self.largeur / 2.0 + scal(v, self.droite) * k, self.hauteur / 2.0 - scal(v, self.haut) * k])
    }

    /// Pixel de `p` et inverse de sa profondeur.
    pub fn ecran_q(&self, p: P3) -> Option<[f64; 3]> {
        let e = self.ecran(p)?;
        Some([e[0], e[1], 1.0 / self.profondeur(p)])
    }

    /// Direction unitaire du rayon sous le pixel.
    pub fn rayon(&self, x: f64, y: f64) -> P3 {
        let t = self.tan;
        let nx = (x / self.largeur * 2.0 - 1.0) * t * self.largeur / self.hauteur.max(1.0);
        let ny = (1.0 - y / self.hauteur * 2.0) * t;
        unitaire([0, 1, 2].map(|k| self.droite[k] * nx + self.haut[k] * ny - self.dos[k]))
    }

    /// Point du plan (normale `n`, passant par `p`) sous le pixel ; `None` s'il est rasant.
    fn sur_plan(&self, x: f64, y: f64, n: P3, p: P3) -> Option<P3> {
        let d = self.rayon(x, y);
        let den = scal(d, n);
        if den.abs() < 0.12 {
            return None;
        }
        let t = scal(sous(p, self.oeil), n) / den;
        (t > 0.0).then(|| [0, 1, 2].map(|k| self.oeil[k] + d[k] * t))
    }
}

/// Où se trouve la poignée dans le monde, pour le dessin comme pour le toucher.
#[derive(Clone, Copy, Debug)]
pub struct Poignee {
    pub mode: Mode,
    pub o: P3,
    /// Longueur d'un axe, dans le monde.
    pub taille: f64,
    pub axes: [bool; 3],
    /// Orientation des axes : ceux du monde, ou ceux d'un os posé.
    pub repere: Q,
}

impl Poignee {
    /// `dp` : taille d'un dp en pixels.
    pub fn new(mode: Mode, centre: P3, pivot: P3, axes: [bool; 3], vue: &Vue, dp: f64) -> Poignee {
        let o = if mode == Mode::Translation { centre } else { pivot };
        Poignee { mode, o, taille: vue.pixel(o) * POIGNEE * dp, axes, repere: [0.0, 0.0, 0.0, 1.0] }
    }

    /// Axe `k` de la poignée, dans le monde.
    pub fn axe(&self, k: usize) -> P3 {
        qappliquer(self.repere, axe(k))
    }

    pub fn bout(&self, k: usize) -> P3 {
        let l = self.taille * if self.mode == Mode::Echelle { BOUT_ECHELLE } else { BOUT_FLECHE };
        let u = self.axe(k);
        [0, 1, 2].map(|i| self.o[i] + u[i] * l)
    }

    /// Anneau d'un axe : perpendiculaire à l'axe, s'il est coché.
    pub fn anneau(&self, k: usize, n: usize) -> Vec<P3> {
        let (u, v) = (self.axe((k + 1) % 3), self.axe((k + 2) % 3));
        cercle(self.o, u, v, self.taille, n)
    }

    /// Anneau de vue : face à la caméra.
    pub fn anneau_vue(&self, vue: &Vue, n: usize) -> Vec<P3> {
        cercle(self.o, vue.droite, vue.haut, self.taille * RAYON_VUE, n)
    }

    pub fn a_anneau_vue(&self) -> bool {
        match self.mode {
            Mode::Translation => false,
            Mode::Rotation => self.axes == [true; 3],
            Mode::Echelle => self.axes.contains(&true),
        }
    }

    /// Partie de la poignée sous le pixel.
    pub fn sous(&self, x: f64, y: f64, vue: &Vue, dp: f64) -> Option<Prise> {
        let mut dmin = CAPTURE * dp;
        let mut choix = None;
        let mut essayer = |prise: Prise, p: P3, dmin: &mut f64| {
            if let Some(e) = vue.ecran(p) {
                let d = (e[0] - x).hypot(e[1] - y);
                if d < *dmin {
                    *dmin = d;
                    choix = Some(prise);
                }
            }
        };
        essayer(Prise::O, self.o, &mut dmin);
        if self.mode != Mode::Rotation {
            for k in (0..3).filter(|k| self.axes[*k]) {
                essayer(Prise::Axe(k), self.bout(k), &mut dmin);
            }
            if self.mode == Mode::Translation {
                return choix;
            }
        }
        let mut trace = |prise: Prise, pts: Vec<P3>, dmin: &mut f64| {
            let e: Vec<Option<[f64; 2]>> = pts.into_iter().map(|p| vue.ecran(p)).collect();
            for w in e.windows(2) {
                if let [Some(a), Some(b)] = [w[0], w[1]] {
                    let d = distance_segment(a, b, x, y);
                    if d < *dmin {
                        *dmin = d;
                        choix = Some(prise);
                    }
                }
            }
        };
        if self.mode == Mode::Rotation {
            for k in (0..3).filter(|k| self.axes[*k]) {
                trace(Prise::Axe(k), self.anneau(k, SEGMENTS), &mut dmin);
            }
        }
        if self.a_anneau_vue() {
            trace(Prise::V, self.anneau_vue(vue, SEGMENTS), &mut dmin);
        }
        choix
    }
}

fn cercle(o: P3, u: P3, v: P3, r: f64, n: usize) -> Vec<P3> {
    (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64 * std::f64::consts::TAU;
            let (c, s) = (t.cos() * r, t.sin() * r);
            [0, 1, 2].map(|i| o[i] + u[i] * c + v[i] * s)
        })
        .collect()
}

pub fn distance_segment(a: [f64; 2], b: [f64; 2], x: f64, y: f64) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (a[0] + t * dx - x).hypot(a[1] + t * dy - y)
}

/// Glissé en cours : tout ce qui est figé au moment où le doigt prend la poignée.
#[derive(Clone, Debug)]
pub struct Geste {
    pub mode: Mode,
    pub prise: Prise,
    pub indices: Vec<usize>,
    pub departs: Vec<Depart>,
    /// Mode Géométrie : positions de départ des sommets `indices`.
    pub sommets: Vec<P3>,
    x0: f64,
    y0: f64,
    vue: Vue,
    axes: [bool; 3],
    centre: P3,
    pivot: P3,
    pixel: f64,
    // Rotation
    axe: P3,
    signe: f64,
    centre_ecran: [f64; 2],
    angle: f64,
    cumul: f64,
    theta: f64,
    // Échelle
    libres: [bool; 3],
    dir: Option<[f64; 2]>,
}

impl Geste {
    #[allow(clippy::too_many_arguments)]
    pub fn new(mode: Mode, prise: Prise, x: f64, y: f64, vue: Vue, axes: [bool; 3], indices: Vec<usize>, departs: Vec<Depart>, centre: P3, pivot: P3) -> Geste {
        let mut g = Geste {
            mode,
            prise,
            indices,
            departs,
            sommets: Vec::new(),
            x0: x,
            y0: y,
            vue,
            axes,
            centre,
            pivot,
            pixel: vue.pixel(centre),
            axe: [0.0; 3],
            signe: 1.0,
            centre_ecran: vue.ecran(pivot).unwrap_or([x, y]),
            angle: 0.0,
            cumul: 0.0, theta: 0.0,
            libres: axes,
            dir: None,
        };
        match mode {
            Mode::Translation => {}
            Mode::Rotation => {
                let v = unitaire(sous(vue.oeil, pivot));
                let k = match prise {
                    Prise::Axe(k) => Some(k),
                    _ if prise == Prise::V || axes == [true; 3] => None,
                    _ => (0..3).filter(|k| axes[*k]).max_by(|a, b| v[*a].abs().total_cmp(&v[*b].abs())),
                };
                g.axe = k.map_or(v, axe);
                g.signe = if scal(g.axe, v) < 0.0 { -1.0 } else { 1.0 };
                g.angle = (y - g.centre_ecran[1]).atan2(x - g.centre_ecran[0]);
            }
            Mode::Echelle => {
                if let Prise::Axe(k) = prise {
                    g.libres = [false; 3];
                    g.libres[k] = true;
                    let mut b = pivot;
                    b[k] += g.pixel * 100.0;
                    if let Some(b) = vue.ecran(b) {
                        let (dx, dy) = (b[0] - g.centre_ecran[0], b[1] - g.centre_ecran[1]);
                        let l = dx.hypot(dy);
                        g.dir = (l > 1e-3).then(|| [dx / l, dy / l]);
                    }
                }
            }
        }
        g
    }

    /// Rotation autour des axes d'un repère tourné de `q`, ceux d'un os posé, au lieu
    /// de ceux du monde.
    pub fn orienter(&mut self, q: Q) {
        if self.mode != Mode::Rotation {
            return;
        }
        let v = unitaire(sous(self.vue.oeil, self.pivot));
        let ax = |k: usize| qappliquer(q, axe(k));
        let k = match self.prise {
            Prise::Axe(k) => Some(k),
            _ if self.prise == Prise::V || self.axes == [true; 3] => None,
            _ => (0..3).filter(|k| self.axes[*k]).max_by(|a, b| scal(ax(*a), v).abs().total_cmp(&scal(ax(*b), v).abs())),
        };
        self.axe = k.map_or(v, ax);
        self.signe = if scal(self.axe, v) < 0.0 { -1.0 } else { 1.0 };
    }

    fn deplacer(&self, x: f64, y: f64, pas: &Pas, pas_t: f64) -> P3 {
        let libres: Vec<usize> = match self.prise {
            Prise::Axe(k) => vec![k],
            _ => (0..3).filter(|k| self.axes[*k]).collect(),
        };
        let (sx, sy) = (x - self.x0, y - self.y0);
        let (droite, haut, pixel) = (self.vue.droite, self.vue.haut, self.pixel);
        let mut d = [0.0; 3];
        if let [k] = libres[..] {
            let u = axe(k);
            let (ux, uy) = (scal(u, droite) / pixel, -scal(u, haut) / pixel);
            let l2 = ux * ux + uy * uy;
            if l2 > 1e-6 {
                d[k] = (sx * ux + sy * uy) / l2;
            }
        } else {
            let bloque = (0..3).find(|k| !libres.contains(k));
            let mut fait = false;
            if let Some(b) = bloque {
                let n = axe(b);
                if let Some(p0) = self.vue.sur_plan(self.x0, self.y0, n, self.centre) {
                    if let Some(p1) = self.vue.sur_plan(x, y, n, self.centre) {
                        d = sous(p1, p0);
                        fait = true;
                    }
                }
            }
            if !fait {
                d = [0, 1, 2].map(|k| droite[k] * sx * pixel - haut[k] * sy * pixel);
            }
            if let Some(b) = bloque {
                d[b] = 0.0;
            }
        }
        if pas.actif {
            d = d.map(|v| (v / pas_t).round() * pas_t);
        }
        d
    }

    fn tourner(&mut self, x: f64, y: f64, pas: &Pas) -> Q {
        use std::f64::consts::{PI, TAU};
        let a = (y - self.centre_ecran[1]).atan2(x - self.centre_ecran[0]);
        let mut da = a - self.angle;
        if da > PI {
            da -= TAU;
        }
        if da < -PI {
            da += TAU;
        }
        self.angle = a;
        self.cumul += da;
        // Écran : y vers le bas, l'angle croît dans le sens horaire
        let mut theta = -self.cumul * self.signe;
        if pas.actif {
            let p = pas.angle.to_radians();
            theta = (theta / p).round() * p;
        }
        self.theta = theta;
        qaxe(self.axe, theta)
    }

    /// Axe et angle (radians) de la rotation en cours.
    pub fn rotation(&self) -> (P3, f64) {
        (self.axe, self.theta)
    }

    fn echelonner(&self, x: f64, y: f64, pas: &Pas) -> P3 {
        let e = self.centre_ecran;
        let mesure = |px: f64, py: f64| match self.dir {
            Some(d) => (px - e[0]) * d[0] + (py - e[1]) * d[1],
            None => (px - e[0]).hypot(py - e[1]),
        };
        let d0 = mesure(self.x0, self.y0);
        let mut f = if d0.abs() < 20.0 { 1.0 + ((x - self.x0) - (y - self.y0)) / 120.0 } else { mesure(x, y) / d0 };
        if pas.actif {
            f = (f / pas.echelle).round() * pas.echelle;
        }
        let f = f.max(0.01); // ni facteur nul, ni faces retournées
        [0, 1, 2].map(|k| if self.libres[k] { f } else { 1.0 })
    }

    /// Transformation que donne le doigt en (`x`, `y`). `pas_t` : pas de translation.
    pub fn suivre(&mut self, x: f64, y: f64, pas: &Pas, pas_t: f64) -> Transfo {
        match self.mode {
            Mode::Translation => Transfo::Translation(self.deplacer(x, y, pas, pas_t)),
            Mode::Rotation => Transfo::Rotation { q: self.tourner(x, y, pas), c: self.pivot },
            Mode::Echelle => Transfo::Echelle { s: self.echelonner(x, y, pas), c: self.pivot },
        }
    }
}

pub fn centre(pts: &[P3]) -> P3 {
    let n = pts.len() as f64;
    let mut c = [0.0; 3];
    for p in pts {
        for k in 0..3 {
            c[k] += p[k] / n;
        }
    }
    c
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pivot {
    Median,
    Boite,
    Actif,
    Curseur,
    Origine,
}

pub const PIVOTS: [(Pivot, &str); 5] = [
    (Pivot::Median, "Point médian"),
    (Pivot::Boite, "Centre de la boîte"),
    (Pivot::Actif, "Élément actif"),
    (Pivot::Curseur, "Curseur 3D"),
    (Pivot::Origine, "Origine"),
];

/// Pivot de la rotation ou de l'échelle ; `pts` n'est pas vide, l'actif en dernier.
pub fn pivot(mode: Pivot, pts: &[P3], curseur: P3) -> P3 {
    match mode {
        Pivot::Curseur => curseur,
        Pivot::Origine => [0.0; 3],
        Pivot::Actif => pts[pts.len() - 1],
        Pivot::Boite => {
            let (mut min, mut max) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for p in pts {
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
            [0, 1, 2].map(|k| (min[k] + max[k]) / 2.0)
        }
        Pivot::Median => centre(pts),
    }
}

/// Nom libre : « cube », « cube 2 »…
pub fn nom_libre(objets: &[Objet], base: &str) -> String {
    let pris = |n: &str| objets.iter().any(|o| o.nom == n);
    if !pris(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base} {n}")).find(|s| !pris(s)).unwrap()
}

/// Échelle figée dans le maillage ; la forme ne change pas. Armatures exclues :
/// leur échelle se fige avec leurs os, pas encore portés.
pub fn figer_echelle(o: &mut Objet) {
    let e = o.echelle;
    let m = Arc::make_mut(&mut o.maillage);
    for p in &mut m.sommets {
        *p = [p[0] * e[0], p[1] * e[1], p[2] * e[2]];
    }
    // Le décalage constant d'une répétition suit, pour que les copies restent en place
    if let J::Liste(l) = &mut o.modifs {
        for m in l {
            let repete = m.get("type").and_then(J::txt) == Some("repetition");
            let relatif = m.get("relatif").is_some_and(|r| !matches!(r, J::Nul | J::Bool(false)));
            let Some(d) = m.get("decalage").and_then(|d| d.nombres::<3>()).filter(|_| repete && !relatif) else { continue };
            if let J::Objet(c) = m {
                let v = J::Liste((0..3).map(|k| J::Num(d[k] * e[k])).collect());
                c.iter_mut().find(|(n, _)| n == "decalage").unwrap().1 = v;
            }
        }
    }
    o.echelle = [1.0; 3];
}

/// Copie en coordonnées du monde.
pub fn vers_monde(o: &Objet) -> Maillage {
    vers_monde_de(o, &o.maillage)
}

/// Copie de `base`, un maillage dans le repère de `o`, en coordonnées du monde.
pub fn vers_monde_de(o: &Objet, base: &Maillage) -> Maillage {
    let mx = o.matrice();
    let mut m = base.clone();
    for p in &mut m.sommets {
        *p = crate::vue::appliquer(&mx, *p);
    }
    m
}

/// Étape d'annulation : les maillages inchangés sont partagés entre les étapes.
#[derive(Clone, Debug, PartialEq)]
pub struct Etape {
    pub objets: Vec<Objet>,
    pub sets: Vec<Set>,
    pub materiaux: Vec<(String, Materiau)>,
    pub actif: Option<usize>,
    pub choix: Vec<usize>,
    /// Copie de travail : l'étape garde aussi le mode.
    pub travail: Option<crate::geometrie::Travail>,
    pub os: Option<crate::armature::EditionOs>,
}

#[derive(Default)]
pub struct Historique {
    pub annuler: Vec<Etape>,
    pub retablir: Vec<Etape>,
}

impl Historique {
    pub fn memoriser(&mut self, e: Etape) {
        self.annuler.push(e);
        if self.annuler.len() > HISTOIRE {
            self.annuler.remove(0);
        }
        self.retablir.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presque(a: &[f64], b: &[f64]) {
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9), "{a:?} ≠ {b:?}");
    }

    #[test]
    fn un_quart_de_tour_est_aligne() {
        let q = qaxe([0.0, 1.0, 0.0], std::f64::consts::FRAC_PI_2);
        assert_eq!(axes_alignes(q), Some([2, 1, 0]));
        assert_eq!(axes_alignes(qaxe([0.0, 1.0, 0.0], 0.3)), None);
    }

    #[test]
    fn euler_aller_retour() {
        let d = [12.0, -40.0, 75.0];
        presque(&euler(depuis_euler(d)), &d);
    }

    #[test]
    fn les_noms_ne_se_repetent_pas() {
        let l = [Objet::neuf("cube", Maillage::cube(1.0)), Objet::neuf("cube 2", Maillage::cube(1.0))];
        assert_eq!(nom_libre(&l, "cube"), "cube 3");
        assert_eq!(nom_libre(&l, "sphere"), "sphere");
    }

    #[test]
    fn figer_l_echelle_ne_change_pas_la_forme() {
        let mut o = Objet::neuf("c", Maillage::cube(2.0));
        o.echelle = [2.0, 1.0, 3.0];
        o.rotation = qaxe([0.0, 1.0, 0.0], 0.4);
        let avant = vers_monde(&o);
        figer_echelle(&mut o);
        assert_eq!(o.echelle, [1.0; 3]);
        for (a, b) in avant.sommets.iter().zip(&vers_monde(&o).sommets) {
            presque(a, b);
        }
    }

    #[test]
    fn un_maillage_partage_n_est_pas_modifie_par_le_figeage() {
        let mut o = Objet::neuf("c", Maillage::cube(2.0));
        let copie = o.clone();
        o.echelle = [2.0; 3];
        figer_echelle(&mut o);
        assert_eq!(copie.maillage.sommets[0], [-1.0; 3]);
    }

    #[test]
    fn le_decalage_constant_suit_l_echelle() {
        let mut o = Objet::neuf("c", Maillage::cube(1.0));
        o.echelle = [2.0, 3.0, 1.0];
        let t = r#"[{"type":"repetition","decalage":[1,1,1]},{"type":"repetition","relatif":true,"decalage":[1,1,1]}]"#;
        o.modifs = crate::json::lire(t).unwrap();
        figer_echelle(&mut o);
        let d = |k: usize| o.modifs.liste().unwrap()[k].get("decalage").unwrap().nombres::<3>().unwrap();
        assert_eq!((d(0), d(1)), ([2.0, 3.0, 1.0], [1.0; 3]));
    }

    #[test]
    fn l_histoire_est_bornee() {
        let mut h = Historique::default();
        let e = Etape { objets: vec![], sets: vec![], materiaux: vec![], actif: None, choix: vec![], travail: None, os: None };
        for _ in 0..HISTOIRE + 5 {
            h.memoriser(e.clone());
        }
        assert_eq!(h.annuler.len(), HISTOIRE);
    }

    fn vue() -> Vue {
        // Regarde vers −Z depuis (0, 0, 10) ; écran 1000 × 1000, 45° verticaux environ
        Vue { oeil: [0.0, 0.0, 10.0], droite: [1.0, 0.0, 0.0], haut: [0.0, 1.0, 0.0], dos: [0.0, 0.0, 1.0], largeur: 1000.0, hauteur: 1000.0, tan: 0.5, pres: 0.05 }
    }

    fn geste(mode: Mode, prise: Prise, x: f64, y: f64, axes: [bool; 3]) -> Geste {
        let o = Objet::neuf("c", Maillage::cube(1.0));
        Geste::new(mode, prise, x, y, vue(), axes, vec![0], depart(&[o], &[0]), [0.0; 3], [0.0; 3])
    }

    #[test]
    fn projection_et_rayon_s_accordent() {
        let v = vue();
        let e = v.ecran([1.0, 2.0, 0.0]).unwrap();
        presque(&e, &[600.0, 300.0]);
        let d = v.rayon(e[0], e[1]);
        let t = 10.0 / -d[2];
        presque(&[d[0] * t, d[1] * t], &[1.0, 2.0]);
        assert!(v.ecran([0.0, 0.0, 11.0]).is_none());
    }

    #[test]
    fn le_bout_d_un_axe_se_touche() {
        let v = vue();
        let p = Poignee::new(Mode::Translation, [0.0; 3], [0.0; 3], [true; 3], &v, 1.0);
        presque(&[p.taille], &[0.9]);
        let e = v.ecran(p.bout(0)).unwrap();
        assert_eq!(p.sous(e[0], e[1], &v, 1.0), Some(Prise::Axe(0)));
        assert_eq!(p.sous(500.0, 500.0, &v, 1.0), Some(Prise::O));
        assert_eq!(p.sous(50.0, 50.0, &v, 1.0), None);
        let sans_x = Poignee { axes: [false, true, true], ..p };
        assert_ne!(sans_x.sous(e[0], e[1], &v, 1.0), Some(Prise::Axe(0)));
    }

    #[test]
    fn un_anneau_se_touche_partout_sauf_decoche() {
        let v = vue();
        let p = Poignee::new(Mode::Rotation, [0.0; 3], [0.0; 3], [true; 3], &v, 1.0);
        let e = v.ecran(p.anneau(2, 48)[6]).unwrap(); // l'anneau Z est face à la caméra
        assert_eq!(p.sous(e[0], e[1], &v, 1.0), Some(Prise::Axe(2)));
        let e = v.ecran(p.anneau_vue(&v, 48)[12]).unwrap();
        assert_eq!(p.sous(e[0], e[1], &v, 1.0), Some(Prise::V));
        assert!(!Poignee { axes: [true, true, false], ..p }.a_anneau_vue());
    }

    #[test]
    fn un_axe_tenu_ne_bouge_que_lui() {
        let mut g = geste(Mode::Translation, Prise::Axe(0), 500.0, 500.0, [true; 3]);
        let Transfo::Translation(d) = g.suivre(600.0, 700.0, &Pas::default(), 1.0) else { panic!() };
        presque(&d, &[1.0, 0.0, 0.0]); // 100 px à 0,01 l'unité
    }

    #[test]
    fn trois_axes_suivent_le_doigt_dans_l_ecran() {
        let mut g = geste(Mode::Translation, Prise::O, 500.0, 500.0, [true; 3]);
        let Transfo::Translation(d) = g.suivre(600.0, 400.0, &Pas::default(), 1.0) else { panic!() };
        presque(&d, &[1.0, 1.0, 0.0]);
    }

    #[test]
    fn deux_axes_restent_dans_leur_plan() {
        let mut g = geste(Mode::Translation, Prise::O, 500.0, 500.0, [true, true, false]);
        let Transfo::Translation(d) = g.suivre(600.0, 400.0, &Pas::default(), 1.0) else { panic!() };
        presque(&d, &[1.0, 1.0, 0.0]);
        // Plan XZ vu par la tranche : rasant, retombe sur le plan de l'écran, Y bloqué
        let mut g = geste(Mode::Translation, Prise::O, 500.0, 500.0, [true, false, true]);
        let Transfo::Translation(d) = g.suivre(600.0, 400.0, &Pas::default(), 1.0) else { panic!() };
        presque(&d, &[1.0, 0.0, 0.0]);
    }

    #[test]
    fn le_pas_arrondit() {
        let mut g = geste(Mode::Translation, Prise::Axe(0), 500.0, 500.0, [true; 3]);
        let p = Pas { actif: true, ..Pas::default() };
        let Transfo::Translation(d) = g.suivre(637.0, 500.0, &p, 0.5) else { panic!() };
        presque(&d, &[1.5, 0.0, 0.0]);
        let mut g = geste(Mode::Rotation, Prise::V, 600.0, 500.0, [true; 3]);
        let Transfo::Rotation { q, .. } = g.suivre(600.0, 490.0, &p, 0.5) else { panic!() }; // 5,7°
        presque(&q, &[0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn tourner_dans_le_sens_antihoraire_est_positif() {
        // Vu de la pointe de Z, soit depuis la caméra
        let mut g = geste(Mode::Rotation, Prise::Axe(2), 600.0, 500.0, [true; 3]);
        let Transfo::Rotation { q, .. } = g.suivre(500.0, 400.0, &Pas::default(), 1.0) else { panic!() };
        presque(&qappliquer(q, [1.0, 0.0, 0.0]), &[0.0, 1.0, 0.0]);
    }

    #[test]
    fn plusieurs_tours_se_cumulent() {
        let mut g = geste(Mode::Rotation, Prise::V, 600.0, 500.0, [true; 3]);
        for k in 1..=9 {
            let a = -(k as f64) * std::f64::consts::FRAC_PI_2;
            g.suivre(500.0 + 100.0 * a.cos(), 500.0 + 100.0 * a.sin(), &Pas::default(), 1.0);
        }
        presque(&[g.cumul], &[-9.0 * std::f64::consts::FRAC_PI_2]);
    }

    #[test]
    fn l_echelle_suit_le_rapport_des_distances() {
        let mut g = geste(Mode::Echelle, Prise::Axe(0), 600.0, 500.0, [true; 3]);
        let Transfo::Echelle { s, .. } = g.suivre(800.0, 900.0, &Pas::default(), 1.0) else { panic!() };
        presque(&s, &[3.0, 1.0, 1.0]);
        let mut g = geste(Mode::Echelle, Prise::V, 600.0, 500.0, [true, true, false]);
        let Transfo::Echelle { s, .. } = g.suivre(700.0, 500.0, &Pas::default(), 1.0) else { panic!() };
        presque(&s, &[2.0, 2.0, 1.0]);
        // Jamais de facteur nul ni négatif
        let mut g = geste(Mode::Echelle, Prise::Axe(0), 600.0, 500.0, [true; 3]);
        let Transfo::Echelle { s, .. } = g.suivre(100.0, 500.0, &Pas::default(), 1.0) else { panic!() };
        presque(&s, &[0.01, 1.0, 1.0]);
    }

    #[test]
    fn un_doigt_parti_du_pivot_agrandit_vers_la_droite() {
        let mut g = geste(Mode::Echelle, Prise::O, 505.0, 500.0, [true; 3]);
        let Transfo::Echelle { s, .. } = g.suivre(625.0, 500.0, &Pas::default(), 1.0) else { panic!() };
        presque(&s, &[2.0, 2.0, 2.0]);
    }
}
