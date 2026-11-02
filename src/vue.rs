//! Du maillage à ce qu'on dessine : tampons de sommets, silhouette, rayons.
//! Sans aucun appel OpenGL, donc testable partout.

use crate::maillage::{Maillage, P3};
use crate::math::Mat4;
use crate::projet::Objet;
use std::collections::HashMap;

/// Taille d'un sommet de face : position, normale et UV.
pub const FACE_F32: usize = 8;
/// UV d'un coin qui n'en a pas : le rendu y garde la seule couleur du matériau.
pub const SANS_UV: f32 = -1e4;
/// Taille d'un sommet de silhouette : position, autre extrémité, côté, n1, n2.
pub const BANDE_F32: usize = 13;

/// Tampons d'un objet, prêts pour le processeur graphique.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tampons {
    pub faces: Vec<f32>,
    pub bandes: Vec<f32>,
    pub bandes_idx: Vec<u32>,
    /// Arêtes, deux points chacune : le fil de fer d'une cible de booléen.
    pub fil: Vec<f32>,
}

fn pousser(v: &mut Vec<f32>, p: P3) {
    v.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
}

/// Faces à facettes : chaque triangle porte la normale de son polygone.
pub fn faces(m: &Maillage) -> Vec<f32> {
    faces_si(m, |_| true)
}

/// Les faces retenues par `garde`, d'après leur rang.
pub fn faces_si(m: &Maillage, garde: impl Fn(usize) -> bool) -> Vec<f32> {
    faces_si_uv(m, garde, |_| true)
}

/// Comme `faces_si` ; seules les faces retenues par `avec_uv` portent leurs UV.
pub fn faces_si_uv(m: &Maillage, garde: impl Fn(usize) -> bool, avec_uv: impl Fn(usize) -> bool) -> Vec<f32> {
    let mut v = Vec::with_capacity(m.faces.len() * 6 * FACE_F32);
    for (k, f) in m.faces.iter().enumerate() {
        if !garde(k) {
            continue;
        }
        let n = m.normale(&f.s);
        for t in m.trianguler(&f.s) {
            for i in t {
                pousser(&mut v, m.sommets[i]);
                pousser(&mut v, n);
                let uv = f.s.iter().position(|x| *x == i).and_then(|c| f.uv.get(c)).filter(|_| avec_uv(k) && f.uv.len() == f.s.len());
                v.extend_from_slice(uv.unwrap_or(&[SANS_UV; 2]));
            }
        }
    }
    v
}

/// Silhouette (`ed_silhouette`) : une bande de deux triangles par arête, dont les
/// sommets connaissent l'autre extrémité, leur côté et les normales des deux faces
/// de l'arête (n2 nulle si elle n'en a pas exactement deux : toujours tracée).
pub fn bandes(m: &Maillage) -> (Vec<f32>, Vec<u32>) {
    let mut ordre: Vec<(usize, usize)> = Vec::new();
    let mut aretes: HashMap<(usize, usize), Vec<P3>> = HashMap::new();
    for f in &m.faces {
        let n = m.normale(&f.s);
        for k in 0..f.s.len() {
            let (i, j) = (f.s[k], f.s[(k + 1) % f.s.len()]);
            let e = aretes.entry((i.min(j), i.max(j))).or_default();
            if e.is_empty() {
                ordre.push((i, j));
            }
            e.push(n);
        }
    }
    let mut v = Vec::with_capacity(ordre.len() * 4 * BANDE_F32);
    let mut idx = Vec::with_capacity(ordre.len() * 6);
    for (i, j) in ordre {
        let n = &aretes[&(i.min(j), i.max(j))];
        let (p, q, n1, n2) = (m.sommets[i], m.sommets[j], n[0], if n.len() == 2 { n[1] } else { [0.0; 3] });
        let d = (v.len() / BANDE_F32) as u32;
        for (a, b, cote) in [(p, q, 1.0), (p, q, -1.0), (q, p, -1.0), (q, p, 1.0)] {
            pousser(&mut v, a);
            pousser(&mut v, b);
            v.push(cote);
            pousser(&mut v, n1);
            pousser(&mut v, n2);
        }
        idx.extend_from_slice(&[d, d + 1, d + 2, d + 1, d + 3, d + 2]);
    }
    (v, idx)
}

