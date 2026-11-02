//! Projet GDA (`.gda`, JSON) : lecture avec migrations, écriture au même format,
//! pour que Rustcraft et GDA ouvrent les fichiers l'un de l'autre.
//!
//! Ce que Rustcraft n'interprète pas encore (armatures, modifieurs) est gardé tel
//! quel et réécrit sans changement.

use crate::json::{self, J};
use crate::maillage::{extension, Maillage, MatMtl, SIGNATURE};
use crate::texture::{self, Texture};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Format écrit ; toute évolution suit `ED_FORMAT` de GDA.
pub const FORMAT: f64 = 9.0;
pub const DEFAUT: &str = "defaut";
pub const SET_DEFAUT: &str = "main set";

const TEINTES: [&str; 8] = ["#d9a066", "#7fae6e", "#6e93c9", "#c9736e", "#b08cc9", "#d9cf6e", "#6ec9c0", "#9a9a9a"];

#[derive(Clone, Debug, PartialEq)]
pub struct Materiau {
    pub couleur: String,
    pub ambiante: String,
    pub speculaire: String,
    pub emission: String,
    pub brillance: f64,
    pub opacite: f64,
    pub refraction: f64,
    pub illumination: f64,
    /// Texture de couleur : un identifiant de `Projet::textures`.
    pub texture: Option<String>,
}

impl Materiau {
    pub fn neuf(couleur: Option<&str>) -> Materiau {
        Materiau {
            couleur: couleur.unwrap_or("#c9c3d8").to_string(),
            ambiante: "#000000".into(),
            speculaire: "#000000".into(),
            emission: "#000000".into(),
            brillance: 10.0,
            opacite: 1.0,
            refraction: 1.0,
            illumination: 1.0,
            texture: None,
        }
    }

    /// Champs connus seulement, bornés (`ed_materiau`).
    pub fn lire(j: &J) -> Materiau {
        let d = Materiau::neuf(None);
        let hex = |k: &str, defaut: &str| hexa(j.get(k).and_then(J::txt)).unwrap_or_else(|| defaut.to_string());
        let nb = |k: &str, defaut: f64, min: f64, max: f64| j.get(k).and_then(J::num).filter(|v| v.is_finite()).map_or(defaut, |v| v.clamp(min, max));
        Materiau {
            couleur: hex("couleur", &d.couleur),
            ambiante: hex("ambiante", &d.ambiante),
            speculaire: hex("speculaire", &d.speculaire),
            emission: hex("emission", &d.emission),
            brillance: nb("brillance", d.brillance, 0.0, 1000.0),
            opacite: nb("opacite", d.opacite, 0.0, 1.0),
            refraction: nb("refraction", d.refraction, 0.001, 10.0),
            illumination: (nb("illumination", d.illumination, 0.0, 10.0) + 0.5).floor(),
            texture: j.get("texture").and_then(J::txt).map(String::from),
        }
    }

    fn vers_json(&self) -> J {
        let t = |k: &str, v: &str| (k.to_string(), J::Txt(v.to_string()));
        let n = |k: &str, v: f64| (k.to_string(), J::Num(v));
        let mut l = vec![
            t("couleur", &self.couleur),
            t("ambiante", &self.ambiante),
            t("speculaire", &self.speculaire),
            t("emission", &self.emission),
            n("brillance", self.brillance),
            n("opacite", self.opacite),
            n("refraction", self.refraction),
            n("illumination", self.illumination),
        ];
        if let Some(x) = &self.texture {
            l.push(t("texture", x));
        }
        J::Objet(l)
    }

    fn depuis_mtl(m: &MatMtl) -> Materiau {
        let d = Materiau::neuf(None);
        let c = |v: &Option<String>, defaut: &str| hexa(v.as_deref()).unwrap_or_else(|| defaut.to_string());
        Materiau {
            couleur: c(&m.couleur, &d.couleur),
            ambiante: c(&m.ambiante, &d.ambiante),
            speculaire: c(&m.speculaire, &d.speculaire),
            emission: c(&m.emission, &d.emission),
            brillance: m.brillance.unwrap_or(d.brillance),
            opacite: m.opacite.unwrap_or(d.opacite).clamp(0.0, 1.0),
            refraction: m.refraction.unwrap_or(d.refraction),
            illumination: m.illumination.unwrap_or(d.illumination),
            texture: None,
        }
    }

