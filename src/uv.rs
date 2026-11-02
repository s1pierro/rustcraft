//! Édition des coordonnées de texture, sans interface. Une UV appartient à un coin de
//! face : deux faces voisines peuvent donc se séparer le long d'une couture, alors que
//! leur sommet reste soudé. Un **point** réunit les coins d'un même sommet aux mêmes
//! coordonnées ; un **îlot** réunit les faces reliées par de tels points.

use crate::maillage::Maillage;
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
