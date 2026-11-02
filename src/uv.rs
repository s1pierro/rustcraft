//! Édition des coordonnées de texture, sans interface. Une UV appartient à un coin de
//! face : deux faces voisines peuvent donc se séparer le long d'une couture, alors que
//! leur sommet reste soudé. Un **point** réunit les coins d'un même sommet aux mêmes
//! coordonnées ; un **îlot** réunit les faces reliées par de tels points.

use crate::maillage::{Maillage, P3};
use std::collections::{HashMap, HashSet};

/// Coin : rang de la face, rang du sommet dans la face.
pub type Coin = (usize, usize);
pub type Uv = [f32; 2];

/// Écart sous lequel deux UV d'un même sommet n'en font qu'une : 1/1024 de pixel en 4K.
const SOUDURE: f32 = 1e-6;

fn cle(i: usize, uv: Uv) -> (usize, i32, i32) {
    (i, (uv[0] / SOUDURE / 4.0).round() as i32, (uv[1] / SOUDURE / 4.0).round() as i32)
}

/// La face a une UV par coin.
pub fn complete(m: &Maillage, f: usize) -> bool {
    let x = &m.faces[f];
    !x.uv.is_empty() && x.uv.len() == x.s.len()
}

pub fn uv_de(m: &Maillage, (f, k): Coin) -> Uv {
    m.faces[f].uv[k]
}

/// Tous les coins des faces données.
pub fn coins(m: &Maillage, faces: &[usize]) -> Vec<Coin> {
    faces.iter().flat_map(|&f| (0..m.faces[f].uv.len()).map(move |k| (f, k))).collect()
}

/// Points des faces données : les coins soudés, dans l'ordre où ils apparaissent.
pub fn points(m: &Maillage, faces: &[usize]) -> Vec<Vec<Coin>> {
    let mut rang: HashMap<(usize, i32, i32), usize> = HashMap::new();
    let mut l: Vec<Vec<Coin>> = Vec::new();
    for c in coins(m, faces) {
        let k = cle(m.faces[c.0].s[c.1], uv_de(m, c));
        let r = *rang.entry(k).or_insert_with(|| {
            l.push(Vec::new());
            l.len() - 1
        });
        l[r].push(c);
    }
    l
}

/// Îlots des faces données : chacun est la liste de ses coins.
pub fn ilots(m: &Maillage, faces: &[usize]) -> Vec<Vec<Coin>> {
    fn racine(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let rang: HashMap<usize, usize> = faces.iter().enumerate().map(|(r, f)| (*f, r)).collect();
    let mut p: Vec<usize> = (0..faces.len()).collect();
    for pt in points(m, faces) {
        let a = rang[&pt[0].0];
        for c in &pt[1..] {
            let (x, y) = (racine(&mut p, a), racine(&mut p, rang[&c.0]));
            p[x] = y;
        }
    }
    let mut groupes: HashMap<usize, Vec<Coin>> = HashMap::new();
    let mut ordre = Vec::new();
    for (r, &f) in faces.iter().enumerate() {
        let x = racine(&mut p, r);
        if !groupes.contains_key(&x) {
            ordre.push(x);
        }
        groupes.entry(x).or_default().extend((0..m.faces[f].uv.len()).map(|k| (f, k)));
    }
    ordre.into_iter().map(|x| groupes.remove(&x).unwrap()).collect()
}

/// Passage de l'espace UV au plan, où les distances et les angles sont justes : une
/// image de 2:1 fait une tuile de 1 × 0,5. Les plus grands côtés valent 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aspect {
    pub l: f32,
    pub h: f32,
}

impl Aspect {
    pub const CARRE: Aspect = Aspect { l: 1.0, h: 1.0 };

    pub fn de(largeur: u32, hauteur: u32) -> Aspect {
        let m = largeur.max(hauteur).max(1) as f32;
        Aspect { l: largeur.max(1) as f32 / m, h: hauteur.max(1) as f32 / m }
    }
}