/// Tampons sans silhouette : ceux de la copie de travail, refaits à chaque glissé.
pub fn tampons_faces(m: &Maillage) -> Tampons {
    Tampons { faces: faces(m), bandes: Vec::new(), bandes_idx: Vec::new(), fil: Vec::new() }
}

pub fn tampons(m: &Maillage) -> Tampons {
    let (bandes, bandes_idx) = bandes(m);
    let mut fil = Vec::new();
    for [i, j] in m.aretes() {
        pousser(&mut fil, m.sommets[i]);
        pousser(&mut fil, m.sommets[j]);
    }
    Tampons { faces: faces(m), bandes, bandes_idx, fil }
}

pub fn mat4(m: &[f64; 16]) -> Mat4 {
    m.map(|x| x as f32)
}

/// Matrice des normales : inverse transposée de la partie 3 × 3, par colonnes.
pub fn mat_normale(m: &[f64; 16]) -> [f32; 9] {
    let [a, b, c, d, e, f, g, h, i] = [m[0], m[1], m[2], m[4], m[5], m[6], m[8], m[9], m[10]];
    let (ca, cb, cc) = (e * i - f * h, f * g - d * i, d * h - e * g);
    let det = a * ca + b * cb + c * cc;
    let k = if det.abs() < 1e-30 { 1.0 } else { 1.0 / det };
    [ca, cb, cc, c * h - b * i, a * i - c * g, b * g - a * h, b * f - c * e, c * d - a * f, a * e - b * d].map(|x| (x * k) as f32)
}

pub fn appliquer(m: &[f64; 16], p: P3) -> P3 {
    [m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12], m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13], m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]]
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// Vecteur du monde ramené dans le repère de l'objet (sans sa position).
fn vers_local(o: &Objet, v: P3) -> P3 {
    let [x, y, z, w] = o.rotation;
    let l = (x * x + y * y + z * z + w * w).sqrt().max(1e-12);
    let (q, w) = ([-x / l, -y / l, -z / l], w / l);
    let t = cross(q, v).map(|c| 2.0 * c);
    let c = cross(q, t);
    [(v[0] + w * t[0] + c[0]) / o.echelle[0], (v[1] + w * t[1] + c[1]) / o.echelle[1], (v[2] + w * t[2] + c[2]) / o.echelle[2]]
}

/// Triangle le plus proche, côté endroit seulement, sur le rayon `o + t·d` : distance
/// et face. Sert à la copie de travail, déjà au monde.
pub fn touche_faces(tris: &[(usize, [P3; 3])], o: P3, d: P3) -> Option<(f64, usize)> {
    let mut mieux: Option<(f64, usize)> = None;
    for (f, [a, b, c]) in tris {
        let (e1, e2) = (sub(*b, *a), sub(*c, *a));
        let p = cross(d, e2);
        let det = dot(e1, p);
        if det <= 1e-14 {
            continue;
        }
        let s = sub(o, *a);
        let u = dot(s, p) / det;
        let q = cross(s, e1);
        let v = dot(d, q) / det;
        if u < 0.0 || v < 0.0 || u + v > 1.0 {
            continue;
        }
        let t = dot(e2, q) / det;
        if t > 0.0 && mieux.is_none_or(|m| t < m.0) {
            mieux = Some((t, *f));
        }
    }
    mieux
}

