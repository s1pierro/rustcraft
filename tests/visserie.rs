//! Compare le portage à la vraie visserie de GDA et boolmesh au vrai Manifold :
//! `tests/visserie.json` vient de `tools/reference-visserie.js`.

use rustcraft::booleens::{operation, Op};
use rustcraft::json::{self, J};
use rustcraft::maillage::Maillage;
use rustcraft::visserie::{self as vs, Options, Parametres, Piece, Tete};

fn reference() -> J {
    json::lire(&std::fs::read_to_string("tests/visserie.json").unwrap()).unwrap()
}

fn piece(c: &J) -> Piece {
    match c.get("piece").unwrap().txt().unwrap() {
        "vis" => Piece::Vis,
        "ecrou" => Piece::Ecrou,
        "rondelle" => Piece::Rondelle,
        _ => Piece::Tige,
    }
}

fn options(j: &J) -> Options {
    let n = |k: &str| j.get(k).and_then(J::num);
    let t = |k: &str| j.get(k).and_then(J::txt).unwrap_or("");
    let d = Options::default();
    Options {
        taille: t("taille").to_string(),
        tete: match t("tete") {
            "cylindrique" => Tete::Cylindrique,
            "fraisee" => Tete::Fraisee,
            _ => Tete::Hexagonale,
        },
        longueur: n("longueur").unwrap_or(d.longueur),
        partiel: t("filetage") == "partiel",
        segments: n("segments").unwrap_or(24.0) as usize,
        rangs: n("rangs").unwrap_or(8.0) as usize,
        soustraction: t("methode") == "soustraction",
        detail: matches!(j.get("detail"), Some(J::Bool(true))),
        d: n("d"),
        p: n("p"),
        profondeur: n("profondeur"),
        sommet: n("sommet"),
        gorge: n("gorge"),
        filetee: n("filetee"),
        lisse: n("lisse"),
        hauteur: n("hauteur"),
        plats: n("plats"),
        interieur: n("interieur"),
        exterieur: n("exterieur"),
        epaisseur: n("epaisseur"),
    }
}

fn pareil(m: &Maillage, c: &J, quoi: &str) {
    let v = c.get("v").unwrap().liste().unwrap();
    let f = c.get("f").unwrap().liste().unwrap();
    assert_eq!(m.sommets.len(), v.len(), "{quoi}");
    assert_eq!(m.faces.len(), f.len(), "{quoi}");
    for (p, q) in m.sommets.iter().zip(v) {
        let q = q.nombres::<3>().unwrap();
        for k in 0..3 {
            assert!((p[k] - q[k]).abs() < 1e-11, "{quoi} : {p:?} {q:?}");
        }
    }
    for (a, b) in m.faces.iter().zip(f) {
        let b: Vec<usize> = b.liste().unwrap().iter().map(|x| x.num().unwrap() as usize).collect();
        assert_eq!(a.s, b, "{quoi}");
    }
}

#[test]
fn les_pieces_par_anneaux_sont_exactement_celles_de_gda() {
    let r = reference();
    let l = r.get("anneaux").unwrap().liste().unwrap();
    assert!(l.len() > 70);
    for c in l {
        let o = options(c.get("o").unwrap());
        pareil(&vs::creer(piece(c), &o).unwrap(), c, &format!("{:?} {o:?}", piece(c)));
    }
}

#[test]
fn les_outils_de_coupe_sont_ceux_de_gda() {
    let r = reference();
    for c in r.get("outils").unwrap().liste().unwrap() {
        let q = parametres(c.get("P").unwrap());
        let n = |k: &str| c.get(k).unwrap().num().unwrap();
        let sortie = matches!(c.get("sortie"), Some(J::Bool(true)));
        pareil(&vs::outil(&q, n("n") as usize, n("y0"), n("y1"), sortie), c, "outil");
    }
}

fn parametres(j: &J) -> Parametres {
    let n = |k: &str| j.get(k).and_then(J::num).unwrap_or(0.0);
    Parametres {
        d: n("d"),
        p: n("p"),
        profondeur: n("profondeur"),
        sommet: n("sommet"),
        gorge: n("gorge"),
        filetee: n("filetee"),
        lisse: n("lisse"),
        hauteur: n("hauteur"),
        plats: n("plats"),
        interieur: n("interieur"),
        exterieur: n("exterieur"),
        epaisseur: n("epaisseur"),
    }
}