/// Coin le plus proche de (u, v), à moins de `rayon` dans le plan. Quand plusieurs coins
/// se superposent, le gagnant est celui dont la face a son centre le plus près du doigt.
pub fn coin_sous(m: &Maillage, faces: &[usize], p: Uv, rayon: f32, a: Aspect) -> Option<Coin> {
    let d2 = |q: Uv| ((q[0] - p[0]) * a.l).powi(2) + ((q[1] - p[1]) * a.h).powi(2);
    let mut mieux: Option<(Coin, f32, f32)> = None;
    for c in coins(m, faces) {
        let d = d2(uv_de(m, c));
        if d > rayon * rayon {
            continue;
        }
        let e = d2(centre_face(m, c.0));
        if mieux.is_none_or(|x| d < x.1 - 1e-9 || ((d - x.1).abs() <= 1e-9 && e < x.2)) {
            mieux = Some((c, d, e));
        }
    }
    mieux.map(|x| x.0)
}

fn centre_face(m: &Maillage, f: usize) -> Uv {
    let l = &m.faces[f].uv;
    let n = l.len().max(1) as f32;
    [l.iter().map(|c| c[0]).sum::<f32>() / n, l.iter().map(|c| c[1]).sum::<f32>() / n]
}

/// Face dont le polygone contient (u, v) ; la dernière dessinée passe devant.
pub fn face_sous(m: &Maillage, faces: &[usize], p: Uv) -> Option<usize> {
    faces.iter().rev().copied().find(|&f| {
        let l = &m.faces[f].uv;
        let mut dedans = false;
        for k in 0..l.len() {
            let (a, b) = (l[k], l[(k + 1) % l.len()]);
            if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
                dedans = !dedans;
            }
        }
        dedans
    })
}

/// Le point du coin : lui et les coins soudés.
pub fn point_de(m: &Maillage, faces: &[usize], c: Coin) -> Vec<Coin> {
    let k = cle(m.faces[c.0].s[c.1], uv_de(m, c));
    coins(m, faces).into_iter().filter(|x| cle(m.faces[x.0].s[x.1], uv_de(m, *x)) == k).collect()
}

/// L'îlot de la face.
pub fn ilot_de(m: &Maillage, faces: &[usize], f: usize) -> Vec<Coin> {
    ilots(m, faces).into_iter().find(|i| i.iter().any(|c| c.0 == f)).unwrap_or_default()
}

/// Coins dans le rectangle [u0, v0, u1, v1].
pub fn dans_boite(m: &Maillage, faces: &[usize], b: [f32; 4]) -> Vec<Coin> {
    let (u, v) = ((b[0].min(b[2]), b[0].max(b[2])), (b[1].min(b[3]), b[1].max(b[3])));
    coins(m, faces).into_iter().filter(|c| {
        let q = uv_de(m, *c);
        (u.0..=u.1).contains(&q[0]) && (v.0..=v.1).contains(&q[1])
    }).collect()
}

/// Centre de la boîte englobante des coins.
pub fn centre(m: &Maillage, l: &HashSet<Coin>) -> Option<Uv> {
    let mut it = l.iter().map(|c| uv_de(m, *c));
    let p = it.next()?;
    let (mut a, mut b) = (p, p);
    for q in it {
        for k in 0..2 {
            a[k] = a[k].min(q[k]);
            b[k] = b[k].max(q[k]);
        }
    }
    Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
}

/// Transformation d'une sélection, depuis ses positions de départ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Geste {
    Translation(Uv),
    /// Angle en degrés, sens des aiguilles à l'écran (v descend), autour du pivot.
    Rotation { degres: f32, pivot: Uv },
    Echelle { facteur: Uv, pivot: Uv },
}

