//! Rejoue `tests/marionnette.json`, produit par la vraie saisie directe de GDA
//! (`moteur/marionnette.js`) avec `tools/reference-marionnette.js` : mêmes chaînes
//! jusqu'à l'ancre, mêmes poses après chaque tirage.

use rustcraft::armature::Armature;
use rustcraft::json::{self, J};

const JEU: f64 = 1e-7;

fn nb<const N: usize>(j: &J) -> [f64; N] {
    j.nombres::<N>().unwrap()
}

#[test]
fn la_saisie_directe_suit_celle_de_gda() {
    let texte = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/marionnette.json")).unwrap();
    let r = json::lire(&texte).unwrap();
    for (n, c) in r.get("cas").unwrap().liste().unwrap().iter().enumerate() {
        let mut a = Armature::depuis_donnees(c.get("donnees").unwrap());
        assert_eq!(a.ancre(), c.get("ancre").and_then(J::txt).map(str::to_string), "cas {n}");
        for (k, s) in c.get("saisies").unwrap().liste().unwrap().iter().enumerate() {
            let ou = format!("cas {n}, saisie {k}");
            let os = s.get("os").and_then(J::txt).unwrap();
            let saisie = a.saisir(os, nb::<3>(s.get("point").unwrap()));
            let chaine: Vec<(String, bool)> = s.get("chaine").unwrap().liste().unwrap().iter().map(|x| {
                let x = x.liste().unwrap();
                (x[0].txt().unwrap().to_string(), x[1].bool().unwrap())
            }).collect();
            assert_eq!(saisie.chaine.iter().map(|p| (p.j.clone(), p.inverse)).collect::<Vec<_>>(), chaine, "{ou}");
            let cibles = s.get("cibles").unwrap().liste().unwrap();
            for (i, (cible, pose)) in cibles.iter().zip(s.get("poses").unwrap().liste().unwrap()).enumerate() {
                a.tirer(&saisie, nb::<3>(cible), 8);
                let m = a.matrices_pose();
                let J::Objet(l) = pose else { panic!() };
                for (id, v) in l {
                    let v = v.liste().unwrap();
                    let (p, q) = (nb::<3>(&v[0]), nb::<4>(&v[1]));
                    let (mp, mq) = m[id];
                    let s = if q.iter().zip(mq).map(|(x, y)| x * y).sum::<f64>() < 0.0 { -1.0 } else { 1.0 };
                    let ecart = (0..3).map(|j| (p[j] - mp[j]).abs()).chain((0..4).map(|j| (q[j] - s * mq[j]).abs())).fold(0.0, f64::max);
                    assert!(ecart < JEU, "{ou}, tirage {i}, os {id} : écart {ecart:e}");
                }
            }
        }
    }
}
