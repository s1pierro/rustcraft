//! Rejoue `tests/reference.json`, écrit par le moteur de GDA (`tools/reference.js`),
//! et exige les mêmes maillages et les mêmes valeurs de retour.

use rustcraft::json::{self, J};
use rustcraft::maillage::*;
use std::sync::Arc;

const SIGN: &str = "GDA";

fn n(j: &J) -> f64 {
    j.num().unwrap()
}
fn ent(j: &J) -> usize {
    n(j) as usize
}
fn liste(j: &J) -> Vec<usize> {
    j.liste().unwrap().iter().map(ent).collect()
}
fn arete(j: &J) -> Arete {
    [ent(&j.liste().unwrap()[0]), ent(&j.liste().unwrap()[1])]
}
fn aretes(j: &J) -> Vec<Arete> {
    j.liste().unwrap().iter().map(arete).collect()
}
fn num(x: usize) -> J {
    J::Num(x as f64)
}
fn nums(l: &[usize]) -> J {
    J::Liste(l.iter().map(|x| num(*x)).collect())
}
fn ar(l: &[Arete]) -> J {
    J::Liste(l.iter().map(|a| nums(a)).collect())
}
fn signes(l: &[isize]) -> J {
    J::Liste(l.iter().map(|x| J::Num(*x as f64)).collect())
}
fn p3(l: &[f64]) -> J {
    J::Liste(l.iter().map(|x| J::Num(*x)).collect())
}
fn hex(b: &[u8]) -> J {
    J::Txt(b.iter().map(|x| format!("{x:02x}")).collect())
}
fn obj(l: Vec<(&str, J)>) -> J {
    J::Objet(l.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
fn etat(m: &Maillage) -> J {
    obj(vec![
        ("v", J::Liste(m.sommets.iter().map(|p| p3(p)).collect())),
        ("f", J::Liste(m.faces.iter().map(|f| { let mut l = vec![J::Txt(f.m.to_string())]; l.extend(f.s.iter().map(|i| num(*i))); J::Liste(l) }).collect())),
        ("l", ar(&m.fils)),
    ])
}

/// Égalité, à l'arrondi près pour les nombres ; les clés de `b` peuvent manquer de `a`.
fn pareil(a: &J, b: &J, chemin: &str) -> Result<(), String> {
    match (a, b) {
        (J::Num(x), J::Num(y)) => {
            let t = 1e-9 * x.abs().max(y.abs()).max(1.0);
            if (x - y).abs() <= t { Ok(()) } else { Err(format!("{chemin} : {x} ≠ {y}")) }
        }
        (J::Liste(x), J::Liste(y)) => {
            if x.len() != y.len() {
                return Err(format!("{chemin} : longueur {} ≠ {}", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().try_for_each(|(i, (p, q))| pareil(p, q, &format!("{chemin}/{i}")))
        }
        (J::Objet(x), J::Objet(_)) => x.iter().try_for_each(|(k, v)| match b.get(k) {
            Some(w) => pareil(v, w, &format!("{chemin}.{k}")),
            None => Err(format!("{chemin}.{k} absent")),
        }),
        (x, y) if x == y => Ok(()),
        (x, y) => Err(format!("{chemin} : {x:?} ≠ {y:?}")),
    }
}

fn cible(s: &str) -> Cible {
    match s {
        "premier" => Cible::Premier,
        "dernier" => Cible::Dernier,
        _ => Cible::Centre,
    }
}

fn base(b: &[J]) -> Maillage {
    let f = |k: usize| n(&b[k]);
    match b[0].txt().unwrap() {
        "boite" => Maillage::boite(f(1), f(2), f(3)),
        "cone" => Maillage::cone(f(1), f(2), f(3), ent(&b[4]), b[5].txt() == Some("eventail")),
        "cylindre" => Maillage::cylindre(f(1), f(2), ent(&b[3]), b[4].txt() == Some("eventail")),
        "sphereUV" => Maillage::sphere_uv(f(1), ent(&b[2]), ent(&b[3])),
        "icosphere" => Maillage::icosphere(f(1), ent(&b[2])),
        "obj" => Maillage::depuis_obj(b[1].txt().unwrap(), "defaut"),
        x => panic!("base {x}"),
    }
}

fn mats(j: &J) -> Vec<(String, MatMtl)> {
    let J::Objet(o) = j else { panic!() };
    o.iter()
        .map(|(k, v)| {
            let t = |c: &str| v.get(c).and_then(J::txt).map(String::from);
            let m = match v {
                J::Txt(c) => MatMtl { couleur: Some(c.clone()), ..MatMtl::default() },
                _ => MatMtl { couleur: t("couleur"), emission: t("emission"), opacite: v.get("opacite").and_then(J::num), ..MatMtl::default() },
            };
            (k.clone(), m)
        })
        .collect()
}

fn erreur(e: String) -> J {
    obj(vec![("erreur", J::Txt(e))])
}

fn op(m: &mut Maillage, nom: &str, a: &[J]) -> J {
    match nom {
        "subdiviser" => {
            let r = m.subdiviser(&aretes(&a[0]), ent(&a[1]));
            let mut l: Vec<_> = r.into_iter().collect();
            l.sort();
            J::Liste(l.iter().map(|(k, v)| J::Liste(vec![nums(&[k.0, k.1]), nums(v)])).collect())
        }
        "couper" => match m.anneau(ent(&a[0]), ent(&a[1])) {
            None => J::Nul,
            Some(an) => {
                let d = m.couper_boucle(&an, ent(&a[2]), n(&a[3]), a[4].bool().unwrap(), a[5].bool().unwrap());
                obj(vec![("rails", ar(&an.rails)), ("quads", nums(&an.quads)), ("depart", num(an.depart)), ("ferme", J::Bool(an.ferme)), ("aretes", ar(&d))])
            }
        },
        "reunir" => {
            let g: Vec<Vec<usize>> = a[0].liste().unwrap().iter().map(liste).collect();
            signes(&m.reunir(&g))
        }
        "fusionner" => signes(&m.fusionner(&liste(&a[0]), cible(a[1].txt().unwrap()))),
        "proches" => {
            let (t, f) = m.fusionner_proches(None, n(&a[0]), &|_| 0);
            J::Liste(vec![signes(&t), num(f.groupes), num(f.retires)])
        }
        "supprimer" => signes(&m.supprimer_sommets(&liste(&a[0]))),
        "souder" => {
            m.souder(n(&a[0]));
            J::Nul
        }
        "revolution" => {
            let o = &a[2];
            let tab = |k: &str, d: P3| o.get(k).and_then(J::nombres::<3>).unwrap_or(d);
            let r = Revolution {
                axe: tab("axe", [0.0, 1.0, 0.0]),
                centre: tab("centre", [0.0; 3]),
                angle: n(o.get("angle").unwrap()),
                segments: ent(o.get("segments").unwrap()),
                fermer: o.get("fermer").and_then(J::bool).unwrap_or(true),
                materiau: Arc::from(o.get("materiau").and_then(J::txt).unwrap_or("defaut")),
            };
            let r = m.revolution(&liste(&a[0]), &aretes(&a[1]), &r);
            let c = r.copies.iter().map(|(i, l)| J::Liste(vec![num(*i), nums(l)])).collect();
            obj(vec![("faces", nums(&r.faces)), ("fils", ar(&r.fils)), ("copies", J::Liste(c))])
        }
        "contour" => match m.contour_faces(&liste(&a[0])) {
            Ok(c) => obj(vec![("s", nums(&c.s)), ("plane", J::Bool(c.plane))]),
            Err(e) => erreur(e),
        },
        "dissoudre" => match m.dissolution(&aretes(&a[0])) {
            Ok(g) => J::Liste(g.iter().map(|g| obj(vec![("faces", nums(&g.faces)), ("s", nums(&g.s)), ("plane", J::Bool(g.plane))])).collect()),
            Err(e) => erreur(e),
        },
        "cheminS" => m.chemin_sommets(ent(&a[0]), ent(&a[1])).map_or(J::Nul, |l| nums(&l)),
        "cheminA" => m.chemin_aretes(arete(&a[0]), arete(&a[1])).map_or(J::Nul, |l| ar(&l)),
        "cheminF" => m.chemin_faces(ent(&a[0]), ent(&a[1])).map_or(J::Nul, |l| nums(&l)),
        "connexe" => {
            let (s, f) = m.connexe(&liste(&a[0]));
            J::Liste(vec![nums(&s), nums(&f)])
        }
        "aretes" => ar(&m.aretes()),
        "ilots" => J::Liste(m.ilots().iter().map(|l| nums(l)).collect()),
        "boite" => m.boite_englobante().map_or(J::Nul, |b| obj(vec![("min", p3(&b.min)), ("max", p3(&b.max))])),
        "trianguler" => J::Liste(m.faces.iter().map(|f| J::Liste(m.trianguler(&f.s).iter().map(|t| nums(t)).collect())).collect()),
        "triangles" => J::Liste(m.triangles().iter().map(|(k, l)| J::Liste(vec![J::Txt(k.to_string()), nums(l)])).collect()),
        "normales" => J::Liste(m.faces.iter().map(|f| p3(&m.normale(&f.s))).collect()),
        "parMateriau" => J::Liste(m.par_materiau().iter().map(|(k, x)| J::Liste(vec![J::Txt(k.to_string()), etat(x)])).collect()),
        "obj" => J::Txt(m.vers_obj_signe(a[0].txt().unwrap(), Some("materiaux.mtl"), SIGN)),
        "stl" => hex(&m.vers_stl_signe(a[0].txt().unwrap(), SIGN)),
        "glb" => hex(&m.vers_glb_signe(a[0].txt().unwrap(), &mats(&a[1]), SIGN)),
        "donnees" => m.vers_donnees(),
        "relireObj" => etat(&Maillage::depuis_obj(&m.vers_obj("x", None), "defaut")),
        "relireStl" => etat(&Maillage::depuis_stl(&m.vers_stl("x"), "defaut", true)),
        "relireGlb" => etat(&Maillage::depuis_gltf(&m.vers_glb("x", &[])).unwrap()),
        x => panic!("opération {x}"),
    }
}

/// Plusieurs découpes d'un polygone régulier sont aussi bonnes (angles égaux, au
/// dernier bit près) : on compare le nombre de triangles et l'aire orientée.
fn bilan_tris(m: &Maillage, j: &J) -> J {
    let aire = |t: &J| {
        let l = liste(t);
        let (a, b, c) = (m.sommets[l[0]], m.sommets[l[1]], m.sommets[l[2]]);
        let (u, v) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [c[0] - a[0], c[1] - a[1], c[2] - a[2]]);
        [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]]
    };
    J::Liste(j.liste().unwrap().iter().map(|f| {
        let l = f.liste().unwrap();
        let mut t = [0.0; 3];
        for x in l {
            let a = aire(x);
            (0..3).for_each(|k| t[k] += a[k]);
        }
        J::Liste(vec![num(l.len()), p3(&t)])
    }).collect())
}

#[test]
fn memes_resultats_que_gda() {
    let texte = std::fs::read_to_string("tests/reference.json").unwrap();
    let r = json::lire(&texte).unwrap();
    let (mut ops, mut echecs) = (0, Vec::new());
    for (k, s) in r.get("scenarios").unwrap().liste().unwrap().iter().enumerate() {
        let mut m = base(s.get("base").unwrap().liste().unwrap());
        for (e, x) in s.get("etapes").unwrap().liste().unwrap().iter().enumerate() {
            let nom = x.get("op").unwrap().txt().unwrap();
            let got = op(&mut m, nom, x.get("args").unwrap().liste().unwrap());
            ops += 1;
            let lieu = format!("suite {k} étape {e} {nom}");
            let (a, b) = if nom == "trianguler" { (bilan_tris(&m, &got), bilan_tris(&m, x.get("r").unwrap())) } else { (got, x.get("r").unwrap().clone()) };
            let bon = pareil(&a, &b, "résultat").and_then(|_| pareil(&etat(&m), x.get("m").unwrap(), "maillage"));
            if let Err(e) = bon {
                echecs.push(format!("{lieu} : {e}"));
                break;
            }
        }
    }
    println!("{ops} opérations comparées");
    assert!(echecs.is_empty(), "{} écarts :\n{}", echecs.len(), echecs[..echecs.len().min(12)].join("\n"));
}

#[test]
fn mtl_comme_gda() {
    let texte = std::fs::read_to_string("tests/reference.json").unwrap();
    let r = json::lire(&texte).unwrap();
    let t = r.get("mtl").unwrap();
    let (lu, textures) = Maillage::depuis_mtl(t.get("texte").unwrap().txt().unwrap());
    let attendu = t.get("lu").unwrap();
    assert_eq!(textures, ent(attendu.get("textures").unwrap()));
    let J::Objet(a) = attendu.get("materiaux").unwrap() else { panic!() };
    assert_eq!(a.len(), lu.len());
    for ((n1, v), (n2, m)) in a.iter().zip(&lu) {
        assert_eq!(n1, n2);
        assert_eq!(v.get("couleur").and_then(J::txt).map(String::from), m.couleur, "{n1}");
        assert_eq!(v.get("emission").and_then(J::txt).map(String::from), m.emission, "{n1}");
        for (c, x) in [("brillance", m.brillance), ("opacite", m.opacite), ("refraction", m.refraction), ("illumination", m.illumination)] {
            assert_eq!(v.get(c).and_then(J::num), x, "{n1}.{c}");
        }
    }
    let un = MatMtl { couleur: Some("#804020".into()), brillance: Some(12.34567), illumination: Some(2.6), ..MatMtl::default() };
    let ecrit = Maillage::vers_mtl_signe(&[("un".into(), un), ("deux".into(), MatMtl::default())], "titre", SIGN);
    assert_eq!(ecrit, t.get("ecrit").unwrap().txt().unwrap());
}

#[test]
fn poses_comme_gda() {
    use rustcraft::edition::*;
    use rustcraft::projet::Objet;
    let texte = std::fs::read_to_string("tests/reference.json").unwrap();
    let r = json::lire(&texte).unwrap();
    let t3 = |j: &J| j.nombres::<3>().unwrap();
    let base = std::sync::Arc::new(Maillage::boite(1.0, 2.0, 3.0));
    let mut n = 0;
    for (k, c) in r.get("poses").unwrap().liste().unwrap().iter().enumerate() {
        let mut objets: Vec<Objet> = c.get("avant").unwrap().liste().unwrap().iter().map(|a| {
            let mut o = Objet::neuf("o", Maillage::default());
            o.maillage = base.clone();
            o.position = t3(a.get("p").unwrap());
            o.rotation = a.get("q").unwrap().nombres::<4>().unwrap();
            o.echelle = t3(a.get("e").unwrap());
            o.armature = (a.get("a").unwrap().bool() == Some(true)).then(|| J::Objet(vec![]));
            o
        }).collect();
        let pts: Vec<P3> = objets.iter().map(|o| o.position).collect();
        let mode = match c.get("pivot").unwrap().txt().unwrap() {
            "boite" => Pivot::Boite,
            "actif" => Pivot::Actif,
            "curseur" => Pivot::Curseur,
            "origine" => Pivot::Origine,
            _ => Pivot::Median,
        };
        pareil(&p3(&pivot(mode, &pts, t3(c.get("curseur").unwrap()))), c.get("attendu").unwrap(), &format!("pose {k} pivot")).unwrap();
        pareil(&p3(&centre(&pts)), c.get("centre").unwrap(), &format!("pose {k} centre")).unwrap();
        let t = c.get("t").unwrap();
        let tr = if let Some(d) = t.get("d") {
            Transfo::Translation(t3(d))
        } else if let Some(q) = t.get("q") {
            Transfo::Rotation { q: q.nombres::<4>().unwrap(), c: t3(t.get("c").unwrap()) }
        } else {
            Transfo::Echelle { s: t3(t.get("s").unwrap()), c: t3(t.get("c").unwrap()) }
        };
        let idx: Vec<usize> = (0..objets.len()).collect();
        let d = depart(&objets, &idx);
        poser(&mut objets, &idx, &d, tr);
        for (i, (a, o)) in c.get("apres").unwrap().liste().unwrap().iter().zip(&objets).enumerate() {
            let lieu = format!("pose {k} objet {i}");
            pareil(&p3(&o.position), a.get("p").unwrap(), &lieu).unwrap();
            pareil(&p3(&o.rotation), a.get("q").unwrap(), &lieu).unwrap();
            pareil(&p3(&o.echelle), a.get("e").unwrap(), &lieu).unwrap();
            match a.get("v").unwrap() {
                J::Nul => assert!(std::sync::Arc::ptr_eq(&o.maillage, &base), "{lieu}"),
                v => pareil(&J::Liste(o.maillage.sommets.iter().map(|p| p3(p)).collect()), v, &lieu).unwrap(),
            }
            n += 1;
        }
    }
    println!("{n} objets posés comme GDA");
    assert!(n > 0);
}

#[test]
fn euler_comme_gda() {
    use rustcraft::edition::euler;
    let texte = std::fs::read_to_string("tests/reference.json").unwrap();
    let r = json::lire(&texte).unwrap();
    for c in r.get("eulers").unwrap().liste().unwrap() {
        let d = euler(c.get("q").unwrap().nombres::<4>().unwrap());
        pareil(&p3(&d), c.get("deg").unwrap(), "euler").unwrap();
    }
}

#[test]
fn geometrie_comme_gda() {
    use rustcraft::geometrie::*;
    use rustcraft::projet::{Objet, Projet};
    let texte = std::fs::read_to_string("tests/reference.json").unwrap();
    let r = json::lire(&texte).unwrap();
    let t3 = |j: &J| j.nombres::<3>().unwrap();
    let cible_de = |c: &str| match c {
        "premier" => Cible::Premier,
        "dernier" => Cible::Dernier,
        _ => Cible::Centre,
    };
    let (mut nops, mut parop) = (0, std::collections::BTreeMap::<String, usize>::new());
    for (k, c) in r.get("geometrie").unwrap().liste().unwrap().iter().enumerate() {
        let mut p = Projet::neuf();
        p.objets = c.get("avant").unwrap().liste().unwrap().iter().enumerate().map(|(i, a)| {
            let mut o = Objet::neuf(&format!("o{i}"), Maillage::depuis_donnees(a.get("v").unwrap()).unwrap());
            o.position = t3(a.get("p").unwrap());
            o.rotation = a.get("q").unwrap().nombres::<4>().unwrap();
            o.echelle = t3(a.get("e").unwrap());
            o
        }).collect();
        let actif = n(c.get("actif").unwrap()) as usize;
        p.actif = Some(actif);
        let tous: Vec<usize> = (0..p.objets.len()).collect();
        let mut choix = tous.clone();
        let mut t = Travail::ouvrir(&p.objets, &tous, actif, [true; 3]);
        let curseur = t3(c.get("curseur").unwrap());
        let mut fini = false;
        for (e, f) in c.get("faites").unwrap().liste().unwrap().iter().enumerate() {
            let lieu = format!("géométrie {k} étape {e}");
            for x in f.get("choisis").unwrap().liste().unwrap() {
                let l = x.liste().unwrap();
                let i = |q: usize| n(&l[q]) as usize;
                t.sel.basculer(match l[0].txt().unwrap() {
                    "s" => El::S(i(1)),
                    "a" => El::A([i(1), i(2)]),
                    _ => El::F(i(1)),
                });
            }
            match n(f.get("filtre").unwrap()) as i32 {
                0 => t.regler_filtres([false, true, true]),
                1 => t.regler_filtres([true, false, true]),
                _ => {}
            }
            let op = f.get("op").unwrap().txt().unwrap();
            *parop.entry(op.to_string()).or_default() += 1;
            let bilan: Bilan = match op {
                "face" => t.creer_face(),
                "arete" => t.creer_arete(),
                "sup-a" => t.supprimer_aretes(),
                "inverser" => t.inverser(),
                "trianguler" => t.trianguler(),
                "dissoudre" => t.dissoudre(),
                "sup-f" => t.supprimer_faces(),
                "sup-s" => t.supprimer_sommets(),
                "sommet" => t.ajouter_sommet(curseur, actif),
                "lies" => t.lies(),
                "tout-s" => { t.tout(T::S); Ok(None) }
                "tout-a" => { t.tout(T::A); Ok(None) }
                "tout-f" => { t.tout(T::F); Ok(None) }
                "aucun-s" => { t.aucun(Some(T::S)); Ok(None) }
                "aucun" => { t.aucun(None); Ok(None) }
                "boucle" => t.boucle(None, 1, 0.0, false, false),
                "separer" => t.separer(&mut p, &mut choix).map(|(m, q)| { fini = q; Some(m) }),
                "proches" => t.fusion_proches(0.3),
                "revolution" => t.revolution(RegleRevolution { axe: [0.0, 1.0, 0.0], angle: 90.0, segments: 3, fermer: false }, curseur),
                "revolution-ferme" => t.revolution(RegleRevolution { axe: [1.0, 0.0, 0.0], angle: -360.0, segments: 6, fermer: true }, curseur),
                o if o.starts_with("c-") => {
                    let ty = |c: u8| match c { b's' => T::S, b'a' => T::A, _ => T::F };
                    t.convertir(ty(o.as_bytes()[2]), ty(o.as_bytes()[3]))
                }
                o if o.starts_with("subdiviser-") => t.subdiviser(o[11..].parse().unwrap()),
                o => t.fusionner(cible_de(&o[7..])),
            };
            let msg = match bilan { Ok(m) => m, Err(m) => Some(m) };
            let attendu = f.get("msg").unwrap().txt().map(str::to_string);
            // Les opérations muettes de GDA gardent le silence, ou le message d'avant
            assert_eq!(msg, attendu, "{lieu} {op}");
            assert_eq!(p.objets.len(), n(f.get("objets").unwrap()) as usize, "{lieu} {op} objets");
            if fini {
                assert_eq!(f.get("apres"), Some(&J::Nul), "{lieu} {op}");
                break;
            }
            let a = f.get("apres").unwrap();
            pareil(&etat(&t.m), &etat(&Maillage::depuis_donnees(a.get("m").unwrap()).unwrap()), &format!("{lieu} {op}")).unwrap();
            let ents = |l: &[usize]| nums(l);
            let ar_ = |l: &[Arete]| ar(l);
            assert_eq!(ents(&t.proprio), *a.get("proprio").unwrap(), "{lieu} {op} proprio");
            let s = a.get("sel").unwrap();
            assert_eq!((ents(&t.sel.sommets), ar_(&t.sel.aretes), ents(&t.sel.faces)), (s.get("s").unwrap().clone(), s.get("a").unwrap().clone(), s.get("f").unwrap().clone()), "{lieu} {op} sélection");
            let d = t.sel.vers_donnees();
            let tt = a.get("tout").unwrap();
            assert_eq!((ents(&d.s), ar_(&d.a), ents(&d.f)), (tt.get("s").unwrap().clone(), tt.get("a").unwrap().clone(), tt.get("f").unwrap().clone()), "{lieu} {op} sélection et cache");
            assert_eq!(J::Liste(t.filtres.iter().map(|b| J::Bool(*b)).collect()), *a.get("filtres").unwrap(), "{lieu} {op} filtres");
            let der = match t.sel.dernier {
                None => J::Nul,
                Some(El::S(i)) => J::Liste(vec![J::Txt("s".into()), num(i)]),
                Some(El::A([i, j])) => J::Liste(vec![J::Txt("a".into()), num(i), num(j)]),
                Some(El::F(i)) => J::Liste(vec![J::Txt("f".into()), num(i)]),
            };
            assert_eq!(der, *a.get("dernier").unwrap(), "{lieu} {op} dernier");
            nops += 1;
        }
        if !fini {
            t.sale = true;
            t.ranger(&mut p.objets);
        }
        for (i, (o, a)) in p.objets.iter().zip(c.get("fin").unwrap().liste().unwrap()).enumerate() {
            let lieu = format!("géométrie {k} fin, objet {i}");
            assert_eq!(o.nom, a.get("nom").unwrap().txt().unwrap(), "{lieu}");
            pareil(&etat(&o.maillage), &etat(&Maillage::depuis_donnees(a.get("m").unwrap()).unwrap()), &lieu).unwrap();
        }
    }
    println!("{nops} opérations de géométrie comme GDA : {parop:?}");
    assert!(nops > 500);
}
