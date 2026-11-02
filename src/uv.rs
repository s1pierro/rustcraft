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

/// Réglages de la mise en page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    /// Part de la tuile autour des îlots et entre eux.
    pub marge: f32,
    /// Chaque îlot peut pivoter pour mieux remplir la tuile.
    pub tourner: bool,
    /// Un même nombre de pixels par unité du modèle, sur tous les îlots.
    pub egaliser: bool,
}

impl Page {
    pub const DEFAUT: Page = Page { marge: 1.0 / 128.0, tourner: false, egaliser: true };
}

/// Résultat d'une mise en page.
pub struct Disposition {
    pub coins: Vec<(Coin, Uv)>,
    pub ilots: usize,
    /// Part de la tuile couverte par les faces, de 0 à 1.
    pub remplissage: f32,
}

/// Aire d'un polygone du modèle, plan ou non.
fn aire_3d(m: &Maillage, f: usize) -> f64 {
    let s = &m.faces[f].s;
    let mut n = [0.0; 3];
    for k in 1..s.len().saturating_sub(1) {
        let (o, a, b) = (m.sommets[s[0]], m.sommets[s[k]], m.sommets[s[k + 1]]);
        let (u, v) = ([0, 1, 2].map(|i| a[i] - o[i]), [0, 1, 2].map(|i| b[i] - o[i]));
        n[0] += u[1] * v[2] - u[2] * v[1];
        n[1] += u[2] * v[0] - u[0] * v[2];
        n[2] += u[0] * v[1] - u[1] * v[0];
    }
    scal(n, n).sqrt() / 2.0
}

/// Aire d'un polygone dans le plan de la tuile.
fn aire_plan(m: &Maillage, f: usize, (tl, th): (f64, f64)) -> f64 {
    let p: Vec<[f64; 2]> = m.faces[f].uv.iter().map(|q| [q[0] as f64 * tl, q[1] as f64 * th]).collect();
    (0..p.len()).map(|k| p[k][0] * p[(k + 1) % p.len()][1] - p[(k + 1) % p.len()][0] * p[k][1]).sum::<f64>().abs() / 2.0
}

/// Échelle de chaque îlot pour que tous aient la même densité. Un îlot sans aire ne dit
/// rien de la sienne : il prend l'échelle médiane.
fn densites(m: &Maillage, ilots: &[Vec<Coin>], tuile: (f64, f64)) -> Vec<f64> {
    let brut: Vec<Option<f64>> = ilots
        .iter()
        .map(|l| {
            let f: HashSet<usize> = l.iter().map(|c| c.0).collect();
            let (a3, a2) = f.iter().fold((0.0, 0.0), |(a, b), f| (a + aire_3d(m, *f), b + aire_plan(m, *f, tuile)));
            (a3 > 1e-18 && a2 > 1e-18).then(|| (a3 / a2).sqrt())
        })
        .collect();
    let mut connues: Vec<f64> = brut.iter().flatten().copied().collect();
    connues.sort_by(f64::total_cmp);
    let mediane = connues.get(connues.len() / 2).copied().unwrap_or(1.0);
    brut.into_iter().map(|x| x.unwrap_or(mediane)).collect()
}

/// Pivote l'îlot pour que sa boîte soit la plus petite : elle s'appuie sur un côté de
/// son enveloppe. Un îlot presque aussi serré tel quel ne bouge pas.
fn serrer(l: &mut Ilot) {
    let mut p: Vec<[f64; 2]> = l.iter().map(|x| x.1).collect();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return;
    }
    let tourne = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    let mut env: Vec<[f64; 2]> = Vec::new();
    for sens in 0..2 {
        let debut = env.len();
        let parcours: Vec<[f64; 2]> = if sens == 0 { p.clone() } else { p.iter().rev().copied().collect() };
        for q in parcours {
            while env.len() >= debut + 2 && tourne(env[env.len() - 2], env[env.len() - 1], q) <= 0.0 {
                env.pop();
            }
            env.push(q);
        }
        env.pop();
    }
    let aire = |c: f64, s: f64| {
        let (mut a, mut b) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for q in &env {
            let r = [q[0] * c + q[1] * s, q[1] * c - q[0] * s];
            for k in 0..2 {
                a[k] = a[k].min(r[k]);
                b[k] = b[k].max(r[k]);
            }
        }
        (b[0] - a[0]) * (b[1] - a[1])
    };
    let (mut meilleur, mut c, mut s) = (aire(1.0, 0.0) * 0.99, 1.0, 0.0);
    for k in 0..env.len() {
        let (a, b) = (env[k], env[(k + 1) % env.len()]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let n = dx.hypot(dy);
        if n > 0.0 && aire(dx / n, dy / n) < meilleur {
            (meilleur, c, s) = (aire(dx / n, dy / n), dx / n, dy / n);
        }
    }
    for x in l.iter_mut() {
        let q = x.1;
        x.1 = [q[0] * c + q[1] * s, q[1] * c - q[0] * s];
    }
}

/// Range des boîtes dans un rectangle, chacune le plus bas possible sur la ligne de ciel
/// des précédentes : un rang (x, sommet, largeur) par palier. Rend la place de chaque
/// boîte et si elle a pivoté d'un quart de tour.
fn ciel(tailles: &[[f64; 2]], ordre: &[usize], (l, h): (f64, f64), ecart: f64, tourner: bool) -> Option<Vec<([f64; 2], bool)>> {
    const JEU: f64 = 1e-9;
    let mut paliers: Vec<[f64; 3]> = vec![[0.0, 0.0, l]];
    let mut places = vec![([0.0; 2], false); tailles.len()];
    for &k in ordre {
        let [w0, t0] = tailles[k];
        let mut mieux: Option<(f64, f64, f64, f64, f64, bool)> = None;
        for quart in [false, true] {
            if quart && !tourner {
                break;
            }
            let (w, t) = if quart { (t0, w0) } else { (w0, t0) };
            for i in 0..paliers.len() {
                let x = paliers[i][0];
                if x + w > l + JEU {
                    break;
                }
                // La marge compte aussi à droite : un palier voisin plus haut peut la border
                let y = paliers[i..].iter().take_while(|p| p[0] < x + w + ecart - JEU).fold(paliers[i][1], |y, p| y.max(p[1]));
                if y + t <= h + JEU && mieux.is_none_or(|m| (y + t, x) < (m.0, m.1)) {
                    mieux = Some((y + t, x, y, w, t, quart));
                }
            }
        }
        let (_, x, y, w, t, quart) = mieux?;
        places[k] = ([x, y], quart);
        let fin = (x + w + ecart).min(l);
        let mut suite: Vec<[f64; 3]> = Vec::with_capacity(paliers.len() + 2);
        let mut poser = |p: [f64; 3]| match suite.last_mut() {
            Some(d) if d[1] == p[1] => d[2] += p[2],
            _ if p[2] > 0.0 => suite.push(p),
            _ => {}
        };
        for p in paliers.iter().filter(|p| p[0] < x) {
            poser([p[0], p[1], (p[0] + p[2]).min(x) - p[0]]);
        }
        poser([x, y + t + ecart, fin - x]);
        for p in paliers.iter().filter(|p| p[0] + p[2] > fin) {
            let a = p[0].max(fin);
            poser([a, p[1], p[0] + p[2] - a]);
        }
        paliers = suite;
    }
    Some(places)
}

