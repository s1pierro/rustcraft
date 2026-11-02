//! Primitives et visserie réglables, reprises de `ED_PRIMITIVES` de GDA : les champs du
//! panneau d'ajout, leur vérification, le nom de l'objet et sa construction.

use crate::maillage::Maillage;
use crate::visserie::{self as vs, Options, Parametres, Piece, Tete};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Prim {
    Cube,
    Cylindre,
    Cone,
    Sphere,
    Icosphere,
    Vis,
    Ecrou,
    Rondelle,
    Tige,
}

pub const TOUTES: [Prim; 9] = [Prim::Cube, Prim::Cylindre, Prim::Cone, Prim::Sphere, Prim::Icosphere, Prim::Vis, Prim::Ecrou, Prim::Rondelle, Prim::Tige];

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    N(f64),
    T(String),
    B(bool),
}

pub type Valeurs = BTreeMap<&'static str, V>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Genre {
    /// Nombre > 0, ou ≥ 0 si `zero`.
    Nombre { zero: bool },
    Entier { min: f64, max: f64, multiple: f64 },
    Choix(&'static [(&'static str, &'static str)]),
    Interrupteur,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Defaut {
    N(f64),
    T(&'static str),
    B(bool),
}

/// Un champ du panneau. `si` : affiché seulement si vrai ; caché, il ne compte pas.
#[derive(Clone, Copy, Debug)]
pub struct Champ {
    pub cle: &'static str,
    pub libelle: &'static str,
    /// Intitulé posé avant le champ.
    pub rubrique: Option<&'static str>,
    pub genre: Genre,
    defaut: Defaut,
    pub si: fn(&Valeurs) -> bool,
}

fn toujours(_: &Valeurs) -> bool {
    true
}

fn detail(v: &Valeurs) -> bool {
    v.get("detail") == Some(&V::B(true))
}

fn iso(v: &Valeurs) -> bool {
    !detail(v)
}

fn anneaux(v: &Valeurs) -> bool {
    !lourde_valeurs(v)
}

fn lourde_valeurs(v: &Valeurs) -> bool {
    matches!(v.get("methode"), Some(V::T(m)) if m == "soustraction")
}

const FERMETURES: &[(&str, &str)] = &[("polygone", "Polygone"), ("eventail", "Éventail")];
const TETES: &[(&str, &str)] = &[("hexagonale", "Hexagonale"), ("cylindrique", "Cylindrique"), ("fraisee", "Fraisée")];
const FILETAGES: &[(&str, &str)] = &[("total", "Toute la longueur"), ("partiel", "Partiel (2d + 6)")];
const METHODES: &[(&str, &str)] = &[("soustraction", "Soustraction"), ("anneaux", "Anneaux")];
const TAILLES: &[(&str, &str)] = &[("M2", "M2"), ("M2,5", "M2,5"), ("M3", "M3"), ("M4", "M4"), ("M5", "M5"), ("M6", "M6"), ("M8", "M8"), ("M10", "M10"), ("M12", "M12"), ("M16", "M16"), ("M20", "M20"), ("M24", "M24")];

const fn n(cle: &'static str, libelle: &'static str, rubrique: Option<&'static str>, d: f64) -> Champ {
    Champ { cle, libelle, rubrique, genre: Genre::Nombre { zero: false }, defaut: Defaut::N(d), si: toujours }
}

const fn z(cle: &'static str, libelle: &'static str, rubrique: Option<&'static str>, d: f64) -> Champ {
    Champ { cle, libelle, rubrique, genre: Genre::Nombre { zero: true }, defaut: Defaut::N(d), si: toujours }
}

const fn e(cle: &'static str, libelle: &'static str, rubrique: Option<&'static str>, d: f64, min: f64, max: f64) -> Champ {
    Champ { cle, libelle, rubrique, genre: Genre::Entier { min, max, multiple: 1.0 }, defaut: Defaut::N(d), si: toujours }
}

const fn c(cle: &'static str, rubrique: &'static str, l: &'static [(&'static str, &'static str)], d: &'static str) -> Champ {
    Champ { cle, libelle: rubrique, rubrique: Some(rubrique), genre: Genre::Choix(l), defaut: Defaut::T(d), si: toujours }
}

const fn si(mut x: Champ, f: fn(&Valeurs) -> bool) -> Champ {
    x.si = f;
    x
}

const TAILLE: Champ = c("taille", "Taille ISO (pas gros ; 1 unité = 1 mm)", TAILLES, "M8");
const DETAIL: Champ = Champ { cle: "detail", libelle: "Paramètres détaillés", rubrique: None, genre: Genre::Interrupteur, defaut: Defaut::B(false), si: toujours };
const METHODE: Champ = c("methode", "Méthode du filet", METHODES, "soustraction");
const SEGMENTS: Champ = Champ { cle: "segments", libelle: "Segments par tour (multiple de 6)", rubrique: Some("Maillage du filet"), genre: Genre::Entier { min: 12.0, max: 96.0, multiple: 6.0 }, defaut: Defaut::N(24.0), si: toujours };
const RANGS: Champ = si(e("rangs", "Anneaux par pas", None, 8.0, 4.0, 16.0), anneaux);
const FILET: [Champ; 5] = [
    si(n("d", "Diamètre extérieur", Some("Filet (repris de la taille, puis libre)"), 8.0), detail),
    si(n("p", "Pas", None, 1.25), detail),
    si(n("profondeur", "Profondeur du filet", None, 0.6766), detail),
    si(z("sommet", "Épaisseur du sommet du filet", None, 0.15625), detail),
    si(z("gorge", "Épaisseur de la gorge (fond)", None, 0.3125), detail),
];
const FERMETURE: Champ = c("fermeture", "Fermeture des bases", FERMETURES, "polygone");
const COTES: Champ = e("cotes", "Côtés", Some("Maillage"), 16.0, 3.0, 256.0);

/// Au-delà, une pièce est refusée.
const VIS_MAX: usize = 60000;

impl Prim {
    pub fn cle(self) -> &'static str {
        match self {
            Prim::Cube => "cube",
            Prim::Cylindre => "cylindre",
            Prim::Cone => "cone",
            Prim::Sphere => "sphere",
            Prim::Icosphere => "icosphere",
            Prim::Vis => "vis",
            Prim::Ecrou => "ecrou",
            Prim::Rondelle => "rondelle",
            Prim::Tige => "tige",
        }
    }

    pub fn titre(self) -> &'static str {
        match self {
            Prim::Cube => "Ajouter un cube",
            Prim::Cylindre => "Ajouter un cylindre",
            Prim::Cone => "Ajouter un cône",
            Prim::Sphere => "Ajouter une sphère UV",
            Prim::Icosphere => "Ajouter une icosphère",
            Prim::Vis => "Ajouter une vis",
            Prim::Ecrou => "Ajouter un écrou",
            Prim::Rondelle => "Ajouter une rondelle",
            Prim::Tige => "Ajouter une tige filetée",
        }
    }

    /// La vue se cadre sur le premier aperçu : la visserie est en mm.
    pub fn recadrer(self) -> bool {
        self.piece().is_some()
    }

    pub fn piece(self) -> Option<Piece> {
        match self {
            Prim::Vis => Some(Piece::Vis),
            Prim::Ecrou => Some(Piece::Ecrou),
            Prim::Rondelle => Some(Piece::Rondelle),
            Prim::Tige => Some(Piece::Tige),
            _ => None,
        }
    }

    pub fn champs(self) -> Vec<Champ> {
        match self {
            Prim::Cube => vec![n("x", "X", Some("Dimensions"), 1.0), n("y", "Y", None, 1.0), n("z", "Z", None, 1.0)],
            Prim::Cylindre => vec![n("rayon", "Rayon", Some("Dimensions (axe Y)"), 0.5), n("hauteur", "Hauteur", None, 1.0), COTES, FERMETURE],
            Prim::Cone => vec![n("rayon", "Rayon de la base", Some("Dimensions (axe Y)"), 0.5), z("haut", "Rayon du haut (0 : pointe)", None, 0.0), n("hauteur", "Hauteur", None, 1.0), COTES, FERMETURE],
            Prim::Sphere => vec![n("rayon", "Rayon", Some("Dimensions"), 0.5), e("segments", "Segments (méridiens)", Some("Maillage (pôles sur l’axe Y)"), 16.0, 3.0, 256.0), e("anneaux", "Anneaux (bandes)", None, 8.0, 2.0, 128.0)],
            Prim::Icosphere => vec![n("rayon", "Rayon", Some("Dimensions"), 0.5), e("subdivisions", "Subdivisions", Some("Maillage (1 : 20 faces ; ×4 par subdivision)"), 2.0, 1.0, 6.0)],
            Prim::Vis => {
                let mut l = vec![
                    TAILLE,
                    c("tete", "Tête", TETES, "hexagonale"),
                    si(n("longueur", "Longueur", Some("Longueur (sous la tête ; fraisée : tête comprise)"), 20.0), iso),
                    si(c("filetage", "Filetage", FILETAGES, "total"), iso),
                    DETAIL,
                ];
                l.extend(FILET);
                l.extend([si(n("filetee", "Hauteur filetée", Some("Longueurs"), 20.0), detail), si(z("lisse", "Hauteur sans filet sous la tête", None, 0.0), detail), METHODE, SEGMENTS, RANGS]);
                l
            }
            Prim::Ecrou => {
                let mut l = vec![TAILLE, DETAIL];
                l.extend(FILET);
                l.extend([si(n("hauteur", "Hauteur", Some("Écrou"), 6.8), detail), si(n("plats", "Sur plats", None, 13.0), detail), METHODE, SEGMENTS, RANGS]);
                l
            }
            Prim::Rondelle => vec![
                TAILLE,
                DETAIL,
                si(n("interieur", "Diamètre intérieur", Some("Rondelle (reprise de la taille, puis libre)"), 8.4), detail),
                si(n("exterieur", "Diamètre extérieur", None, 16.0), detail),
                si(n("epaisseur", "Épaisseur", None, 1.6), detail),
                e("segments", "Segments", Some("Maillage"), 32.0, 6.0, 256.0),
            ],
            Prim::Tige => {
                let mut l = vec![TAILLE, n("longueur", "Longueur", Some("Longueur"), 50.0), DETAIL];
                l.extend(FILET);
                l.extend([METHODE, SEGMENTS, RANGS]);
                l
            }
        }
    }

    pub fn defauts(self) -> Valeurs {
        self.champs()
            .iter()
            .map(|c| {
                (c.cle, match c.defaut {
                    Defaut::N(x) => V::N(x),
                    Defaut::T(t) => V::T(t.to_string()),
                    Defaut::B(b) => V::B(b),
                })
            })
            .collect()
    }

    /// Le calcul passe par un booléen : long, il se fait hors du fil de rendu.
    pub fn lourde(self, v: &Valeurs) -> bool {
        self.piece().is_some_and(|p| p != Piece::Rondelle) && lourde_valeurs(v)
    }

    /// Valeurs du champ `c` hors bornes, ou non numériques.
    pub fn invalide(self, c: &Champ, v: &Valeurs) -> bool {
        let Some(V::N(x)) = v.get(c.cle) else { return false };
        match c.genre {
            Genre::Nombre { zero } => !x.is_finite() || *x < 0.0 || (*x == 0.0 && !zero),
            Genre::Entier { min, max, multiple } => x.fract() != 0.0 || *x < min || *x > max || x % multiple != 0.0,
            _ => false,
        }
    }

    /// Tous les champs affichés sont bons.
    pub fn valide(self, v: &Valeurs) -> bool {
        self.champs().iter().all(|c| !(c.si)(v) || !self.invalide(c, v))
    }

    pub fn options(self, v: &Valeurs) -> Options {
        let t = |k: &str| match v.get(k) {
            Some(V::T(s)) => s.as_str(),
            _ => "",
        };
        let nb = |k: &str| match v.get(k) {
            Some(V::N(x)) => Some(*x),
            _ => None,
        };
        let d = Options::default();
        Options {
            taille: t("taille").to_string(),
            tete: match t("tete") {
                "cylindrique" => Tete::Cylindrique,
                "fraisee" => Tete::Fraisee,
                _ => Tete::Hexagonale,
            },
            longueur: nb("longueur").unwrap_or(d.longueur),
            partiel: t("filetage") == "partiel",
            segments: nb("segments").unwrap_or(24.0) as usize,
            rangs: nb("rangs").unwrap_or(8.0) as usize,
            soustraction: lourde_valeurs(v),
            detail: detail(v),
            d: nb("d"),
            p: nb("p"),
            profondeur: nb("profondeur"),
            sommet: nb("sommet"),
            gorge: nb("gorge"),
            filetee: nb("filetee"),
            lisse: nb("lisse"),
            hauteur: nb("hauteur"),
            plats: nb("plats"),
            interieur: nb("interieur"),
            exterieur: nb("exterieur"),
            epaisseur: nb("epaisseur"),
        }
    }

    /// Après un changement de taille, ou à l'ouverture des paramètres détaillés : les
    /// champs détaillés reprennent les cotes de la taille.
    pub fn remplir(self, v: &mut Valeurs, cle: &str) {
        let Some(piece) = self.piece() else { return };
        if !(cle == "taille" || (cle == "detail" && detail(v))) {
            return;
        }
        let mut o = self.options(v);
        o.detail = false;
        let Ok(q) = vs::parametres(piece, &o) else { return };
        let cotes: [(&str, f64); 12] = [("d", q.d), ("p", q.p), ("profondeur", q.profondeur), ("sommet", q.sommet), ("gorge", q.gorge), ("filetee", q.filetee), ("lisse", q.lisse), ("hauteur", q.hauteur), ("plats", q.plats), ("interieur", q.interieur), ("exterieur", q.exterieur), ("epaisseur", q.epaisseur)];
        for (k, x) in cotes {
            if let Some(c) = v.get_mut(k) {
                *c = V::N((x * 1e4 + 0.5).floor() / 1e4);
            }
        }
    }

    /// Ligne d'information : cotes et nombre de sommets, ou ce qui empêche la pièce ; le
    /// booléen dit qu'Ajouter reste grisé.
    pub fn verifier(self, v: &Valeurs) -> Option<(String, bool)> {
        let piece = self.piece()?;
        let o = self.options(v);
        let tete = if o.detail { "Réglages détaillés".to_string() } else { o.taille.clone() };
        if let Some(e) = vs::probleme(piece, &o) {
            return Some((format!("{tete} : {e}"), true));
        }
        let q = vs::parametres(piece, &o).ok()?;
        let n = vs::sommets(piece, &o);
        let c = cotes(piece, &q);
        let mut t = format!("{tete} : {c} · ≈ {} sommets", milliers(n));
        let lourd = n > VIS_MAX;
        if lourd {
            t += &format!(" — trop lourde (plus de {} sommets) : moins d’anneaux par pas ou de segments", milliers(VIS_MAX));
        }
        Some((t, lourd))
    }

    fn longueur_vis(v: &Valeurs, o: &Options) -> f64 {
        let (Ok(q), Ok(t)) = (vs::parametres(Piece::Vis, o), vs::cotes(&o.taille)) else { return o.longueur };
        if detail(v) { q.lisse + q.filetee + if o.tete == Tete::Fraisee { (t.dk_f - t.d) / 2.0 } else { 0.0 } } else { o.longueur }
    }

    /// Nom de l'objet ajouté.
    pub fn nom(self, v: &Valeurs) -> String {
        let o = self.options(v);
        let diametre = |p: Piece| match (o.detail, vs::parametres(p, &o)) {
            (true, Ok(q)) => format!("Ø{}", virgule(q.d)),
            _ => o.taille.clone(),
        };
        match self {
            Prim::Cube => "cube".into(),
            Prim::Cylindre => "cylindre".into(),
            Prim::Cone => "cône".into(),
            Prim::Sphere => "sphère".into(),
            Prim::Icosphere => "icosphère".into(),
            Prim::Vis => {
                let t = match o.tete {
                    Tete::Hexagonale => "H",
                    Tete::Cylindrique => "CHC",
                    Tete::Fraisee => "FHC",
                };
                format!("vis {t} {}×{}", diametre(Piece::Vis), virgule(Prim::longueur_vis(v, &o)))
            }
            Prim::Ecrou => format!("écrou {}", diametre(Piece::Ecrou)),
            Prim::Rondelle => match (o.detail, o.interieur, o.exterieur) {
                (true, Some(i), Some(e)) => format!("rondelle {}×{}", virgule(i), virgule(e)),
                _ => format!("rondelle {}", o.taille),
            },
            Prim::Tige => format!("tige {}×{}", diametre(Piece::Tige), virgule(o.longueur)),
        }
    }

    /// Le maillage, centré sur l'origine de l'objet.
    pub fn creer(self, v: &Valeurs) -> Result<Maillage, String> {
        let nb = |k: &str| match v.get(k) {
            Some(V::N(x)) => *x,
            _ => 0.0,
        };
        let eventail = matches!(v.get("fermeture"), Some(V::T(f)) if f == "eventail");
        Ok(match self {
            Prim::Cube => Maillage::boite(nb("x"), nb("y"), nb("z")),
            Prim::Cylindre => Maillage::cylindre(nb("rayon"), nb("hauteur"), nb("cotes") as usize, eventail),
            Prim::Cone => Maillage::cone(nb("rayon"), nb("haut"), nb("hauteur"), nb("cotes") as usize, eventail),
            Prim::Sphere => Maillage::sphere_uv(nb("rayon"), nb("segments") as usize, nb("anneaux") as usize),
            Prim::Icosphere => Maillage::icosphere(nb("rayon"), nb("subdivisions") as usize),
            _ => return vs::creer(self.piece().unwrap(), &self.options(v)),
        })
    }
}