    pub fn vers_mtl(&self) -> MatMtl {
        MatMtl {
            couleur: Some(self.couleur.clone()),
            ambiante: Some(self.ambiante.clone()),
            speculaire: Some(self.speculaire.clone()),
            emission: Some(self.emission.clone()),
            brillance: Some(self.brillance),
            opacite: Some(self.opacite),
            refraction: Some(self.refraction),
            illumination: Some(self.illumination),
            texture: None,
        }
    }
}

fn hexa(v: Option<&str>) -> Option<String> {
    let v = v?;
    let ok = v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|c| c.is_ascii_hexdigit());
    ok.then(|| v.to_lowercase())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objet {
    pub nom: String,
    /// Partagé entre les étapes d'annulation et avec le rendu tant qu'il ne change pas.
    pub maillage: Arc<Maillage>,
    pub position: [f64; 3],
    pub rotation: [f64; 4],
    pub echelle: [f64; 3],
    pub set: String,
    pub visible: bool,
    pub materiau: String,
    /// Modifieurs, gardés tels quels jusqu'à leur portage.
    pub modifs: J,
    /// Armature, gardée telle quelle : son maillage reste vide.
    pub armature: Option<J>,
    /// Sélection en mode Géométrie, hors du fichier, comme dans GDA.
    pub sel: crate::geometrie::Donnees,
}

impl Objet {
    pub fn neuf(nom: &str, maillage: Maillage) -> Objet {
        Objet {
            nom: nom.to_string(),
            maillage: Arc::new(maillage),
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            echelle: [1.0; 3],
            set: SET_DEFAUT.to_string(),
            visible: true,
            materiau: DEFAUT.to_string(),
            modifs: J::Liste(vec![]),
            armature: None,
            sel: Default::default(),
        }
    }

    /// Les modifieurs booléens désignent leur cible par son nom : ils suivent ces
    /// renommages (ancien, nouveau).
    pub fn suivre_cibles(&mut self, noms: &[(String, String)]) {
        let J::Liste(l) = &mut self.modifs else { return };
        for m in l {
            let J::Objet(c) = m else { continue };
            if !c.iter().any(|(k, v)| k == "type" && matches!(v, J::Txt(t) if t == "booleen")) {
                continue;
            }
            for (k, v) in c {
                if let (true, J::Txt(t)) = (k == "cible", v) {
                    if let Some((_, n)) = noms.iter().find(|(a, _)| a == t) {
                        *t = n.clone();
                    }
                }
            }
        }
    }