/// Range les îlots des faces données dans la tuile, à la plus grande échelle commune qui
/// tienne, marges comprises. Les îlots restent rigides et ne sont jamais retournés ; ce
/// sont leurs boîtes qui se rangent. `None` : la marge ne laisse pas la place.
pub fn mettre_en_page(m: &Maillage, faces: &[usize], page: Page, a: Aspect) -> Option<Disposition> {
    let (tl, th) = (a.l as f64, a.h as f64);
    let groupes = ilots(m, faces);
    let echelles = if page.egaliser { densites(m, &groupes, (tl, th)) } else { vec![1.0; groupes.len()] };
    let plans: Vec<Ilot> = groupes
        .iter()
        .zip(&echelles)
        .map(|(l, s)| l.iter().map(|c| (*c, { let q = uv_de(m, *c); [q[0] as f64 * tl * s, q[1] as f64 * th * s] })).collect())
        .collect();
    disposer(&plans, page, a)
}

/// Range des îlots donnés dans le plan.
fn disposer(plans: &[Ilot], page: Page, a: Aspect) -> Option<Disposition> {
    let droite = ranger(plans.to_vec(), Page { tourner: false, ..page }, a);
    if !page.tourner {
        return droite;
    }
    // Pivoter n'aide pas toujours : une longue bande seule tient mieux en diagonale
    let tournee = ranger(plans.to_vec(), page, a);
    match (&droite, &tournee) {
        (Some(d), Some(t)) if d.remplissage >= t.remplissage => droite,
        (Some(_), None) => droite,
        _ => tournee,
    }
}

/// Aire d'un îlot : ses coins, face par face et dans l'ordre.
fn aire_ilot(l: &Ilot) -> f64 {
    let mut faces: HashMap<usize, Vec<(usize, [f64; 2])>> = HashMap::new();
    for ((f, k), p) in l {
        faces.entry(*f).or_default().push((*k, *p));
    }
    faces.values_mut().map(|c| {
        c.sort_by_key(|x| x.0);
        let n = c.len();
        (0..n).map(|k| c[k].1[0] * c[(k + 1) % n].1[1] - c[(k + 1) % n].1[0] * c[k].1[1]).sum::<f64>().abs() / 2.0
    }).sum()
}

fn ranger(mut plans: Vec<Ilot>, page: Page, a: Aspect) -> Option<Disposition> {
    let (tl, th) = (a.l as f64, a.h as f64);
    if page.tourner {
        plans.iter_mut().for_each(serrer);
    }
    let marge = page.marge as f64;
    let (ul, uh) = (tl - 2.0 * marge, th - 2.0 * marge);
    let boites: Vec<[f64; 4]> = plans.iter().map(etendue).collect();
    let bases: Vec<[f64; 2]> = boites.iter().map(|b| [b[2], b[3]]).collect();
    let trie = |f: &dyn Fn(&[f64; 2]) -> f64| {
        let mut o: Vec<usize> = (0..bases.len()).collect();
        o.sort_by(|a, b| f(&bases[*b]).total_cmp(&f(&bases[*a])));
        o
    };
    let ordres = [trie(&|b| b[1]), trie(&|b| b[0]), trie(&|b| b[0] * b[1]), trie(&|b| b[0].max(b[1]))];
    let essai = |k: f64| {
        let tailles: Vec<[f64; 2]> = bases.iter().map(|b| [b[0] * k, b[1] * k]).collect();
        ordres.iter().find_map(|o| ciel(&tailles, o, (ul, uh), marge, page.tourner).map(|p| (p, tailles.clone())))
    };
    let mut trouve = essai(0.0)?;
    let cote = |f: &dyn Fn(&[f64; 2]) -> f64| bases.iter().map(f).fold(0.0, f64::max).max(1e-12);
    let plafond = if page.tourner {
        ul.min(uh).max(ul.max(uh) * cote(&|b| b[0].min(b[1])) / cote(&|b| b[0].max(b[1]))) / cote(&|b| b[0].min(b[1]))
    } else {
        (ul / cote(&|b| b[0])).min(uh / cote(&|b| b[1]))
    };
    let surface: f64 = bases.iter().map(|b| b[0] * b[1]).sum();
    let plafond = if surface > 0.0 { plafond.min((ul * uh / surface).sqrt()) } else { plafond };
    let mut k = 0.0;
    let (mut bas, mut haut) = (0.0, plafond);
    for essais in 0..20 {
        // Un îlot seul, ou des îlots égaux, atteignent souvent le plafond
        let x = if essais == 0 { haut } else { (bas + haut) / 2.0 };
        match essai(x) {
            Some(r) => {
                (trouve, k, bas) = (r, x, x);
                if essais == 0 {
                    break;
                }
            }
            None => haut = x,
        }
    }
    let (places, tailles) = trouve;
    let (mut large, mut grand) = (0.0_f64, 0.0_f64);
    for (([x, y], quart), [w, t]) in places.iter().zip(&tailles) {
        let (w, t) = if *quart { (*t, *w) } else { (*w, *t) };
        large = large.max(x + w);
        grand = grand.max(y + t);
    }
    let (dx, dy) = ((tl - large) / 2.0, (th - grand) / 2.0);
    let mut coins = Vec::new();
    for (i, l) in plans.iter().enumerate() {
        let ([x, y], quart) = places[i];
        let b = boites[i];
        for (c, q) in l {
            let (u, v) = (q[0] - b[0], q[1] - b[1]);
            let (u, v) = if quart { (v, b[2] - u) } else { (u, v) };
            coins.push((*c, [((dx + x + u * k) / tl) as f32, ((dy + y + v * k) / th) as f32]));
        }
    }
    let couvert: f64 = plans.iter().map(aire_ilot).sum::<f64>() * k * k;
    Some(Disposition { coins, ilots: plans.len(), remplissage: (couvert / (tl * th)) as f32 })
}

