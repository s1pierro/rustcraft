//! Compare `src/peau.rs` à `tests/peau.json`, produit par le vrai `moteur/peau.js` de GDA avec
//! `tools/reference-peau.js` : mêmes os par sommet, mêmes poids, mêmes déformations.

use rustcraft::armature::Armature;
use rustcraft::json::{self, J};
use rustcraft::maillage::Maillage;
use rustcraft::peau::{calculer, INFLUENCES};
use std::collections::BTreeMap;

fn nombres(j: &J) -> Vec<f64> {
    j.liste().unwrap().iter().map(|x| x.num().unwrap()).collect()
}

#[test]
fn memes_poids_et_deformations_que_gda() {
    let t = std::fs::read_to_string("tests/peau.json").unwrap();
    let cas = json::lire(&t).unwrap();
    for (c, x) in cas.liste().unwrap().iter().enumerate() {
        let m = Maillage::depuis_donnees(x.get("mesh").unwrap()).unwrap();
        let a = Armature::depuis_donnees(x.get("donnees").unwrap());
        let groupe: Vec<String> = match x.get("groupe").and_then(J::liste) {
            Some(l) => l.iter().map(|i| i.txt().unwrap().to_string()).collect(),
            None => a.os.iter().map(|b| b.id.clone()).collect(),
        };
        let mut repos = a.clone();
        repos.pose.clear();
        let p = calculer(&m.sommets, &m, &repos, &groupe);
        let ids: Vec<&str> = x.get("ids").unwrap().liste().unwrap().iter().map(|i| i.txt().unwrap()).collect();
        assert_eq!(p.ids, ids, "cas {c}");
        let (indices, poids) = (nombres(x.get("indices").unwrap()), nombres(x.get("poids").unwrap()));
        assert_eq!(p.poids.len(), poids.len(), "cas {c}");
        // Sommet par sommet : un poids égal à l'autre peut changer de case
        let (mut pire, n) = (0f64, m.sommets.len());
        let mut coupes = 0;
        for i in 0..n {
            let table = |ind: &dyn Fn(usize) -> usize, po: &dyn Fn(usize) -> f64| {
                let mut r: BTreeMap<usize, f64> = BTreeMap::new();
                for k in 0..INFLUENCES {
                    if po(i * INFLUENCES + k) != 0.0 {
                        r.insert(ind(i * INFLUENCES + k), po(i * INFLUENCES + k));
                    }
                }
                r
            };
            let u = table(&|k| p.indices[k] as usize, &|k| p.poids[k] as f64);
            let v = table(&|k| indices[k] as usize, &|k| poids[k]);
            if u.keys().ne(v.keys()) {
                coupes += 1;
            }
            for k in u.keys().chain(v.keys()) {
                pire = pire.max((u.get(k).unwrap_or(&0.0) - v.get(k).unwrap_or(&0.0)).abs());
            }
        }
        assert!(pire < 1e-5 && coupes == 0, "cas {c} : écart de poids {pire}, {coupes} sommets à l'autre liste d'os");
        let v = p.deformer(&m.sommets, &a);
        let d = nombres(x.get("deforme").unwrap());
        assert!(!a.pose.is_empty());
        for (i, s) in v.iter().enumerate() {
            for k in 0..3 {
                assert!((s[k] - d[i * 3 + k]).abs() < 1e-5, "cas {c}, sommet {i} : {s:?}");
            }
        }
    }
}