    /// Matrice objet → monde, par colonnes.
    pub fn matrice(&self) -> [f64; 16] {
        crate::maillage::composer(self.position, self.rotation, self.echelle)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub nom: String,
    pub visible: bool,
    pub replie: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Projet {
    pub objets: Vec<Objet>,
    pub sets: Vec<Set>,
    pub actif: Option<usize>,
    pub curseur: [f64; 3],
    pub materiaux: Vec<(String, Materiau)>,
    /// Images des matériaux, par identifiant de contenu. Hors de l'annulation : défaire
    /// un import laisse l'image ici, et `ecrire` ne garde que celles qui servent.
    pub textures: BTreeMap<String, Texture>,
}

fn echelle(j: Option<&J>) -> [f64; 3] {
    j.and_then(J::nombres::<3>).filter(|e| e.iter().all(|v| v.is_finite() && *v > 0.0)).unwrap_or([1.0; 3])
}

impl Projet {
    /// Ajoute les objets d'un autre projet dans le set `set`, et rend leurs rangs. Les
    /// noms pris sont changés, jusque dans les modifieurs booléens qui les désignent ;
    /// un matériau de même nom mais différent reste distinct.
    pub fn fusionner(&mut self, autre: Projet, set: &str) -> Vec<usize> {
        let Projet { mut objets, materiaux, textures, .. } = autre;
        self.textures.extend(textures);

        // Noms d'objets : d'abord tous décidés, une cible peut suivre son objet
        let mut noms: Vec<(String, String)> = Vec::new();
        let mut pris: Vec<Objet> = self.objets.clone();
        for o in &mut objets {
            let n = crate::edition::nom_libre(&pris, &o.nom);
            if n != o.nom {
                noms.push((o.nom.clone(), n.clone()));
            }
            o.nom = n;
            pris.push(o.clone());
        }

        // Matériaux utilisés seulement
        let mut pareils: Vec<(String, String)> = Vec::new();
        for (nom, m) in materiaux {
            let sert = objets.iter().any(|o| o.materiau == nom || o.maillage.faces.iter().any(|f| *f.m == *nom));
            if !sert {
                continue;
            }
            match self.materiau(&nom) {
                None => self.materiaux.push((nom, m)),
                Some(x) if *x == m => {}
                Some(_) => {
                    let n = (2..).map(|k| format!("{nom} {k}")).find(|n| self.materiau(n).is_none()).unwrap();
                    self.materiaux.push((n.clone(), m));
                    pareils.push((nom, n));
                }
            }
        }

        let debut = self.objets.len();
        for mut o in objets {
            o.set = set.to_string();
            if let Some((_, n)) = pareils.iter().find(|(a, _)| *a == o.materiau) {
                o.materiau = n.clone();
            }
            if pareils.iter().any(|(a, _)| o.maillage.faces.iter().any(|f| *f.m == **a)) {
                for f in &mut Arc::make_mut(&mut o.maillage).faces {
                    if let Some((_, n)) = pareils.iter().find(|(a, _)| *f.m == **a) {
                        f.m = n.as_str().into();
                    }
                }
            }
            o.suivre_cibles(&noms);
            self.objets.push(o);
        }
        (debut..self.objets.len()).collect()
    }

    /// Scène de départ de GDA : un cube.
    pub fn neuf() -> Projet {
        let mut p = Projet {
            objets: vec![Objet::neuf("cube", Maillage::cube(1.0))],
            sets: vec![],
            actif: Some(0),
            curseur: [0.0; 3],
            materiaux: vec![],
            textures: BTreeMap::new(),
        };
        p.verifier();
        p
    }

    /// Lecture (`ed_lireProjet`, `_ouvrirProjet`) ; erreur lisible sinon.
    pub fn lire(texte: &str) -> Result<Projet, String> {
        let p = json::lire(texte).ok_or("fichier .gda illisible")?;
        if p.get("format").and_then(J::txt) != Some("gda") {
            return Err("ce n’est pas un projet GDA".into());
        }
        let version = p.get("version").and_then(J::num).filter(|v| *v >= 1.0).ok_or("version du projet inconnue")?;
        if version > FORMAT {
            return Err(format!("projet au format {version}, plus récent que cette application (format {FORMAT}) : mettez-la à jour"));
        }
        // Formats 1 à 8 : les champs absents ont leur valeur par défaut. Les
        // solidaires d'armature (format 7) seront convertis avec les armatures.
        let sc = p.get("scene").ok_or("projet sans objets")?;
        let liste = sc.get("objets").and_then(J::liste).ok_or("projet sans objets")?;
        let mut objets = Vec::with_capacity(liste.len());
        for o in liste {
            let nom = o.get("nom").and_then(J::txt).filter(|s| !s.is_empty());
            let m = o.get("maillage").ok_or_else(|| format!("objet « {} » sans maillage", nom.unwrap_or("?")))?;
            let maillage = Maillage::depuis_donnees(m).ok_or_else(|| format!("objet « {} » sans maillage", nom.unwrap_or("?")))?;
            let premiere = m.get("f").and_then(J::liste).and_then(|l| l.first()).and_then(J::liste).and_then(|f| f.first()).and_then(J::txt);
            let materiau = o.get("materiau").and_then(J::txt).filter(|s| !s.is_empty()).or(premiere).unwrap_or(DEFAUT);
            let e = echelle(o.get("echelle"));
            let armature = o.get("armature").filter(|a| !matches!(a, J::Nul)).cloned();
            objets.push(Objet {
                nom: nom.unwrap_or("objet").to_string(),
                maillage: Arc::new(maillage),
                position: o.get("position").and_then(J::nombres::<3>).unwrap_or([0.0; 3]),
                rotation: o.get("rotation").and_then(J::nombres::<4>).unwrap_or([0.0, 0.0, 0.0, 1.0]),
                echelle: if armature.is_some() { [e[0]; 3] } else { e },
                sel: Default::default(),
                set: o.get("set").and_then(J::txt).unwrap_or("").to_string(),
                visible: o.get("visible").and_then(J::bool).unwrap_or(true),
                materiau: materiau.to_string(),
                modifs: o.get("modifs").cloned().unwrap_or(J::Liste(vec![])),
                armature,
            });
        }
        let mut sets: Vec<Set> = Vec::new();
        for e in sc.get("sets").and_then(J::liste).unwrap_or(&[]) {
            let Some(nom) = e.get("nom").and_then(J::txt).filter(|s| !s.is_empty()) else { continue };
            if !sets.iter().any(|f| f.nom == nom) {
                sets.push(Set {
                    nom: nom.to_string(),
                    visible: e.get("visible").and_then(J::bool) != Some(false),
                    replie: e.get("replie").and_then(J::bool).unwrap_or(false),
                });
            }
        }
        let mut materiaux = Vec::new();
        if let Some(J::Objet(m)) = p.get("materiaux") {
            for (n, v) in m {
                if !n.is_empty() {
                    materiaux.push((n.clone(), Materiau::lire(v)));
                }
            }
        }
        let actif = sc.get("actif").and_then(J::num).map(|v| v as usize).filter(|&k| k < objets.len()).or(objets.len().checked_sub(1));
        let curseur = sc.get("curseur").and_then(J::nombres::<3>).unwrap_or([0.0; 3]);
        let mut textures = BTreeMap::new();
        if let Some(J::Objet(l)) = p.get("textures") {
            for (id, t) in l {
                if let Some(d) = t.get("donnees").and_then(J::txt) {
                    let nom = t.get("nom").and_then(J::txt).unwrap_or(id).to_string();
                    textures.insert(id.clone(), Texture { nom, octets: Arc::new(crate::maillage::base64(d)) });
                }
            }
        }
        let mut r = Projet { objets, sets, actif, curseur, materiaux, textures };
        r.verifier();
        Ok(r)
    }

    /// `_verifierSets` et `_verifierMateriaux` : chaque objet a son set et son matériau.
    pub fn verifier(&mut self) {
        if self.sets.is_empty() {
            self.sets.push(Set { nom: SET_DEFAUT.into(), visible: true, replie: false });
        }
        let premier = self.sets[0].nom.clone();
        for o in &mut self.objets {
            if o.set.is_empty() {
                o.set = premier.clone();
            }
            if !self.sets.iter().any(|e| e.nom == o.set) {
                self.sets.push(Set { nom: o.set.clone(), visible: true, replie: false });
            }
        }
        if self.materiau(DEFAUT).is_none() {
            self.materiaux.push((DEFAUT.to_string(), Materiau::neuf(None)));
        }
        for k in 0..self.objets.len() {
            let nom = self.objets[k].materiau.clone();
            if self.materiau(&nom).is_none() {
                let t = TEINTES[self.materiaux.len().saturating_sub(1) % TEINTES.len()];
                self.materiaux.push((nom, Materiau::neuf(Some(t))));
            }
        }
    }

    pub fn materiau(&self, nom: &str) -> Option<&Materiau> {
        self.materiaux.iter().find(|(n, _)| n == nom).map(|(_, m)| m)
    }

    pub fn affiche(&self, o: &Objet) -> bool {
        o.visible && self.sets.iter().find(|e| e.nom == o.set).is_none_or(|e| e.visible)
    }

    /// Texte du fichier, présenté comme par GDA. `maillage.l` est écrit lui aussi :
    /// GDA le relit, alors que son enregistrement le perd.
    pub fn ecrire(&self) -> String {
        let t = |v: &str| J::Txt(v.to_string());
        let nums = |v: &[f64]| J::Liste(v.iter().map(|x| J::Num(*x)).collect());
        let c = |k: &str, v: J| (k.to_string(), v);
        let objets = self
            .objets
            .iter()
            .map(|o| {
                let mut d = (*o.maillage).clone();
                for f in &mut d.faces {
                    f.m = std::sync::Arc::from(o.materiau.as_str());
                }
                let mut l = vec![
                    c("nom", t(&o.nom)),
                    c("position", nums(&o.position)),
                    c("rotation", nums(&o.rotation)),
                    c("echelle", nums(&o.echelle)),
                    c("set", t(&o.set)),
                    c("visible", J::Bool(o.visible)),
                    c("modifs", o.modifs.clone()),
                    c("materiau", t(&o.materiau)),
                ];
                if let Some(a) = &o.armature {
                    l.push(c("armature", a.clone()));
                }
                l.push(c("maillage", d.vers_donnees()));
                J::Objet(l)
            })
            .collect();
        let sets = self
            .sets
            .iter()
            .map(|e| J::Objet(vec![c("nom", t(&e.nom)), c("visible", J::Bool(e.visible)), c("replie", J::Bool(e.replie))]))
            .collect();
        let actif = self.actif.map_or(-1.0, |k| k as f64);
        let scene = J::Objet(vec![c("objets", J::Liste(objets)), c("sets", J::Liste(sets)), c("actif", J::Num(actif)), c("curseur", nums(&self.curseur))]);
        let materiaux = J::Objet(self.materiaux.iter().map(|(n, m)| (n.clone(), m.vers_json())).collect());
        // Seules les images encore utilisées : une image n'est jamais écrite deux fois
        let textures = self
            .textures
            .iter()
            .filter(|(id, _)| self.materiaux.iter().any(|(_, m)| m.texture.as_ref() == Some(*id)))
            .map(|(id, x)| (id.clone(), J::Objet(vec![c("nom", t(&x.nom)), c("donnees", J::Txt(texture::vers_base64(&x.octets)))])))
            .collect();
        let p = J::Objet(vec![
            c("format", t("gda")),
            c("version", J::Num(FORMAT)),
            c("application", J::Txt(format!("{SIGNATURE} {}", env!("CARGO_PKG_VERSION")))),
            c("scene", scene),
            c("materiaux", materiaux),
            c("textures", J::Objet(textures)),
        ]);
        let mut o = String::new();
        json::indente(&p, 0, &mut o);
        o
    }

    /// Fichier 3D importé comme scène : un objet par « o » de l'OBJ ou nœud du glTF.
    pub fn depuis_fichier3d(nom: &str, contenu: &[u8]) -> Result<Projet, String> {
        Projet::depuis_fichiers(nom, contenu, &[]).map(|x| x.0)
    }

    /// Texture du contenu `octets`, gardée une seule fois ; rend son identifiant.
    fn garder_texture(&mut self, nom: &str, octets: Vec<u8>) -> String {
        let id = texture::identifiant(&octets);
        self.textures.entry(id.clone()).or_insert_with(|| Texture { nom: nom.to_string(), octets: Arc::new(octets) });
        id
    }

    /// Matériaux et textures du fichier ; rend les images qu'il réclame sans les avoir.
    fn lire_materiaux(&mut self, ext: &str, contenu: &[u8], annexes: &[(String, Vec<u8>)]) -> Vec<String> {
        let mut manquantes = Vec::new();
        if ext == "obj" {
            for (_, t) in annexes.iter().filter(|(n, _)| extension(n) == "mtl") {
                let t = String::from_utf8_lossy(t);
                let mtl = Maillage::depuis_mtl(&t).0;
                self.materiaux.extend(mtl.iter().map(|(n, m)| (n.clone(), Materiau::depuis_mtl(m))));
                for (mat, fichier) in Maillage::textures_mtl(&t) {
                    match annexes.iter().find(|(n, _)| n.eq_ignore_ascii_case(&fichier)) {
                        Some((n, octets)) => {
                            let id = self.garder_texture(n, octets.clone());
                            if let Some((_, m)) = self.materiaux.iter_mut().find(|(n, _)| *n == mat) {
                                m.texture = Some(id);
                                // La couleur multiplie l'image : un Kd absent ou noir la cacherait
                                let sans_kd = mtl.iter().find(|(n, _)| *n == mat).is_none_or(|(_, x)| x.couleur.is_none());
                                if sans_kd || m.couleur == "#000000" {
                                    m.couleur = "#ffffff".into();
                                }
                            }
                        }
                        None => manquantes.push(fichier),
                    }
                }
            }
        } else if matches!(ext, "glb" | "gltf") {
            for m in Maillage::materiaux_gltf(contenu).unwrap_or_default() {
                let mut x = Materiau::neuf(Some(&hex_lineaire(m.couleur)));
                x.opacite = m.couleur[3].clamp(0.0, 1.0);
                x.texture = m.image.map(|(n, o)| self.garder_texture(&n, o));
                if m.manquante {
                    manquantes.push(m.nom.clone());
                }
                self.materiaux.push((m.nom, x));
            }
        }
        // Deux matériaux de même nom : le premier, comme le trouve `materiau`
        let mut vus = std::collections::HashSet::new();
        self.materiaux.retain(|(n, _)| vus.insert(n.clone()));
        manquantes
    }

    /// Un objet n'a qu'un matériau : un objet aux textures diverses se coupe, sans
    /// quoi une seule se verrait.
    fn couper_par_texture(&mut self) {
        let mut l = Vec::with_capacity(self.objets.len());
        for o in std::mem::take(&mut self.objets) {
            let parts = o.maillage.par_materiau();
            let texture = |nom: &str| self.materiau(nom).and_then(|m| m.texture.as_ref());
            if parts.len() < 2 || parts.iter().all(|(n, _)| texture(n).is_none()) {
                l.push(o);
                continue;
            }
            for (mat, m) in parts {
                let mut x = Objet::neuf(&format!("{} {mat}", o.nom), m);
                x.materiau = mat.to_string();
                l.push(x);
            }
        }
        self.objets = l;
    }

    /// Fichier 3D et ses compagnons (.mtl, images). Rend aussi les images manquantes.
    pub fn depuis_fichiers(nom: &str, contenu: &[u8], annexes: &[(String, Vec<u8>)]) -> Result<(Projet, Vec<String>), String> {
        let ext = extension(nom);
        let base = nom.rsplit_once('.').map_or(nom, |x| x.0);
        let objets: Vec<(String, Maillage)> = match ext.as_str() {
            "obj" => Maillage::objets_depuis_obj(&String::from_utf8_lossy(contenu), DEFAUT),
            "glb" | "gltf" => Maillage::objets_depuis_gltf(contenu)?,
            _ => vec![(base.to_string(), Maillage::depuis_fichier(nom, contenu)?)],
        };
        // Un fichier vide ou d'un autre format se lit parfois sans erreur, mais sans rien
        let objets: Vec<_> = objets.into_iter().filter(|(_, m)| !m.sommets.is_empty()).collect();
        if objets.is_empty() {
            return Err("aucun maillage dans ce fichier".into());
        }
        let mut p = Projet { objets: vec![], sets: vec![], actif: Some(0), curseur: [0.0; 3], materiaux: vec![], textures: BTreeMap::new() };
        let manquantes = p.lire_materiaux(&ext, contenu, annexes);
        for (n, m) in objets {
            let mat = m.faces.first().map_or(DEFAUT.to_string(), |f| f.m.to_string());
            let mut o = Objet::neuf(&n, m);
            o.materiau = mat;
            p.objets.push(o);
        }
        p.couper_par_texture();
        p.verifier();
        Ok((p, manquantes))
    }

    pub fn a_des_textures(&self) -> bool {
        self.objets.iter().any(|o| self.texture_de(o).is_some())
    }

    /// Identifiant de la texture de l'objet, si l'image est là.
    pub fn texture_de(&self, o: &Objet) -> Option<&String> {
        self.materiau(&o.materiau)?.texture.as_ref().filter(|id| self.textures.contains_key(*id))
    }
}

/// Couleur linéaire du glTF, en « #rrggbb » sRGB.
fn hex_lineaire(c: [f64; 4]) -> String {
    let s = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        let e = if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
        (e * 255.0 + 0.5) as u8
    };
    format!("#{:02x}{:02x}{:02x}", s(c[0]), s(c[1]), s(c[2]))
}

#[cfg(test)]
pub mod tests_textures {
    use super::*;
    use crate::texture::tests::png_de;