pub fn transformer(q: Uv, g: Geste, a: Aspect) -> Uv {
    match g {
        Geste::Translation(d) => [q[0] + d[0], q[1] + d[1]],
        Geste::Rotation { degres, pivot } => {
            let (s, c) = degres.to_radians().sin_cos();
            let (x, y) = ((q[0] - pivot[0]) * a.l, (q[1] - pivot[1]) * a.h);
            [pivot[0] + (x * c - y * s) / a.l, pivot[1] + (x * s + y * c) / a.h]
        }
        Geste::Echelle { facteur, pivot } => [pivot[0] + (q[0] - pivot[0]) * facteur[0], pivot[1] + (q[1] - pivot[1]) * facteur[1]],
    }
}

/// `v` arrondi au multiple de `pas` le plus proche.
pub fn au_pas(v: f32, pas: f32) -> f32 {
    if pas > 0.0 { (v / pas).round() * pas } else { v }
}

/// Forme sur laquelle les faces sont projetées.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// À plat, depuis la vue : axes droite et haut de l'écran.
    Vue { droite: P3, haut: P3 },
    /// Six côtés : chaque face va au côté que sa normale regarde.
    Cube,
    /// `axe` : 0 à 2. Les faces qui regardent le long de l'axe font deux disques.
    Cylindre { axe: usize },
    /// `axe` : 0 à 2, celui des pôles.
    Sphere { axe: usize },
}

/// Part de la tuile laissée autour des îlots et entre eux.
const MARGE: f32 = 1.0 / 64.0;

/// Coins d'un îlot et leur position dans son plan, à la vraie échelle du modèle.
type Ilot = Vec<(Coin, [f64; 2])>;