#[cfg(test)]
mod tests_page {
    use super::*;
    use crate::maillage::Maillage;

    /// Un cube projeté : six îlots carrés.
    fn cube() -> Maillage {
        cube_pour(Aspect::CARRE)
    }

    fn cube_pour(a: Aspect) -> Maillage {
        let mut m = Maillage::boite(2.0, 2.0, 2.0);
        let faces: Vec<usize> = (0..m.faces.len()).collect();
        for ((f, k), q) in projeter(&m, &faces, Projection::Cube, a) {
            let n = m.faces[f].s.len();
            m.faces[f].uv.resize(n, [0.0; 2]);
            m.faces[f].uv[k] = q;
        }
        m
    }

    fn applique(m: &mut Maillage, d: &Disposition) {
        for ((f, k), q) in &d.coins {
            m.faces[*f].uv[*k] = *q;
        }
    }

    fn tout(m: &Maillage) -> Vec<usize> {
        (0..m.faces.len()).collect()
    }

    fn range(m: &mut Maillage, page: Page, a: Aspect) {
        let d = mettre_en_page(m, &tout(m), page, a).unwrap();
        applique(m, &d);
    }

    /// Boîtes des îlots, dans le plan.
    fn boites(m: &Maillage, a: Aspect) -> Vec<[f32; 4]> {
        ilots(m, &tout(m)).iter().map(|l| {
            let (mut p, mut q) = ([f32::MAX; 2], [f32::MIN; 2]);
            for c in l {
                let w = uv_de(m, *c);
                let w = [w[0] * a.l, w[1] * a.h];
                for k in 0..2 {
                    p[k] = p[k].min(w[k]);
                    q[k] = q[k].max(w[k]);
                }
            }
            [p[0], p[1], q[0], q[1]]
        }).collect()
    }

    fn verifie_rangement(m: &Maillage, a: Aspect, marge: f32) {
        let b = boites(m, a);
        for (i, x) in b.iter().enumerate() {
            assert!(x[0] >= marge - 1e-4 && x[1] >= marge - 1e-4, "{x:?}");
            assert!(x[2] <= a.l - marge + 1e-4 && x[3] <= a.h - marge + 1e-4, "{x:?}");
            for y in &b[i + 1..] {
                let separes = x[2] + marge <= y[0] + 1e-4 || y[2] + marge <= x[0] + 1e-4 || x[3] + marge <= y[1] + 1e-4 || y[3] + marge <= x[1] + 1e-4;
                assert!(separes, "{x:?} {y:?}");
            }
        }
    }

    /// Aire signée d'une face : son signe dit si l'îlot a été retourné.
    fn signe(m: &Maillage, f: usize) -> f32 {
        let p = &m.faces[f].uv;
        (0..p.len()).map(|k| p[k][0] * p[(k + 1) % p.len()][1] - p[(k + 1) % p.len()][0] * p[k][1]).sum()
    }

    #[test]
    fn les_ilots_ne_se_touchent_pas_et_restent_dans_la_tuile() {
        for (a, tourner) in [(Aspect::CARRE, false), (Aspect::CARRE, true), (Aspect { l: 1.0, h: 0.5 }, false), (Aspect { l: 0.25, h: 1.0 }, true)] {
            let mut m = cube();
            let page = Page { tourner, ..Page::DEFAUT };
            let d = mettre_en_page(&m, &tout(&m), page, a).unwrap();
            assert_eq!(d.ilots, 6);
            applique(&mut m, &d);
            verifie_rangement(&m, a, page.marge);
        }
    }

    #[test]
    fn six_carres_remplissent_mieux_la_tuile_que_les_rangees_de_la_projection() {
        let m = cube();
        let d = mettre_en_page(&m, &tout(&m), Page::DEFAUT, Aspect::CARRE).unwrap();
        // Trois colonnes, deux rangs : 6/9 de la tuile, moins les marges
        assert!(d.remplissage > 0.6, "{}", d.remplissage);
    }