#[test]
fn parametres_problemes_et_sommets_sont_ceux_de_gda() {
    let r = reference();
    let l = r.get("reglages").unwrap().liste().unwrap();
    assert!(l.len() > 1000);
    for c in l {
        let (q, o) = (piece(c), options(c.get("o").unwrap()));
        assert_eq!(vs::parametres(q, &o).unwrap(), parametres(c.get("parametres").unwrap()), "{q:?} {o:?}");
        let attendu = c.get("probleme").unwrap().txt().unwrap();
        assert_eq!(vs::probleme(q, &o).unwrap_or_default(), attendu, "{q:?} {o:?}");
        // Pour une pièce impossible, le décompte ne sert pas (et peut être négatif dans GDA)
        if attendu.is_empty() {
            assert_eq!(vs::sommets(q, &o) as f64, c.get("sommets").unwrap().num().unwrap(), "{q:?} {o:?}");
        }
    }
}

#[test]
fn les_pieces_taillees_par_boolmesh_valent_celles_de_manifold() {
    let r = reference();
    let mut bilan = Vec::new();
    for c in r.get("coupes").unwrap().liste().unwrap() {
        let o = options(c.get("o").unwrap());
        let t = std::time::Instant::now();
        let m = vs::creer(piece(c), &o).unwrap_or_else(|e| panic!("{:?} {} : {e}", piece(c), o.taille));
        let v0 = c.get("volume").unwrap().num().unwrap();
        let v = vs::volume(&m);
        let (a, b) = (m.boite_englobante().unwrap(), c.get("min").unwrap().nombres::<3>().unwrap());
        let d = c.get("max").unwrap().nombres::<3>().unwrap();
        bilan.push(format!("{:?} {} {}×{} : volume {v:.3} / {v0:.3} ({:+.4} %), {} / {} sommets, {:.0} ms", piece(c), o.taille, o.longueur, o.segments, (v - v0) / v0 * 100.0, m.sommets.len(), c.get("sommets").unwrap().num().unwrap(), t.elapsed().as_secs_f64() * 1e3));
        assert!(((v - v0) / v0).abs() < 2e-3, "{}", bilan.last().unwrap());
        for k in 0..3 {
            assert!((a.min[k] - b[k]).abs() < 1e-2 && (a.max[k] - d[k]).abs() < 1e-2, "{}", bilan.last().unwrap());
        }
    }
    println!("{}", bilan.join("\n"));
}

#[test]
fn les_booleens_valent_ceux_de_manifold() {
    let forme = |n: &str| match n {
        "cube" => Maillage::cube(1.0),
        "boite" => Maillage::boite(2.0, 0.5, 1.5),
        "cylindre" => Maillage::cylindre(0.5, 2.0, 24, false),
        "cone" => Maillage::cone(0.8, 0.1, 1.4, 16, false),
        "sphere" => Maillage::sphere_uv(0.7, 24, 12),
        _ => Maillage::icosphere(0.6, 3),
    };
    let r = reference();
    let (mut ecarts, mut pannes, mut n) = (Vec::new(), Vec::new(), 0);
    for c in r.get("booleens").unwrap().liste().unwrap() {
        let t = |k: &str| c.get(k).unwrap().txt().unwrap();
        let d = c.get("d").unwrap().nombres::<3>().unwrap();
        let ech = c.get("ech").unwrap().num().unwrap();
        let mut b = forme(t("b"));
        for p in &mut b.sommets {
            *p = [0, 1, 2].map(|k| p[k] * ech + d[k]);
        }
        let op = match t("op") {
            "union" => Op::Union,
            "difference" => Op::Difference,
            _ => Op::Intersection,
        };
        let v0 = c.get("volume").unwrap().num().unwrap();
        let cas = format!("{} {} {} {d:?}", t("a"), t("op"), t("b"));
        n += 1;
        match operation(&forme(t("a")), &b, op) {
            Ok(m) => {
                let v = vs::volume(&m);
                if (v - v0).abs() > 1e-6 * v0.abs().max(1.0) {
                    ecarts.push(format!("{cas} : {v} / {v0}"));
                }
            }
            Err(e) => pannes.push(format!("{cas} : {e}")),
        }
    }
    println!("{n} cas ; {} écarts, {} erreurs\n{}\n{}", ecarts.len(), pannes.len(), ecarts.join("\n"), pannes.join("\n"));
    assert!(ecarts.is_empty() && pannes.is_empty(), "{} écarts, {} erreurs", ecarts.len(), pannes.len());
}
