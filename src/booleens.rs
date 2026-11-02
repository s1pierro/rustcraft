//! Opérations booléennes entre maillages fermés, par `boolmesh` (Rust pur, à la place de
//! Manifold dans GDA). Le matériau et les UV de chaque face traversent l'opération : ils
//! suivent chaque coin de triangle, et les faces coupées sont regroupées ensuite.

use crate::maillage::{Face, Maillage, Mat};
use boolmesh::prelude::*;
use std::collections::{HashMap, HashSet};
use std::ops::{Add, Mul};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Union,
    Difference,
    Intersection,
}

/// Ce que porte un coin : ses UV, et le numéro de sa face d'origine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Coin {
    u: f64,
    v: f64,
    face: f64,
}

impl Add for Coin {
    type Output = Coin;
    fn add(self, o: Coin) -> Coin {
        Coin { u: self.u + o.u, v: self.v + o.v, face: self.face + o.face }
    }
}

impl Mul<f64> for Coin {
    type Output = Coin;
    fn mul(self, k: f64) -> Coin {
        Coin { u: self.u * k, v: self.v * k, face: self.face * k }
    }
}

/// Face d'origine : son matériau, et si elle a des UV.
type Origine = (Mat, bool);

fn vers_manifold(m: &Maillage, origines: &mut Vec<Origine>) -> Result<Manifold<Coin>, String> {
    const NON_FERME: &str = "maillage non fermé";
    if m.faces.is_empty() {
        return Err(NON_FERME.into());
    }
    let mut idx = Vec::with_capacity(m.faces.len() * 6);
    let mut coins = Vec::with_capacity(m.faces.len() * 6);
    let mut pos = Vec::with_capacity(m.sommets.len() * 3);
    let mut local: HashMap<usize, usize> = HashMap::with_capacity(m.sommets.len());
    for f in &m.faces {
        let avec_uv = f.uv.len() == f.s.len();
        let id = origines.len() as f64;
        origines.push((f.m.clone(), avec_uv));
        for t in m.trianguler(&f.s) {
            for i in t {
                let c = if avec_uv { f.s.iter().position(|x| *x == i).map_or([0.0; 2], |k| f.uv[k]) } else { [0.0; 2] };
                coins.push(Coin { u: c[0] as f64, v: c[1] as f64, face: id });
                idx.push(*local.entry(i).or_insert_with(|| {
                    pos.extend_from_slice(&m.sommets[i]);
                    pos.len() / 3 - 1
                }));
            }
        }
    }
    Manifold::new_with_values(&pos, &idx, &coins).map_err(|_| NON_FERME.to_string())
}

fn vers_maillage(r: &Manifold<Coin>, origines: &[Origine]) -> Maillage {
    let mut m = Maillage::default();
    m.sommets = r.ps.iter().map(|p| [p.x, p.y, p.z]).collect();
    // Triangles de chaque face d'origine, dans l'ordre d'apparition
    let mut ordre: Vec<usize> = Vec::new();
    let mut groupes: HashMap<usize, Vec<[usize; 3]>> = HashMap::new();
    let mut uv: HashMap<(usize, usize), [f32; 2]> = HashMap::new();
    for t in 0..r.hs.len() / 3 {
        let h = &r.hs[3 * t..3 * t + 3];
        if h.iter().any(|x| x.tail >= m.sommets.len()) || r.vals.len() < 3 * t + 3 {
            continue;
        }
        let id = (r.vals[3 * t].face + 0.5).floor().max(0.0) as usize;
        groupes.entry(id).or_insert_with(|| {
            ordre.push(id);
            Vec::new()
        });
        groupes.get_mut(&id).unwrap().push([h[0].tail, h[1].tail, h[2].tail]);
        for k in 0..3 {
            let c = &r.vals[3 * t + k];
            uv.insert((id, h[k].tail), [c.u as f32, c.v as f32]);
        }
    }
    for id in ordre {
        let tris = &groupes[&id];
        let (mat, avec_uv) = origines.get(id).cloned().unwrap_or_else(|| (crate::maillage::defaut(), false));
        let coins = |s: &[usize]| if avec_uv { s.iter().map(|i| uv[&(id, *i)]).collect() } else { Vec::new() };
        match (tris.len() > 1).then(|| contour(&m.sommets, tris)).flatten() {
            Some(s) => {
                let c = coins(&s);
                m.faces.push(Face { s, m: mat, uv: c });
            }
            None => {
                for t in tris {
                    let c = coins(t);
                    m.faces.push(Face { s: t.to_vec(), m: mat.clone(), uv: c });
                }
            }
        }
    }
    m
}