/// Distance le long du rayon `o + t·d` (monde) à la face touchée la plus proche.
pub fn toucher(obj: &Objet, m: &Maillage, o: P3, d: P3) -> Option<f64> {
    let (ol, dl) = (vers_local(obj, sub(o, obj.position)), vers_local(obj, d));
    let mut mieux: Option<f64> = None;
    for f in &m.faces {
        for [a, b, c] in m.trianguler(&f.s) {
            let (a, e1, e2) = (m.sommets[a], sub(m.sommets[b], m.sommets[a]), sub(m.sommets[c], m.sommets[a]));
            let p = cross(dl, e2);
            let det = dot(e1, p);
            if det.abs() < 1e-14 {
                continue;
            }
            let s = sub(ol, a);
            let u = dot(s, p) / det;
            let q = cross(s, e1);
            let v = dot(dl, q) / det;
            if u < 0.0 || v < 0.0 || u + v > 1.0 {
                continue;
            }
            let t = dot(e2, q) / det;
            if t > 0.0 && mieux.is_none_or(|x| t < x) {
                mieux = Some(t);
            }
        }
    }
    mieux
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_uv_suivent_leurs_coins_et_les_faces_sans_uv_le_disent() {
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let mat = crate::maillage::defaut();
        m.faces.push(crate::maillage::Face { s: vec![0, 1, 2], m: mat.clone(), uv: vec![[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]] });
        m.faces.push(crate::maillage::Face { s: vec![0, 2, 3], m: mat, uv: Vec::new() });
        let v = tampons_faces(&m).faces;
        assert_eq!(v.len(), 6 * FACE_F32);
        for k in 0..3 {
            let c = &v[k * FACE_F32..(k + 1) * FACE_F32];
            let attendu = m.faces[0].uv.iter().zip(&m.faces[0].s).find(|(_, i)| m.sommets[**i][0] as f32 == c[0] && m.sommets[**i][1] as f32 == c[1]).unwrap().0;
            assert_eq!([c[6], c[7]], *attendu);
        }
        assert_eq!(&v[3 * FACE_F32 + 6..3 * FACE_F32 + 8], &[SANS_UV, SANS_UV]);
    }

    #[test]
    fn un_cube_donne_douze_triangles_et_douze_bandes() {
        let c = Maillage::cube(2.0);
        assert_eq!(faces(&c).len(), 36 * FACE_F32);
        let (v, i) = bandes(&c);
        assert_eq!((v.len(), i.len()), (12 * 4 * BANDE_F32, 12 * 6));
        // Chaque arête d'un cube borde deux faces : n2 n'est jamais nulle.
        assert!(v.chunks(BANDE_F32).all(|b| b[10..13].iter().any(|c| *c != 0.0)));
    }

    #[test]
    fn un_rayon_touche_la_face_avant_d_un_objet_deplace() {
        let mut o = Objet::neuf("c", Maillage::cube(2.0));
        o.position = [10.0, 0.0, 0.0];
        o.echelle = [1.0, 1.0, 3.0];
        let t = toucher(&o, &o.maillage, [10.0, 0.0, 10.0], [0.0, 0.0, -1.0]).unwrap();
        assert!((t - 7.0).abs() < 1e-9, "{t}");
        assert!(toucher(&o, &o.maillage, [0.0, 0.0, 10.0], [0.0, 0.0, -1.0]).is_none());
    }

    #[test]
    fn la_rotation_est_prise_en_compte() {
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let mut o = Objet::neuf("c", Maillage::boite(10.0, 1.0, 1.0));
        o.rotation = [0.0, s, 0.0, s]; // quart de tour autour de Y : la longueur passe en Z
        assert!(toucher(&o, &o.maillage, [0.0, 10.0, 4.0], [0.0, -1.0, 0.0]).is_some());
        assert!(toucher(&o, &o.maillage, [4.0, 10.0, 0.0], [0.0, -1.0, 0.0]).is_none());
    }

    #[test]
    fn la_matrice_des_normales_compense_l_echelle() {
        let o = {
            let mut o = Objet::neuf("c", Maillage::cube(1.0));
            o.echelle = [2.0, 1.0, 1.0];
            o
        };
        let n = mat_normale(&o.matrice());
        assert!((n[0] - 0.5).abs() < 1e-6 && (n[4] - 1.0).abs() < 1e-6);
    }
}