    #[test]
    fn la_meme_densite_ramene_un_ilot_retreci_a_sa_taille() {
        let mut m = cube();
        for q in &mut m.faces[0].uv {
            *q = [q[0] / 4.0, q[1] / 4.0];
        }
        let f = tout(&m);
        let aire = |m: &Maillage, f: usize| aire_plan(m, f, (1.0, 1.0));
        let mut gardee = m.clone();
        applique(&mut gardee, &mettre_en_page(&m, &f, Page { egaliser: false, ..Page::DEFAUT }, Aspect::CARRE).unwrap());
        assert!((aire(&gardee, 1) / aire(&gardee, 0) - 16.0).abs() < 0.01);
        range(&mut m, Page::DEFAUT, Aspect::CARRE);
        assert!((aire(&m, 1) / aire(&m, 0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn la_densite_tient_compte_de_la_forme_de_l_image() {
        let a = Aspect { l: 1.0, h: 0.5 };
        let mut m = cube_pour(a);
        range(&mut m, Page::DEFAUT, a);
        // À l'écran, le carré du modèle reste carré
        let b = boites(&m, a);
        for x in b {
            assert!(((x[2] - x[0]) / (x[3] - x[1]) - 1.0).abs() < 0.01, "{x:?}");
        }
    }

    #[test]
    fn un_ilot_garde_sa_forme_ses_soudures_et_son_sens() {
        for tourner in [false, true] {
            let mut m = cube();
            let f = tout(&m);
            let (avant, points) = (f.iter().map(|x| signe(&m, *x)).collect::<Vec<_>>(), points(&m, &f).len());
            let long = |m: &Maillage, f: usize| {
                let p = &m.faces[f].uv;
                (p[0][0] - p[1][0]).hypot(p[0][1] - p[1][1]) / (p[2][0] - p[1][0]).hypot(p[2][1] - p[1][1])
            };
            let rapports: Vec<f32> = f.iter().map(|x| long(&m, *x)).collect();
            range(&mut m, Page { tourner, ..Page::DEFAUT }, Aspect::CARRE);
            assert_eq!(super::points(&m, &f).len(), points);
            for x in &f {
                assert!(signe(&m, *x) * avant[*x] > 0.0, "îlot retourné");
                assert!((long(&m, *x) - rapports[*x]).abs() < 0.01);
            }
        }
    }

    #[test]
    fn sans_rotation_un_ilot_garde_son_orientation() {
        let mut m = cube();
        let v = |m: &Maillage, f: usize| {
            let p = &m.faces[f].uv;
            let n = (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]);
            [(p[1][0] - p[0][0]) / n, (p[1][1] - p[0][1]) / n]
        };
        let avant: Vec<_> = tout(&m).iter().map(|f| v(&m, *f)).collect();
        range(&mut m, Page::DEFAUT, Aspect::CARRE);
        for f in tout(&m) {
            assert!((v(&m, f)[0] - avant[f][0]).abs() < 1e-3 && (v(&m, f)[1] - avant[f][1]).abs() < 1e-3);
        }
    }

    /// Des faces sans sommet commun, une par liste d'UV.
    fn eparses(uv: &[Vec<Uv>]) -> Maillage {
        let mut m = Maillage::boite(1.0, 1.0, 1.0);
        let modele = m.faces[0].clone();
        let (mut sommets, mut faces) = (Vec::new(), Vec::new());
        for q in uv {
            let mut f = modele.clone();
            f.s = q.iter().map(|p| {
                sommets.push([p[0] as f64, p[1] as f64, 0.0]);
                sommets.len() - 1
            }).collect();
            f.uv = q.clone();
            faces.push(f);
        }
        (m.sommets, m.faces) = (sommets, faces);
        m
    }

    /// Bande de biais, longue de 0,4 et large de 0,04.
    fn bande(angle: f32, o: f32) -> Vec<Uv> {
        let (c, s) = (angle.to_radians().cos(), angle.to_radians().sin());
        [[0.0, 0.0], [0.4, 0.0], [0.4, 0.04], [0.0, 0.04]].iter().map(|p: &[f32; 2]| [o + p[0] * c - p[1] * s, o + p[0] * s + p[1] * c]).collect()
    }

    #[test]
    fn la_rotation_couche_les_ilots_de_biais() {
        let m = eparses(&(0..6).map(|i| bande(45.0, 0.1 * i as f32)).collect::<Vec<_>>());
        let f = tout(&m);
        let p = Page { egaliser: false, ..Page::DEFAUT };
        let d0 = mettre_en_page(&m, &f, p, Aspect::CARRE).unwrap();
        let d1 = mettre_en_page(&m, &f, Page { tourner: true, ..p }, Aspect::CARRE).unwrap();
        assert!(d1.remplissage > d0.remplissage * 3.0, "{} {}", d1.remplissage, d0.remplissage);
    }

    #[test]
    fn la_rotation_ne_fait_jamais_perdre_de_place() {
        let m = eparses(&[bande(45.0, 0.3)]);
        let f = tout(&m);
        let p = Page { egaliser: false, ..Page::DEFAUT };
        let d0 = mettre_en_page(&m, &f, p, Aspect::CARRE).unwrap();
        let d1 = mettre_en_page(&m, &f, Page { tourner: true, ..p }, Aspect::CARRE).unwrap();
        assert!(d1.remplissage >= d0.remplissage);
    }

    #[test]
    fn beaucoup_d_ilots_se_rangent_sans_chevauchement_et_une_marge_trop_grande_est_refusee() {
        let uv: Vec<Vec<Uv>> = (0..3000).map(|i| {
            let (x, l, t) = ((i % 7) as f32 * 0.1, 0.02 + (i % 11) as f32 * 0.01, 0.02 + (i % 5) as f32 * 0.02);
            vec![[x, 0.0], [x + l, 0.0], [x + l, t], [x, t]]
        }).collect();
        let mut m = eparses(&uv);
        let t = std::time::Instant::now();
        let d = mettre_en_page(&m, &tout(&m), Page::DEFAUT, Aspect::CARRE);
        eprintln!("3000 îlots : {:?}", t.elapsed());
        let d = d.expect("place");
        assert_eq!(d.ilots, 3000);
        assert!(d.remplissage > 0.1, "{}", d.remplissage);
        assert!(mettre_en_page(&m, &tout(&m), Page { marge: 0.2, ..Page::DEFAUT }, Aspect::CARRE).is_none());
        // Marge de la plus petite taille d'image courante : le rangement reste juste
        let p = Page { marge: 1.0 / 2048.0, ..Page::DEFAUT };
        range(&mut m, p, Aspect::CARRE);
        verifie_rangement(&m, Aspect::CARRE, p.marge);
    }

    #[test]
    fn les_faces_sans_aire_ne_font_pas_de_nan() {
        let mut m = cube();
        for f in &mut m.faces {
            for q in &mut f.uv {
                *q = [0.5, 0.5];
            }
        }
        let d = mettre_en_page(&m, &tout(&m), Page::DEFAUT, Aspect::CARRE).unwrap();
        assert!(d.coins.iter().all(|(_, q)| q[0].is_finite() && q[1].is_finite()));
    }
}

/// Arêtes où un dépliage coupe : celles marquées, celles qui bordent plus de deux faces,
/// et celles de deux faces de sens opposés, qui ne se déplient pas d'un seul tenant.
/// Chaque carte est un morceau d'un seul tenant, à plat ; un sommet coupé y compte double.
struct Carte {
    /// Coin → sommet de la carte.
    coins: Vec<(Coin, usize)>,
    positions: Vec<P3>,
    triangles: Vec<[usize; 3]>,
}

fn racine(p: &mut [usize], mut x: usize) -> usize {
    while p[x] != x {
        p[x] = p[p[x]];
        x = p[x];
    }
    x
}

fn cartes(m: &Maillage, faces: &[usize]) -> Vec<Carte> {
    let cle_a = |a: usize, b: usize| (a.min(b), a.max(b));
    let coutures: HashSet<(usize, usize)> = m.coutures_valides().iter().map(|a| cle_a(a[0], a[1])).collect();
    let mut debut = Vec::with_capacity(faces.len());
    let mut nb = 0;
    for &f in faces {
        debut.push(nb);
        nb += m.faces[f].s.len();
    }
    let mut pf: Vec<usize> = (0..faces.len()).collect();
    let mut pc: Vec<usize> = (0..nb).collect();
    // Arête → rang de la face, coin de départ, sens
    let mut par_arete: HashMap<(usize, usize), Vec<(usize, usize, bool)>> = HashMap::new();
    for (r, &f) in faces.iter().enumerate() {
        let s = &m.faces[f].s;
        for k in 0..s.len() {
            let (a, b) = (s[k], s[(k + 1) % s.len()]);
            par_arete.entry(cle_a(a, b)).or_default().push((r, k, a < b));
        }
    }
    for (c, l) in &par_arete {
        let [x, y] = l[..] else { continue };
        if coutures.contains(c) || x.2 == y.2 {
            continue;
        }
        let (rx, ry) = (racine(&mut pf, x.0), racine(&mut pf, y.0));
        pf[rx] = ry;
        // Les deux faces parcourent l'arête en sens opposés
        let (nx, ny) = (m.faces[faces[x.0]].s.len(), m.faces[faces[y.0]].s.len());
        for (a, b) in [(debut[x.0] + x.1, debut[y.0] + (y.1 + 1) % ny), (debut[x.0] + (x.1 + 1) % nx, debut[y.0] + y.1)] {
            let (ra, rb) = (racine(&mut pc, a), racine(&mut pc, b));
            pc[ra] = rb;
        }
    }
    let mut rangs: HashMap<usize, usize> = HashMap::new();
    let mut l: Vec<Carte> = Vec::new();
    let mut sommets: Vec<HashMap<usize, usize>> = Vec::new();
    for (r, &f) in faces.iter().enumerate() {
        let n = rangs.len();
        let c = *rangs.entry(racine(&mut pf, r)).or_insert(n);
        if c == l.len() {
            l.push(Carte { coins: Vec::new(), positions: Vec::new(), triangles: Vec::new() });
            sommets.push(HashMap::new());
        }
        let s = &m.faces[f].s;
        let mut locaux = HashMap::new();
        for (k, &i) in s.iter().enumerate() {
            let q = racine(&mut pc, debut[r] + k);
            let v = *sommets[c].entry(q).or_insert_with(|| {
                l[c].positions.push(m.sommets[i]);
                l[c].positions.len() - 1
            });
            l[c].coins.push(((f, k), v));
            locaux.insert(i, v);
        }
        l[c].triangles.extend(m.trianguler(s).iter().map(|t| t.map(|i| locaux[&i])));
    }
    l
}

/// Moindres carrés conformes (Lévy et al., 2002) : chaque triangle veut rester semblable
/// à lui-même, deux sommets du bord fixés. Rend la position de chaque sommet ; `None` si
/// la carte n'a pas de bord. Résolu par gradient conjugué, sans jamais former la matrice.
fn lscm(c: &Carte) -> Option<Vec<[f64; 2]>> {
    let n = c.positions.len();
    let mut compte: HashMap<(usize, usize), u32> = HashMap::new();
    for t in &c.triangles {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *compte.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let bord: Vec<usize> = compte.iter().filter(|x| *x.1 == 1).flat_map(|x| [x.0 .0, x.0 .1]).collect();
    let d2 = |a: usize, b: usize| {
        let (p, q) = (c.positions[a], c.positions[b]);
        [0, 1, 2].map(|k| (p[k] - q[k]).powi(2)).iter().sum::<f64>()
    };
    let loin = |de: usize| bord.iter().copied().max_by(|x, y| d2(de, *x).total_cmp(&d2(de, *y)).then(x.cmp(y)));
    let pin_a = loin(*bord.iter().min()?)?;
    let pin_b = loin(pin_a)?;
    if pin_a == pin_b {
        return None;
    }
    let ecart = d2(pin_a, pin_b).sqrt().max(1e-12);
    // Variables : u puis v de chaque sommet ; PIN : fixé
    const PIN: u32 = u32::MAX;
    let mut rang = vec![0u32; 2 * n];
    let mut libres = 0;
    for i in 0..n {
        for k in 0..2 {
            rang[2 * i + k] = if i == pin_a || i == pin_b {
                PIN
            } else {
                libres += 1;
                libres - 1
            };
        }
    }
    let libres = libres as usize;
    let fixe = |i: usize, k: usize| if i == pin_b && k == 0 { ecart } else { 0.0 };
    // Deux lignes par triangle, trois sommets de deux coordonnées chacune
    let mut lignes: Vec<([u32; 6], [f64; 6])> = Vec::with_capacity(2 * c.triangles.len());
    let mut second: Vec<f64> = Vec::with_capacity(2 * c.triangles.len());
    for t in &c.triangles {
        let (p0, p1, p2) = (c.positions[t[0]], c.positions[t[1]], c.positions[t[2]]);
        let e = [0, 1, 2].map(|k| p1[k] - p0[k]);
        let f = [0, 1, 2].map(|k| p2[k] - p0[k]);
        let l = scal(e, e).sqrt();
        if l < 1e-12 {
            continue;
        }
        let x2 = scal(f, e) / l;
        let y2 = (scal(f, f) - x2 * x2).max(0.0).sqrt();
        let double = l * y2;
        if double < 1e-14 {
            continue;
        }
        let p = [[0.0, 0.0], [l, 0.0], [x2, y2]];
        let w = 1.0 / double.sqrt();
        let (mut cols, mut re, mut im) = ([0u32; 6], [0.0; 6], [0.0; 6]);
        for j in 0..3 {
            let (a, b) = (p[(j + 2) % 3], p[(j + 1) % 3]);
            let (wr, wi) = ((a[0] - b[0]) * w, (a[1] - b[1]) * w);
            cols[2 * j] = rang[2 * t[j]];
            cols[2 * j + 1] = rang[2 * t[j] + 1];
            re[2 * j] = wr;
            re[2 * j + 1] = -wi;
            im[2 * j] = wi;
            im[2 * j + 1] = wr;
        }
        for v in [re, im] {
            let mut b = 0.0;
            for j in 0..3 {
                for k in 0..2 {
                    if cols[2 * j + k] == PIN {
                        b -= v[2 * j + k] * fixe(t[j], k);
                    }
                }
            }
            lignes.push((cols, v));
            second.push(b);
        }
    }
    let fois = |x: &[f64], y: &mut [f64]| {
        for (r, (l, v)) in y.iter_mut().zip(&lignes) {
            *r = (0..6).filter(|k| l[*k] != PIN).map(|k| v[k] * x[l[k] as usize]).sum();
        }
    };
    let transposee = |x: &[f64], y: &mut [f64]| {
        y.fill(0.0);
        for (r, (l, v)) in x.iter().zip(&lignes) {
            for k in 0..6 {
                if l[k] != PIN {
                    y[l[k] as usize] += v[k] * r;
                }
            }
        }
    };
    let mut diag = vec![0.0; libres];
    for (l, v) in &lignes {
        for k in 0..6 {
            if l[k] != PIN {
                diag[l[k] as usize] += v[k] * v[k];
            }
        }
    }
    let inverse: Vec<f64> = diag.iter().map(|d| if *d > 0.0 { 1.0 / d } else { 0.0 }).collect();
    // Départ : à plat dans le plan moyen, les deux points fixes à leur place
    let mut x = vec![0.0; libres];
    {
        let (a, b) = (c.positions[pin_a], c.positions[pin_b]);
        let axe = [0, 1, 2].map(|k| (b[k] - a[k]) / ecart);
        let mut normale = [0.0; 3];
        for t in &c.triangles {
            let (p, q, r) = (c.positions[t[0]], c.positions[t[1]], c.positions[t[2]]);
            let (e, f) = ([0, 1, 2].map(|k| q[k] - p[k]), [0, 1, 2].map(|k| r[k] - p[k]));
            normale[0] += e[1] * f[2] - e[2] * f[1];
            normale[1] += e[2] * f[0] - e[0] * f[2];
            normale[2] += e[0] * f[1] - e[1] * f[0];
        }
        let croix = [normale[1] * axe[2] - normale[2] * axe[1], normale[2] * axe[0] - normale[0] * axe[2], normale[0] * axe[1] - normale[1] * axe[0]];
        let l = scal(croix, croix).sqrt();
        for i in 0..n {
            let d = [0, 1, 2].map(|k| c.positions[i][k] - a[k]);
            if rang[2 * i] != PIN {
                x[rang[2 * i] as usize] = scal(d, axe);
                if l > 1e-12 {
                    x[rang[2 * i + 1] as usize] = scal(d, croix) / l;
                }
            }
        }
    }
    let mut r = vec![0.0; lignes.len()];
    fois(&x, &mut r);
    r.iter_mut().zip(&second).for_each(|(a, b)| *a = b - *a);
    let mut s = vec![0.0; libres];
    transposee(&r, &mut s);
    let mut z: Vec<f64> = s.iter().zip(&inverse).map(|(a, b)| a * b).collect();
    let mut p = z.clone();
    let mut gamma: f64 = s.iter().zip(&z).map(|(a, b)| a * b).sum();
    let depart = gamma;
    let mut q = vec![0.0; lignes.len()];
    for _ in 0..(4 * libres + 200).min(30000) {
        if gamma <= depart * 1e-20 || gamma == 0.0 {
            break;
        }
        fois(&p, &mut q);
        let d: f64 = q.iter().map(|a| a * a).sum();
        if d <= 0.0 {
            break;
        }
        let alpha = gamma / d;
        x.iter_mut().zip(&p).for_each(|(a, b)| *a += alpha * b);
        r.iter_mut().zip(&q).for_each(|(a, b)| *a -= alpha * b);
        transposee(&r, &mut s);
        z.iter_mut().zip(s.iter().zip(&inverse)).for_each(|(a, (b, c))| *a = b * c);
        let nouveau: f64 = s.iter().zip(&z).map(|(a, b)| a * b).sum();
        let beta = nouveau / gamma;
        gamma = nouveau;
        p.iter_mut().zip(&z).for_each(|(a, b)| *a = b + beta * *a);
    }
    Some((0..n).map(|i| [0, 1].map(|k| if rang[2 * i + k] == PIN { fixe(i, k) } else { x[rang[2 * i + k] as usize] })).collect())
}

/// Met une carte dépliée à l'échelle du modèle, à l'endroit (l'image n'est pas vue en
/// miroir, le haut du modèle vers le haut de l'image). Rend les triangles retournés.
fn redresser(c: &Carte, p: &mut [[f64; 2]]) -> usize {
    let signee = |p: &[[f64; 2]], t: &[usize; 3]| {
        let (a, b, d) = (p[t[0]], p[t[1]], p[t[2]]);
        ((b[0] - a[0]) * (d[1] - a[1]) - (b[1] - a[1]) * (d[0] - a[0])) / 2.0
    };
    let total: f64 = c.triangles.iter().map(|t| signee(p, t)).sum();
    // v descend : une face vue de l'extérieur tourne en sens négatif
    if total > 0.0 {
        p.iter_mut().for_each(|q| q[0] = -q[0]);
    }
    let a2: f64 = c.triangles.iter().map(|t| signee(p, t).abs()).sum();
    let a3: f64 = c.triangles.iter().map(|t| {
        let (a, b, d) = (c.positions[t[0]], c.positions[t[1]], c.positions[t[2]]);
        let (e, f) = ([0, 1, 2].map(|k| b[k] - a[k]), [0, 1, 2].map(|k| d[k] - a[k]));
        let n = [e[1] * f[2] - e[2] * f[1], e[2] * f[0] - e[0] * f[2], e[0] * f[1] - e[1] * f[0]];
        scal(n, n).sqrt() / 2.0
    }).sum();
    if a2 > 1e-18 && a3 > 1e-18 {
        let s = (a3 / a2).sqrt();
        p.iter_mut().for_each(|q| *q = [q[0] * s, q[1] * s]);
    }
    let mut g = [0.0, 0.0];
    for t in &c.triangles {
        let (a, b, d) = (p[t[0]], p[t[1]], p[t[2]]);
        let (ha, hb, hd) = (c.positions[t[0]][1], c.positions[t[1]][1], c.positions[t[2]][1]);
        g[0] -= (ha * (b[1] - d[1]) + hb * (d[1] - a[1]) + hd * (a[1] - b[1])) / 2.0;
        g[1] -= (ha * (d[0] - b[0]) + hb * (a[0] - d[0]) + hd * (b[0] - a[0])) / 2.0;
    }
    // Une pièce à plat n'a pas de haut : seule une pente nette la fait pivoter
    if g[0].hypot(g[1]) > 0.2 * a3.max(1e-18) {
        let theta = -std::f64::consts::FRAC_PI_2 - g[1].atan2(g[0]);
        let (sin, cos) = theta.sin_cos();
        p.iter_mut().for_each(|q| *q = [q[0] * cos - q[1] * sin, q[0] * sin + q[1] * cos]);
    }
    c.triangles.iter().filter(|t| signee(p, t) > 1e-12 * a3.max(1e-12)).count()
}

/// Résultat d'un dépliage.
pub struct Depliage {
    pub disposition: Disposition,
    /// Triangles que le dépliage a retournés : il manque des coutures.
    pub retournes: usize,
}

/// Déplie les faces données, coupées à leurs coutures et à leur bord, puis range les
/// morceaux dans la tuile. Ils ont tous la densité du modèle.
pub fn deplier(m: &Maillage, faces: &[usize], page: Page, a: Aspect) -> Result<Depliage, String> {
    let mut plans: Vec<Ilot> = Vec::new();
    let mut retournes = 0;
    for c in cartes(m, faces) {
        let mut p = lscm(&c).ok_or("Un morceau est fermé : marquez une couture dessus")?;
        retournes += redresser(&c, &mut p);
        plans.push(c.coins.iter().map(|(coin, v)| (*coin, p[*v])).collect());
    }
    let disposition = disposer(&plans, page, a).ok_or("Trop de morceaux pour cette marge")?;
    Ok(Depliage { disposition, retournes })
}

#[cfg(test)]
mod tests_depliage {
    use super::*;
    use crate::maillage::Maillage;

    fn tout(m: &Maillage) -> Vec<usize> {
        (0..m.faces.len()).collect()
    }

    fn applique(m: &mut Maillage, d: &Disposition) {
        for f in &mut m.faces {
            f.uv = vec![[0.0; 2]; f.s.len()];
        }
        for ((f, k), q) in &d.coins {
            m.faces[*f].uv[*k] = *q;
        }
    }

    fn deplie(m: &mut Maillage) -> Depliage {
        let d = deplier(m, &tout(m), Page::DEFAUT, Aspect::CARRE).unwrap();
        applique(m, &d.disposition);
        d
    }

    /// Rapports des côtés de chaque face entre UV et modèle : tous égaux si rien n'est étiré.
    fn etirement(m: &Maillage) -> f64 {
        let mut r: Vec<f64> = Vec::new();
        for f in &m.faces {
            for k in 0..f.s.len() {
                let j = (k + 1) % f.s.len();
                let (a, b) = (m.sommets[f.s[k]], m.sommets[f.s[j]]);
                let l3 = [0, 1, 2].map(|c| (a[c] - b[c]).powi(2)).iter().sum::<f64>().sqrt();
                let l2 = ((f.uv[k][0] - f.uv[j][0]) as f64).hypot((f.uv[k][1] - f.uv[j][1]) as f64);
                r.push(l2 / l3);
            }
        }
        let (a, b) = (r.iter().copied().fold(f64::MAX, f64::min), r.iter().copied().fold(0.0, f64::max));
        b / a
    }

    fn sens(m: &Maillage, f: usize) -> f32 {
        let p = &m.faces[f].uv;
        (0..p.len()).map(|k| p[k][0] * p[(k + 1) % p.len()][1] - p[(k + 1) % p.len()][0] * p[k][1]).sum()
    }

    fn grille(n: usize) -> Maillage {
        let sommets = (0..=n).flat_map(|j| (0..=n).map(move |i| [i as f64, j as f64 * 0.5, (j as f64 * 0.4).sin() * 0.0])).collect();
        let faces = (0..n).flat_map(|j| (0..n).map(move |i| {
            let c = |x: usize, y: usize| y * (n + 1) + x;
            Face { s: vec![c(i, j), c(i + 1, j), c(i + 1, j + 1), c(i, j + 1)], m: crate::maillage::defaut(), uv: Vec::new() }
        })).collect();
        Maillage::new(sommets, faces, Vec::new())
    }

    use crate::maillage::Face;

    /// Cube coupé en croix : sept arêtes.
    fn cube_coupe() -> Maillage {
        let mut m = Maillage::boite(2.0, 2.0, 2.0);
        let aretes = m.aretes();
        let p = |i: usize| m.sommets[i];
        // Tout le tour du dessus sauf une arête, plus les quatre verticales sauf une
        let haut: Vec<_> = aretes.iter().copied().filter(|a| p(a[0])[1] > 0.0 && p(a[1])[1] > 0.0).collect();
        let debout: Vec<_> = aretes.iter().copied().filter(|a| (p(a[0])[1] > 0.0) != (p(a[1])[1] > 0.0)).collect();
        // La dernière arête rejoint le sommet du bas que les verticales ont laissé
        let pris: Vec<usize> = debout[..3].iter().flatten().copied().collect();
        let bas = aretes.iter().copied().find(|a| p(a[0])[1] < 0.0 && p(a[1])[1] < 0.0 && (pris.contains(&a[0]) != pris.contains(&a[1]))).unwrap();
        let mut tour = vec![haut[0]];
        while tour.len() < 3 {
            let suite = haut.iter().find(|a| !tour.contains(a) && tour.iter().flatten().any(|i| a.contains(i))).unwrap();
            tour.push(*suite);
        }
        m.coutures = tour.iter().chain(&debout[..3]).copied().chain([bas]).collect();
        m
    }

    #[test]
    fn un_plan_se_deplie_sans_aucune_deformation() {
        let mut m = grille(6);
        let d = deplie(&mut m);
        assert_eq!((d.disposition.ilots, d.retournes), (1, 0));
        assert!(etirement(&m) < 1.01, "{}", etirement(&m));
    }

    #[test]
    fn un_cylindre_coupe_se_deroule_en_rectangle() {
        let mut m = Maillage::cylindre(1.0, 3.0, 24, false);
        let pointes: Vec<usize> = (0..m.faces.len()).filter(|f| m.faces[*f].s.len() != 4).collect();
        m.supprimer_faces_pour_test(&pointes);
        let cote = m.aretes().into_iter().find(|a| (m.sommets[a[0]][1] - m.sommets[a[1]][1]).abs() > 1.0).unwrap();
        m.coutures = vec![cote];
        let d = deplie(&mut m);
        assert_eq!(d.disposition.ilots, 1);
        assert_eq!(d.retournes, 0);
        assert!(etirement(&m) < 1.05, "{}", etirement(&m));
    }

    #[test]
    fn une_couture_reste_soudee_sur_le_modele_et_coupe_dans_l_image() {
        let mut m = Maillage::cylindre(1.0, 3.0, 12, false);
        let pointes: Vec<usize> = (0..m.faces.len()).filter(|f| m.faces[*f].s.len() != 4).collect();
        m.supprimer_faces_pour_test(&pointes);
        let n = m.sommets.len();
        let cote = m.aretes().into_iter().find(|a| (m.sommets[a[0]][1] - m.sommets[a[1]][1]).abs() > 1.0).unwrap();
        m.coutures = vec![cote];
        deplie(&mut m);
        assert_eq!(m.sommets.len(), n);
        // Les deux bouts de la couture ont chacun deux places
        assert_eq!(points(&m, &tout(&m)).len(), n + 2);
    }

    #[test]
    fn un_cube_coupe_est_six_carres_parfaits() {
        let mut m = cube_coupe();
        let d = deplie(&mut m);
        assert_eq!((d.disposition.ilots, d.retournes), (1, 0));
        assert!(etirement(&m) < 1.01, "{}", etirement(&m));
    }

    #[test]
    fn des_coutures_tout_autour_font_des_morceaux() {
        let mut m = Maillage::boite(2.0, 2.0, 2.0);
        m.coutures = m.aretes();
        let d = deplie(&mut m);
        assert_eq!(d.disposition.ilots, 6);
        assert!(etirement(&m) < 1.01);
    }

    #[test]
    fn un_morceau_ferme_est_refuse() {
        let m = Maillage::sphere_uv(1.0, 12, 8);
        let e = deplier(&m, &tout(&m), Page::DEFAUT, Aspect::CARRE).err().unwrap();
        assert!(e.contains("fermé"), "{e}");
    }

    #[test]
    fn seules_les_faces_visees_se_deplient_et_le_reste_fait_bord() {
        // Le bord des faces visées suffit : la sphère n'est pas fermée pour elles
        let m = Maillage::sphere_uv(1.0, 12, 8);
        let faces: Vec<usize> = (0..m.faces.len() / 2).collect();
        let d = deplier(&m, &faces, Page::DEFAUT, Aspect::CARRE).unwrap();
        assert!(d.disposition.coins.iter().all(|((f, _), _)| *f < faces.len()));
    }

    #[test]
    fn le_depliage_est_vu_a_l_endroit_comme_une_projection() {
        let mut m = cube_coupe();
        deplie(&mut m);
        let mut p = Maillage::boite(2.0, 2.0, 2.0);
        for ((f, k), q) in { let t = tout(&p); for f in &mut p.faces { f.uv = vec![[0.0; 2]; f.s.len()]; } projeter(&p, &t, Projection::Cube, Aspect::CARRE) } {
            p.faces[f].uv[k] = q;
        }
        for f in tout(&m) {
            assert!(sens(&m, f) * sens(&p, f) > 0.0, "face {f} en miroir");
        }
    }

    #[test]
    fn le_haut_du_modele_est_en_haut_de_l_image() {
        let mut m = Maillage::cylindre(1.0, 6.0, 24, false);
        let pointes: Vec<usize> = (0..m.faces.len()).filter(|f| m.faces[*f].s.len() != 4).collect();
        m.supprimer_faces_pour_test(&pointes);
        let cote = m.aretes().into_iter().find(|a| (m.sommets[a[0]][1] - m.sommets[a[1]][1]).abs() > 1.0).unwrap();
        m.coutures = vec![cote];
        deplie(&mut m);
        for f in &m.faces {
            for k in 0..f.s.len() {
                for j in 0..f.s.len() {
                    if m.sommets[f.s[k]][1] > m.sommets[f.s[j]][1] + 1.0 {
                        assert!(f.uv[k][1] < f.uv[j][1], "{:?}", f.uv);
                    }
                }
            }
        }
    }

    #[test]
    fn une_tete_decoupee_se_deplie_sans_retournement() {
        // Sphère ouverte d'un méridien, de pôle à pôle
        let mut m = Maillage::sphere_uv(1.0, 32, 16);
        let (a, b): (Vec<_>, Vec<_>) = m.sommets.iter().enumerate().map(|(i, _)| i).partition(|i| m.sommets[*i][2].abs() < 1e-9 && m.sommets[*i][0] >= -1e-9);
        let _ = b;
        let mut aretes: Vec<_> = m.aretes().into_iter().filter(|e| a.contains(&e[0]) && a.contains(&e[1])).collect();
        m.coutures.append(&mut aretes);
        let t = std::time::Instant::now();
        let d = deplie(&mut m);
        eprintln!("sphère : {:?}, {} retournés", t.elapsed(), d.retournes);
        assert_eq!(d.disposition.ilots, 1);
        assert!(m.faces.iter().flat_map(|f| &f.uv).all(|q| (0.0..=1.0).contains(&q[0]) && (0.0..=1.0).contains(&q[1])));
    }

    #[test]
    fn deux_faces_de_sens_opposes_se_separent() {
        let mut m = grille(2);
        m.faces[0].s.reverse();
        let d = deplie(&mut m);
        assert_eq!(d.disposition.ilots, 2);
    }
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