    pub fn image(n: u32) -> Vec<u8> {
        png_de(n, n, |x, y| [x as u8 * 40, y as u8 * 40, 7, 255])
    }

    /// GLB minimal : deux triangles, chacun son matériau, le premier texturé.
    pub fn glb(image: &[u8]) -> Vec<u8> {
        let mut bin: Vec<u8> = Vec::new();
        let pos: [f32; 18] = [0., 0., 0., 1., 0., 0., 0., 1., 0., 2., 0., 0., 3., 0., 0., 2., 1., 0.];
        let uv: [f32; 6] = [0., 0., 1., 0., 0., 1.];
        pos.iter().chain(&uv).for_each(|v| bin.extend(v.to_le_bytes()));
        let fin = bin.len();
        bin.extend(image);
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"n"}}],
"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"material":0}},{{"attributes":{{"POSITION":1}},"material":1}}]}}],
"materials":[{{"name":"bois","pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}},{{"name":"rouge","pbrMetallicRoughness":{{"baseColorFactor":[1,0,0,0.5]}}}}],
"textures":[{{"source":0}}],"images":[{{"bufferView":3,"mimeType":"image/png"}}],
"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}],
"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":24}},{{"buffer":0,"byteOffset":{fin},"byteLength":{}}}],
"buffers":[{{"byteLength":{}}}]}}"#,
            image.len(),
            bin.len()
        );
        let mut j = json.into_bytes();
        while j.len() % 4 != 0 {
            j.push(b' ');
        }
        let mut o = Vec::new();
        o.extend(b"glTF");
        o.extend(2u32.to_le_bytes());
        o.extend(((12 + 8 + j.len() + 8 + bin.len()) as u32).to_le_bytes());
        o.extend((j.len() as u32).to_le_bytes());
        o.extend(b"JSON");
        o.extend(&j);
        o.extend((bin.len() as u32).to_le_bytes());
        o.extend(b"BIN\0");
        o.extend(&bin);
        o
    }

    const OBJ: &[u8] = b"mtllib a.mtl\no piece\nv 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl bois\nf 1/1 2/2 3/3\nusemtl acier\nf 2 4 3\n";
    const MTL: &[u8] = b"newmtl bois\nmap_Kd tex/bois.png\nnewmtl acier\nKd 0.5 0.5 0.5\n";

    fn annexes() -> Vec<(String, Vec<u8>)> {
        vec![("a.mtl".to_string(), MTL.to_vec()), ("BOIS.PNG".to_string(), image(2))]
    }

    #[test]
    fn un_glb_texture_garde_uv_image_couleur_et_coupe_par_materiau() {
        let (p, manquantes) = Projet::depuis_fichiers("x.glb", &glb(&image(2)), &[]).unwrap();
        assert!(manquantes.is_empty());
        assert_eq!(p.objets.len(), 2);
        let bois = p.objets.iter().find(|o| o.materiau == "bois").unwrap();
        assert_eq!(bois.maillage.faces[0].uv.len(), 3);
        assert!(p.texture_de(bois).is_some());
        let rouge = p.materiau("rouge").unwrap();
        assert_eq!((rouge.couleur.as_str(), rouge.opacite), ("#ff0000", 0.5));
        assert_eq!(p.textures.len(), 1);
    }

    #[test]
    fn un_obj_et_ses_compagnons() {
        let (p, manquantes) = Projet::depuis_fichiers("a.obj", OBJ, &annexes()).unwrap();
        assert!(manquantes.is_empty());
        assert_eq!(p.objets.len(), 2, "un objet par matériau, l'un étant texturé");
        let bois = p.materiau("bois").unwrap();
        assert!(bois.texture.is_some());
        assert_eq!(bois.couleur, "#ffffff", "sans Kd, l'image n'est pas teintée");
        assert_eq!(p.materiau("acier").unwrap().couleur, "#808080");
    }

    #[test]
    fn une_image_manquante_est_dite_et_l_objet_reste() {
        let (p, manquantes) = Projet::depuis_fichiers("a.obj", OBJ, &annexes()[..1]).unwrap();
        assert_eq!(manquantes, vec!["bois.png"]);
        assert!(p.textures.is_empty());
        assert!(!p.a_des_textures());
        assert!(!p.objets.is_empty());
    }

    #[test]
    fn le_gda_rend_uv_et_textures_et_oublie_les_images_sans_usage() {
        let (mut p, _) = Projet::depuis_fichiers("a.obj", OBJ, &annexes()).unwrap();
        p.textures.insert("orpheline".into(), Texture { nom: "o".into(), octets: Arc::new(vec![1, 2]) });
        let t = p.ecrire();
        let q = Projet::lire(&t).unwrap();
        assert_eq!(q.textures.len(), 1);
        assert!(!q.textures.contains_key("orpheline"));
        assert_eq!(q.objets[0].maillage.faces[0].uv, p.objets[0].maillage.faces[0].uv);
        assert!(q.a_des_textures());
    }

    #[test]
    fn fusionner_distingue_deux_materiaux_de_meme_nom_aux_images_differentes() {
        let (mut a, _) = Projet::depuis_fichiers("x.glb", &glb(&image(2)), &[]).unwrap();
        let (b, _) = Projet::depuis_fichiers("y.glb", &glb(&image(4)), &[]).unwrap();
        let set = a.objets[0].set.clone();
        a.fusionner(b, &set);
        assert_eq!(a.textures.len(), 2);
        let ids: std::collections::HashSet<_> = a.objets.iter().filter_map(|o| a.texture_de(o)).collect();
        assert_eq!(ids.len(), 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusionner_garde_noms_materiaux_et_cibles_distincts() {
        let mut a = Projet::neuf();
        a.materiaux.push(("bois".into(), Materiau::neuf(Some("#112233"))));
        let t = r##"{"format":"gda","version":9,"materiaux":{"bois":{"couleur":"#445566"},"inutile":{"couleur":"#778899"}},"scene":{"objets":[
          {"nom":"cube","materiau":"bois","maillage":{"v":[[0,0,0],[1,0,0],[0,1,0]],"f":[["bois",0,1,2]]},"modifs":[{"type":"booleen","op":"union","cible":"cube"}]}]}}"##;
        let l = a.fusionner(Projet::lire(t).unwrap(), SET_DEFAUT);
        assert_eq!(l, [1]);
        let o = &a.objets[1];
        assert_eq!((o.nom.as_str(), o.materiau.as_str(), &*o.maillage.faces[0].m), ("cube 2", "bois 2", "bois 2"));
        assert_eq!(a.materiau("bois 2").unwrap().couleur, "#445566");
        assert_eq!(a.materiau("bois").unwrap().couleur, "#112233");
        assert!(a.materiau("inutile").is_none());
        let J::Liste(l) = &o.modifs else { panic!() };
        assert_eq!(l[0].get("cible"), Some(&J::Txt("cube 2".into())));
    }

    #[test]
    fn fusionner_reprend_un_materiau_identique() {
        let mut a = Projet::neuf();
        let n = a.materiaux.len();
        a.fusionner(Projet::neuf(), SET_DEFAUT);
        assert_eq!(a.materiaux.len(), n);
        assert_eq!(a.objets[1].nom, "cube 2");
    }

    #[test]
    fn refus_lisibles() {
        assert_eq!(Projet::lire("x").unwrap_err(), "fichier .gda illisible");
        assert_eq!(Projet::lire(r#"{"format":"autre"}"#).unwrap_err(), "ce n’est pas un projet GDA");
        assert!(Projet::lire(r#"{"format":"gda","version":99,"scene":{"objets":[]}}"#).unwrap_err().contains("plus récent"));
        assert_eq!(Projet::lire(r#"{"format":"gda","version":3,"scene":{}}"#).unwrap_err(), "projet sans objets");
    }

    #[test]
    fn un_ancien_format_prend_les_valeurs_par_defaut() {
        let t = r#"{"format":"gda","version":1,"scene":{"objets":[{"maillage":{"v":[[0,0,0],[1,0,0],[0,1,0]],"f":[["bois",0,1,2]]}}]}}"#;
        let p = Projet::lire(t).unwrap();
        let o = &p.objets[0];
        assert_eq!((o.nom.as_str(), o.materiau.as_str(), o.echelle, o.set.as_str()), ("objet", "bois", [1.0; 3], SET_DEFAUT));
        assert_eq!(p.materiau("bois").unwrap().couleur, TEINTES[0]);
    }

    #[test]
    fn enregistrer_puis_relire_ne_change_rien() {
        let mut p = Projet::neuf();
        p.objets[0].position = [1.5, -2.0, 0.25];
        p.objets[0].modifs = json::lire(r#"[{"type":"repetition","n":3}]"#).unwrap();
        assert_eq!(Projet::lire(&p.ecrire()).unwrap(), p);
    }

    #[test]
    fn un_set_inconnu_est_cree() {
        let t = r#"{"format":"gda","version":9,"scene":{"objets":[{"set":"a","maillage":{"v":[],"f":[]}}],"sets":[{"nom":"b"},{"nom":"b"}]}}"#;
        let p = Projet::lire(t).unwrap();
        let noms: Vec<&str> = p.sets.iter().map(|e| e.nom.as_str()).collect();
        assert_eq!(noms, ["b", "a"]);
    }
}