/// Contour d'un groupe de triangles coplanaires : une boucle unique et convexe, sinon rien.
/// Un quadrilatère coupé redevient ainsi un quadrilatère.
fn contour(sommets: &[[f64; 3]], tris: &[[usize; 3]]) -> Option<Vec<usize>> {
    let aretes: HashSet<(usize, usize)> = tris.iter().flat_map(|&[a, b, c]| [(a, b), (b, c), (c, a)]).collect();
    let mut suivant: HashMap<usize, usize> = HashMap::new();
    let mut depart = None;
    for &[a, b, c] in tris {
        for (i, j) in [(a, b), (b, c), (c, a)] {
            if aretes.contains(&(j, i)) {
                continue; // arête intérieure
            }
            if suivant.insert(i, j).is_some() {
                return None; // contour qui se touche lui-même
            }
            depart.get_or_insert(i);
        }
    }
    let depart = depart?;
    let mut boucle = Vec::with_capacity(suivant.len());
    let mut i = depart;
    loop {
        boucle.push(i);
        i = *suivant.get(&i)?;
        if i == depart {
            break;
        }
        if boucle.len() > suivant.len() {
            return None;
        }
    }
    if boucle.len() != suivant.len() {
        return None; // plusieurs boucles : un trou
    }
    // Convexité : tous les virages du même côté que la normale, alignements tolérés
    let p: Vec<[f64; 3]> = boucle.iter().map(|i| sommets[*i]).collect();
    let l = p.len();
    let mut n = [0.0; 3];
    for k in 0..l {
        let (a, b) = (p[k], p[(k + 1) % l]);
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let virage = |k: usize| {
        let (a, b, c) = (p[k], p[(k + 1) % l], p[(k + 2) % l]);
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let w = [c[0] - b[0], c[1] - b[1], c[2] - b[2]];
        [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]]
    };
    for k in 0..l {
        let x = virage(k);
        if x[0] * n[0] + x[1] * n[1] + x[2] * n[2] < -1e-9 {
            return None;
        }
    }
    // Commencer sur un vrai coin : l'éventail ne produit pas de triangle plat au départ
    let coin = (0..l).find(|k| {
        let x = virage((k + l - 1) % l);
        (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() > 1e-9
    });
    boucle.rotate_left(coin.unwrap_or(0));
    Some(boucle)
}

fn calculer(a: &Maillage, b: &Maillage, op: Op) -> Result<Maillage, String> {
    let mut origines = Vec::new();
    let (x, y) = (vers_manifold(a, &mut origines)?, vers_manifold(b, &mut origines)?);
    let t = match op {
        Op::Union => OpType::Add,
        Op::Difference => OpType::Subtract,
        Op::Intersection => OpType::Intersect,
    };
    match compute_boolean(&x, &y, t) {
        Ok(r) => Ok(vers_maillage(&r, &origines)),
        // boolmesh le dit ainsi quand rien ne reste : un résultat, pas une panne
        Err(e) if e.contains("empty") => Ok(Maillage::default()),
        Err(e) => Err(e),
    }
}

/// `a` et `b`, deux maillages fermés dans le même repère. Une panique de la bibliothèque
/// devient une erreur : une forme difficile ne doit pas fermer l'application.
pub fn operation(a: &Maillage, b: &Maillage, op: Op) -> Result<Maillage, String> {
    std::panic::catch_unwind(|| calculer(a, b, op)).unwrap_or_else(|_| Err("calcul impossible sur cette forme".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visserie::volume;

    fn cube(d: f64) -> Maillage {
        let mut c = Maillage::cube(1.0);
        c.sommets.iter_mut().for_each(|p| p[0] += d);
        c
    }

    #[test]
    fn les_trois_operations_donnent_le_bon_volume() {
        let (a, b) = (cube(0.0), cube(0.5));
        let v = |o| volume(&operation(&a, &b, o).unwrap());
        assert!((v(Op::Union) - 1.5).abs() < 1e-9);
        assert!((v(Op::Difference) - 0.5).abs() < 1e-9);
        assert!((v(Op::Intersection) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn une_face_coupee_redevient_un_polygone() {
        let r = operation(&cube(0.0), &cube(0.5), Op::Difference).unwrap();
        assert!(r.faces.iter().all(|f| f.s.len() == 4), "{:?}", r.faces.iter().map(|f| f.s.len()).collect::<Vec<_>>());
        assert_eq!(r.faces.len(), 6);
    }

    #[test]
    fn rien_de_commun_est_un_resultat_vide_pas_une_erreur() {
        let r = operation(&cube(0.0), &cube(5.0), Op::Intersection).unwrap();
        assert!(r.faces.is_empty());
    }

    #[test]
    fn un_maillage_ouvert_est_refuse_sans_panique() {
        let mut o = cube(0.0);
        o.faces.pop();
        assert_eq!(operation(&o, &cube(0.5), Op::Union).unwrap_err(), "maillage non fermé");
        assert!(operation(&Maillage::default(), &cube(0.5), Op::Union).is_err());
    }

    #[test]
    fn matieres_et_uv_traversent_l_operation() {
        let mut a = cube(0.0);
        let bois: Mat = "bois".into();
        for f in &mut a.faces {
            f.m = bois.clone();
            let c: Vec<[f32; 2]> = f.s.iter().map(|i| [a.sommets[*i][0] as f32 + 0.5, a.sommets[*i][1] as f32 + 0.5]).collect();
            f.uv = c;
        }
        let r = operation(&a, &cube(0.5), Op::Difference).unwrap();
        let (mut de_a, mut de_b) = (0, 0);
        for f in &r.faces {
            if f.m == bois {
                de_a += 1;
                assert_eq!(f.uv.len(), f.s.len());
                for (i, c) in f.s.iter().zip(&f.uv) {
                    assert!((c[0] as f64 - r.sommets[*i][0] - 0.5).abs() < 1e-5, "UV interpolées");
                }
            } else {
                de_b += 1;
                assert!(f.uv.is_empty());
            }
        }
        assert!(de_a > 0 && de_b > 0);
    }
}
