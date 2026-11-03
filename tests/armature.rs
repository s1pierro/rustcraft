//! Rejoue `tests/armature.json`, produit par le vrai `moteur/armature.js` de GDA avec
//! `tools/reference-armature.js`, et exige les mêmes os, les mêmes bouts, les mêmes
//! soudures et la même enveloppe.

use rustcraft::armature::{Armature, Bout};
use rustcraft::json::{self, J};

const JEU: f64 = 1e-9;

fn nb<const N: usize>(j: &J) -> [f64; N] {
    j.nombres::<N>().unwrap()
}

fn pres(a: &[f64], b: &[f64], quoi: &str) {
    assert_eq!(a.len(), b.len(), "{quoi}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < JEU, "{quoi} : {a:?} / {b:?}");
    }
}

/// Un quaternion et son opposé sont la même rotation.
fn memes_q(a: &[f64; 4], b: &[f64; 4], quoi: &str) {
    let s = if a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>() < 0.0 { -1.0 } else { 1.0 };
    pres(&a.map(|x| x * s), b, quoi);
}

fn compare(a: &Armature, r: &J, ou: &str) {
    let d = r.get("donnees").unwrap().liste().unwrap();
    assert_eq!(a.os.len(), d.len(), "{ou}");
    for (b, x) in a.os.iter().zip(d) {
        let x = x.liste().unwrap();
        assert_eq!(b.id, x[0].txt().unwrap(), "{ou}");
        assert_eq!(b.nom, x[1].txt().unwrap(), "{ou}");
        assert_eq!(b.parent.as_deref(), x[2].txt(), "{ou}");
        pres(&b.position, &nb::<3>(&x[3]), &format!("{ou} position {}", b.id));
        memes_q(&b.rotation, &nb::<4>(&x[4]), &format!("{ou} rotation {}", b.id));
        pres(&[b.longueur], &[x[5].num().unwrap()], &format!("{ou} longueur {}", b.id));
    }
    let ordre: Vec<&str> = r.get("ordre").unwrap().liste().unwrap().iter().map(|x| x.txt().unwrap()).collect();
    assert_eq!(a.ordre().iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ordre, "{ou}");
    let s = a.segments();
    for b in &a.os {
        let x = r.get("segments").unwrap().get(&b.id).unwrap().liste().unwrap();
        pres(&s[&b.id].tete, &nb::<3>(&x[0]), &format!("{ou} tête {}", b.id));
        pres(&s[&b.id].queue, &nb::<3>(&x[1]), &format!("{ou} queue {}", b.id));
    }
    let sd = a.soudures();
    for b in &a.os {
        for (cle, bout) in [(format!("{}:t", b.id), Bout::tete(&b.id)), (format!("{}:q", b.id), Bout::queue(&b.id))] {
            let attendu: Vec<&str> = r.get("soudures").unwrap().get(&cle).unwrap().liste().unwrap().iter().map(|x| x.txt().unwrap()).collect();
            let mut vu: Vec<String> = sd[&bout].iter().map(|x| format!("{}:{}", x.os, if x.queue { "q" } else { "t" })).collect();
            vu.sort();
            assert_eq!(vu, attendu, "{ou} soudure {cle}");
        }
    }
    let e = a.enveloppe();
    let v = r.get("enveloppe").unwrap().get("v").unwrap().liste().unwrap();
    assert_eq!(e.sommets.len(), v.len(), "{ou}");
    for (p, q) in e.sommets.iter().zip(v) {
        pres(p, &nb::<3>(q), &format!("{ou} enveloppe"));
    }
    let f = r.get("enveloppe").unwrap().get("f").unwrap().liste().unwrap();
    assert_eq!(e.faces.len(), f.len(), "{ou}");
    for (p, q) in e.faces.iter().zip(f) {
        assert_eq!(p.s, q.liste().unwrap().iter().map(|x| x.num().unwrap() as usize).collect::<Vec<_>>(), "{ou}");
    }
}

#[test]
fn memes_resultats_que_gda() {
    let t = std::fs::read_to_string("tests/armature.json").unwrap();
    let cas = json::lire(&t).unwrap();
    let mut n = 0;
    for (c, x) in cas.liste().unwrap().iter().enumerate() {
        let mut a = if x.get("depart").unwrap().txt() == Some("un") { Armature::nouvelle() } else { Armature::humanoide() };
        let r = x.get("resultats").unwrap().liste().unwrap();
        compare(&a, &r[0], &format!("cas {c}, départ"));
        for (k, op) in x.get("ops").unwrap().liste().unwrap().iter().enumerate() {
            let op = op.liste().unwrap();
            match op[0].txt().unwrap() {
                "extruder" => drop(a.extruder(op[1].txt().unwrap())),
                "retirer" => a.retirer(op[1].txt().unwrap()),
                "parent" => assert_eq!(a.changer_parent(op[1].txt().unwrap(), op[2].txt()), op[3].bool().unwrap(), "cas {c}, op {k}"),
                "segment" => drop(a.ajouter_segment("neuf", op[1].txt(), nb::<3>(&op[2]), nb::<3>(&op[3])).unwrap()),
                _ => {
                    let (id, queue, d) = (op[1].txt().unwrap(), op[2].bool().unwrap(), nb::<3>(&op[3]));
                    let mut s = a.segments();
                    let x = s.get_mut(id).unwrap();
                    let b = if queue { &mut x.queue } else { &mut x.tete };
                    *b = [b[0] + d[0], b[1] + d[1], b[2] + d[2]];
                    a.appliquer_segments(&s);
                }
            }
            compare(&a, &r[k + 1], &format!("cas {c}, op {k} {:?}", op[0].txt()));
            n += 1;
        }
    }
    assert!(n > 100, "{n} opérations");
}

#[test]
fn memes_poses_que_gda() {
    let t = std::fs::read_to_string("tests/armature-poses.json").unwrap();
    let cas = json::lire(&t).unwrap();
    for (c, x) in cas.liste().unwrap().iter().enumerate() {
        let d = x.get("donnees").unwrap();
        let a = Armature::depuis_donnees(d);
        assert!(!a.pose.is_empty());
        let s = a.segments_pose();
        for b in &a.os {
            let r = x.get("segments").unwrap().get(&b.id).unwrap().liste().unwrap();
            pres(&s[&b.id].tete, &nb::<3>(&r[0]), &format!("cas {c}, tête de {}", b.id));
            pres(&s[&b.id].queue, &nb::<3>(&r[1]), &format!("cas {c}, queue de {}", b.id));
            memes_q(&s[&b.id].q, &nb::<4>(&r[2]), &format!("cas {c}, rotation de {}", b.id));
        }
        let v = x.get("enveloppe").unwrap().liste().unwrap();
        let e = a.enveloppe_pose();
        assert_eq!(e.sommets.len(), v.len());
        for (p, q) in e.sommets.iter().zip(v) {
            pres(p, &nb::<3>(q), &format!("cas {c}, enveloppe"));
        }
        let relue = Armature::depuis_donnees(&a.vers_donnees());
        assert_eq!(relue.pose, a.pose, "cas {c}");
    }
}