fn cotes(piece: Piece, q: &Parametres) -> String {
    match piece {
        Piece::Rondelle => format!("Ø {} × {}, épaisseur {}", virgule(q.interieur), virgule(q.exterieur), virgule(q.epaisseur)),
        Piece::Ecrou => format!("Ø {}, pas {}, {} sur plats, hauteur {}", virgule(q.d), virgule(q.p), virgule(q.plats), virgule(q.hauteur)),
        Piece::Tige => format!("Ø {}, pas {}", virgule(q.d), virgule(q.p)),
        Piece::Vis => {
            let l = if q.lisse > 0.0 { format!(", lisse sur {}", virgule(q.lisse)) } else { String::new() };
            format!("Ø {}, pas {}, filetée sur {}{l}", virgule(q.d), virgule(q.p), virgule(q.filetee))
        }
    }
}

/// Nombre à quatre décimales au plus, à la française.
pub fn virgule(x: f64) -> String {
    format!("{}", (x * 1e4).round() / 1e4).replace('.', ",")
}

fn milliers(n: usize) -> String {
    let s = n.to_string();
    let mut r = String::new();
    for (k, c) in s.chars().enumerate() {
        if k > 0 && (s.len() - k) % 3 == 0 {
            r.push('\u{202F}');
        }
        r.push(c);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_primitive_se_construit_avec_ses_reglages_par_defaut() {
        for p in TOUTES {
            let v = p.defauts();
            assert!(p.valide(&v), "{p:?}");
            let m = p.creer(&v).unwrap_or_else(|e| panic!("{p:?} : {e}"));
            assert!(!m.faces.is_empty(), "{p:?}");
            assert!(!p.nom(&v).is_empty());
        }
    }

    #[test]
    fn un_champ_hors_bornes_grise_l_ajout() {
        let p = Prim::Vis;
        let mut v = p.defauts();
        v.insert("segments", V::N(25.0));
        assert!(!p.valide(&v), "multiple de 6");
        v.insert("segments", V::N(24.0));
        v.insert("longueur", V::N(-1.0));
        assert!(!p.valide(&v));
        v.insert("detail", V::B(true));
        assert!(p.valide(&v), "caché, le champ ne compte pas");
        let mut c = Prim::Cone.defauts();
        c.insert("haut", V::N(0.0));
        assert!(Prim::Cone.valide(&c), "0 : pointe");
    }

    #[test]
    fn changer_de_taille_reprend_les_cotes_detaillees() {
        let p = Prim::Ecrou;
        let mut v = p.defauts();
        v.insert("taille", V::T("M12".into()));
        p.remplir(&mut v, "taille");
        assert_eq!(v["d"], V::N(12.0));
        assert_eq!(v["plats"], V::N(18.0));
        assert_eq!(v["hauteur"], V::N(10.8));
    }

    #[test]
    fn la_ligne_d_information_dit_les_cotes_ou_le_probleme() {
        let p = Prim::Vis;
        let mut v = p.defauts();
        let (t, grise) = p.verifier(&v).unwrap();
        assert!(t.starts_with("M8 : Ø 8, pas 1,25, filetée sur 20"), "{t}");
        assert!(!grise);
        v.insert("detail", V::B(true));
        v.insert("gorge", V::N(2.0));
        let (t, grise) = p.verifier(&v).unwrap();
        assert!(t.contains("sommet + gorge") && grise, "{t}");
        assert!(Prim::Cube.verifier(&Prim::Cube.defauts()).is_none());
    }

    #[test]
    fn une_piece_trop_lourde_est_refusee() {
        let p = Prim::Tige;
        let mut v = p.defauts();
        v.insert("longueur", V::N(2000.0));
        v.insert("taille", V::T("M2".into()));
        v.insert("segments", V::N(96.0));
        let (t, grise) = p.verifier(&v).unwrap();
        assert!(grise && t.contains("trop lourde"), "{t}");
    }

    #[test]
    fn les_noms_sont_ceux_de_gda() {
        let mut v = Prim::Vis.defauts();
        assert_eq!(Prim::Vis.nom(&v), "vis H M8×20");
        v.insert("tete", V::T("fraisee".into()));
        v.insert("longueur", V::N(12.5));
        assert_eq!(Prim::Vis.nom(&v), "vis FHC M8×12,5");
        assert_eq!(Prim::Tige.nom(&Prim::Tige.defauts()), "tige M8×50");
        assert_eq!(Prim::Rondelle.nom(&Prim::Rondelle.defauts()), "rondelle M8");
    }
}