fn scal(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn base(axe: usize) -> (P3, P3, P3) {
    let e = |k: usize| {
        let mut u = [0.0; 3];
        u[k] = 1.0;
        u
    };
    // Repère direct (a, b, axe) : vue de l'extérieur, l'angle croît vers la droite
    (e((axe + 1) % 3), e((axe + 2) % 3), e(axe))
}

/// Coordonnées autour de l'axe. Une face à cheval sur la couture de l'angle reste d'un
/// seul tenant : ses coins suivent le premier, et ceux posés sur l'axe prennent la
/// moyenne des autres.
fn angles(pts: &[(f64, f64)]) -> Vec<f64> {
    let hors: Vec<Option<f64>> = pts.iter().map(|&(a, b)| (a.hypot(b) > 1e-9).then(|| b.atan2(a))).collect();
    let Some(t0) = hors.iter().flatten().next().copied() else { return vec![0.0; pts.len()] };
    let suivi: Vec<Option<f64>> = hors.iter().map(|t| t.map(|t| t0 + (t - t0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI)).collect();
    let n = suivi.iter().flatten().count().max(1) as f64;
    let moyenne = suivi.iter().flatten().sum::<f64>() / n;
    suivi.iter().map(|t| t.unwrap_or(moyenne)).collect()
}

fn centre_de(m: &Maillage, faces: &[usize]) -> P3 {
    let (mut a, mut b) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for &f in faces {
        for i in &m.faces[f].s {
            for k in 0..3 {
                a[k] = a[k].min(m.sommets[*i][k]);
                b[k] = b[k].max(m.sommets[*i][k]);
            }
        }
    }
    [0, 1, 2].map(|k| (a[k] + b[k]) / 2.0)
}

/// Les îlots d'une projection, à l'échelle du modèle : v descend, comme dans l'image.
fn ilots_de(m: &Maillage, faces: &[usize], p: Projection) -> Vec<Ilot> {
    let c = centre_de(m, faces);
    let rel = |i: usize| [0, 1, 2].map(|k| m.sommets[i][k] - c[k]);
    let a_plat = |l: &mut Ilot, f: usize, u: P3, v: P3| {
        l.extend(m.faces[f].s.iter().enumerate().map(|(k, i)| ((f, k), [scal(rel(*i), u), scal(rel(*i), v)])));
    };
    match p {
        Projection::Vue { droite, haut } => {
            let bas = haut.map(|x| -x);
            let mut l = Ilot::new();
            faces.iter().for_each(|&f| a_plat(&mut l, f, droite, bas));
            vec![l]
        }
        Projection::Cube => {
            // Pour chaque côté : sa droite et son bas, vus de l'extérieur
            let cotes: [(P3, P3); 6] = [
                ([0.0, 0.0, -1.0], [0.0, -1.0, 0.0]), // +X
                ([0.0, 0.0, 1.0], [0.0, -1.0, 0.0]),  // -X
                ([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),   // +Y
                ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),  // -Y
                ([1.0, 0.0, 0.0], [0.0, -1.0, 0.0]),  // +Z
                ([-1.0, 0.0, 0.0], [0.0, -1.0, 0.0]), // -Z
            ];
            let mut l: Vec<Ilot> = vec![Ilot::new(); 6];
            for &f in faces {
                let n = m.normale(&m.faces[f].s);
                let k = (0..3).max_by(|a, b| n[*a].abs().total_cmp(&n[*b].abs())).unwrap();
                let cote = 2 * k + (n[k] < 0.0) as usize;
                a_plat(&mut l[cote], f, cotes[cote].0, cotes[cote].1);
            }
            l.retain(|x| !x.is_empty());
            l
        }
        Projection::Cylindre { axe } => {
            let (ea, eb, ez) = base(axe);
            let bas = ez.map(|x| -x);
            let (mut corps, mut haut, mut dessous) = (Ilot::new(), Ilot::new(), Ilot::new());
            let sommets: HashSet<usize> = faces.iter().flat_map(|f| m.faces[*f].s.iter().copied()).collect();
            let r = sommets.iter().map(|i| scal(rel(*i), ea).hypot(scal(rel(*i), eb))).sum::<f64>() / sommets.len().max(1) as f64;
            for &f in faces {
                let n = scal(m.normale(&m.faces[f].s), ez);
                if n > 0.7 {
                    a_plat(&mut haut, f, eb, ea);
                } else if n < -0.7 {
                    a_plat(&mut dessous, f, eb, ea.map(|x| -x));
                } else {
                    let s = &m.faces[f].s;
                    let pts: Vec<(f64, f64)> = s.iter().map(|i| (scal(rel(*i), ea), scal(rel(*i), eb))).collect();
                    let t = angles(&pts);
                    corps.extend(s.iter().enumerate().map(|(k, i)| ((f, k), [t[k] * r, scal(rel(*i), bas)])));
                }
            }
            [corps, haut, dessous].into_iter().filter(|x| !x.is_empty()).collect()
        }
        Projection::Sphere { axe } => {
            let (ea, eb, ez) = base(axe);
            let sommets: HashSet<usize> = faces.iter().flat_map(|f| m.faces[*f].s.iter().copied()).collect();
            let longueur = |i: usize| scal(rel(i), rel(i)).sqrt();
            let r = sommets.iter().map(|i| longueur(*i)).sum::<f64>() / sommets.len().max(1) as f64;
            let mut l = Ilot::new();
            for &f in faces {
                let s = &m.faces[f].s;
                let pts: Vec<(f64, f64)> = s.iter().map(|i| (scal(rel(*i), ea), scal(rel(*i), eb))).collect();
                let t = angles(&pts);
                l.extend(s.iter().enumerate().map(|(k, i)| {
                    let d = longueur(*i).max(1e-12);
                    ((f, k), [t[k] * r, (scal(rel(*i), ez) / d).clamp(-1.0, 1.0).acos() * r])
                }));
            }
            vec![l]
        }
    }
}

/// Boîte [min u, min v, largeur, hauteur] d'un îlot.
fn etendue(l: &Ilot) -> [f64; 4] {
    let (mut a, mut b) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for (_, p) in l {
        for k in 0..2 {
            a[k] = a[k].min(p[k]);
            b[k] = b[k].max(p[k]);
        }
    }
    [a[0], a[1], b[0] - a[0], b[1] - a[1]]
}

/// Range des boîtes par rangées dans un rectangle : leur place (dans l'ordre donné),
/// l'encombrement total, ou `None` si elles n'y tiennent pas.
fn rangees(tailles: &[[f64; 2]], (l, h): (f64, f64), ecart: f64) -> Option<(Vec<[f64; 2]>, [f64; 2])> {
    let mut ordre: Vec<usize> = (0..tailles.len()).collect();
    ordre.sort_by(|a, b| tailles[*b][1].total_cmp(&tailles[*a][1]));
    let mut places = vec![[0.0; 2]; tailles.len()];
    let (mut x, mut y, mut rangee, mut large) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for k in ordre {
        let [w, t] = tailles[k];
        if w > l {
            return None;
        }
        if x > 0.0 && x + w > l {
            y += rangee + ecart;
            (x, rangee) = (0.0, 0.0);
        }
        if y + t > h {
            return None;
        }
        places[k] = [x, y];
        x += w + ecart;
        large = large.max(x - ecart);
        rangee = rangee.max(t);
    }
    Some((places, [large, y + rangee]))
}

/// UV de chaque coin : les îlots gardent leurs proportions à l'écran, à la même échelle
/// entre eux, la plus grande qui tienne dans la tuile, marges comprises.
pub fn projeter(m: &Maillage, faces: &[usize], p: Projection, a: Aspect) -> Vec<(Coin, Uv)> {
    let ilots = ilots_de(m, faces, p);
    let (tl, th) = (a.l as f64, a.h as f64);
    let marge = MARGE as f64;
    let (ul, uh) = (tl - 2.0 * marge, th - 2.0 * marge);
    let boites: Vec<[f64; 4]> = ilots.iter().map(etendue).collect();
    let tailles = |k: f64| -> Vec<[f64; 2]> { boites.iter().map(|b| [b[2] * k, b[3] * k]).collect() };
    let plus_grand = boites.iter().map(|b| b[2].max(b[3])).fold(0.0, f64::max).max(1e-12);
    // Recherche de la plus grande échelle : l'ordre des rangées rend la limite irrégulière
    let (mut bas, mut haut) = (0.0, tl.max(th) / plus_grand);
    for _ in 0..40 {
        let k = (bas + haut) / 2.0;
        if rangees(&tailles(k), (ul, uh), marge).is_some() {
            bas = k;
        } else {
            haut = k;
        }
    }
    let (places, total) = rangees(&tailles(bas), (ul, uh), marge).unwrap_or((vec![[0.0; 2]; boites.len()], [0.0; 2]));
    let (dx, dy) = ((tl - total[0]) / 2.0, (th - total[1]) / 2.0);
    let mut r = Vec::new();
    for (k, l) in ilots.iter().enumerate() {
        for (c, q) in l {
            let x = dx + places[k][0] + (q[0] - boites[k][0]) * bas;
            let y = dy + places[k][1] + (q[1] - boites[k][1]) * bas;
            r.push((*c, [(x / tl) as f32, (y / th) as f32]));
        }
    }
    r
}

#[cfg(test)]
mod tests_projection {
    use super::*;
    use crate::maillage::Maillage;

    fn dans_la_tuile(m: &Maillage) {
        for f in &m.faces {
            assert_eq!(f.uv.len(), f.s.len());
            assert!(f.uv.iter().flatten().all(|x| (-1e-5..=1.00001).contains(x)), "{:?}", f.uv);
        }
    }

    fn pose(m: &mut Maillage, p: Projection, a: Aspect) {
        let faces: Vec<usize> = (0..m.faces.len()).collect();
        for f in &mut m.faces {
            f.uv = vec![[0.0; 2]; f.s.len()];
        }
        for ((f, k), q) in projeter(m, &faces, p, a) {
            m.faces[f].uv[k] = q;
        }
    }

    fn cote_uv(m: &Maillage, f: usize) -> (f32, f32) {
        let u = &m.faces[f].uv;
        ((u[1][0] - u[0][0]).hypot(u[1][1] - u[0][1]), (u[2][0] - u[1][0]).hypot(u[2][1] - u[1][1]))
    }

    /// Aire signée du polygone en UV, dans le repère image (v descend).
    fn aire(u: &[Uv]) -> f32 {
        (0..u.len()).map(|k| { let (a, b) = (u[k], u[(k + 1) % u.len()]); a[0] * b[1] - b[0] * a[1] }).sum::<f32>() / 2.0
    }

    #[test]
    fn le_cube_fait_six_carres_egaux_sans_chevauchement() {
        let mut m = Maillage::cube(2.0);
        pose(&mut m, Projection::Cube, Aspect::CARRE);
        dans_la_tuile(&m);
        assert_eq!(ilots(&m, &(0..6).collect::<Vec<_>>()).len(), 6);
        for f in 0..6 {
            let (a, b) = cote_uv(&m, f);
            assert!((a - b).abs() < 1e-5 && (a - cote_uv(&m, 0).0).abs() < 1e-5);
            assert!(a > 0.25, "le rangement utilise mal la tuile : {a}");
        }
        for (i, f) in m.faces.iter().enumerate() {
            let c = centre_face(&m, i);
            for j in 0..6 {
                assert!(j == i || face_sous(&m, &[j], c).is_none(), "{i} {j} {:?}", f.uv);
            }
        }
    }

    #[test]
    fn aucune_projection_ne_montre_une_image_a_l_envers() {
        // Vue de l'extérieur, une face tourne dans le sens direct. Une image non retournée
        // la montre tourner de même : avec v qui descend, son aire signée est négative.
        for p in [Projection::Cube, Projection::Cylindre { axe: 1 }, Projection::Sphere { axe: 1 }, Projection::Cylindre { axe: 0 }, Projection::Cylindre { axe: 2 }, Projection::Sphere { axe: 2 }] {
            let mut m = if p == Projection::Cube { Maillage::cube(2.0) } else { Maillage::sphere_uv(1.0, 24, 12) };
            let centre = centre_de(&m, &(0..m.faces.len()).collect::<Vec<_>>());
            let n = m.normale(&m.faces[0].s);
            let q = m.sommets[m.faces[0].s[0]];
            assert!(scal(n, [0, 1, 2].map(|k| q[k] - centre[k])) > 0.0, "le maillage d'essai tourne vers l'extérieur");
            pose(&mut m, p, Aspect::CARRE);
            for f in m.faces.iter().filter(|f| f.s.len() == 4) {
                assert!(aire(&f.uv) < 0.0, "{p:?} {:?}", f.uv);
            }
        }
        let vers_nous = Projection::Vue { droite: [1.0, 0.0, 0.0], haut: [0.0, 1.0, 0.0] };
        let mut m = Maillage::cube(2.0);
        pose(&mut m, vers_nous, Aspect::CARRE);
        let f = m.faces.iter().find(|f| m.normale(&f.s)[2] > 0.5).unwrap();
        assert!(aire(&f.uv) < 0.0, "{:?}", f.uv);
        // Le haut du modèle reste en haut de l'image, sa droite à droite
        let de = |i: usize| m.sommets[f.s[i]];
        let (b, c) = ((0..4).max_by(|a, b| de(*a)[1].total_cmp(&de(*b)[1])).unwrap(), (0..4).min_by(|a, b| de(*a)[1].total_cmp(&de(*b)[1])).unwrap());
        assert!(f.uv[b][1] < f.uv[c][1]);
    }

    #[test]
    fn le_cylindre_deroule_le_corps_et_garde_deux_disques() {
        let mut m = Maillage::cylindre(1.0, 3.0, 24, false);
        pose(&mut m, Projection::Cylindre { axe: 1 }, Aspect::CARRE);
        dans_la_tuile(&m);
        let f: Vec<usize> = (0..m.faces.len()).collect();
        assert_eq!(ilots(&m, &f).len(), 3);
        // Le corps garde ses proportions : largeur / hauteur = 2πr / h, à une face près
        let corps = f.iter().copied().filter(|k| m.faces[*k].s.len() == 4).collect::<Vec<_>>();
        let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
        for c in coins(&m, &corps) {
            for k in 0..2 {
                a[k] = a[k].min(uv_de(&m, c)[k]);
                b[k] = b[k].max(uv_de(&m, c)[k]);
            }
        }
        let attendu = std::f32::consts::TAU / 3.0;
        assert!(((b[0] - a[0]) / (b[1] - a[1]) - attendu).abs() < 0.05, "{a:?} {b:?}");
    }

    #[test]
    fn la_couture_ne_traverse_pas_la_tuile() {
        for p in [Projection::Cylindre { axe: 1 }, Projection::Sphere { axe: 1 }] {
            let mut m = Maillage::sphere_uv(1.0, 24, 12);
            pose(&mut m, p, Aspect::CARRE);
            dans_la_tuile(&m);
            for f in &m.faces {
                let (a, b) = (f.uv.iter().map(|c| c[0]).fold(f32::MAX, f32::min), f.uv.iter().map(|c| c[0]).fold(f32::MIN, f32::max));
                assert!(b - a < 0.15, "{p:?} {:?}", f.uv);
            }
        }
    }

    #[test]
    fn la_sphere_fait_un_rectangle_de_deux_pour_un() {
        let mut m = Maillage::sphere_uv(1.0, 32, 16);
        pose(&mut m, Projection::Sphere { axe: 1 }, Aspect::de(2048, 1024));
        dans_la_tuile(&m);
        // Sur une image 2:1, le dépliage remplit la tuile
        let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
        for q in m.faces.iter().flat_map(|f| f.uv.iter()) {
            for k in 0..2 {
                a[k] = a[k].min(q[k]);
                b[k] = b[k].max(q[k]);
            }
        }
        assert!(b[0] - a[0] > 0.9 && b[1] - a[1] > 0.9, "{a:?} {b:?}");
    }

    #[test]
    fn la_vue_garde_les_proportions_sur_une_image_etiree() {
        let mut m = Maillage::boite(4.0, 2.0, 1.0);
        let k = (0..m.faces.len()).find(|f| m.normale(&m.faces[*f].s)[2] > 0.5).unwrap();
        let a = Aspect::de(1024, 2048);
        let p = Projection::Vue { droite: [1.0, 0.0, 0.0], haut: [0.0, 1.0, 0.0] };
        m.faces[k].uv = vec![[0.0; 2]; 4];
        for ((f, c), q) in projeter(&m, &[k], p, a) {
            m.faces[f].uv[c] = q;
        }
        let u = &m.faces[k].uv;
        let ecart = |c: usize| u.iter().map(|q| q[c]).fold(f32::MIN, f32::max) - u.iter().map(|q| q[c]).fold(f32::MAX, f32::min);
        // 4 × 2 à l'écran, sur une image deux fois plus haute que large
        assert!((ecart(0) * a.l / (ecart(1) * a.h) - 2.0).abs() < 1e-4, "{u:?}");
        assert!(ecart(0) > 0.9, "la largeur remplit la tuile");
    }

    #[test]
    fn un_axe_choisi_tourne_le_cylindre() {
        let mut m = Maillage::cylindre(1.0, 3.0, 12, false);
        for p in &mut m.sommets {
            *p = [p[1], p[2], p[0]]; // axe Y → X
        }
        pose(&mut m, Projection::Cylindre { axe: 0 }, Aspect::CARRE);
        dans_la_tuile(&m);
        assert_eq!(ilots(&m, &(0..m.faces.len()).collect::<Vec<_>>()).len(), 3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maillage::{Face, Maillage};

    /// Deux carrés qui partagent une arête : (`u0` décale le second : une couture).
    fn deux(decalage: f32) -> Maillage {
        let s = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let m = crate::maillage::defaut();
        let a = Face { s: vec![0, 1, 2, 3], m: m.clone(), uv: vec![[0.0, 0.0], [0.5, 0.0], [0.5, 0.5], [0.0, 0.5]] };
        let d = decalage;
        let b = Face { s: vec![1, 4, 5, 2], m, uv: vec![[0.5 + d, 0.0], [1.0 + d, 0.0], [1.0 + d, 0.5], [0.5 + d, 0.5]] };
        Maillage::new(s, vec![a, b], Vec::new())
    }

    #[test]
    fn les_coins_soudes_font_un_point() {
        assert_eq!(points(&deux(0.0), &[0, 1]).len(), 6);
        assert_eq!(points(&deux(0.1), &[0, 1]).len(), 8);
        let m = deux(0.0);
        assert_eq!(point_de(&m, &[0, 1], (0, 1)), vec![(0, 1), (1, 0)]);
    }

    #[test]
    fn une_couture_separe_les_ilots() {
        assert_eq!(ilots(&deux(0.0), &[0, 1]).len(), 1);
        let m = deux(0.1);
        assert_eq!(ilots(&m, &[0, 1]).len(), 2);
        assert_eq!(ilot_de(&m, &[0, 1], 1).len(), 4);
    }

    #[test]
    fn le_coin_sous_le_doigt_prefere_la_face_la_plus_proche() {
        let m = deux(0.0);
        assert_eq!(coin_sous(&m, &[0, 1], [0.02, 0.01], 0.05, Aspect::CARRE), Some((0, 0)));
        assert_eq!(coin_sous(&m, &[0, 1], [0.2, 0.2], 0.05, Aspect::CARRE), None);
        // Le coin partagé : le doigt est du côté de la seconde face
        assert_eq!(coin_sous(&m, &[0, 1], [0.52, 0.0], 0.05, Aspect::CARRE), Some((1, 0)));
    }

    #[test]
    fn un_doigt_dans_une_face_la_trouve() {
        let m = deux(0.0);
        assert_eq!(face_sous(&m, &[0, 1], [0.7, 0.2]), Some(1));
        assert_eq!(face_sous(&m, &[0, 1], [0.7, 0.8]), None);
    }

    #[test]
    fn la_rotation_respecte_la_forme_de_l_image() {
        // Sur une image 2:1, un quart de tour d'un écart (0,25 ; 0) donne (0 ; 0,5) en UV
        let a = Aspect::de(2048, 1024);
        let q = transformer([0.75, 0.5], Geste::Rotation { degres: 90.0, pivot: [0.5, 0.5] }, a);
        assert!((q[0] - 0.5).abs() < 1e-5 && (q[1] - 1.0).abs() < 1e-5, "{q:?}");
    }

    #[test]
    fn echelle_translation_et_pas() {
        let a = Aspect::CARRE;
        assert_eq!(transformer([1.0, 1.0], Geste::Echelle { facteur: [2.0, 0.5], pivot: [0.5, 0.5] }, a), [1.5, 0.75]);
        assert_eq!(transformer([1.0, 1.0], Geste::Translation([0.5, -0.25]), a), [1.5, 0.75]);
        assert_eq!(au_pas(0.13, 0.25), 0.25);
    }

    #[test]
    fn centre_de_la_boite_et_selection_en_boite() {
        let m = deux(0.0);
        let s: HashSet<Coin> = dans_boite(&m, &[0, 1], [0.4, -0.1, 1.1, 0.6]).into_iter().collect();
        assert_eq!(s.len(), 6);
        assert_eq!(centre(&m, &s), Some([0.75, 0.25]));
    }
}
