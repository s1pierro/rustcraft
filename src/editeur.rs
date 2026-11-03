//! Contexte d'édition 3D : la scène d'un projet, sa caméra, ses panneaux.
//!
//! Modes Scène et Géométrie : poignées, transformations, annulation, opérations,
//! enregistrement et export. Les calculs sont dans `edition` et `geometrie`.

use crate::theme::{self, coul, C};
use crate::icones::{self, Icone};
use crate::draw::{Color, Rect};
use crate::armature::{Armature, Bout, Ecart, EditionOs, Limite, Os, Pose, Segments, Touche};
use crate::actions::{self, Action as A, Choix, Contexte, Groupe, Menu, CATALOGUE, GROUPES};
use crate::edition::{self as ed, Etape, Geste, Historique, Mode, Pas, Pivot, Poignee, Prise, Transfo, Vue, PIVOTS};
use crate::geometrie::{self as geo, Bilan, El, RegleRevolution, Travail, T};
use crate::json::J;
use crate::maillage::{self, Arete, Boite, Cible, Maillage, P3};
use crate::math::{self, Mat4};
use crate::modifs;
use crate::peau;
use crate::primitives::{self, Genre, Prim, Valeurs, V};
use crate::projet::{Materiau, Objet, Projet, DEFAUT};
use crate::shell::{Layout, Shell, Side};
use crate::texture::{self, Image};
use crate::uv::{self, Aspect, Coin, Uv};
use crate::ui::{id, sub_id, Id, Sense};
use crate::vue::{self, Tampons};
use crate::widgets::{Cx, Icon, Scrolls};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

const OBJETS: Id = id("objets");
const BARRE_RAPIDE: Id = id("barre-rapide");
const NOM_PROJET: Id = id("nom-projet");
const FICHIERS: Id = id("pastille-fichier");
const SCENE: Id = id("pastille-scene");
const CONTEXTE: Id = id("pastille-contexte");
const OS: Id = id("pastille-os");
const TOILE_UV: Id = id("toile-uv");
const MODIFIEURS: Id = id("pastille-modifieurs");
const REGLAGES: Id = id("reglages");
const AIDE: Id = id("aide");
const ANNULER: Id = id("annuler");
const RETABLIR: Id = id("retablir");
const BARRE: Id = id("barre");
const PAVE: Id = id("pave");

/// Bibliothèque annoncée par les OBJ exportés.
const MTL: &str = "materiaux.mtl";
const MIME: &str = "application/octet-stream";
/// Onglets du panneau de réglages.
const ONGLETS: [&str; 2] = ["Couleurs", "Barre rapide"];
/// Ce que montre le panneau de gauche.
#[derive(Clone, Debug, PartialEq)]
pub enum PageListe {
    Objets,
    /// Réglages d'un set.
    Set(String),
    /// Choix d'un set pour ces objets.
    Deplacer(Vec<usize>),
}

const ONGLET_COULEURS: usize = 0;
const ONGLET_MATERIAUX: usize = 1;
/// Valeurs d'`illum` du format Wavefront, comme dans GDA.
const ILLUM: [&str; 11] = ["Couleur seule, sans éclairage", "Diffuse et ambiante", "Avec reflets spéculaires", "Reflets et réflexion (lancer de rayons)", "Verre : transparence et réflexion", "Réflexion de Fresnel", "Réfraction", "Réfraction et Fresnel", "Réflexion sans lancer de rayons", "Verre sans lancer de rayons", "Ombres portées sur des surfaces invisibles"];
/// Couleurs d'un matériau : libellé, et si elle compte à l'écran.
const COULEURS_MAT: [(&str, bool); 4] = [("Diffuse", true), ("Émission", true), ("Ambiante", false), ("Spéculaire", false)];
const ONGLET_DERNIERE: usize = 1;

const COUL_AXES: [C; 3] = [C::AxeX, C::AxeY, C::AxeZ];
const NOMS_AXES: [&str; 3] = ["X", "Y", "Z"];
const NOMS_PIVOTS: [&str; 5] = ["Médian", "Boîte", "Actif", "Curseur", "Origine"];
const ICONES_PIVOTS: [&Icone; 5] = [&icones::PIVOT_MEDIAN, &icones::PIVOT_BOITE, &icones::PIVOT_ACTIF, &icones::PIVOT_CURSEUR, &icones::PIVOT_ORIGINE];
const NOMS_FILTRES: [&str; 3] = ["Sommets", "Arêtes", "Faces"];
const ICONES_FILTRES: [&Icone; 3] = [&icones::PIVOT_ACTIF, &icones::ARETES, &icones::FACES];
const TYPES: [T; 3] = [T::S, T::A, T::F];
const AXES_REVOLUTION: [&str; 4] = ["X", "Y", "Z", "Vue"];

/// Tampons de la copie de travail : faces ordinaires, faces choisies. Une adresse de
/// maillage ne vaut jamais 1 ou 2.
const CLE_FACES: usize = 1;
const CLE_CHOISIES: usize = 2;
/// Numéros d'ordre affichés au plus sur les sommets choisis.
const MAX_NUMEROS: usize = 24;
/// Opacité des objets non édités, et des faces en Rayons X.
const OPACITE_PASSIF: f32 = 0.4;
const AUCUN_PARENT: &str = "— aucun —";
const OPACITE_RAYONS: f32 = 0.45;

/// Champ de vision vertical de GDA, en degrés.
const CHAMP: f64 = 50.0;
const LAMPE: P3 = [-0.35, 0.45, 1.0];
const SOLEIL: P3 = [0.4, 1.0, 0.8];
const ANIM_CAMERA: f32 = 0.4;
const DUREE_MESSAGE: f32 = 4.0;


/// Part de sRGB 0..1 → linéaire.
fn lineaire(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn lin_octets(c: Color) -> [f32; 3] {
    [lineaire(c[0] as f32 / 255.0), lineaire(c[1] as f32 / 255.0), lineaire(c[2] as f32 / 255.0)]
}

fn lin_hex(h: &str) -> [f32; 3] {
    maillage::rvb(h).map(|c| lineaire(c as f32))
}

/// Nom montré d'un matériau.
fn nom_mat(n: &str) -> &str {
    if n == DEFAUT { "défaut" } else { n }
}

fn couleur_mat(m: &Materiau, k: usize) -> &str {
    [&m.couleur, &m.emission, &m.ambiante, &m.speculaire][k]
}

fn couleur_mat_mut(m: &mut Materiau, k: usize) -> &mut String {
    match k {
        0 => &mut m.couleur,
        1 => &mut m.emission,
        2 => &mut m.ambiante,
        _ => &mut m.speculaire,
    }
}

fn octets_hex(h: &str) -> Color {
    let c = maillage::rvb(h);
    [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rendu {
    Solide,
    Jeu,
}

/// Un objet à dessiner ; `tampon` nomme les tampons de son maillage.
#[derive(Clone, Debug)]
pub struct ObjetVue {
    pub tampon: usize,
    pub modele: Mat4,
    pub normale: [f32; 9],
    pub albedo: [f32; 3],
    pub emission: [f32; 3],
    pub opacite: f32,
    pub contour: bool,
    /// Cible d'un booléen : son seul fil de fer.
    pub filaire: bool,
    /// Identifiant d'une image de `Scene::images`, chargée maintenant ou avant.
    pub texture: Option<String>,
    /// Couleur de l'envers, s'il se voit.
    pub envers: Option<[f32; 3]>,
    /// Dessiné après les autres, à travers eux : une armature au premier plan.
    pub devant: bool,
}

/// Fil, points et arêtes choisies de la copie de travail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeoTampons {
    pub lignes: Vec<f32>,
    pub points: Vec<f32>,
    pub choisis: Vec<f32>,
    /// Bandes : position, autre bout, côté.
    pub bandes: Vec<f32>,
    /// Bandes des coutures.
    pub coutures: Vec<f32>,
}

pub struct GeoVue {
    pub maj: Option<GeoTampons>,
    pub rayons: bool,
}

/// Couleurs de la vue 3D, de 0 à 1, telles que le moteur de rendu les affiche.
pub struct CouleursVue {
    pub fond: [f32; 3],
    pub fond_centre: [f32; 3],
    pub fond_bord: [f32; 3],
    pub grille: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub arete: [f32; 3],
    pub couture: [f32; 3],
    pub sommet: [f32; 3],
    pub arete_choisie: [f32; 3],
    pub sommet_choisi: [f32; 3],
}

impl CouleursVue {
    fn maintenant() -> CouleursVue {
        let f = |c: C| { let v = coul(c); [0, 1, 2].map(|k| v[k] as f32 / 255.0) };
        CouleursVue {
            fond: f(C::Fond),
            fond_centre: f(C::FondCentre),
            fond_bord: f(C::FondBord),
            grille: f(C::Grille),
            axes: COUL_AXES.map(f),
            arete: f(C::Arete),
            couture: f(C::Couture),
            sommet: f(C::Sommet),
            arete_choisie: f(C::AreteChoisie),
            sommet_choisi: f(C::SommetChoisi),
        }
    }
}

/// Ce que le moteur de rendu doit dessiner dans l'espace de travail.
pub struct Scene {
    pub workspace: Rect,
    pub vue_proj: Mat4,
    pub oeil: [f32; 3],
    pub vue: Mat4,
    pub lumiere: [f32; 3],
    pub objets: Vec<ObjetVue>,
    pub pas_grille: f32,
    pub largeur_contour: f32,
    pub contour: [f32; 3],
    pub couleurs: CouleursVue,
    /// Maillages nouveaux pour le processeur graphique, et ceux à oublier.
    pub ajouts: Vec<(usize, Tampons)>,
    pub retraits: Vec<usize>,
    /// Images nouvelles pour le processeur graphique, et celles à oublier.
    pub images: Vec<(String, Image)>,
    pub images_retraits: Vec<String>,
    pub geo: Option<GeoVue>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selection {
    Toucher,
    Boite,
    Chemin,
}

/// Dernière opération, réglable : elle se rejoue sur l'état d'avant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Derniere {
    Subdivision { coupes: usize },
    Boucle { arete: Arete, coupes: usize, glissement: f64, uniforme: bool, inverser: bool },
    Fusion { cible: Cible },
    Revolution { axe: usize, vue: P3, angle: f64, inverse: bool, segments: usize, fermer: bool },
    Proches { tolerance: f64 },
    Translation { d: P3 },
    /// `axe` : 0 à 2, ou 3 pour l'axe de la vue, gardé dans `vue`.
    Rotation { degres: f64, axe: usize, vue: Option<P3>, pivot: P3 },
    Echelle { s: P3, pivot: P3 },
    /// Faces cibles relues à chaque rejeu : la sélection n'a pas changé.
    Projection { p: uv::Projection, aspect: Aspect },
    /// Comme la projection : les faces de la vue UV, relues à chaque rejeu.
    MiseEnPage { page: uv::Page, aspect: Aspect },
    Depliage { page: uv::Page },
}

impl Derniere {
    /// Ce qu'un glissé a fait, s'il a fait quelque chose.
    fn du_geste(g: &Geste, t: Transfo) -> Option<Derniere> {
        match t {
            Transfo::Translation(d) => (d != [0.0; 3]).then_some(Derniere::Translation { d }),
            Transfo::Echelle { s, c } => (s != [1.0; 3]).then_some(Derniere::Echelle { s, pivot: c }),
            Transfo::Rotation { c, .. } => {
                let (u, theta) = g.rotation();
                let degres = theta.to_degrees();
                (theta != 0.0).then(|| match (0..3).find(|k| (u[*k].abs() - 1.0).abs() < 1e-9) {
                    // Axe du monde : le signe passe dans l'angle
                    Some(k) => Derniere::Rotation { degres: degres * u[k].signum(), axe: k, vue: None, pivot: c },
                    None => Derniere::Rotation { degres, axe: 3, vue: Some(u), pivot: c },
                })
            }
        }
    }

    fn titre(self) -> &'static str {
        match self {
            Derniere::Subdivision { .. } => "Subdivision",
            Derniere::Boucle { .. } => "Boucle de coupe",
            Derniere::Fusion { .. } => "Fusion",
            Derniere::Revolution { .. } => "Révolution",
            Derniere::Proches { .. } => "Fusion des confondus",
            Derniere::Translation { .. } => "Translation",
            Derniere::Rotation { .. } => "Rotation",
            Derniere::Echelle { .. } => "Échelle",
            Derniere::Projection { .. } => "Projection UV",
            Derniere::MiseEnPage { .. } => "Mise en page UV",
            Derniere::Depliage { .. } => "Dépliage UV",
        }
    }
}

/// Fichier à déposer hors de l'application.
#[derive(Clone, Debug, PartialEq)]
pub struct Sortie {
    pub nom: String,
    pub mime: &'static str,
    pub octets: Vec<u8>,
}

/// Valeur que le pavé numérique modifie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Champ {
    Position(usize),
    Rotation(usize),
    Dimension(usize),
    PasTranslation,
    PasAngle,
    PasEchelle,
    Coupes,
    Glissement,
    Angle,
    Segments,
    Tolerance,
    Couleur(usize),
    Decalage(usize),
    Facteur(usize),
    Pivot(usize),
    Nom,
    Projet,
    MatNom,
    SetNom,
    OsNom,
    OsLongueur,
    /// Borne d'une limite de rotation de l'os actif : son axe, et vrai pour le maximum.
    OsLimite(usize, bool),
    /// Nom de la pose choisie dans le catalogue.
    PoseNom,
    /// Code d'une couleur du matériau de l'objet actif.
    MatCode(usize),
    /// Champ du panneau d'ajout : son rang et son libellé.
    Primitive(usize, &'static str),
    /// Nombre de copies d'une répétition.
    ModifN(usize),
    ModifDecalage(usize, usize),
}

impl Champ {
    fn titre(self) -> String {
        match self {
            Champ::Primitive(_, l) => l.to_string(),
            Champ::ModifN(_) => "Nombre de copies".into(),
            Champ::ModifDecalage(_, k) => format!("Décalage {}", NOMS_AXES[k]),
            Champ::Position(k) => format!("Position {}", NOMS_AXES[k]),
            Champ::Rotation(k) => format!("Rotation {}", NOMS_AXES[k]),
            Champ::Dimension(k) => format!("Dimensions {}", NOMS_AXES[k]),
            Champ::PasTranslation => "Pas : translation".into(),
            Champ::PasAngle => "Pas : angle".into(),
            Champ::PasEchelle => "Pas : échelle".into(),
            Champ::Coupes => "Coupes".into(),
            Champ::Glissement => "Glissement (-1 à 1)".into(),
            Champ::Angle => "Angle (degrés)".into(),
            Champ::Segments => "Segments".into(),
            Champ::Tolerance => "Tolérance".into(),
            Champ::Couleur(i) => format!("{} (#rrggbb)", theme::LIGNES[i].libelle),
            Champ::Decalage(k) => format!("Décalage {}", NOMS_AXES[k]),
            Champ::Facteur(k) => format!("Facteur {}", NOMS_AXES[k]),
            Champ::Pivot(k) => format!("Pivot {}", NOMS_AXES[k]),
            Champ::Nom => "Nom de l'objet".into(),
            Champ::MatNom => "Nom du matériau".into(),
            Champ::SetNom => "Nom du set".into(),
            Champ::OsNom => "Nom de l'os".into(),
            Champ::PoseNom => "Nom de la pose".into(),
            Champ::OsLongueur => "Longueur de l'os".into(),
            Champ::OsLimite(k, max) => format!("{} {}", if max { "Maximum" } else { "Minimum" }, NOMS_AXES[k]),
            Champ::MatCode(k) => format!("{} (#rrggbb)", COULEURS_MAT[k].0),
            Champ::Projet => "Nom du projet".into(),
        }
    }

    /// Champ qui accepte du texte, pas seulement un nombre.
    fn libre(self) -> bool {
        matches!(self, Champ::Nom | Champ::Projet | Champ::Couleur(_) | Champ::MatNom | Champ::MatCode(_) | Champ::SetNom | Champ::OsNom | Champ::PoseNom)
    }
}

/// Touches du mode alphanumérique, avec leur largeur relative.
const CLAVIER: [&[(&str, f32)]; 4] = [
    &[("a", 1.0), ("z", 1.0), ("e", 1.0), ("r", 1.0), ("t", 1.0), ("y", 1.0), ("u", 1.0), ("i", 1.0), ("o", 1.0), ("p", 1.0)],
    &[("q", 1.0), ("s", 1.0), ("d", 1.0), ("f", 1.0), ("g", 1.0), ("h", 1.0), ("j", 1.0), ("k", 1.0), ("l", 1.0), ("m", 1.0)],
    &[("Maj", 2.0), ("w", 1.0), ("x", 1.0), ("c", 1.0), ("v", 1.0), ("b", 1.0), ("n", 1.0), ("'", 1.0), ("-", 1.0)],
    &[("é", 1.0), ("è", 1.0), ("ê", 1.0), ("à", 1.0), ("ç", 1.0), ("ù", 1.0), ("Espace", 2.0), ("_", 1.0), (".", 1.0)],
];

struct Pave {
    texte: String,
    /// Valeur reprise du champ : inchangée, elle ne s'applique pas.
    depart: String,
    /// Valeur tout juste reprise : un chiffre la remplace, comme une sélection.
    neuf: bool,
    /// Position du curseur, en caractères.
    curseur: usize,
    cible: Champ,
    /// Mode alphanumérique affiché, et majuscule pour la prochaine lettre.
    alpha: bool,
    maj: bool,
}

impl Pave {
    fn remplacer_si_neuf(&mut self) {
        if std::mem::take(&mut self.neuf) {
            self.texte.clear();
            self.curseur = 0;
        }
    }

    /// Rang en octets du caractère `n`.
    fn octet(&self, n: usize) -> usize {
        self.texte.char_indices().nth(n).map_or(self.texte.len(), |c| c.0)
    }

    fn inserer(&mut self, s: &str) {
        let o = self.octet(self.curseur);
        self.texte.insert_str(o, s);
        self.curseur += s.chars().count();
    }

    fn touche(&mut self, k: &str) {
        self.curseur = self.curseur.min(self.texte.chars().count());
        match k {
            "Maj" => self.maj = !self.maj,
            "Gauche" => {
                self.neuf = false;
                self.curseur = self.curseur.saturating_sub(1);
            }
            "Droite" => {
                self.neuf = false;
                self.curseur = (self.curseur + 1).min(self.texte.chars().count());
            }
            "Effacer" => {
                self.neuf = false;
                if self.curseur > 0 {
                    self.curseur -= 1;
                    let o = self.octet(self.curseur);
                    self.texte.remove(o);
                }
            }
            "Espace" => {
                self.remplacer_si_neuf();
                self.inserer(" ");
            }
            k if self.cible.libre() => {
                self.remplacer_si_neuf();
                if std::mem::take(&mut self.maj) {
                    self.inserer(&k.to_uppercase());
                } else {
                    self.inserer(k);
                }
            }
            // Le signe bascule, où que soit le curseur, sans perdre la valeur
            "-" => {
                self.neuf = false;
                if self.texte.starts_with('-') {
                    self.texte.remove(0);
                    self.curseur = self.curseur.saturating_sub(1);
                } else {
                    self.texte.insert(0, '-');
                    self.curseur += 1;
                }
            }
            "," => {
                self.remplacer_si_neuf();
                if !self.texte.contains(',') {
                    self.apres_le_signe();
                    let vide = self.texte[..self.octet(self.curseur)].trim_start_matches('-').is_empty();
                    self.inserer(if vide { "0," } else { "," });
                }
            }
            k => {
                self.remplacer_si_neuf();
                self.apres_le_signe();
                self.inserer(k);
            }
        }
    }

    /// Un chiffre ne s'écrit pas devant le signe : ce ne serait plus un nombre.
    fn apres_le_signe(&mut self) {
        if self.curseur == 0 && self.texte.starts_with('-') {
            self.curseur = 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Camera {
    cible: P3,
    distance: f64,
    lacet: f64,
    hauteur: f64,
}

#[derive(Clone, Copy)]
struct Vol {
    depart: Camera,
    arrivee: Camera,
    t: f32,
}

/// Une description du catalogue, en phrase.
fn phrase(aide: &str) -> String {
    let mut c = aide.chars();
    c.next().map_or(String::new(), |p| p.to_uppercase().chain(c).chain(['.']).collect())
}

fn t_edite(t: &Option<Travail>, k: usize) -> bool {
    t.as_ref().is_some_and(|t| t.edites.contains(&k))
}

/// Pilule d'une rangée.
struct Pilule {
    cle: Id,
    nom: String,
    l: f32,
    actif: bool,
    couleur: Color,
    /// Écoute l'appui long, ce qui retire l'appui bref au-delà du délai.
    long: bool,
    /// Pictogramme à la place du nom, ou avant lui avec `nomme`.
    icone: Option<&'static Icone>,
    nomme: bool,
}

impl Pilule {
    fn new(cx: &mut Cx, cle: Id, nom: &str, actif: bool) -> Pilule {
        Pilule { cle, nom: nom.to_string(), l: cx.pill_width(nom), actif, couleur: coul(C::Accent), long: false, icone: None, nomme: false }
    }

    /// Pilule à pictogramme : `nom` ne sert qu'à la reconnaître.
    fn icone(cx: &Cx, cle: Id, nom: &str, icone: &'static Icone, actif: bool) -> Pilule {
        Pilule { cle, nom: nom.to_string(), l: 50.0 * cx.dp, actif, couleur: coul(C::Accent), long: false, icone: Some(icone), nomme: false }
    }

    /// Pictogramme et nom ; le nom se tait, sur écran étroit, si la pilule n'est pas active.
    fn nommee(cx: &mut Cx, cle: Id, nom: &str, icone: &'static Icone, actif: bool, etroit: bool) -> Pilule {
        let mut p = Pilule::icone(cx, cle, nom, icone, actif);
        if actif || !etroit {
            p.nomme = true;
            p.l = cx.pill_icone_width(nom);
        }
        p
    }
}

/// Ce que fera le fichier choisi dans le sélecteur d'Android. La valeur est le code de
/// demande qui aller-retourne par `Activite.java`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Selecteur {
    Ouvrir = 1,
    Importer = 2,
    /// Une image pour un matériau.
    Image = 3,
}

/// Choix en cours de la page « Lier à une armature ». Les objets sont ceux de la sélection.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLiaison {
    pub armature: Option<String>,
    pub os: Vec<String>,
}

/// Ce qu'on touche dans l'arbre des armatures.
enum ClicArbre {
    Armature(usize),
    Os(usize, String),
    Repos,
    /// Appui long : toute l'armature, ou un os et ses descendants.
    Tout(usize, Option<String>),
}

/// Ce que montre le panneau de fin.
#[derive(Clone, Copy, PartialEq)]
pub enum Contenu {
    Contexte,
    Fichier,
    Reglages,
    Aide,
    /// Rendu, vue et ajouts à la scène.
    Scene,
    /// Réglages d'une primitive en cours d'ajout.
    Ajout,
    /// Modifieurs de l'objet actif.
    Modifieurs,
    /// Vue UV de la copie de travail, dans le panneau élargi.
    Uv,
    /// Os de l'armature éditée.
    Os,
}

#[derive(Clone, Copy, PartialEq)]
enum OutilUv {
    Deplacer,
    Tourner,
    Echelle,
}

/// Geste en cours sur la vue UV.
enum PriseUv {
    Boite { fin: (f32, f32) },
    Transfo { departs: Vec<(Coin, Uv)>, pivot: Uv, dernier: Option<uv::Geste> },
}

/// État de la vue UV. La sélection est à elle : les faces choisies en 3D disent ce
/// qu'on y voit, les coins choisis ici disent ce qu'on y déplace.
struct EditeurUv {
    sel: HashSet<Coin>,
    ilots: bool,
    /// Les coins soudés bougent ensemble.
    lies: bool,
    outil: OutilUv,
    axes: [bool; 2],
    /// Origine à l'écran et pixels par unité ; `None` : la tuile cadrée.
    cadre: Option<[f32; 3]>,
    /// Cadre et forme de la dernière image.
    vu: [f32; 3],
    aspect: Aspect,
    prise: Option<PriseUv>,
    /// Forme de chaque texture, connue à son décodage.
    aspects: HashMap<String, Aspect>,
    /// Texture que le modèle montre.
    texture: Option<String>,
    page: uv::Page,
}

impl Default for EditeurUv {
    fn default() -> EditeurUv {
        EditeurUv { page: uv::Page::DEFAUT, sel: HashSet::new(), ilots: false, lies: true, outil: OutilUv::Deplacer, axes: [true; 2], cadre: None, vu: [0.0, 0.0, 1.0], aspect: Aspect::CARRE, prise: None, aspects: HashMap::new(), texture: None }
    }
}

impl EditeurUv {
    fn ecran(&self, q: Uv) -> [f32; 2] {
        let c = self.vu;
        [c[0] + q[0] * self.aspect.l * c[2], c[1] + q[1] * self.aspect.h * c[2]]
    }

    fn vers_uv(&self, x: f32, y: f32) -> Uv {
        let c = self.vu;
        [(x - c[0]) / (self.aspect.l * c[2]), (y - c[1]) / (self.aspect.h * c[2])]
    }
}

/// Un trente-deuxième de la tuile : le pas de translation de la vue UV.
const PAS_UV: f32 = 1.0 / 32.0;

/// Primitive en cours d'ajout : ses réglages et son aperçu, posé sur le curseur 3D.
struct Ajout {
    prim: Prim,
    valeurs: Valeurs,
    apercu: Option<Arc<Maillage>>,
    /// Ligne d'information, et si elle grise Ajouter.
    info: Option<(String, bool)>,
    /// Calcul long en cours.
    calcul: Option<Receiver<Result<Maillage, String>>>,
    cadre: bool,
}

/// Action sur un modifieur de l'objet actif.
enum CmdModif {
    Monter(usize),
    Descendre(usize),
    Appliquer(usize),
    Supprimer(usize),
    Fixer(usize, &'static str, J),
}

pub struct App {
    pub shell: Shell,
    pub projet: Projet,
    pub nom: String,
    pub rendu: Rendu,
    cam: Camera,
    vol: Option<Vol>,
    choix: Vec<usize>,
    pub contenu: Contenu,
    pub onglet: usize,
    pub onglet_contexte: usize,
    pub onglet_fichier: usize,
    pub couleurs: theme::Perso,
    /// Couleur dépliée, et ses teinte, saturation et luminosité : une couleur grise
    /// n'a plus de teinte, le curseur doit pourtant la garder.
    pub couleur_ouverte: Option<(usize, [f32; 3])>,
    couleurs_changees: bool,
    pub onglet_aide: usize,
    /// Opération dont l'aide est dépliée.
    deplie: Option<A>,
    scrolls: Scrolls,
    message: Option<(String, f32)>,
    /// Taille de l'espace de travail, pour cadrer.
    espace: Rect,
    pub mode: Mode,
    axes: [bool; 3],
    pas: Pas,
    /// Pas de translation du glissé en cours : la grille ne doit pas changer sous le doigt.
    pas_t: f64,
    pivot: usize,
    histoire: Historique,
    geste: Option<Geste>,
    pave: Option<Pave>,
    /// Place du pavé dans la marge de l'écran, de 0 à 1 sur chaque axe : elle survit
    /// à une rotation. Gardée dans les réglages.
    pave_place: [f32; 2],
    /// Doigt sur la barre du pavé : son écart au coin du pavé.
    pave_prise: Option<(f32, f32)>,
    /// Maillages déjà envoyés au rendu, gardés vivants : leur adresse leur sert de nom.
    connus: HashMap<usize, Arc<Maillage>>,
    /// Maillages modifiés, et le calcul en cours.
    evals: modifs::Cache,
    calcul: Option<Receiver<Vec<(String, modifs::Ev)>>>,
    ajout: Option<Ajout>,
    uv: EditeurUv,
    pub page_liste: PageListe,
    /// Onglet de la pastille Scène.
    pub onglet_scene: usize,
    /// Couleur de matériau dépliée : matériau, couleur, teinte.
    mat_couleur: Option<(String, usize, [f32; 3])>,
    /// Glissé en cours sur un réglage : une seule étape d'annulation.
    mat_saisie: bool,
    /// Matériau qui attend l'image du sélecteur.
    image_pour: Option<String>,
    /// Départ de la boîte en cours dans la vue UV.
    boite_uv: Option<(f32, f32)>,
    /// Derniers réglages de chaque primitive, repris à l'ajout suivant.
    primitives: HashMap<Prim, Valeurs>,
    /// Images déjà passées au processeur graphique, ou illisibles (dit une seule fois).
    images_vues: HashSet<String>,
    sorties: Vec<Sortie>,
    selecteur: Option<Selecteur>,
    a_sauver: bool,
    /// Copie de travail : présente, on est en mode Géométrie.
    pub travail: Option<Travail>,
    filtres: [bool; 3],
    pub rayons: bool,
    pub selection: Selection,
    /// Boîte en cours de tracé, pixels de l'écran.
    boite: Option<[f32; 4]>,
    boite_retire: bool,
    pub derniere: Option<Derniere>,
    fusion: Cible,
    /// Version de la copie déjà envoyée au rendu.
    version_geo: Option<u64>,
    /// Mode Armature : l'armature éditée et ses bouts sélectionnés.
    pub os: Option<EditionOs>,
    /// Os de l'armature au début du geste.
    depart_os: Segments,
    depart_pose: Pose,
    /// Peaux calculées, par identifiant d'objet, et le calcul en cours dans son fil.
    peaux: HashMap<String, Arc<peau::Liee>>,
    calcul_peau: Option<Receiver<(String, Arc<peau::Liee>)>>,
    /// Objets déformés par la pose, avec ce dont ils dépendent.
    deformes: HashMap<String, (String, Arc<Maillage>)>,
    /// Objets à la couleur des poids de l'os actif.
    vues_poids: HashMap<String, (String, Arc<Maillage>)>,
    pub voir_poids: bool,
    /// En mode Scène, l'os actif choisi dans le panneau Os : l'armature, puis l'os.
    os_scene: Option<(String, String)>,
    /// Curseurs d'angle de l'os actif : tenus, l'os et ses angles, figés pendant le
    /// glissé pour que les autres ne sautent pas ; l'annulation est déjà mémorisée.
    angles_tenus: Option<(String, [f64; 3])>,
    angle_saisie: bool,
    /// Lignes repliées de l'arbre des armatures : `a:` et l'identifiant d'une
    /// armature, ou l'identifiant d'un os.
    pub os_plies: HashSet<String>,
    /// Mode Pose : onglet Pose ou Catalogue, et la pose nommée choisie dans celui-ci.
    pub onglet_os: usize,
    pose_choisie: Option<usize>,
    /// Objets liés emportés par le geste en cours.
    suivis: Vec<ed::Suivi>,
    pub liaison: Option<PageLiaison>,
    /// Liste des parents dépliée, dans la fiche de l'os actif.
    parents_ouverts: bool,
    /// Les limites de rotation de l'os actif sont dépliées.
    pub limites_ouvertes: bool,
    /// Barre rapide : opérations choisies, affichage, décalage du défilement.
    acces: Choix,
    pub barre_visible: bool,
    defile: f32,
    reglages_changes: bool,
    /// Le contexte de la copie de travail coûte cher : il vaut pour une version.
    contexte_geo: Option<(u64, Contexte)>,
}

impl App {
    pub fn new(dp: f32) -> App {
        let mut a = App {
            shell: Shell::new(dp),
            projet: Projet::neuf(),
            nom: String::new(),
            rendu: Rendu::Solide,
            cam: Camera { cible: [0.0; 3], distance: 4.0, lacet: 0.6, hauteur: 0.5 },
            vol: None,
            choix: Vec::new(),
            contenu: Contenu::Scene,
            onglet: ONGLET_COULEURS,
            onglet_contexte: 0,
            onglet_fichier: 0,
            couleurs: [None; theme::NB],
            couleur_ouverte: None,
            couleurs_changees: false,
            onglet_aide: 0,
            deplie: None,
            scrolls: Scrolls::new(),
            message: None,
            espace: Rect::new(0.0, 0.0, 1.0, 1.0),
            mode: Mode::Translation,
            axes: [true; 3],
            pas: Pas::default(),
            pas_t: 1.0,
            pivot: 0,
            histoire: Historique::default(),
            geste: None,
            pave: None,
            pave_place: [0.5; 2],
            pave_prise: None,
            connus: HashMap::new(),
            evals: modifs::Cache::new(),
            calcul: None,
            ajout: None,
            uv: EditeurUv::default(),
            page_liste: PageListe::Objets,
            onglet_scene: 0,
            mat_couleur: None,
            mat_saisie: false,
            image_pour: None,
            boite_uv: None,
            primitives: HashMap::new(),
            images_vues: HashSet::new(),
            sorties: Vec::new(),
            selecteur: None,
            a_sauver: false,
            travail: None,
            filtres: [true; 3],
            rayons: false,
            selection: Selection::Toucher,
            boite: None,
            boite_retire: false,
            derniere: None,
            fusion: Cible::Centre,
            version_geo: None,
            os: None,
            depart_os: Segments::new(),
            depart_pose: Pose::new(),
            peaux: HashMap::new(),
            calcul_peau: None,
            deformes: HashMap::new(),
            vues_poids: HashMap::new(),
            voir_poids: false,
            os_scene: None,
            angles_tenus: None,
            angle_saisie: false,
            os_plies: HashSet::new(),
            onglet_os: 0,
            pose_choisie: None,
            suivis: Vec::new(),
            liaison: None,
            parents_ouverts: false,
            limites_ouvertes: false,
            acces: Choix::new(),
            barre_visible: true,
            defile: 0.0,
            reglages_changes: false,
            contexte_geo: None,
        };
        a.shell.start.open = true;
        a
    }

    pub fn animating(&self) -> bool {
        self.shell.animating() || self.vol.is_some() || self.message.is_some() || self.calcul.is_some() || self.calcul_peau.is_some() || self.ajout.as_ref().is_some_and(|a| a.calcul.is_some())
    }

    /// Maillage que l'objet montre : le modifié, ou son dernier résultat en attendant.
    fn maillage_vu(&self, k: usize) -> Arc<Maillage> {
        let o = &self.projet.objets[k];
        let posable = o.liaison.as_ref().is_some_and(|l| !self.au_repos(&l.armature));
        match self.deformes.get(&o.id).filter(|_| posable) {
            Some(d) => d.1.clone(),
            None => self.maillage_base(k),
        }
    }

    /// Maillage après modifieurs, au repos.
    fn maillage_base(&self, k: usize) -> Arc<Maillage> {
        let o = &self.projet.objets[k];
        match self.evals.get(&o.nom).filter(|_| !modifs::liste(o).is_empty()) {
            Some(e) => e.maillage.clone(),
            None => o.maillage.clone(),
        }
    }

    /// Reçoit le calcul fini et lance le suivant : un objet affiché, ou une cible, dont le
    /// résultat n'est plus à jour. Le maillage suivant la dernière modification arrive
    /// ainsi sans freiner les gestes.
    fn avancer_calculs(&mut self) {
        if let Some(rx) = &self.calcul {
            match rx.try_recv() {
                Ok(l) => {
                    self.evals.extend(l);
                    let p = &self.projet;
                    self.evals.retain(|n, _| p.objets.iter().any(|o| o.nom == *n));
                    self.calcul = None;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => self.calcul = None,
            }
        }
        let p = &self.projet;
        let perime = self.affiches().into_iter().find(|&k| !modifs::liste(&p.objets[k]).is_empty() && self.evals.get(&p.objets[k].nom).is_none_or(|e| e.cle != modifs::cle(p, k, &[])));
        let Some(k) = perime else { return };
        let (p, mut c, nom) = (self.projet.clone(), self.evals.clone(), self.projet.objets[k].nom.clone());
        let (tx, rx) = mpsc::channel();
        let fil = std::thread::Builder::new().stack_size(32 << 20).spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                modifs::evaluer(&p, k, &[], &mut c);
                c
            }));
            let _ = tx.send(match r {
                Ok(c) => c.into_iter().collect(),
                Err(_) => vec![(nom, modifs::Ev::echec(&p, k))],
            });
        });
        if fil.is_ok() {
            self.calcul = Some(rx);
        }
    }

    /// Reçoit la peau calculée, lance la suivante et déforme les objets que la pose
    /// atteint. Un objet garde sa dernière peau tant que la nouvelle n'est pas là.
    fn avancer_peaux(&mut self) {
        if let Some(rx) = &self.calcul_peau {
            match rx.try_recv() {
                Ok((id, l)) => {
                    self.peaux.insert(id, l);
                    self.calcul_peau = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.calcul_peau = None,
            }
        }
        let p = &self.projet;
        let voir = self.voir_poids && self.os_vu().is_some();
        let ouverte = self.os.as_ref().filter(|e| e.pose).and_then(|e| e.objet);
        let (mut lies, mut gardes, mut neufs, mut a_lancer) = (HashSet::new(), HashSet::new(), Vec::new(), None);
        for (k, o) in p.objets.iter().enumerate() {
            let Some(l) = o.liaison.as_ref().filter(|l| l.os.len() > 1) else { continue };
            let Some(x) = p.objet_id(&l.armature) else { continue };
            let Some(a) = &x.armature else { continue };
            lies.insert(o.id.clone());
            if !p.affiche(o) || self.travail.as_ref().is_some_and(|t| t.edites.contains(&k)) {
                continue;
            }
            if !voir && a.pose.is_empty() && ouverte.is_none_or(|i| p.objets[i].id != x.id) {
                continue;
            }
            let base = self.maillage_base(k);
            let cle = peau::cle(&base, o, x, &l.os);
            let e = self.peaux.get(&o.id);
            if a_lancer.is_none() && e.is_none_or(|e| e.cle != cle) {
                a_lancer = Some((k, base.clone(), cle.clone()));
            }
            let Some(e) = e.filter(|e| !a.pose.is_empty() && !self.au_repos(&x.id) && peau::valide(e, base.sommets.len())) else { continue };
            gardes.insert(o.id.clone());
            let ecarts: Vec<_> = a.os.iter().map(|b| a.pose.get(&b.id)).collect();
            let c = format!("{:p}|{cle}|{ecarts:?}", Arc::as_ptr(e));
            if self.deformes.get(&o.id).is_none_or(|d| d.0 != c) {
                neufs.push((o.id.clone(), c, Arc::new(peau::deforme(e, o, x, a))));
            }
        }
        for (id, c, m) in neufs {
            self.deformes.insert(id, (c, m));
        }
        self.deformes.retain(|i, _| gardes.contains(i));
        self.peaux.retain(|i, _| lies.contains(i));
        let (Some((k, base, cle)), None) = (a_lancer, &self.calcul_peau) else { return };
        let o = self.projet.objets[k].clone();
        let Some(l) = o.liaison.clone() else { return };
        let Some(x) = self.projet.objet_id(&l.armature).cloned() else { return };
        let (tx, rx) = mpsc::channel();
        let fil = std::thread::Builder::new().stack_size(8 << 20).spawn(move || {
            let (b, c) = (base.clone(), cle.clone());
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| peau::lier(base, &o, &x, &l.os, cle)));
            // Un échec laisse l'objet au repos, sans relancer le calcul
            let _ = tx.send((o.id.clone(), Arc::new(r.unwrap_or_else(|_| peau::Liee { cle: c, peau: Default::default(), repos: Vec::new(), base: b }))));
        });
        if fil.is_ok() {
            self.calcul_peau = Some(rx);
        }
    }

    /// Le maillage de `k` à la couleur des poids de l'os actif : l'U de chaque coin porte
    /// le poids, et un dégradé en texture le colore. Rien tant que sa peau n'est pas prête.
    fn vue_poids(&mut self, k: usize, vu: &Arc<Maillage>) -> Option<Arc<Maillage>> {
        if !self.voir_poids {
            return None;
        }
        let (x, actif) = self.os_vu()?;
        let actif = actif?;
        let o = &self.projet.objets[k];
        let l = o.liaison.as_ref().filter(|l| l.armature == x.id)?;
        let peau = self.peaux.get(&o.id).filter(|p| l.os.len() > 1 && peau::valide(p, vu.sommets.len()));
        if l.os.len() > 1 && peau.is_none() {
            return None;
        }
        let cle = format!("{:p}|{actif}|{:?}|{:?}", Arc::as_ptr(vu), peau.map(Arc::as_ptr), l.os);
        if let Some(v) = self.vues_poids.get(&o.id).filter(|v| v.0 == cle) {
            return Some(v.1.clone());
        }
        let w = match peau {
            Some(p) => p.peau.poids_de(&actif),
            None => vec![if l.os.contains(&actif) { 1.0 } else { 0.0 }; vu.sommets.len()],
        };
        let mut m = (**vu).clone();
        for f in &mut m.faces {
            f.uv = f.s.iter().map(|i| [w[*i].clamp(0.01, 0.99), 0.5]).collect();
        }
        let (id, m) = (o.id.clone(), Arc::new(m));
        self.vues_poids.insert(id, (cle, m.clone()));
        Some(m)
    }

    /// Attend la fin des calculs : pour les aperçus hors écran et les vérifications.
    pub fn attendre_calculs(&mut self) {
        loop {
            self.avancer_calculs();
            self.avancer_ajout();
            self.avancer_peaux();
            if self.calcul.is_none() && self.calcul_peau.is_none() && self.ajout.as_ref().is_none_or(|a| a.calcul.is_none()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// Ouvre le panneau d'ajout d'une primitive, sur ses derniers réglages.
    pub fn ouvrir_ajout(&mut self, prim: Prim) {
        let valeurs = self.primitives.get(&prim).cloned().unwrap_or_else(|| prim.defauts());
        self.ajout = Some(Ajout { prim, valeurs, apercu: None, info: None, calcul: None, cadre: !prim.recadrer() });
        self.contenu = Contenu::Ajout;
        self.shell.end.open = true;
        self.relancer_ajout();
    }

    /// Valeurs changées : ligne d'information et nouvel aperçu, rien si elles sont invalides.
    fn relancer_ajout(&mut self) {
        let Some(a) = &mut self.ajout else { return };
        a.info = a.prim.verifier(&a.valeurs);
        a.calcul = None;
        if !a.prim.valide(&a.valeurs) || a.info.as_ref().is_some_and(|i| i.1) {
            a.apercu = None;
            return;
        }
        if a.prim.lourde(&a.valeurs) {
            let (prim, v) = (a.prim, a.valeurs.clone());
            let (tx, rx) = mpsc::channel();
            let fil = std::thread::Builder::new().stack_size(32 << 20).spawn(move || {
                let _ = tx.send(prim.creer(&v));
            });
            if fil.is_ok() {
                a.calcul = Some(rx);
            }
            return;
        }
        let r = a.prim.creer(&a.valeurs);
        self.recevoir_apercu(r);
    }

    fn recevoir_apercu(&mut self, r: Result<Maillage, String>) {
        let Some(a) = &mut self.ajout else { return };
        a.calcul = None;
        match r {
            Ok(m) => a.apercu = Some(Arc::new(m)),
            Err(e) => {
                a.apercu = None;
                a.info = Some((e, true));
                return;
            }
        }
        if !a.cadre {
            a.cadre = true;
            let c = self.projet.curseur;
            let b = self.ajout.as_ref().and_then(|a| a.apercu.as_ref()?.boite_englobante());
            if let Some(b) = b {
                let b = Boite { min: [0, 1, 2].map(|k| b.min[k] + c[k]), max: [0, 1, 2].map(|k| b.max[k] + c[k]) };
                self.vol = Some(Vol { depart: self.cam, arrivee: self.cadre_boite(&b), t: 0.0 });
            }
        }
    }

    fn avancer_ajout(&mut self) {
        // Panneau fermé, autre contenu ou mode Géométrie : l'ajout est abandonné
        if self.ajout.is_some() && (self.contenu != Contenu::Ajout || !self.shell.end.open || self.travail.is_some() || self.os.is_some()) {
            self.ajout = None;
        }
        let r = self.ajout.as_ref().and_then(|a| a.calcul.as_ref()).map(|rx| rx.try_recv());
        match r {
            Some(Ok(r)) => self.recevoir_apercu(r),
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.recevoir_apercu(Err("calcul impossible sur cette forme".into())),
            _ => {}
        }
    }

    fn valider_ajout(&mut self) {
        let Some(a) = self.ajout.take() else { return };
        let Some(m) = a.apercu else { return };
        let nom = a.prim.nom(&a.valeurs);
        self.primitives.insert(a.prim, a.valeurs);
        self.reglages_changes = true;
        self.shell.end.open = false;
        self.ajouter(&nom, (*m).clone());
    }

    fn panneau_ajout(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some(a) = &self.ajout else { return };
        let (prim, v) = (a.prim, a.valeurs.clone());
        let champs = prim.champs();
        let mut change: Option<(&'static str, V)> = None;
        for (i, c) in champs.iter().enumerate().filter(|(_, c)| (c.si)(&v)) {
            if let Some(r) = c.rubrique {
                cx.section(b, &r.to_uppercase());
            }
            let cle = sub_id(id("p-champ"), i);
            match (c.genre, v.get(c.cle)) {
                (Genre::Choix(l), Some(V::T(t))) => {
                    let noms: Vec<&str> = l.iter().map(|x| x.1).collect();
                    let k = l.iter().position(|x| x.0 == t);
                    let par_ligne = if l.len() > 4 { 4 } else { l.len() };
                    if let Some(j) = cx.choix(b, cle, &noms, k, par_ligne) {
                        change = Some((c.cle, V::T(l[j].0.to_string())));
                    }
                }
                (Genre::Interrupteur, Some(V::B(x))) => {
                    let mut y = *x;
                    cx.toggle(b, cle, c.libelle, &mut y);
                    if y != *x {
                        change = Some((c.cle, V::B(y)));
                    }
                }
                (_, Some(V::N(x))) => {
                    let couleur = if prim.invalide(c, &v) { coul(C::AxeX) } else { coul(C::Texte) };
                    if cx.champ(b, cle, c.libelle, couleur, &primitives::virgule(*x)) {
                        self.saisir(Champ::Primitive(i, c.libelle));
                    }
                }
                _ => {}
            }
        }
        if let Some((k, x)) = change {
            let a = self.ajout.as_mut().unwrap();
            a.valeurs.insert(k, x);
            let (p, mut v) = (a.prim, std::mem::take(&mut a.valeurs));
            p.remplir(&mut v, k);
            a.valeurs = v;
            self.relancer_ajout();
        }
        let Some(a) = &self.ajout else { return };
        if !prim.valide(&a.valeurs) {
            cx.paragraph(b, "Un réglage est hors de ses bornes.");
        } else if let Some((t, _)) = &a.info {
            let t = if a.calcul.is_some() { format!("{t} · calcul…") } else { t.clone() };
            cx.paragraph(b, &t);
        }
        let pret = a.apercu.is_some() && a.calcul.is_none();
        if cx.bouton(b, id("p-ajouter"), "Ajouter", pret) {
            self.valider_ajout();
        }
    }

    /// Ouvre la vue UV de la copie de travail.
    pub fn ouvrir_uv(&mut self) {
        self.contenu = Contenu::Uv;
        self.shell.end.open = true;
    }

    /// Faces de la vue UV, et leur texture : les faces choisies, toutes si aucune ne
    /// l'est, parmi celles qui ont des UV. Une vue ne montre qu'une image à la fois :
    /// rend aussi le nombre de faces masquées parce qu'elles en ont une autre.
    fn faces_uv(&self) -> (Vec<usize>, Option<String>, usize) {
        let Some(t) = &self.travail else { return (Vec::new(), None, 0) };
        let m = &t.m;
        let image = |f: usize| m.faces[f].s.first().and_then(|i| t.proprio.get(*i)).and_then(|k| self.projet.texture_de(&self.projet.objets[*k]));
        let tout: Vec<usize> = (0..m.faces.len()).collect();
        let liste = if t.sel.faces.is_empty() { &tout } else { &t.sel.faces };
        let avec: Vec<usize> = liste.iter().copied().filter(|f| uv::complete(m, *f)).collect();
        let tex = avec.iter().find_map(|f| image(*f)).cloned();
        let n = avec.len();
        let faces: Vec<usize> = avec.into_iter().filter(|f| image(*f) == tex.as_ref()).collect();
        let masquees = n - faces.len();
        (faces, tex, masquees)
    }

    fn panneau_uv(&mut self, cx: &mut Cx, corps: Rect) {
        let d = cx.dp;
        let avec_axe = matches!(self.derniere, Some(Derniere::Projection { p: uv::Projection::Cylindre { .. } | uv::Projection::Sphere { .. }, .. }));
        let en_page = matches!(self.derniere, Some(Derniere::MiseEnPage { .. } | Derniere::Depliage { .. }));
        let bas = ((if avec_axe { 354.0 } else { 304.0 } + if en_page { 64.0 } else { 0.0 }) * d).min(corps.h * 0.5);
        let toile = Rect::new(corps.x, corps.y, corps.w, corps.h - bas);
        let (faces, tex, masquees) = self.faces_uv();
        self.toile_uv(cx, toile, &faces, tex.as_deref());
        let mut b = cx.body(id("corps-uv"), Rect::new(corps.x, toile.bottom(), corps.w, bas), &self.scrolls);
        let u = &mut self.uv;
        if faces.is_empty() {
            cx.paragraph(&mut b, "Ces faces n'ont pas encore d'UV : choisissez une projection.");
        } else {
            let p = if faces.len() > 1 { "s" } else { "" };
            let cache = if masquees > 0 { format!(" · {masquees} d'une autre texture masquées") } else { String::new() };
            cx.paragraph(&mut b, &format!("{} face{p} · {} coins choisis{cache}", faces.len(), u.sel.len()));
        }
        let actuelle = match self.derniere {
            Some(Derniere::Projection { p, .. }) => Some(p),
            _ => None,
        };
        let genre = actuelle.map(|p| match p {
            uv::Projection::Vue { .. } => 0,
            uv::Projection::Cube => 1,
            uv::Projection::Cylindre { .. } => 2,
            uv::Projection::Sphere { .. } => 3,
        });
        if let Some(k) = cx.choix(&mut b, id("uv-projeter"), &["Vue", "Cube", "Cylindre", "Sphère"], genre, 4) {
            self.projeter(match k {
                0 => {
                    let (_, droite, haut, _) = self.repere();
                    uv::Projection::Vue { droite, haut }
                }
                1 => uv::Projection::Cube,
                2 => uv::Projection::Cylindre { axe: 1 },
                _ => uv::Projection::Sphere { axe: 1 },
            });
        }
        // L'axe se change après coup : la projection se rejoue sans nouvelle étape
        if let Some(axe) = actuelle.and_then(|p| match p {
            uv::Projection::Cylindre { axe } | uv::Projection::Sphere { axe } => Some(axe),
            _ => None,
        }) {
            if let Some(k) = cx.choix(&mut b, id("uv-axe"), &NOMS_AXES, Some(axe), 3) {
                if let (Some(Derniere::Projection { p, aspect }), true) = (self.derniere, k != axe) {
                    let p = match p {
                        uv::Projection::Cylindre { .. } => uv::Projection::Cylindre { axe: k },
                        _ => uv::Projection::Sphere { axe: k },
                    };
                    self.uv.sel.clear();
                    self.rejouer(Derniere::Projection { p, aspect });
                }
            }
        }
        match cx.bascules(&mut b, id("uv-couture"), &[("Couture", false), ("Sans couture", false), ("Depuis UV", false), ("Déplier", false)], 4) {
            Some(0) => self.operer(|t| t.marquer_coutures(true)),
            Some(1) => self.operer(|t| t.marquer_coutures(false)),
            Some(2) => self.coutures_depuis_uv(),
            Some(_) => self.deplier(),
            None => {}
        }
        let page = self.uv.page;
        let mut neuve = page;
        match cx.bascules(&mut b, id("uv-page"), &[("Mettre en page", false), ("Même densité", page.egaliser), ("Rotation", page.tourner)], 3) {
            Some(0) => self.mettre_en_page(),
            Some(1) => neuve.egaliser = !page.egaliser,
            Some(2) => neuve.tourner = !page.tourner,
            _ => {}
        }
        if en_page {
            let (mut v, shown) = (neuve.marge * 100.0, format!("{:.1} %", neuve.marge * 100.0));
            cx.slider(&mut b, id("uv-marge"), "Marge", &shown, &mut v, 0.0, 4.0);
            neuve.marge = v / 100.0;
        }
        // Un réglage changé après coup rejoue la mise en page, sans nouvelle étape
        if neuve != page {
            self.uv.page = neuve;
            match self.derniere {
                Some(Derniere::MiseEnPage { aspect, .. }) => self.rejouer(Derniere::MiseEnPage { page: neuve, aspect }),
                // Les morceaux à plat sont là : la marge ne demande que de les ranger
                Some(Derniere::Depliage { .. }) if neuve.tourner == page.tourner && neuve.egaliser == page.egaliser => {
                    let groupes = self.groupes_par_image();
                    if let Some(t) = &mut self.travail {
                        for (f, a) in &groupes {
                            let _ = t.mettre_en_page(f, neuve, *a);
                        }
                    }
                    self.derniere = Some(Derniere::Depliage { page: neuve });
                    self.a_sauver = true;
                }
                Some(Derniere::Depliage { .. }) => self.rejouer(Derniere::Depliage { page: neuve }),
                _ => {}
            }
        }
        let u = &mut self.uv;
        match cx.bascules(&mut b, id("uv-mode"), &[("Points", !u.ilots), ("Îlots", u.ilots), ("Coins liés", u.lies), ("Cadrer", false), ("Image…", false)], 5) {
            Some(0) | Some(1) => {
                u.ilots = !u.ilots;
                u.sel.clear();
                self.version_geo = None;
            }
            Some(2) => u.lies = !u.lies,
            Some(3) => u.cadre = None,
            Some(_) => self.choisir_image(),
            None => {}
        }
        let u = &mut self.uv;
        let o = u.outil;
        match cx.bascules(&mut b, id("uv-outil"), &[("Déplacer", o == OutilUv::Deplacer), ("Tourner", o == OutilUv::Tourner), ("Échelle", o == OutilUv::Echelle), ("U", u.axes[0]), ("V", u.axes[1])], 5) {
            Some(0) => u.outil = OutilUv::Deplacer,
            Some(1) => u.outil = OutilUv::Tourner,
            Some(2) => u.outil = OutilUv::Echelle,
            Some(k) => u.axes[k - 3] = !u.axes[k - 3],
            None => {}
        }
        cx.body_end(b, &mut self.scrolls);
    }

    fn faces_visees(&self) -> Vec<usize> {
        let Some(t) = &self.travail else { return Vec::new() };
        if t.sel.faces.is_empty() { (0..t.m.faces.len()).collect() } else { t.sel.faces.clone() }
    }

    fn coutures_depuis_uv(&mut self) {
        let f = self.faces_visees();
        self.operer(|t| t.coutures_depuis_uv(&f));
    }

    /// Déplie les faces choisies, ou toutes.
    pub fn deplier(&mut self) {
        self.uv.sel.clear();
        self.lancer(Derniere::Depliage { page: self.uv.page });
        self.ouvrir_uv();
    }

    /// Faces visées par image : chacune a sa tuile.
    fn groupes_par_image(&mut self) -> Vec<(Vec<usize>, Aspect)> {
        let faces = self.faces_visees();
        let Some(t) = &self.travail else { return Vec::new() };
        let mut groupes: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        for f in faces {
            let i = t.m.faces[f].s.first().and_then(|i| t.proprio.get(*i)).and_then(|k| self.projet.texture_de(&self.projet.objets[*k])).cloned();
            match groupes.iter_mut().find(|g| g.0 == i) {
                Some(g) => g.1.push(f),
                None => groupes.push((i, vec![f])),
            }
        }
        groupes.into_iter().map(|(i, f)| (f, i.and_then(|i| self.aspect_de(&i)).unwrap_or(Aspect::CARRE))).collect()
    }

    /// Range les îlots de la vue UV dans leur image.
    pub fn mettre_en_page(&mut self) {
        let (faces, tex, _) = self.faces_uv();
        if faces.is_empty() {
            return self.dire("Pas d'UV à ranger : choisissez d'abord une projection".into());
        }
        let aspect = tex.and_then(|i| self.aspect_de(&i)).unwrap_or(Aspect::CARRE);
        self.lancer(Derniere::MiseEnPage { page: self.uv.page, aspect });
        self.ouvrir_uv();
    }

    /// Projette les faces visées, sous la forme de l'image qu'elles portent.
    pub fn projeter(&mut self, p: uv::Projection) {
        let Some(t) = &self.travail else { return };
        let vise = t.sel.faces.first().copied().or((!t.m.faces.is_empty()).then_some(0));
        let image = vise.and_then(|f| t.m.faces[f].s.first()).and_then(|i| t.proprio.get(*i)).and_then(|k| self.projet.texture_de(&self.projet.objets[*k])).cloned();
        let aspect = image.and_then(|i| self.aspect_de(&i)).unwrap_or(Aspect::CARRE);
        self.uv.sel.clear();
        self.lancer(Derniere::Projection { p, aspect });
        self.ouvrir_uv();
    }

    /// Forme d'une image, décodée au besoin : elle n'a peut-être pas encore été vue.
    fn aspect_de(&mut self, id: &str) -> Option<Aspect> {
        if let Some(a) = self.uv.aspects.get(id) {
            return Some(*a);
        }
        let i = texture::decoder(&self.projet.textures.get(id)?.octets).ok()?;
        let a = Aspect::de(i.l, i.h);
        self.uv.aspects.insert(id.to_string(), a);
        Some(a)
    }

    /// Vue UV : gestes, puis dessin.
    fn toile_uv(&mut self, cx: &mut Cx, r: Rect, faces: &[usize], tex: Option<&str>) {
        let Some(t) = &self.travail else { return };
        let a = tex.and_then(|i| self.uv.aspects.get(i).copied()).unwrap_or(Aspect::CARRE);
        // La sélection ne garde que ce qui se voit encore
        let vus: HashSet<usize> = faces.iter().copied().collect();
        let m = t.m.clone();
        let avant = self.uv.sel.len();
        self.uv.sel.retain(|c| vus.contains(&c.0) && c.1 < m.faces[c.0].uv.len());
        if self.uv.sel.len() != avant {
            self.version_geo = None;
        }
        let z = (r.w / a.l).min(r.h / a.h) * 0.9;
        let cale = [r.cx() - a.l * z / 2.0, r.cy() - a.h * z / 2.0, z];
        let mut c = self.uv.cadre.unwrap_or(cale);
        let drag = cx.ui.toile(TOILE_UV, r);
        if let Some(p) = cx.ui.take_pince(TOILE_UV) {
            let zoom = (c[2] * p.zoom).clamp(z * 0.1, z * 200.0) / c[2];
            c = [p.centre.0 + (c[0] - p.centre.0) * zoom + p.pan.0, p.centre.1 + (c[1] - p.centre.1) * zoom + p.pan.1, c[2] * zoom];
            self.uv.cadre = Some(c);
        }
        self.uv.vu = c;
        self.uv.aspect = a;
        let rayon = 24.0 * cx.dp / c[2];
        if let Some((x, y)) = cx.ui.take_tap_toile(TOILE_UV) {
            self.toucher_uv(&m, faces, x, y, rayon);
        }
        drop(m);
        match (drag, self.uv.prise.is_some()) {
            (Some(d), false) if d.moved => self.saisir_uv(faces, d.depart, rayon),
            (None, true) => self.lacher_uv(faces),
            _ => {}
        }
        if let (Some(d), true) = (drag, self.uv.prise.is_some()) {
            self.suivre_uv(d);
        }
        self.dessiner_uv(cx, r, faces, tex);
    }

    /// Le coin ou l'îlot sous le doigt, s'il y en a, avec tout ce qui bouge avec lui.
    fn groupe_uv(&self, m: &Maillage, faces: &[usize], p: Uv, rayon: f32) -> Option<Vec<Coin>> {
        let a = self.uv.aspect;
        if self.uv.ilots {
            let f = uv::coin_sous(m, faces, p, rayon, a).map(|c| c.0).or(uv::face_sous(m, faces, p))?;
            return Some(uv::ilot_de(m, faces, f));
        }
        let c = uv::coin_sous(m, faces, p, rayon, a)?;
        Some(if self.uv.lies { uv::point_de(m, faces, c) } else { vec![c] })
    }

    fn toucher_uv(&mut self, m: &Maillage, faces: &[usize], x: f32, y: f32, rayon: f32) {
        match self.groupe_uv(m, faces, self.uv.vers_uv(x, y), rayon) {
            Some(g) if g.iter().all(|c| self.uv.sel.contains(c)) => self.uv.sel.retain(|c| !g.contains(c)),
            Some(g) => self.uv.sel.extend(g),
            None => self.uv.sel.clear(),
        }
        self.version_geo = None;
    }

    /// Un doigt part : sur la sélection, il la transforme ; sur un point libre, il le
    /// prend et l'emmène ; ailleurs, il trace une boîte.
    fn saisir_uv(&mut self, faces: &[usize], depart: (f32, f32), rayon: f32) {
        let Some(t) = &self.travail else { return };
        let m = t.m.clone();
        let p = self.uv.vers_uv(depart.0, depart.1);
        let sous = self.groupe_uv(&m, faces, p, rayon);
        let sur = sous.as_ref().is_some_and(|g| g.iter().any(|c| self.uv.sel.contains(c)));
        let libre = self.uv.outil == OutilUv::Deplacer;
        if !sur && libre {
            if let Some(g) = sous {
                self.uv.sel = g.into_iter().collect();
                self.version_geo = None;
            }
        }
        let touche = self.groupe_uv(&m, faces, p, rayon).is_some_and(|g| g.iter().any(|c| self.uv.sel.contains(c)));
        let prend = !self.uv.sel.is_empty() && (!libre || touche);
        if !prend {
            self.boite_uv = Some(depart);
            self.uv.prise = Some(PriseUv::Boite { fin: depart });
            return;
        }
        let pivot = uv::centre(&m, &self.uv.sel).unwrap_or([0.5; 2]);
        let departs = self.uv.sel.iter().map(|c| (*c, uv::uv_de(&m, *c))).collect();
        drop(m);
        self.memoriser();
        self.uv.prise = Some(PriseUv::Transfo { departs, pivot, dernier: None });
    }

    fn suivre_uv(&mut self, d: crate::ui::Drag) {
        let (a, pas) = (self.uv.aspect, self.pas);
        let (outil, axes) = (self.uv.outil, if self.uv.axes == [false; 2] { [true; 2] } else { self.uv.axes });
        let (de, a_) = (self.uv.vers_uv(d.depart.0, d.depart.1), self.uv.vers_uv(d.x, d.y));
        let (plan_d, plan_a) = (self.uv.ecran(de), self.uv.ecran(a_));
        let mut pose = None;
        match &mut self.uv.prise {
            Some(PriseUv::Boite { fin }) => *fin = (d.x, d.y),
            Some(PriseUv::Transfo { departs, pivot, dernier }) => {
                let pv = {
                    let c = self.uv.vu;
                    [c[0] + pivot[0] * a.l * c[2], c[1] + pivot[1] * a.h * c[2]]
                };
                let g = match outil {
                    OutilUv::Deplacer => {
                        let k = |v: f32, ok: bool| if !ok { 0.0 } else if pas.actif { uv::au_pas(v, PAS_UV) } else { v };
                        uv::Geste::Translation([k(a_[0] - de[0], axes[0]), k(a_[1] - de[1], axes[1])])
                    }
                    OutilUv::Tourner => {
                        let angle = |q: [f32; 2]| (q[1] - pv[1]).atan2(q[0] - pv[0]);
                        let degres = (angle(plan_a) - angle(plan_d)).to_degrees();
                        uv::Geste::Rotation { degres: if pas.actif { uv::au_pas(degres, pas.angle as f32) } else { degres }, pivot: *pivot }
                    }
                    OutilUv::Echelle => {
                        let dist = |q: [f32; 2]| (q[0] - pv[0]).hypot(q[1] - pv[1]).max(1.0);
                        let k = dist(plan_a) / dist(plan_d);
                        let k = if pas.actif { uv::au_pas(k, pas.echelle as f32) } else { k };
                        uv::Geste::Echelle { facteur: [if axes[0] { k } else { 1.0 }, if axes[1] { k } else { 1.0 }], pivot: *pivot }
                    }
                };
                if *dernier != Some(g) {
                    *dernier = Some(g);
                    pose = Some(departs.iter().map(|(c, q)| (*c, uv::transformer(*q, g, a))).collect::<Vec<_>>());
                }
            }
            None => {}
        }
        if let (Some(l), Some(t)) = (pose, &mut self.travail) {
            t.poser_uv(&l);
        }
    }

    fn lacher_uv(&mut self, faces: &[usize]) {
        match self.uv.prise.take() {
            Some(PriseUv::Transfo { .. }) => self.a_sauver = true,
            Some(PriseUv::Boite { fin }) => {
                // Le départ a quitté le geste : il revit dans `boite_uv`
                if let (Some(t), Some(d)) = (&self.travail, self.boite_uv.take()) {
                    let m = t.m.clone();
                    let (p, q) = (self.uv.vers_uv(d.0, d.1), self.uv.vers_uv(fin.0, fin.1));
                    let l = uv::dans_boite(&m, faces, [p[0], p[1], q[0], q[1]]);
                    if self.uv.ilots {
                        let fs: HashSet<usize> = l.iter().map(|c| c.0).collect();
                        for f in fs {
                            self.uv.sel.extend(uv::ilot_de(&m, faces, f));
                        }
                    } else {
                        self.uv.sel.extend(l);
                    }
                    self.version_geo = None;
                }
            }
            None => {}
        }
    }

    fn dessiner_uv(&mut self, cx: &mut Cx, r: Rect, faces: &[usize], tex: Option<&str>) {
        let Some(t) = &self.travail else { return };
        let (m, d, u) = (&t.m, cx.dp, &self.uv);
        cx.clip(Some(r));
        cx.dl.rrect(r, 0.0, coul(C::Fond));
        let (p0, p1) = (u.ecran([0.0, 0.0]), u.ecran([1.0, 1.0]));
        let tuile = Rect::new(p0[0], p0[1], p1[0] - p0[0], p1[1] - p0[1]);
        cx.dl.rrect(tuile, 0.0, coul(C::Surface));
        cx.dl.image(tuile, tex.unwrap_or(texture::DAMIER));
        for (a, b) in [(p0, [p1[0], p0[1]]), ([p1[0], p0[1]], p1), (p1, [p0[0], p1[1]]), ([p0[0], p1[1]], p0)] {
            cx.dl.line(a, b, 1.5 * d, coul(C::Bordures));
        }
        let hors = |a: [f32; 2], b: [f32; 2]| a[0].max(b[0]) < r.x || a[0].min(b[0]) > r.right() || a[1].max(b[1]) < r.y || a[1].min(b[1]) > r.bottom();
        for choisis in [false, true] {
            let (e, c) = if choisis { (2.2 * d, coul(C::AreteChoisie)) } else { (1.1 * d, coul(C::Arete)) };
            for &f in faces {
                let n = m.faces[f].uv.len();
                for k in 0..n {
                    let l = (k + 1) % n;
                    if (u.sel.contains(&(f, k)) && u.sel.contains(&(f, l))) != choisis {
                        continue;
                    }
                    let (a, b) = (u.ecran(m.faces[f].uv[k]), u.ecran(m.faces[f].uv[l]));
                    if !hors(a, b) {
                        cx.dl.line(a, b, e, c);
                    }
                }
            }
        }
        if !u.ilots {
            let pts = uv::points(m, faces);
            let tous = pts.len() < 6000;
            for g in pts {
                let on = g.iter().any(|c| u.sel.contains(c));
                if tous || on {
                    let p = u.ecran(uv::uv_de(m, g[0]));
                    if r.contains(p[0], p[1]) {
                        let q = if on { 4.5 * d } else { 3.0 * d };
                        cx.dl.rrect(Rect::new(p[0] - q, p[1] - q, 2.0 * q, 2.0 * q), q, if on { coul(C::SommetChoisi) } else { coul(C::Sommet) });
                    }
                }
            }
        }
        if u.outil != OutilUv::Deplacer {
            if let Some(q) = uv::centre(m, &u.sel) {
                let p = u.ecran(q);
                cx.dl.rrect(Rect::new(p[0] - 6.0 * d, p[1] - 6.0 * d, 12.0 * d, 12.0 * d), 6.0 * d, coul(C::Curseur));
            }
        }
        if let (Some(PriseUv::Boite { fin }), Some(a)) = (&u.prise, self.boite_uv) {
            let (x, y) = ([a.0, fin.0], [a.1, fin.1]);
            for (p, q) in [([x[0], y[0]], [x[1], y[0]]), ([x[1], y[0]], [x[1], y[1]]), ([x[1], y[1]], [x[0], y[1]]), ([x[0], y[1]], [x[0], y[0]])] {
                cx.dl.line(p, q, 1.5 * d, coul(C::AreteChoisie));
            }
        }
        cx.clip(None);
    }

    /// Matériau de l'objet actif. Une armature n'en a pas.
    fn materiau_actif(&self) -> Option<String> {
        let o = self.projet.objets.get(self.projet.actif?)?;
        o.armature.is_none().then(|| o.materiau.clone())
    }

    /// Change un matériau. Un glissé ne fait qu'une étape : `mat_saisie` tient jusqu'au
    /// lever du doigt. Le rendu En jeu montre le résultat.
    fn regler_materiau(&mut self, nom: &str, f: impl FnOnce(&mut Materiau)) {
        if !self.mat_saisie {
            self.memoriser();
            self.mat_saisie = true;
        }
        if let Some((_, m)) = self.projet.materiaux.iter_mut().find(|x| x.0 == nom) {
            f(m);
        }
        self.a_sauver = true;
        self.montrer_materiaux();
    }

    fn montrer_materiaux(&mut self) {
        if self.travail.is_none() {
            self.rendu = Rendu::Jeu;
        }
    }

    fn objets_choisis_a_materiau(&self) -> Vec<usize> {
        let p = &self.projet;
        self.choix.iter().copied().filter(|k| p.objets.get(*k).is_some_and(|o| o.armature.is_none())).collect()
    }

    fn appliquer_materiau(&mut self, nom: &str) {
        let l = self.objets_choisis_a_materiau();
        if l.is_empty() {
            return self.dire("Aucun objet choisi : touchez un objet".into());
        }
        if l.iter().all(|k| self.projet.objets[*k].materiau == nom) {
            return;
        }
        self.memoriser();
        for k in l {
            self.projet.objets[k].materiau = nom.to_string();
        }
        self.a_sauver = true;
        self.version_geo = None;
        self.montrer_materiaux();
    }

    /// Nouveau matériau, copie de `modele` ou d'une teinte neuve, appliqué aux objets
    /// choisis. Rend son nom.
    fn nouveau_materiau(&mut self, modele: Option<&str>) -> String {
        self.memoriser();
        let base = modele.filter(|n| *n != DEFAUT).unwrap_or("materiau");
        let nom = self.projet.nom_materiau_libre(base);
        let m = modele.and_then(|n| self.projet.materiau(n).cloned()).unwrap_or_else(|| Materiau::neuf(Some(self.projet.teinte_libre())));
        self.projet.materiaux.push((nom.clone(), m));
        let l = self.objets_choisis_a_materiau();
        let applique = !l.is_empty();
        for k in l {
            self.projet.objets[k].materiau = nom.clone();
        }
        self.a_sauver = true;
        self.version_geo = None;
        self.dire(format!("Matériau « {nom} » créé{}", if applique { " et appliqué" } else { "" }));
        nom
    }

    /// Ses objets reprennent le matériau par défaut.
    fn supprimer_materiau(&mut self, nom: &str) {
        if nom == DEFAUT || self.projet.materiau(nom).is_none() {
            return;
        }
        self.memoriser();
        for o in &mut self.projet.objets {
            if o.materiau == nom {
                o.materiau = DEFAUT.to_string();
            }
        }
        self.projet.materiaux.retain(|x| x.0 != nom);
        self.a_sauver = true;
        self.version_geo = None;
    }

    fn renommer_materiau(&mut self, ancien: &str, nouveau: &str) {
        let nouveau = nouveau.trim();
        if ancien == DEFAUT || nouveau.is_empty() || nouveau == ancien || self.projet.materiau(ancien).is_none() {
            return;
        }
        if self.projet.materiau(nouveau).is_some() || nouveau == DEFAUT || nouveau == nom_mat(DEFAUT) {
            return self.dire("Ce nom est déjà pris".into());
        }
        self.memoriser();
        for (n, _) in &mut self.projet.materiaux {
            if n == ancien {
                *n = nouveau.to_string();
            }
        }
        for o in &mut self.projet.objets {
            if o.materiau == ancien {
                o.materiau = nouveau.to_string();
            }
        }
        self.mat_couleur = None;
        self.a_sauver = true;
    }

    /// Demande une image au sélecteur, pour le matériau de l'objet actif.
    fn choisir_image(&mut self) {
        let Some(n) = self.materiau_actif() else { return self.dire("Touchez d'abord un objet".into()) };
        self.image_pour = Some(n);
        self.selecteur = Some(Selecteur::Image);
    }

    /// Image reçue du sélecteur. Le matériau par défaut, que tout objet partage, ne la
    /// porte pas : un nouveau matériau prend le nom du fichier. La couleur repasse au
    /// blanc, sans quoi elle teinterait l'image.
    pub fn recevoir_image(&mut self, fichier: &str, octets: &[u8]) {
        let Some(mut nom) = self.image_pour.take() else { return };
        let i = match texture::decoder(octets) {
            Ok(i) => i,
            Err(e) => return self.dire(format!("Image illisible : {fichier} ({e})")),
        };
        let cle = texture::identifiant(octets);
        self.uv.aspects.insert(cle.clone(), Aspect::de(i.l, i.h));
        let base = fichier.rsplit(['/', '\\']).next().unwrap_or(fichier);
        let base = base.rsplit_once('.').map_or(base, |x| x.0).to_string();
        self.memoriser();
        self.projet.textures.entry(cle.clone()).or_insert_with(|| crate::texture::Texture { nom: fichier.to_string(), octets: Arc::new(octets.to_vec()) });
        if nom == DEFAUT {
            let n = self.projet.nom_materiau_libre(if base.is_empty() { "materiau" } else { &base });
            self.projet.materiaux.push((n.clone(), Materiau::neuf(None)));
            let mut l = self.objets_choisis_a_materiau();
            l.extend(self.projet.actif.filter(|k| !l.contains(k)));
            for k in l {
                self.projet.objets[k].materiau = n.clone();
            }
            nom = n;
        }
        if let Some((_, m)) = self.projet.materiaux.iter_mut().find(|x| x.0 == nom) {
            m.texture = Some(cle);
            m.couleur = "#ffffff".to_string();
        }
        self.a_sauver = true;
        self.version_geo = None;
        self.montrer_materiaux();
        self.dire(format!("Image {} × {} sur « {nom} »", i.l, i.h));
    }

    fn onglet_materiaux(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let actif = self.materiau_actif();
        let titre = self.projet.actif.and_then(|k| self.projet.objets.get(k)).filter(|_| actif.is_some()).map_or("MATÉRIAUX".to_string(), |o| format!("OBJET ACTIF : {}", o.nom.to_uppercase()));
        cx.section(b, &titre);
        match &actif {
            Some(n) => self.reglages_materiau(cx, b, n),
            None => cx.paragraph(b, "Aucun objet actif : touchez un objet pour régler son matériau."),
        }
        cx.section(b, "BIBLIOTHÈQUE DU PROJET");
        let nc = self.objets_choisis_a_materiau().len();
        cx.paragraph(b, &match nc {
            0 => "Choisissez des objets pour leur appliquer un matériau.".to_string(),
            1 => "Toucher un matériau l'applique à l'objet choisi.".to_string(),
            n => format!("Toucher un matériau l'applique aux {n} objets choisis."),
        });
        let lignes: Vec<(String, Color, usize)> = self.projet.materiaux.iter().map(|(n, m)| (n.clone(), octets_hex(&m.couleur), self.projet.objets.iter().filter(|o| o.materiau == *n && o.armature.is_none()).count())).collect();
        for (k, (n, c, compte)) in lignes.iter().enumerate() {
            let usage = match compte {
                0 => "inutilisé".to_string(),
                1 => "1 objet".to_string(),
                x => format!("{x} objets"),
            };
            if cx.row(b, sub_id(id("mat-ligne"), k), &format!("{} · {usage}", nom_mat(n)), Some(*c), actif.as_ref() == Some(n)) {
                self.appliquer_materiau(n);
            }
        }
        if cx.button(b, id("mat-nouveau"), "Nouveau matériau") {
            self.nouveau_materiau(None);
        }
    }

    fn reglages_materiau(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, nom: &str) {
        let Some(m) = self.projet.materiau(nom).cloned() else { return };
        let d = cx.dp;
        if nom == DEFAUT {
            cx.paragraph(b, "Matériau par défaut, partagé : son nom ne change pas et il ne porte pas d'image.");
        } else if cx.champ(b, id("mat-nom"), "Nom", coul(C::Texte), nom) {
            self.saisir(Champ::MatNom);
        }
        cx.section(b, "IMAGE");
        let image = m.texture.as_ref().filter(|i| self.projet.textures.contains_key(*i));
        if let Some(i) = image {
            let cote = 96.0 * d;
            let a = self.aspect_de(i).unwrap_or(Aspect::CARRE);
            let r = Rect::new(b.x, b.y + 4.0 * d + (1.0 - a.h) * cote / 2.0, cote * a.l, cote * a.h);
            cx.dl.rrect(r, 0.0, coul(C::Fond));
            cx.dl.image(r, i);
            let t = self.projet.textures[i].nom.rsplit(['/', '\\']).next().unwrap_or("").to_string();
            cx.text_at(b.x + cote + 12.0 * d, r.cy(), &t, 14.0, coul(C::Secondaire));
            b.y += cote + 12.0 * d;
            match cx.rangee(b, id("mat-image"), &[("Changer…", true), ("Retirer", true)]) {
                Some(0) => self.choisir_image(),
                Some(_) => {
                    self.mat_saisie = false;
                    self.regler_materiau(nom, |m| m.texture = None);
                    self.mat_saisie = false;
                    self.version_geo = None;
                }
                None => {}
            }
        } else if cx.button(b, id("mat-choisir"), "Choisir une image…") {
            self.choisir_image();
        }
        let mut tenu = false;
        cx.section(b, "EN JEU ET À L'EXPORT");
        self.couleurs_materiau(cx, b, nom, &m, true, &mut tenu);
        let (mut o, z) = (m.opacite as f32 * 100.0, id("mat-opacite"));
        cx.slider(b, z, "Opacité", &format!("{:.0} %", o), &mut o, 0.0, 100.0);
        tenu |= cx.ui.pressed(z);
        if o != m.opacite as f32 * 100.0 {
            self.regler_materiau(nom, |m| m.opacite = o as f64 / 100.0);
        }
        let (mut e, z) = (m.illumination as f32, id("mat-illum"));
        let rang = (m.illumination.round() as usize).min(10);
        cx.slider(b, z, "Éclairage", &format!("{rang} · {}", ILLUM[rang]), &mut e, 0.0, 10.0);
        tenu |= cx.ui.pressed(z);
        if e.round() as f64 != m.illumination {
            self.regler_materiau(nom, |m| m.illumination = e.round() as f64);
        }
        cx.section(b, "À L'EXPORT SEULEMENT");
        self.couleurs_materiau(cx, b, nom, &m, false, &mut tenu);
        // Un fichier importé peut sortir des bornes : le curseur reste sur sa piste
        let (brut, z) = (m.brillance as f32, id("mat-brillance"));
        let mut v = brut.clamp(0.0, 1000.0);
        let avant = v;
        cx.slider(b, z, "Brillance", &format!("{brut:.0}"), &mut v, 0.0, 1000.0);
        tenu |= cx.ui.pressed(z);
        if v != avant {
            self.regler_materiau(nom, |m| m.brillance = v.round() as f64);
        }
        let (brut, z) = (m.refraction as f32, id("mat-refraction"));
        let mut v = brut.clamp(1.0, 3.0);
        let avant = v;
        cx.slider(b, z, "Réfraction", &format!("{brut:.2}"), &mut v, 1.0, 3.0);
        tenu |= cx.ui.pressed(z);
        if v != avant {
            self.regler_materiau(nom, |m| m.refraction = v as f64);
        }
        match cx.rangee(b, id("mat-actions"), &[("Dupliquer", true), ("Supprimer", nom != DEFAUT)]) {
            Some(0) => drop(self.nouveau_materiau(Some(nom))),
            Some(_) => self.supprimer_materiau(nom),
            None => {}
        }
        if !tenu {
            self.mat_saisie = false;
        }
    }

    /// Lignes de couleur : toucher déplie teinte, saturation, luminosité et code.
    fn couleurs_materiau(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, nom: &str, m: &Materiau, a_l_ecran: bool, tenu: &mut bool) {
        for (k, (libelle, ecran)) in COULEURS_MAT.iter().enumerate().filter(|x| x.1 .1 == a_l_ecran) {
            let _ = ecran;
            let c = octets_hex(couleur_mat(m, k));
            let (ouvre, _) = cx.ligne_couleur(b, sub_id(id("mat-couleur"), k), sub_id(id("mat-couleur-raz"), k), libelle, c, false);
            let ici = self.mat_couleur.as_ref().is_some_and(|o| o.0 == nom && o.1 == k);
            if ouvre {
                self.mat_couleur = if ici { None } else { Some((nom.to_string(), k, theme::vers_tsl(c))) };
            }
            let Some((_, _, tsl)) = self.mat_couleur.clone().filter(|o| o.0 == nom && o.1 == k) else { continue };
            let mut neuf = tsl;
            for (j, (n, max, unite)) in [("Teinte", 360.0, "°"), ("Saturation", 100.0, " %"), ("Luminosité", 100.0, " %")].iter().enumerate() {
                let z = sub_id(sub_id(id("mat-tsl"), k), j);
                cx.slider(b, z, n, &format!("{:.0}{unite}", neuf[j]), &mut neuf[j], 0.0, *max);
                *tenu |= cx.ui.pressed(z);
            }
            if neuf != tsl {
                let code = theme::hex(theme::depuis_tsl(neuf));
                self.regler_materiau(nom, |x| *couleur_mat_mut(x, k) = code);
                self.mat_couleur = Some((nom.to_string(), k, neuf));
            }
            if cx.champ(b, sub_id(id("mat-code"), k), "Code", coul(C::Texte), couleur_mat(m, k)) {
                self.saisir(Champ::MatCode(k));
            }
        }
    }

    /// Objet que les modifieurs concernent : l'actif.
    fn objet_modifie(&self) -> Option<usize> {
        self.projet.actif.filter(|k| *k < self.projet.objets.len() && self.choix.contains(k))
    }

    fn ajouter_modif(&mut self, genre: &str) {
        let Some(k) = self.objet_modifie() else { return };
        // Première cible proposée : l'objet choisi suivant, sinon le plus récent
        let p = &self.projet;
        let libre = |x: &usize| *x != k && p.objets[*x].armature.is_none();
        let t = self.choix.iter().copied().find(|x| libre(x)).or_else(|| (0..p.objets.len()).rev().find(|x| libre(x)));
        let cible = t.map(|t| p.objets[t].nom.clone());
        let m = modifs::neuf(genre, cible.as_deref());
        self.memoriser();
        if let J::Liste(l) = &mut self.projet.objets[k].modifs {
            l.push(m);
        }
        self.a_sauver = true;
        self.derniere = None;
        self.contenu = Contenu::Modifieurs;
        self.shell.end.open = true;
    }

    fn commander_modif(&mut self, k: usize, c: CmdModif) {
        if let CmdModif::Appliquer(i) = c {
            let m = match modifs::appliquer(&self.projet, k, i) {
                Ok(m) => m,
                Err(e) => return self.dire(e),
            };
            self.memoriser();
            let o = &mut self.projet.objets[k];
            o.maillage = Arc::new(m);
            o.sel = Default::default();
            if let J::Liste(l) = &mut o.modifs {
                l.remove(i);
            }
        } else {
            self.memoriser();
            let J::Liste(l) = &mut self.projet.objets[k].modifs else { return };
            match c {
                CmdModif::Monter(i) => l.swap(i, i - 1),
                CmdModif::Descendre(i) => l.swap(i, i + 1),
                CmdModif::Supprimer(i) => drop(l.remove(i)),
                CmdModif::Fixer(i, cle, v) => modifs::fixer(&mut l[i], cle, v),
                CmdModif::Appliquer(_) => {}
            }
        }
        self.a_sauver = true;
        self.derniere = None;
    }

    fn panneau_modifs(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some(k) = self.objet_modifie() else {
            return cx.paragraph(b, "Aucun objet actif : touchez un objet pour lui ajouter des modifieurs.");
        };
        let o = &self.projet.objets[k];
        let (l, nom) = (modifs::liste(o).to_vec(), o.nom.clone());
        let erreurs = self.evals.get(&nom).map_or(Vec::new(), |e| e.erreurs.clone());
        let autres: Vec<String> = self.projet.objets.iter().filter(|x| x.nom != nom && x.armature.is_none()).map(|x| x.nom.clone()).collect();
        cx.section(b, &nom.to_uppercase());
        if l.is_empty() {
            cx.paragraph(b, "Aucun modifieur. Ils s'appliquent dans l'ordre, sans changer le maillage de base ; le mode Géométrie édite ce dernier.");
        }
        let mut cmd = None;
        for (i, m) in l.iter().enumerate() {
            let cle = |s: &str| sub_id(id(s), i);
            cx.section(b, &modifs::titre(m).to_uppercase());
            let mut actif = modifs::actif(m);
            cx.toggle(b, cle("m-actif"), "Actif", &mut actif);
            if actif != modifs::actif(m) {
                cmd = Some(CmdModif::Fixer(i, "actif", J::Bool(actif)));
            }
            let e = erreurs.get(i).cloned().flatten();
            match cx.rangee(b, cle("m-actions"), &[("↑", i > 0), ("↓", i + 1 < l.len()), ("Appliquer", e.is_none()), ("✕", true)]) {
                Some(0) => cmd = Some(CmdModif::Monter(i)),
                Some(1) => cmd = Some(CmdModif::Descendre(i)),
                Some(2) => cmd = Some(CmdModif::Appliquer(i)),
                Some(_) => cmd = Some(CmdModif::Supprimer(i)),
                None => {}
            }
            if modifs::genre(m) == "repetition" {
                let n = m.get("n").and_then(J::num).unwrap_or(1.0);
                if cx.champ(b, cle("m-n"), "Copies", coul(C::Texte), &chiffres(n)) {
                    self.saisir(Champ::ModifN(i));
                }
                let relatif = matches!(m.get("relatif"), Some(J::Bool(true)));
                if let Some(j) = cx.choix(b, cle("m-relatif"), &["Relatif (× taille)", "Constant"], Some(!relatif as usize), 2) {
                    cmd = Some(CmdModif::Fixer(i, "relatif", J::Bool(j == 0)));
                }
                let d = m.get("decalage").and_then(J::nombres::<3>).unwrap_or([0.0; 3]);
                for a in 0..3 {
                    if cx.champ(b, sub_id(cle("m-d"), a), NOMS_AXES[a], coul(COUL_AXES[a]), &chiffres(d[a])) {
                        self.saisir(Champ::ModifDecalage(i, a));
                    }
                }
                let unir = matches!(m.get("unir"), Some(J::Bool(true)));
                if let Some(j) = cx.choix(b, cle("m-unir"), &["Copies séparées", "Unir (booléen)"], Some(unir as usize), 2) {
                    cmd = Some(CmdModif::Fixer(i, "unir", J::Bool(j == 1)));
                }
            } else if modifs::genre(m) == "booleen" {
                let op = m.get("op").and_then(J::txt).unwrap_or("");
                let noms: Vec<&str> = modifs::OPS.iter().map(|x| x.1).collect();
                if let Some(j) = cx.choix(b, cle("m-op"), &noms, modifs::OPS.iter().position(|x| x.0 == op), 3) {
                    cmd = Some(CmdModif::Fixer(i, "op", J::Txt(modifs::OPS[j].0.into())));
                }
                cx.paragraph(b, "Cible");
                let cible = modifs::cible(m);
                for (j, n) in autres.iter().enumerate() {
                    if cx.row(b, sub_id(cle("m-cible"), j), n, None, n == cible) {
                        cmd = Some(CmdModif::Fixer(i, "cible", J::Txt(n.clone())));
                    }
                }
                if autres.is_empty() {
                    cx.paragraph(b, "Aucun autre objet dans la scène.");
                }
            }
            if let Some(e) = e {
                cx.paragraph(b, &format!("Impossible : {e}"));
            }
        }
        if self.calcul.is_some() {
            cx.paragraph(b, "Calcul…");
        }
        if let Some(c) = cmd {
            self.commander_modif(k, c);
        }
        self.operations(cx, b, Menu::Modifieurs);
    }

    /// Ouvre un fichier reçu : projet .gda, ou OBJ, STL, glTF. En cas d'échec, la
    /// scène en cours reste en place et un message dit pourquoi.
    pub fn ouvrir(&mut self, nom: &str, contenu: &[u8]) {
        let base = nom.rsplit_once('.').map_or(nom, |x| x.0).to_string();
        match Self::lire(nom, contenu, &[]) {
            Ok((p, _)) => {
                self.remplacer(p, &base);
                let n = self.projet.objets.len();
                self.dire(format!("« {base} » ouvert : {n} objet{}", if n > 1 { "s" } else { "" }));
            }
            Err(e) => self.dire(format!("Ouverture impossible : {e}")),
        }
    }

    fn lire(nom: &str, contenu: &[u8], annexes: &[(String, Vec<u8>)]) -> Result<(Projet, Vec<String>), String> {
        let ext = maillage::extension(nom);
        let gda = ext == "gda" || contenu.iter().find(|c| !c.is_ascii_whitespace()) == Some(&b'{') && ext != "gltf";
        if gda {
            let p = std::str::from_utf8(contenu).map_err(|_| "fichier .gda illisible".to_string()).and_then(Projet::lire)?;
            Ok((p, Vec::new()))
        } else {
            Projet::depuis_fichiers(nom, contenu, annexes)
        }
    }

    /// Ajoute le contenu d'un fichier à la scène, l'origine du fichier sur le curseur 3D.
    /// L'import s'annule ; en cas d'échec, la scène reste en place.
    pub fn importer(&mut self, nom: &str, contenu: &[u8]) {
        self.importer_fichiers(vec![(nom.to_string(), contenu.to_vec())]);
    }

    /// Import de plusieurs fichiers choisis ensemble : un fichier 3D, avec son .mtl et
    /// ses images. Une image n'est gardée que si un matériau la nomme.
    pub fn importer_fichiers(&mut self, mut fichiers: Vec<(String, Vec<u8>)>) {
        let est_3d = |n: &str| matches!(maillage::extension(n).as_str(), "obj" | "glb" | "gltf" | "stl" | "gda");
        if fichiers.iter().all(|(n, _)| texture::est_image(n)) {
            return self.dire("Import impossible : choisissez aussi le fichier 3D".to_string());
        }
        let k = fichiers.iter().position(|(n, _)| est_3d(n)).unwrap_or(0);
        let (nom, contenu) = fichiers.remove(k);
        let base = nom.rsplit_once('.').map_or(&*nom, |x| x.0).to_string();
        let (mut p, manquantes) = match Self::lire(&nom, &contenu, &fichiers) {
            Ok(x) => x,
            Err(e) => return self.dire(format!("Import impossible : {e}")),
        };
        let images: HashSet<&String> = p.objets.iter().filter_map(|o| p.texture_de(o)).collect();
        let nb_images = images.len();
        self.sortir_geometrie();
        self.memoriser();
        let c = self.projet.curseur;
        for o in &mut p.objets {
            o.position = [0, 1, 2].map(|k| o.position[k] + c[k]);
        }
        let set = self.set_visible();
        let l = self.projet.fusionner(p, &set);
        let n = l.len();
        self.choix = l.into_iter().filter(|k| self.projet.affiche(&self.projet.objets[*k])).collect();
        self.projet.actif = self.choix.last().copied().or(self.projet.actif);
        self.a_sauver = true;
        self.recadrer();
        if nb_images > 0 {
            self.rendu = Rendu::Jeu; // le rendu Solide est neutre : rien ne se verrait
        }
        let mut m = format!("« {base} » importé : {n} objet{}", if n > 1 { "s" } else { "" });
        if nb_images > 0 {
            m += &format!(", {nb_images} texture{}", if nb_images > 1 { "s" } else { "" });
        }
        if !manquantes.is_empty() {
            m += &format!(". Texture absente : {}", manquantes.join(", "));
        }
        self.dire(m);
    }

    /// Set qui reçoit un import : celui de l'objet actif, ou le premier visible. Si tous
    /// sont masqués, le premier se montre : un import invisible semblerait raté.
    fn set_visible(&mut self) -> String {
        let p = &mut self.projet;
        let actif = p.actif.and_then(|k| p.objets.get(k)).map(|o| o.set.clone());
        if let Some(n) = actif.filter(|n| p.sets.iter().any(|e| e.nom == *n && e.visible)) {
            return n;
        }
        if !p.sets.iter().any(|e| e.visible) {
            p.sets[0].visible = true;
        }
        p.sets.iter().find(|e| e.visible).unwrap().nom.clone()
    }

    pub fn remplacer(&mut self, p: Projet, nom: &str) {
        if p.a_des_textures() {
            self.rendu = Rendu::Jeu;
        }
        self.projet = p;
        self.ajout = None;
        self.nom = nom.to_string();
        self.choix.clear();
        self.histoire = Historique::default();
        self.travail = None;
        self.os = None;
        self.derniere = None;
        self.geste = None;
        self.pave = None;
        self.a_sauver = true;
        self.vol = None;
        self.cam = self.cadre(&[]).unwrap_or(self.cam);
    }

    /// Reprend la sauvegarde automatique du lancement précédent, sans rien dire si
    /// elle est illisible.
    pub fn reprendre(&mut self, nom: &str, contenu: &str) {
        if let Ok(p) = Projet::lire(contenu) {
            self.remplacer(p, nom);
            self.a_sauver = false;
        }
    }

    /// Sélecteur de fichiers que l'application demande d'ouvrir.
    pub fn prendre_selecteur(&mut self) -> Option<Selecteur> {
        self.selecteur.take()
    }

    /// Fichiers que l'application demande de déposer.
    pub fn prendre_sorties(&mut self) -> Vec<Sortie> {
        std::mem::take(&mut self.sorties)
    }

    /// Scène à sauvegarder, si elle a changé et qu'aucun geste n'est en cours.
    pub fn prendre_sauvegarde(&mut self) -> Option<(String, Projet)> {
        (self.a_sauver && self.geste.is_none()).then(|| {
            self.a_sauver = false;
            (self.nom.clone(), self.projet_range())
        })
    }

    /// Le projet, avec la copie de travail rangée dans ses objets.
    fn projet_range(&self) -> Projet {
        let mut p = self.projet.clone();
        if let Some(t) = &self.travail {
            t.clone().ranger(&mut p.objets);
        }
        p
    }

    /// Passe à l'édition des os, le mode Géométrie d'une armature : sur l'armature
    /// active, sinon la dernière choisie. Le panneau Os reste comme il était.
    pub fn entrer_armature(&mut self) {
        if self.os.is_some() {
            return;
        }
        let p = &self.projet;
        let est = |k: usize| p.objets.get(k).is_some_and(|o| o.armature.is_some());
        let k = p.actif.filter(|k| est(*k)).or(self.choix.iter().rev().copied().find(|k| est(*k)));
        if k.is_some_and(|k| !p.affiche(&p.objets[k])) {
            return self.dire("Objet caché".into());
        }
        self.sortir_geometrie();
        self.os = Some(EditionOs::default());
        if let Some(k) = k {
            self.ouvrir_armature(k);
        }
        // L'os choisi dans le panneau, en mode Scène, reste actif
        let id = self.os.as_ref().and_then(|e| e.objet).map(|k| self.projet.objets[k].id.clone());
        if let (Some(e), Some((a, x))) = (&mut self.os, &self.os_scene) {
            if id.as_ref() == Some(a) {
                e.actif = Some(x.clone());
            }
        }
        self.parents_ouverts = false;
        self.fin_geste();
        self.redessiner_armatures();
    }

    /// Le dessin des armatures : au repos celle qu'on édite ainsi, en pose toutes les autres.
    pub fn redessiner_armatures(&mut self) {
        let ouverte = self.os.as_ref().filter(|e| !e.pose).and_then(|e| e.objet);
        for (k, o) in self.projet.objets.iter_mut().enumerate() {
            let Some(a) = &o.armature else { continue };
            let m = if ouverte == Some(k) { a.enveloppe() } else { a.enveloppe_pose() };
            if *o.maillage != m {
                o.maillage = Arc::new(m);
            }
        }
        self.version_geo = None;
    }

    /// Passe en mode Pose sur l'armature active : depuis la Scène, ou depuis ses os au repos.
    pub fn entrer_pose(&mut self) {
        if self.os.is_none() {
            if self.travail.is_some() {
                return;
            }
            self.entrer_armature();
        }
        if self.os.as_ref().is_some_and(|e| !e.pose && e.objet.is_some()) {
            self.basculer_pose();
        }
    }

    /// Bascule entre l'édition du repos (mode Géométrie) et la pose (mode Pose) ; la
    /// pose reste, d'un mode à l'autre.
    pub fn basculer_pose(&mut self) {
        let Some(e) = &mut self.os else { return };
        e.pose = !e.pose;
        e.version += 1;
        let m = if e.pose { "Pose : touchez un os, puis tournez-le" } else { "Géométrie : les os s'éditent au repos, la pose est gardée" };
        self.fin_geste();
        self.redessiner_armatures();
        self.dire(m.into());
    }

    /// L'armature `id` est montrée au repos : ouverte en mode Armature, hors de la pose.
    fn au_repos(&self, id: &str) -> bool {
        self.os.as_ref().filter(|e| !e.pose).and_then(|e| e.objet).and_then(|k| self.projet.objets.get(k)).is_some_and(|o| o.id == id)
    }

    /// Où la pose emmène l'objet `k`, s'il suit un os.
    fn place_posee(&self, k: usize) -> Option<(P3, ed::Q)> {
        let o = &self.projet.objets[k];
        if self.au_repos(&o.liaison.as_ref()?.armature) {
            return None;
        }
        self.projet.place_posee(o)
    }

    /// L'objet tel qu'il est montré.
    fn objet_vu(&self, k: usize) -> Cow<'_, Objet> {
        let o = &self.projet.objets[k];
        match self.place_posee(k) {
            Some((position, rotation)) => {
                let mut x = o.clone();
                (x.position, x.rotation) = (position, rotation);
                Cow::Owned(x)
            }
            None => Cow::Borrowed(o),
        }
    }

    /// Un objet choisi suit un os posé : il se déplace au repos seulement.
    fn selection_posee(&mut self) -> bool {
        let oui = self.travail.is_none() && self.os.is_none() && self.choix.iter().any(|k| self.place_posee(*k).is_some() || self.deformes.contains_key(&self.projet.objets[*k].id));
        if oui {
            self.dire("Objet posé par ses os : remettez-les au repos pour le déplacer".into());
        }
        oui
    }

    /// Ramène au repos tous les os de l'armature `k`, depuis le mode Scène.
    fn repos_armature(&mut self, k: usize) {
        self.memoriser();
        let Some(a) = &mut self.projet.objets[k].armature else { return };
        let l: Vec<String> = a.os.iter().map(|b| b.id.clone()).collect();
        a.remettre(&l);
        self.a_sauver = true;
        self.redessiner_armatures();
        self.dire("Armature au repos".into());
    }

    /// Ramène à leur repos les os choisis, sinon tous.
    fn repos_os(&mut self) {
        self.changer_os(|a, e| {
            let mut l = e.os_choisis(a);
            if l.is_empty() {
                l = a.os.iter().map(|b| b.id.clone()).collect();
            }
            a.remettre(&l);
        });
    }

    /// Dans le mode Armature : cette armature devient celle qu'on édite.
    fn ouvrir_armature(&mut self, k: usize) {
        let pose = self.os.as_ref().is_some_and(|e| e.pose);
        self.pose_choisie = None;
        self.os = Some(EditionOs { objet: Some(k), pose, ..Default::default() });
        self.projet.actif = Some(k);
        self.choix = vec![k];
        self.parents_ouverts = false;
        self.redessiner_armatures();
    }

    /// Passe en mode Géométrie : les objets choisis, l'actif compris. Sur une
    /// armature active, ce sont ses os qui s'éditent, panneau Os ouvert.
    pub fn entrer_geometrie(&mut self) {
        if self.os.as_ref().is_some_and(|e| e.pose) {
            return self.basculer_pose();
        }
        if self.travail.is_some() || self.os.is_some() {
            return;
        }
        let Some(a) = self.projet.actif.filter(|k| *k < self.projet.objets.len()) else {
            return self.dire("Aucun objet actif : touchez un objet".into());
        };
        if self.projet.objets[a].armature.is_some() {
            return self.entrer_armature();
        }
        let l: Vec<usize> = self.choix.iter().copied().filter(|k| self.projet.affiche(&self.projet.objets[*k])).collect();
        self.travail = Some(Travail::ouvrir(&self.projet.objets, &l, a, self.filtres));
        self.fin_geste();
        self.version_geo = None;
    }

    fn sortir_os(&mut self) {
        if let Some(e) = self.os.take() {
            let a = e.objet.and_then(|k| self.projet.objets.get(k)).map(|o| o.id.clone());
            if let (Some(a), Some(x)) = (a, e.actif) {
                self.os_scene = Some((a, x));
            }
            self.fin_geste();
            self.redessiner_armatures();
        }
    }

    pub fn sortir_geometrie(&mut self) {
        self.sortir_os();
        if let Some(mut t) = self.travail.take() {
            t.ranger(&mut self.projet.objets);
            self.filtres = t.filtres;
            if self.selection == Selection::Chemin {
                self.selection = Selection::Toucher;
            }
            self.choix = t.edites.iter().copied().filter(|k| *k < self.projet.objets.len()).collect();
            self.a_sauver = true;
            self.fin_geste();
        }
    }

    fn fin_geste(&mut self) {
        self.geste = None;
        self.boite = None;
        self.derniere = None;
    }

    // ----- Mode Armature --------------------------------------------------

    /// Le mode Armature, l'objet et l'armature qu'il édite.
    /// L'armature dont le panneau Os montre les poids, et son os actif s'il y en a un :
    /// celle qu'on édite, ou en mode Scène l'armature active.
    fn os_vu(&self) -> Option<(&Objet, Option<String>)> {
        if let Some((e, o, _)) = self.armature_os() {
            return Some((o, e.actif.clone()));
        }
        if self.travail.is_some() || self.os.is_some() {
            return None;
        }
        let o = self.projet.actif.and_then(|k| self.projet.objets.get(k)).filter(|o| o.armature.is_some())?;
        let x = self.os_scene.as_ref().filter(|(a, _)| *a == o.id).map(|(_, x)| x.clone());
        Some((o, x))
    }

    fn armature_os(&self) -> Option<(&EditionOs, &Objet, &Armature)> {
        let e = self.os.as_ref()?;
        let o = self.projet.objets.get(e.objet?)?;
        Some((e, o, o.armature.as_ref()?))
    }

    fn os_actif(&self) -> Option<(&Objet, &Os)> {
        let (e, o, a) = self.armature_os()?;
        Some((o, a.trouver(e.actif.as_ref()?)?))
    }

    /// Ce qui est sous le doigt, en pixels de la vue : un bout d'abord, sinon un corps.
    fn os_sous(&self, x: f64, y: f64, vue: &Vue, rayon: f64) -> Option<Touche> {
        let (_, o, a) = self.armature_os()?;
        let (e, s, mx) = (self.os.as_ref()?, segments_os(self.os.as_ref()?, a), o.matrice());
        let ecran = |p: P3| vue.ecran(vue::appliquer(&mx, p));
        let mut bout: Option<(f64, Touche)> = None;
        let mut corps: Option<(f64, Touche)> = None;
        for b in a.ordre() {
            let Some(g) = s.get(&b.id) else { continue };
            let (t, q) = (ecran(g.tete), ecran(g.queue));
            for (queue, e) in [(false, t), (true, q)] {
                let Some(e) = e else { continue };
                let d = (e[0] - x).hypot(e[1] - y);
                if d < rayon * 0.7 && bout.as_ref().is_none_or(|m| d < m.0) {
                    bout = Some((d, Touche { os: b.id.clone(), queue: Some(queue) }));
                }
            }
            if let (Some(t), Some(q)) = (t, q) {
                let d = ed::distance_segment(t, q, x, y);
                if d < rayon && corps.as_ref().is_none_or(|m| d < m.0) {
                    corps = Some((d, Touche { os: b.id.clone(), queue: None }));
                }
            }
        }
        // En pose, on prend un os entier
        if e.pose { corps } else { bout.or(corps) }.map(|m| m.1)
    }

    /// Le doigt est sur des bouts tous sélectionnés.
    fn sur_os(&self, x: f64, y: f64, vue: &Vue, rayon: f64) -> bool {
        let (Some((e, _, a)), Some(t)) = (self.armature_os(), self.os_sous(x, y, vue, rayon)) else { return false };
        EditionOs::cles_de(a, &t).iter().all(|c| e.bouts.contains(c))
    }

    fn toucher_os(&mut self, t: &Touche) {
        let Some((_, _, a)) = self.armature_os() else { return };
        let a = a.clone();
        if let Some(e) = &mut self.os {
            e.basculer(&a, t);
        }
    }

    /// Une boîte ajoute à la sélection les bouts qu'elle couvre, ou les en retire.
    fn boite_os(&mut self, r: [f64; 4], vue: &Vue, retire: bool) {
        let Some((e, o, a)) = self.armature_os() else { return };
        let (s, mx, soudures) = (segments_os(e, a), o.matrice(), a.soudures());
        let mut pris: Vec<Bout> = Vec::new();
        for b in &a.os {
            let Some(g) = s.get(&b.id) else { continue };
            for (bout, p) in [(Bout::tete(&b.id), g.tete), (Bout::queue(&b.id), g.queue)] {
                if vue.ecran(vue::appliquer(&mx, p)).is_some_and(|e| e[0] >= r[0] && e[0] <= r[2] && e[1] >= r[1] && e[1] <= r[3]) {
                    pris.extend(soudures.get(&bout).into_iter().flatten().cloned());
                }
            }
        }
        let Some(e) = &mut self.os else { return };
        let avant = e.bouts.len();
        if retire {
            e.bouts.retain(|b| !pris.contains(b));
        } else {
            for b in pris {
                if !e.bouts.contains(&b) {
                    e.bouts.push(b);
                }
            }
        }
        e.version += 1;
        let n = e.bouts.len().abs_diff(avant);
        self.dire(format!("Boîte : {}{n} bout{}", if retire { "−" } else { "+" }, if n > 1 { "s" } else { "" }));
    }

    /// Modification mémorisée de l'armature éditée ; son dessin suit.
    fn changer_os(&mut self, f: impl FnOnce(&mut Armature, &mut EditionOs)) {
        self.memoriser();
        let Some(e) = &mut self.os else { return };
        let Some(o) = e.objet.and_then(|k| self.projet.objets.get_mut(k)) else { return };
        let Some(mut a) = o.armature.take() else { return };
        f(&mut a, e);
        o.maillage = Arc::new(if e.pose { a.enveloppe_pose() } else { a.enveloppe() });
        o.armature = Some(a);
        e.version += 1;
        self.a_sauver = true;
    }

    fn ajouter_os(&mut self) {
        let Some((e, o, a)) = self.armature_os() else { return };
        let p = geo::vers_local(o, self.projet.curseur);
        let l = e.actif.as_ref().and_then(|i| a.trouver(i)).map_or(1.0, |b| b.longueur);
        self.changer_os(|a, e| {
            let id = a.ajouter("os", None, p, [0.0, 0.0, 0.0, 1.0], l);
            e.selectionner(a, std::slice::from_ref(&id), false);
            e.actif = Some(id);
        });
    }

    fn extruder_os(&mut self) {
        let Some((e, _, a)) = self.armature_os() else { return };
        let l = e.queues_choisies(a);
        if l.is_empty() {
            return self.dire("Sélectionnez la queue d’un os".into());
        }
        self.changer_os(|a, e| {
            let nouveaux: Vec<String> = l.iter().filter_map(|id| a.extruder(id)).collect();
            let s = a.soudures();
            e.bouts = nouveaux.iter().flat_map(|id| s[&Bout::queue(id)].iter().cloned()).collect();
            e.actif = nouveaux.last().cloned();
        });
    }

    fn supprimer_os(&mut self) {
        let Some((e, _, a)) = self.armature_os() else { return };
        let l = e.os_choisis(a);
        if l.is_empty() {
            return;
        }
        if l.len() >= a.os.len() {
            return self.dire("Une armature garde au moins un os : supprimez-la en mode Scène".into());
        }
        self.changer_os(|a, e| {
            for id in &l {
                a.retirer(id);
            }
            e.bouts.clear();
            e.actif = None;
        });
        self.projet.verifier_liaisons();
    }

    fn parenter_os(&mut self) {
        let Some((e, _, a)) = self.armature_os() else { return };
        let Some(actif) = e.actif.clone() else { return };
        let l: Vec<String> = e.os_choisis(a).into_iter().filter(|i| *i != actif).collect();
        if l.is_empty() {
            return;
        }
        let mut refus = 0;
        self.changer_os(|a, _| refus = l.iter().filter(|id| !a.changer_parent(id, Some(&actif))).count());
        if refus > 0 {
            self.dire(format!("{refus} os refusé{} : l’actif en descend", if refus > 1 { "s" } else { "" }));
        }
    }

    fn detacher_os(&mut self) {
        let Some((e, _, a)) = self.armature_os() else { return };
        let l = e.os_choisis(a);
        self.changer_os(|a, _| {
            for id in &l {
                a.changer_parent(id, None);
            }
        });
    }

    fn changer_parent_os(&mut self, id: &str, parent: Option<&str>) {
        let Some((_, _, a)) = self.armature_os() else { return };
        if parent.is_some_and(|p| a.descendants(id).iter().any(|d| d == p)) {
            return self.dire("Cet os descend de celui-ci".into());
        }
        self.changer_os(|a, _| {
            a.changer_parent(id, parent);
        });
        self.parents_ouverts = false;
    }

    fn tous_os(&mut self, tout: bool) {
        let Some((_, _, a)) = self.armature_os() else { return };
        let (a, ids): (Armature, Vec<String>) = (a.clone(), if tout { a.os.iter().map(|b| b.id.clone()).collect() } else { Vec::new() });
        if let Some(e) = &mut self.os {
            e.selectionner(&a, &ids, false);
        }
    }

    fn descendants_os(&mut self) {
        let Some((e, _, a)) = self.armature_os() else { return };
        let Some(ids) = e.actif.as_ref().map(|i| a.descendants(i)) else { return };
        let a = a.clone();
        if let Some(e) = &mut self.os {
            e.selectionner(&a, &ids, true);
        }
    }

    fn renommer_os(&mut self, nom: &str) {
        let nom = nom.trim().to_string();
        if nom.is_empty() {
            return self.dire("Nom vide".into());
        }
        let Some((e, _, a)) = self.armature_os() else { return };
        let Some(id) = e.actif.clone() else { return };
        if a.os.iter().any(|b| b.nom == nom && b.id != id) {
            return self.dire("Ce nom est déjà pris".into());
        }
        self.changer_os(|a, _| {
            if let Some(b) = a.os.iter_mut().find(|b| b.id == id) {
                b.nom = nom;
            }
        });
    }

    /// Limites de rotation de l'os actif.
    pub fn regler_limites(&mut self, l: [Limite; 3]) {
        let Some(id) = self.os_actif().map(|x| x.1.id.clone()) else { return };
        if self.os_actif().is_some_and(|x| x.1.limites() == l) {
            return;
        }
        self.changer_os(|a, _| {
            if let Some(b) = a.os.iter_mut().find(|b| b.id == id) {
                b.regler_limites(l);
            }
            // La pose gardée y revient
            a.borner(&a.pose.clone());
        });
    }

    /// L'état suivant d'une limite : libre, limitée, bloquée.
    pub fn changer_etat_limite(&mut self, k: usize) {
        let Some(mut l) = self.os_actif().map(|x| x.1.limites()) else { return };
        l[k] = match l[k] {
            Limite::Libre => Limite::Bornee(-45.0, 45.0),
            Limite::Bornee(..) => Limite::Bloquee,
            Limite::Bloquee => Limite::Libre,
        };
        self.regler_limites(l);
    }

    fn saisir_limite(&mut self, k: usize, max: bool, v: f64) {
        let Some(mut l) = self.os_actif().map(|x| x.1.limites()) else { return };
        let Limite::Bornee(a, b) = l[k] else { return };
        let v = v.clamp(-180.0, 180.0);
        // Une borne qui passe l'autre l'emmène
        l[k] = if max { Limite::Bornee(a.min(v), v) } else { Limite::Bornee(v, b.max(v)) };
        self.regler_limites(l);
    }

    /// Longueur au monde de l'os actif : sa queue et ce qui y est soudé glissent le long de lui.
    fn saisir_longueur_os(&mut self, v: f64) {
        let Some((e, o, a)) = self.armature_os() else { return };
        let Some(id) = e.actif.clone() else { return };
        let (s, mx) = (a.segments(), o.matrice());
        let Some(g) = s.get(&id) else { return };
        let (t, q) = (vue::appliquer(&mx, g.tete), vue::appliquer(&mx, g.queue));
        let u = [q[0] - t[0], q[1] - t[1], q[2] - t[2]];
        let l = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
        if v <= 0.0 || (v - l).abs() < 1e-9 || l <= 0.0 {
            return;
        }
        let cles = a.soudures()[&Bout::queue(&id)].clone();
        self.memoriser();
        let Some(e) = &mut self.os else { return };
        let Some(o) = e.objet.and_then(|k| self.projet.objets.get_mut(k)) else { return };
        poser_bouts(o, &cles, &s, Transfo::Translation(u.map(|x| x * (v - l) / l)));
        e.version += 1;
    }

    /// Bouts et os vers le rendu, à travers les autres objets, seulement s'ils ont changé.
    fn vue_os(&mut self) -> Option<GeoVue> {
        let (e, o, a) = self.armature_os()?;
        if self.version_geo == Some(e.version) {
            return Some(GeoVue { maj: None, rayons: true });
        }
        let version = e.version;
        let (s, mx) = (segments_os(e, a), o.matrice());
        let f3 = |p: P3| vue::appliquer(&mx, p).map(|x| x as f32);
        let mut g = GeoTampons::default();
        for b in &a.os {
            let Some(x) = s.get(&b.id) else { continue };
            let (t, q) = (f3(x.tete), f3(x.queue));
            g.points.extend(t);
            g.points.extend(q);
            g.lignes.extend(t);
            g.lignes.extend(q);
        }
        for b in &e.bouts {
            if let Some(x) = s.get(&b.os) {
                g.choisis.extend(f3(if b.queue { x.queue } else { x.tete }));
            }
        }
        for id in e.os_choisis(a) {
            let Some(x) = s.get(&id) else { continue };
            let (p, q) = (f3(x.tete), f3(x.queue));
            for (a, b, c) in [(p, q, 1.0), (p, q, -1.0), (q, p, -1.0), (p, q, -1.0), (q, p, 1.0), (q, p, -1.0)] {
                g.bandes.extend(a);
                g.bandes.extend(b);
                g.bandes.push(c);
            }
        }
        self.version_geo = Some(version);
        Some(GeoVue { maj: Some(g), rayons: true })
    }

    /// Opération sur la copie de travail : mémorisée si elle réussit.
    fn operer(&mut self, f: impl FnOnce(&mut Travail) -> Bilan) {
        let avant = self.etape();
        let Some(t) = &mut self.travail else { return };
        match f(t) {
            Ok(m) => {
                self.histoire.memoriser(avant);
                self.a_sauver = true;
                self.derniere = None;
                if let Some(m) = m {
                    self.dire(m);
                }
            }
            Err(m) => self.dire(m),
        }
    }

    /// Changement de sélection seule : rien à annuler.
    fn selectionner(&mut self, f: impl FnOnce(&mut Travail) -> Bilan) {
        let Some(t) = &mut self.travail else { return };
        if let Ok(Some(m)) | Err(m) = f(t) {
            self.dire(m);
        }
    }

    /// Lance une opération réglable, retenue pour le panneau.
    fn lancer(&mut self, d: Derniere) {
        let avant = self.etape();
        match self.executer(d) {
            Ok(m) => {
                self.histoire.memoriser(avant);
                self.a_sauver = true;
                self.derniere = Some(d);
                self.onglet_contexte = ONGLET_DERNIERE;
                if let Some(m) = m {
                    self.dire(m);
                }
            }
            Err(m) => self.dire(m),
        }
    }

    fn executer(&mut self, d: Derniere) -> Bilan {
        if let Derniere::MiseEnPage { page, aspect } = d {
            let faces = self.faces_uv().0;
            let Some(t) = &mut self.travail else { return Ok(None) };
            return t.mettre_en_page(&faces, page, aspect);
        }
        if let Derniere::Depliage { page } = d {
            let groupes = self.groupes_par_image();
            let Some(t) = &mut self.travail else { return Ok(None) };
            return t.deplier(&groupes, page);
        }
        let transfo = match d {
            Derniere::Translation { d } => Some(Transfo::Translation(d)),
            Derniere::Echelle { s, pivot } => Some(Transfo::Echelle { s, c: pivot }),
            Derniere::Rotation { degres, axe, vue, pivot } => {
                let u = vue.filter(|_| axe == 3).unwrap_or([0, 1, 2].map(|k| if k == axe { 1.0 } else { 0.0 }));
                Some(Transfo::Rotation { q: ed::qaxe(u, degres.to_radians()), c: pivot })
            }
            _ => None,
        };
        if let Some(t) = transfo {
            self.poser_selection(t);
            return Ok(None);
        }
        let curseur = self.projet.curseur;
        let Some(t) = &mut self.travail else { return Ok(None) };
        match d {
            Derniere::Subdivision { coupes } => t.subdiviser(coupes),
            Derniere::Boucle { arete, coupes, glissement, uniforme, inverser } => t.boucle(Some(arete), coupes, glissement, uniforme, inverser),
            Derniere::Fusion { cible } => t.fusionner(cible),
            Derniere::Proches { tolerance } => t.fusion_proches(tolerance),
            Derniere::Revolution { axe, vue, angle, inverse, segments, fermer } => {
                if angle == 360.0 && fermer && segments < 3 {
                    return Err("Tour complet fermé : au moins 3 segments".into());
                }
                let a = if axe < 3 { let mut u = [0.0; 3]; u[axe] = 1.0; u } else { vue };
                t.revolution(RegleRevolution { axe: a, angle: if inverse { -angle } else { angle }, segments, fermer }, curseur)
            }
            Derniere::MiseEnPage { .. } | Derniere::Depliage { .. } => unreachable!("traitées plus haut"),
            Derniere::Projection { p, aspect } => {
                let tout: Vec<usize> = (0..t.m.faces.len()).collect();
                let faces = if t.sel.faces.is_empty() { tout } else { t.sel.faces.clone() };
                t.projeter(&faces, p, aspect)
            }
            Derniere::Translation { .. } | Derniere::Rotation { .. } | Derniere::Echelle { .. } => unreachable!("posées plus haut"),
        }
    }

    /// Rejoue la dernière opération sur l'état d'avant, avec ses nouveaux réglages.
    fn rejouer(&mut self, d: Derniere) {
        let Some(e) = self.histoire.annuler.last().cloned() else { return };
        self.restaurer(e);
        self.derniere = Some(d);
        match self.executer(d) {
            Ok(Some(m)) | Err(m) => self.dire(m),
            Ok(None) => {}
        }
        self.a_sauver = true;
    }

    fn revolution(&mut self, w: Rect) {
        if self.travail.as_ref().is_none_or(|t| t.sel.sommets.is_empty() && t.sel.aretes.is_empty()) {
            return self.dire("Sélectionnez des sommets ou des arêtes".into());
        }
        let v = self.vue_ecran(w);
        self.lancer(Derniere::Revolution { axe: 1, vue: v.dos.map(|x| -x), angle: 360.0, inverse: false, segments: 16, fermer: true });
    }

    fn separer(&mut self) {
        let avant = self.etape();
        let Some(mut t) = self.travail.take() else { return };
        match t.separer(&mut self.projet, &mut self.choix) {
            Ok((m, fin)) => {
                if !fin {
                    self.travail = Some(t);
                }
                self.histoire.memoriser(avant);
                self.a_sauver = true;
                self.derniere = None;
                self.version_geo = None;
                self.dire(m);
            }
            Err(m) => {
                self.travail = Some(t);
                self.dire(m);
            }
        }
    }

    fn curseur_vers_selection(&mut self) {
        let pts = self.points();
        if !pts.is_empty() {
            self.projet.curseur = ed::centre(&pts);
            self.a_sauver = true;
        }
    }

    fn selection_vers_curseur(&mut self) {
        let pts = self.points();
        if pts.is_empty() || self.selection_posee() {
            return;
        }
        let (c, u) = (ed::centre(&pts), self.projet.curseur);
        self.deplacer_selection([u[0] - c[0], u[1] - c[1], u[2] - c[2]]);
    }

    /// Translation mémorisée de la sélection, en bloc.
    fn deplacer_selection(&mut self, d: P3) {
        self.memoriser();
        self.poser_selection(Transfo::Translation(d));
        self.a_sauver = true;
    }

    /// Transforme la sélection depuis son état présent.
    fn poser_selection(&mut self, v: Transfo) {
        if let Some(e) = &mut self.os {
            let Some(o) = e.objet.and_then(|k| self.projet.objets.get_mut(k)) else { return };
            if e.pose {
                let Some(depart) = o.armature.as_ref().map(|a| a.pose.clone()) else { return };
                poser_os(o, e, &depart, v);
            } else {
                let Some(depart) = o.armature.as_ref().map(Armature::segments) else { return };
                poser_bouts(o, &e.bouts, &depart, v);
            }
            e.version += 1;
        } else if let Some(t) = &mut self.travail {
            let l = t.impliques();
            let depart: Vec<P3> = l.iter().map(|i| t.m.sommets[*i]).collect();
            t.poser(&l, &depart, v);
        } else {
            let l = self.choix.clone();
            let departs = ed::depart(&self.projet.objets, &l);
            let suivis = ed::suivis(&self.projet.objets, &l);
            ed::poser(&mut self.projet.objets, &l, &departs, v);
            ed::suivre(&mut self.projet.objets, &suivis);
        }
    }

    fn etape(&self) -> Etape {
        let p = &self.projet;
        Etape { objets: p.objets.clone(), sets: p.sets.clone(), materiaux: p.materiaux.clone(), actif: p.actif, choix: self.choix.clone(), travail: self.travail.clone(), os: self.os.clone() }
    }

    /// La visibilité échappe à l'annulation : refaire la dernière opération la défairait.
    fn visibilite_changee(&mut self) {
        self.a_sauver = true;
        self.derniere = None;
    }

    fn memoriser(&mut self) {
        let e = self.etape();
        self.histoire.memoriser(e);
        self.derniere = None;
    }

    fn restaurer(&mut self, e: Etape) {
        let p = &mut self.projet;
        (p.objets, p.sets, p.materiaux, p.actif) = (e.objets, e.sets, e.materiaux, e.actif);
        self.choix = e.choix;
        self.travail = e.travail;
        // L'étape vaut pour les données : on reste en pose ou au repos
        let pose = self.os.as_ref().map(|x| x.pose);
        self.os = e.os;
        if let (Some(x), Some(p)) = (&mut self.os, pose) {
            x.pose = p;
        }
        self.redessiner_armatures();
        self.contexte_geo = None;
        self.a_sauver = true;
        self.boite = None;
        self.derniere = None;
    }

    pub fn annuler(&mut self) {
        if let Some(e) = self.histoire.annuler.pop().filter(|_| self.geste.is_none()) {
            let ici = self.etape();
            self.histoire.retablir.push(ici);
            self.restaurer(e);
        }
    }

    pub fn retablir(&mut self) {
        if let Some(e) = self.histoire.retablir.pop().filter(|_| self.geste.is_none()) {
            let ici = self.etape();
            self.histoire.annuler.push(ici);
            self.restaurer(e);
        }
    }

    pub fn dire(&mut self, m: String) {
        self.message = Some((m, DUREE_MESSAGE));
    }

    /// L'objet seul, ou plus rien.
    pub fn choisir(&mut self, k: Option<usize>) {
        self.choix.clear();
        self.choix.extend(k);
        if k.is_some() {
            self.projet.actif = k;
        }
    }

    /// Appui sur un objet : le choisit, ou le retire s'il était l'actif.
    fn basculer_objet(&mut self, k: usize) {
        match self.choix.iter().position(|c| *c == k) {
            Some(i) if self.projet.actif == Some(k) => {
                self.choix.remove(i);
            }
            i => {
                if let Some(i) = i {
                    self.choix.remove(i);
                }
                self.choix.push(k);
                self.projet.actif = Some(k);
            }
        }
    }

    /// Points que transforme la poignée : sommets concernés, ou origines des objets.
    fn points(&self) -> Vec<P3> {
        if let Some((e, o, a)) = self.armature_os() {
            let mx = o.matrice();
            if e.pose {
                let m = a.matrices_pose();
                return e.racines(a).iter().filter_map(|i| m.get(i)).map(|x| vue::appliquer(&mx, x.0)).collect();
            }
            let s = a.segments();
            return e.bouts.iter().filter_map(|b| s.get(&b.os).map(|x| vue::appliquer(&mx, if b.queue { x.queue } else { x.tete }))).collect();
        }
        match &self.travail {
            Some(t) => t.points(),
            None => self.choix.iter().map(|k| self.objet_vu(*k).position).collect(),
        }
    }

    fn pivot_courant(&self, pts: &[P3]) -> P3 {
        let m = PIVOTS[self.pivot].0;
        match &self.travail {
            Some(t) if m == Pivot::Actif => t.centre_actif().unwrap_or(pts[pts.len() - 1]),
            _ => ed::pivot(m, pts, self.projet.curseur),
        }
    }

    /// Le seul objet choisi.
    fn seul(&self) -> Option<usize> {
        match self.choix[..] {
            [k] => Some(k),
            _ => None,
        }
    }

    fn vue_ecran(&self, w: Rect) -> Vue {
        let (oeil, droite, haut, dos) = self.repere();
        Vue { oeil, droite, haut, dos, largeur: w.w as f64, hauteur: w.h as f64, tan: (CHAMP.to_radians() / 2.0).tan(), pres: self.cam.distance / 200.0 }
    }

    fn poignee(&self, vue: &Vue, dp: f32) -> Option<Poignee> {
        let pts = self.points();
        if pts.is_empty() {
            return None;
        }
        let pivot = self.pivot_courant(&pts);
        let mut p = Poignee::new(self.mode, ed::centre(&pts), pivot, self.axes, vue, dp as f64);
        if let Some(q) = self.repere_pose() {
            p.repere = q;
            // Un axe bloqué de l'os actif n'a pas d'anneau
            if let Some((l, _)) = self.limites_pose() {
                for k in 0..3 {
                    p.axes[k] &= l[k] != Limite::Bloquee;
                }
            }
        }
        Some(p)
    }

    /// En mode Pose, les limites de l'os actif et ses angles du moment.
    fn limites_pose(&self) -> Option<([Limite; 3], [f64; 3])> {
        let (e, _, a) = self.armature_os()?;
        if !e.pose {
            return None;
        }
        let id = e.actif.clone().filter(|i| a.trouver(i).is_some()).or_else(|| e.os_choisis(a).into_iter().next())?;
        let l = a.trouver(&id)?.limites();
        (l != [Limite::Libre; 3]).then(|| (l, angles_os(a.pose.get(&id).map_or([0.0, 0.0, 0.0, 1.0], |p| p.r))))
    }

    /// En mode Pose, le repère de l'os actif posé, dans le monde : les poignées et les
    /// curseurs d'angle tournent les os autour de ses axes.
    fn repere_pose(&self) -> Option<ed::Q> {
        let (e, o, a) = self.armature_os()?;
        if !e.pose {
            return None;
        }
        let id = e.actif.clone().filter(|i| a.trouver(i).is_some()).or_else(|| e.os_choisis(a).into_iter().next())?;
        let (_, q) = *a.matrices_pose().get(&id)?;
        Some(ed::qnorm(ed::qmul(o.rotation, q)))
    }

    fn pas_translation(&self) -> f64 {
        self.pas.translation.unwrap_or(self.pas_grille() as f64 / 2.0)
    }

    fn rayon(&self, dp: f32) -> f64 {
        geo::RAYON * dp as f64
    }

    /// Un doigt commence à glisser : s'il part d'une poignée ou de la sélection, il
    /// la transforme ; en mode Boîte, il trace la boîte ; sinon il tourne la vue.
    fn commencer(&mut self, cx: &mut Cx, w: Rect, (x, y): (f32, f32)) {
        if !w.contains(x, y) {
            return;
        }
        let vue = self.vue_ecran(w);
        let (px, py) = ((x - w.x) as f64, (y - w.y) as f64);
        let dp = cx.dp as f64;
        let boite = self.selection == Selection::Boite;
        let poignee = self.poignee(&vue, cx.dp);
        let mut prise = poignee.and_then(|p| p.sous(px, py, &vue, dp));
        // En pose, les anneaux de l'os croisent son corps : l'os touché l'emporte
        if poignee.is_some() && prise != Some(Prise::O) && self.os.as_ref().is_some_and(|e| e.pose) && self.sur_os(px, py, &vue, self.rayon(cx.dp)) {
            prise = Some(Prise::O);
        }
        if prise.is_none() && !boite && poignee.is_some() {
            let dessus = match &self.travail {
                Some(t) => t.sur_selection(px, py, &vue, self.rayon(cx.dp), self.rayons),
                None if self.os.is_some() => self.sur_os(px, py, &vue, self.rayon(cx.dp)),
                None => self.objet_sous(w, x, y).is_some_and(|k| self.choix.contains(&k)),
            };
            if dessus {
                prise = Some(Prise::O);
            }
        }
        let Some(prise) = prise else {
            if boite {
                self.boite = Some([x, y, x, y]);
                cx.ui.saisir((x, y));
            }
            return;
        };
        if self.selection_posee() {
            return;
        }
        self.memoriser();
        let pts = self.points();
        let pivot = self.pivot_courant(&pts);
        self.pas_t = self.pas_translation();
        let (indices, departs) = if self.travail.is_some() || self.os.is_some() {
            (Vec::new(), Vec::new())
        } else {
            (self.choix.clone(), ed::depart(&self.projet.objets, &self.choix))
        };
        self.suivis = ed::suivis(&self.projet.objets, &indices);
        let mut g = Geste::new(self.mode, prise, px, py, vue, self.axes, indices, departs, ed::centre(&pts), pivot);
        if let Some(q) = self.repere_pose() {
            g.orienter(q);
        }
        if let Some(t) = &self.travail {
            g.indices = t.impliques();
            g.sommets = pts;
        } else if let Some((e, _, a)) = self.armature_os() {
            g.indices = (0..e.bouts.len()).collect();
            let (s, p) = (a.segments(), a.pose.clone());
            self.depart_os = s;
            self.depart_pose = p;
        }
        self.geste = Some(g);
        self.vol = None;
        cx.ui.saisir((x, y));
    }

    /// Le doigt saisi pose la sélection, puis la lâche.
    fn suivre(&mut self, cx: &mut Cx, w: Rect) {
        let Some(((x, y), pose)) = cx.ui.prise() else { return };
        if let Some(b) = &mut self.boite {
            (b[2], b[3]) = (x, y);
            if !pose {
                cx.ui.lacher_prise();
                self.tracer_boite(w);
            }
            return;
        }
        // Un second doigt a pu changer la scène pendant le glissé
        let n = match (&self.os, &self.travail) {
            (Some(e), _) => e.bouts.len(),
            (_, Some(t)) => t.m.sommets.len(),
            _ => self.projet.objets.len(),
        };
        if self.geste.as_ref().is_some_and(|g| g.indices.iter().any(|k| *k >= n)) {
            self.geste = None;
        }
        let mut faite = None;
        if let Some(g) = &mut self.geste {
            let v = g.suivre((x - w.x) as f64, (y - w.y) as f64, &self.pas, self.pas_t);
            if let Some(e) = &mut self.os {
                if let Some(o) = e.objet.and_then(|k| self.projet.objets.get_mut(k)) {
                    if e.pose {
                        poser_os(o, e, &self.depart_pose, v);
                    } else {
                        poser_bouts(o, &e.bouts, &self.depart_os, v);
                    }
                    e.version += 1;
                }
            } else {
                match &mut self.travail {
                    Some(t) => t.poser(&g.indices, &g.sommets, v),
                    None => {
                        ed::poser(&mut self.projet.objets, &g.indices, &g.departs, v);
                        ed::suivre(&mut self.projet.objets, &self.suivis);
                    }
                }
            }
            faite = Some(v);
        }
        if !pose {
            cx.ui.lacher_prise();
            if let (Some(g), Some(v)) = (self.geste.take(), faite) {
                self.a_sauver = true;
                self.derniere = Derniere::du_geste(&g, v);
            }
        }
    }

    /// Boîte lâchée : elle ajoute à la sélection, ou en retire.
    fn tracer_boite(&mut self, w: Rect) {
        let Some(b) = self.boite.take() else { return };
        if (b[2] - b[0]).abs() <= 2.0 && (b[3] - b[1]).abs() <= 2.0 {
            return;
        }
        let vue = self.vue_ecran(w);
        let r = [b[0].min(b[2]) - w.x, b[1].min(b[3]) - w.y, b[0].max(b[2]) - w.x, b[1].max(b[3]) - w.y].map(|v| v as f64);
        let (retire, rayons) = (self.boite_retire, self.rayons);
        if self.os.is_some() {
            return self.boite_os(r, &vue, retire);
        }
        if let Some(t) = &mut self.travail {
            let m = t.boite(r, &vue, retire, rayons);
            return self.dire(m);
        }
        let mut n = 0;
        for k in self.affiches() {
            let Some(b) = self.boite_monde(&[k]) else { continue };
            let c = [0, 1, 2].map(|i| (b.min[i] + b.max[i]) / 2.0);
            if !vue.ecran(c).is_some_and(|e| e[0] >= r[0] && e[0] <= r[2] && e[1] >= r[1] && e[1] <= r[3]) {
                continue;
            }
            let dedans = self.choix.contains(&k);
            if retire && dedans {
                self.choix.retain(|x| *x != k);
                n += 1;
            }
            if !retire && !dedans {
                self.choix.push(k);
                self.projet.actif = Some(k);
                n += 1;
            }
        }
        self.dire(format!("Boîte : {}{n} objet{}", if retire { "−" } else { "+" }, if n > 1 { "s" } else { "" }));
    }

    fn appui_long(&mut self, cx: &mut Cx, w: Rect, (x, y): (f32, f32)) {
        if !w.contains(x, y) {
            return;
        }
        if self.os.is_some() {
            let (vue, r) = (self.vue_ecran(w), self.rayon(cx.dp));
            let t = self.os_sous((x - w.x) as f64, (y - w.y) as f64, &vue, r);
            if let Some(e) = &mut self.os {
                e.bouts.clear();
                e.version += 1;
            }
            if let Some(t) = t {
                self.toucher_os(&t);
            }
            return;
        }
        if self.travail.is_some() {
            let (vue, r, ry) = (self.vue_ecran(w), self.rayon(cx.dp), self.rayons);
            if let Some(t) = &mut self.travail {
                let el = t.element_sous((x - w.x) as f64, (y - w.y) as f64, &vue, r, ry);
                t.seul(el);
            }
            return;
        }
        let k = self.objet_sous(w, x, y);
        self.choix = k.into_iter().collect();
        if k.is_some() {
            self.projet.actif = k;
        }
    }

    fn appui(&mut self, cx: &mut Cx, w: Rect, t: crate::ui::Tap) {
        if !w.contains(t.x, t.y) {
            return;
        }
        // Les bouts d'axes et les anneaux ne servent qu'à glisser
        let vue = self.vue_ecran(w);
        let (px, py) = ((t.x - w.x) as f64, (t.y - w.y) as f64);
        let h = self.poignee(&vue, cx.dp).and_then(|p| p.sous(px, py, &vue, cx.dp as f64));
        if h.is_some_and(|h| h != Prise::O) {
            return;
        }
        let (r, ry, chemin) = (self.rayon(cx.dp), self.rayons, self.selection == Selection::Chemin);
        if self.os.as_ref().is_some_and(|e| e.objet.is_none()) {
            // Rien d'ouvert : un appui prend l'armature touchée
            if let Some(k) = self.objet_sous(w, t.x, t.y).filter(|k| self.projet.objets[*k].armature.is_some()) {
                self.ouvrir_armature(k);
            } else if t.double {
                self.recadrer();
            }
            return;
        }
        if self.os.is_some() {
            match self.os_sous(px, py, &vue, r) {
                Some(_) if t.double => {}
                Some(x) => self.toucher_os(&x),
                None if t.double => self.recadrer(),
                None => {}
            }
            return;
        }
        if let Some(c) = &mut self.travail {
            let el = c.element_sous(px, py, &vue, r, ry);
            if t.double {
                if el.is_none() {
                    self.recadrer();
                }
                return;
            }
            let Some(el) = el else { return };
            if chemin {
                if let Some(m) = c.chemin(el) {
                    return self.dire(m);
                }
            }
            c.basculer(el);
            return;
        }
        match self.objet_sous(w, t.x, t.y) {
            Some(k) if t.double => {
                if !self.choix.contains(&k) {
                    self.choix = vec![k];
                }
                self.projet.actif = Some(k);
                self.entrer_geometrie();
            }
            Some(k) => self.basculer_objet(k),
            None if t.double => self.recadrer(),
            None => {}
        }
    }

    fn ajouter(&mut self, nom: &str, m: Maillage) {
        self.sortir_geometrie();
        self.memoriser();
        self.poser_objet(nom, m);
    }

    fn poser_objet(&mut self, nom: &str, m: Maillage) {
        let p = &mut self.projet;
        let mut o = Objet::neuf(&ed::nom_libre(&p.objets, nom), m);
        o.position = p.curseur;
        if let Some(s) = p.actif.and_then(|k| p.objets.get(k)).map(|a| a.set.clone()).or_else(|| p.sets.first().map(|s| s.nom.clone())) {
            o.set = s;
        }
        p.objets.push(o);
        p.actif = Some(p.objets.len() - 1);
        self.choix = vec![p.objets.len() - 1];
        self.a_sauver = true;
    }

    /// Armature sur le curseur. Ajoutée depuis le mode Armature, elle s'y ouvre, et
    /// l'annulation y revient.
    pub fn ajouter_armature(&mut self, nom: &str, a: Armature) {
        if self.os.is_some() {
            self.memoriser();
            self.poser_objet(nom, a.enveloppe());
        } else {
            self.ajouter(nom, a.enveloppe());
        }
        let k = self.projet.objets.len() - 1;
        self.projet.objets[k].armature = Some(a);
        if self.os.is_some() {
            self.ouvrir_armature(k);
        }
    }

    /// Copies sur place : les maillages restent partagés jusqu'à ce que l'un change.
    fn dupliquer(&mut self) {
        if self.choix.is_empty() {
            return self.dire("Aucun objet sélectionné".into());
        }
        self.memoriser();
        let p = &mut self.projet;
        let mut ids: Vec<(String, String)> = Vec::new();
        self.choix = self
            .choix
            .iter()
            .map(|&k| {
                let mut o = p.objets[k].clone();
                o.nom = ed::nom_libre(&p.objets, &o.nom);
                ids.push((std::mem::replace(&mut o.id, crate::projet::nouvel_id()), o.id.clone()));
                p.objets.push(o);
                p.objets.len() - 1
            })
            .collect();
        // Une armature copiée avec ses objets garde les copies ; seuls, ils restent sur l'originale
        for &k in &self.choix {
            if let Some(l) = &mut p.objets[k].liaison {
                if let Some((_, n)) = ids.iter().find(|(a, _)| *a == l.armature) {
                    l.armature = n.clone();
                }
            }
        }
        p.actif = self.choix.last().copied();
        self.a_sauver = true;
        self.dire("Dupliqué : glissez pour écarter la copie".into());
    }

    fn supprimer(&mut self) {
        if self.choix.is_empty() {
            return self.dire("Aucun objet sélectionné".into());
        }
        self.memoriser();
        let retires = std::mem::take(&mut self.choix);
        self.retirer_objets(&retires);
        self.a_sauver = true;
    }

    /// Retire des objets ; l'actif et la sélection gardent leurs objets.
    fn retirer_objets(&mut self, retires: &[usize]) {
        let avant = |k: usize| k - retires.iter().filter(|r| **r < k).count();
        let p = &mut self.projet;
        p.actif = p.actif.filter(|a| !retires.contains(a)).map(avant);
        self.choix = self.choix.iter().copied().filter(|k| !retires.contains(k)).map(avant).collect();
        let mut k = 0;
        p.objets.retain(|_| {
            k += 1;
            !retires.contains(&(k - 1))
        });
        p.verifier_liaisons();
    }

    /// Objets choisis qu'on peut lier : pas les armatures.
    fn cibles_liaison(&self) -> Vec<usize> {
        self.choix.iter().copied().filter(|k| self.projet.objets[*k].armature.is_none()).collect()
    }

    /// Ouvre la page de liaison, sur l'armature choisie, sinon celle de la liaison en
    /// place, sinon la seule du projet ; une nouvelle armature propose tous ses os.
    pub fn ouvrir_liaison(&mut self) {
        let p = &self.projet;
        let objets = || self.choix.iter().map(|k| &p.objets[*k]);
        let lien = objets().find_map(|o| o.liaison.as_ref());
        let armatures = || p.objets.iter().filter(|o| o.armature.is_some());
        let seule = (armatures().count() == 1).then(|| armatures().next()).flatten();
        let armature = objets().find(|o| o.armature.is_some()).or(lien.and_then(|l| p.objet_id(&l.armature))).or(seule);
        let page = match (armature, lien) {
            (Some(a), Some(l)) if a.id == l.armature => PageLiaison { armature: Some(a.id.clone()), os: l.os.clone() },
            (Some(a), _) => PageLiaison { armature: Some(a.id.clone()), os: a.armature.iter().flat_map(|x| &x.os).map(|b| b.id.clone()).collect() },
            (None, _) => PageLiaison { armature: None, os: Vec::new() },
        };
        self.liaison = Some(page);
        self.contenu = Contenu::Contexte;
        self.onglet_contexte = 0;
        self.shell.end.open = true;
    }

    /// Lie les objets choisis selon la page ouverte.
    pub fn lier(&mut self) {
        let Some(PageLiaison { armature: Some(armature), os }) = self.liaison.clone() else { return };
        let l = self.cibles_liaison();
        let Some(nom) = self.projet.objet_id(&armature).map(|o| o.nom.clone()) else { return };
        if os.is_empty() || l.is_empty() {
            return;
        }
        self.memoriser();
        for &k in &l {
            self.projet.objets[k].liaison = Some(crate::projet::Liaison { armature: armature.clone(), os: os.clone() });
        }
        self.liaison = None;
        self.a_sauver = true;
        let qui = if let [k] = l[..] { format!("« {} » lié", self.projet.objets[k].nom) } else { format!("{} objets liés", l.len()) };
        let n = os.len();
        self.dire(format!("{qui} à « {nom} » : {n} os"));
    }

    fn delier(&mut self) {
        let l: Vec<usize> = self.choix.iter().copied().filter(|k| self.projet.objets[*k].liaison.is_some()).collect();
        if l.is_empty() {
            return;
        }
        self.memoriser();
        for &k in &l {
            self.projet.objets[k].liaison = None;
        }
        self.a_sauver = true;
        let qui = if let [k] = l[..] { format!("« {} » délié", self.projet.objets[k].nom) } else { format!("{} objets déliés", l.len()) };
        self.dire(qui);
    }

    fn appliquer_echelle(&mut self) {
        let l: Vec<usize> = self.choix.iter().copied().filter(|k| { let o = &self.projet.objets[*k]; o.armature.is_none() && o.echelle != [1.0; 3] }).collect();
        if l.is_empty() {
            return self.dire("Aucun objet choisi n'a d'échelle à appliquer".into());
        }
        self.memoriser();
        for &k in &l {
            ed::figer_echelle(&mut self.projet.objets[k]);
        }
        self.a_sauver = true;
        let m = if let [k] = l[..] { format!("Échelle appliquée à « {} »", self.projet.objets[k].nom) } else { format!("Échelle appliquée à {} objets", l.len()) };
        self.dire(m);
    }

    /// Ouvre le pavé numérique sur une valeur.
    /// Ouvre le pavé sur un champ, avec sa valeur du moment.
    pub fn saisir(&mut self, cible: Champ) {
        let Some(texte) = self.valeur_champ(cible) else { return };
        let curseur = texte.chars().count();
        self.pave = Some(Pave { depart: texte.clone(), texte, neuf: true, curseur, cible, alpha: cible.libre(), maj: false });
    }

    /// Valeur du champ, telle qu'il l'affiche ; rien si le champ n'existe pas pour
    /// la sélection du moment.
    fn valeur_champ(&self, c: Champ) -> Option<String> {
        let objet = self.seul().map(|k| &self.projet.objets[k]);
        let modif = |i: usize| self.objet_modifie().and_then(|k| modifs::liste(&self.projet.objets[k]).get(i));
        match c {
            Champ::Primitive(i, _) => {
                let a = self.ajout.as_ref()?;
                return match a.valeurs.get(a.prim.champs().get(i)?.cle)? {
                    V::N(x) => Some(primitives::virgule(*x)),
                    _ => None,
                };
            }
            Champ::ModifN(i) => return modif(i)?.get("n")?.num().map(chiffres),
            Champ::ModifDecalage(i, a) => return modif(i)?.get("decalage")?.nombres::<3>().map(|d| chiffres(d[a])),
            _ => {}
        }
        let v = match (c, self.derniere) {
            (Champ::Position(k), _) => {
                let p = self.points();
                (!p.is_empty()).then(|| ed::centre(&p)[k])?
            }
            (Champ::Rotation(k), _) => ed::euler(objet?.rotation)[k],
            (Champ::Dimension(k), _) => self.base(self.seul()?)[k] * objet?.echelle[k],
            (Champ::PasTranslation, _) => self.pas_translation(),
            (Champ::PasAngle, _) => self.pas.angle,
            (Champ::PasEchelle, _) => self.pas.echelle,
            (Champ::Coupes, Some(Derniere::Subdivision { coupes } | Derniere::Boucle { coupes, .. })) => coupes as f64,
            (Champ::Glissement, Some(Derniere::Boucle { glissement, .. })) => glissement,
            (Champ::Angle, Some(Derniere::Revolution { angle: a, .. } | Derniere::Rotation { degres: a, .. })) => a,
            (Champ::Decalage(k), Some(Derniere::Translation { d })) => d[k],
            (Champ::Facteur(k), Some(Derniere::Echelle { s, .. })) => s[k],
            (Champ::Pivot(k), Some(Derniere::Echelle { pivot, .. } | Derniere::Rotation { pivot, .. })) => pivot[k],
            (Champ::Segments, Some(Derniere::Revolution { segments, .. })) => segments as f64,
            (Champ::Tolerance, Some(Derniere::Proches { tolerance })) => return Some(format!("{tolerance}").replace('.', ",")),
            (Champ::Nom, _) => return Some(objet?.nom.clone()),
            (Champ::Projet, _) => return Some(self.nom.clone()),
            (Champ::Couleur(i), _) => return Some(theme::hex(coul(theme::LIGNES[i].c))),
            (Champ::OsNom, _) => return self.os_actif().map(|x| x.1.nom.clone()),
            (Champ::PoseNom, _) => return self.armature_os().and_then(|(_, _, a)| a.poses_nommees().into_iter().nth(self.pose_choisie?)).map(|p| p.0),
            (Champ::OsLongueur, _) => self.os_actif().map(|(o, x)| x.longueur * o.echelle[0])?,
            (Champ::OsLimite(k, max), _) => match self.os_actif()?.1.limites()[k] {
                Limite::Bornee(a, b) => if max { b } else { a },
                _ => return None,
            },
            (Champ::SetNom, _) => return match &self.page_liste {
                PageListe::Set(n) => Some(n.clone()),
                _ => None,
            },
            (Champ::MatNom, _) => return self.materiau_actif().map(|n| nom_mat(&n).to_string()),
            (Champ::MatCode(k), _) => return self.materiau_actif().and_then(|n| self.projet.materiau(&n).map(|m| couleur_mat(m, k).to_string())),
            _ => return None,
        };
        Some(chiffres(v))
    }

    /// Champ suivant du même groupe : les axes d'une section, les pas, les réglages
    /// de la dernière opération.
    fn champ_suivant(&self, c: Champ) -> Option<Champ> {
        let s = match c {
            Champ::Position(k) if k < 2 => Champ::Position(k + 1),
            Champ::Rotation(k) if k < 2 => Champ::Rotation(k + 1),
            Champ::Dimension(k) if k < 2 => Champ::Dimension(k + 1),
            Champ::PasTranslation => Champ::PasAngle,
            Champ::PasAngle => Champ::PasEchelle,
            Champ::Coupes => Champ::Glissement,
            Champ::Angle if matches!(self.derniere, Some(Derniere::Rotation { .. })) => Champ::Pivot(0),
            Champ::Angle => Champ::Segments,
            Champ::Decalage(k) if k < 2 => Champ::Decalage(k + 1),
            Champ::Facteur(k) if k < 2 => Champ::Facteur(k + 1),
            Champ::Facteur(_) => Champ::Pivot(0),
            Champ::Pivot(k) if k < 2 => Champ::Pivot(k + 1),
            Champ::OsLimite(k, false) => Champ::OsLimite(k, true),
            Champ::ModifDecalage(i, k) if k < 2 => Champ::ModifDecalage(i, k + 1),
            Champ::Primitive(i, _) => {
                // Le champ numérique affiché suivant
                let a = self.ajout.as_ref()?;
                let l = a.prim.champs();
                let (j, c) = l.iter().enumerate().skip(i + 1).find(|(_, c)| (c.si)(&a.valeurs) && matches!(a.valeurs.get(c.cle), Some(V::N(_))))?;
                Champ::Primitive(j, c.libelle)
            }
            _ => return None,
        };
        self.valeur_champ(s).map(|_| s)
    }

    /// Nouveau nom d'un objet, unique dans la scène ; ses cibles booléennes le suivent.
    fn renommer(&mut self, k: usize, nom: &str) {
        let nom = nom.trim();
        if nom.is_empty() {
            return self.dire("Un objet garde un nom".into());
        }
        let ancien = self.projet.objets[k].nom.clone();
        if nom == ancien {
            return;
        }
        self.memoriser();
        let libre = ed::nom_libre(&self.projet.objets, nom);
        let noms = [(ancien, libre.clone())];
        for o in &mut self.projet.objets {
            o.suivre_cibles(&noms);
        }
        self.projet.objets[k].nom = libre.clone();
        self.a_sauver = true;
        if libre != nom {
            self.dire(format!("« {nom} » est pris : « {libre} »"));
        }
    }

    /// Base de l'objet : son maillage, avant échelle.
    fn base(&self, k: usize) -> [f64; 3] {
        self.maillage_base(k).boite_englobante().map_or([0.0; 3], |b| [0, 1, 2].map(|c| b.max[c] - b.min[c]))
    }

    fn appliquer_champ(&mut self, c: Champ, v: f64) {
        match c {
            Champ::Primitive(i, _) => {
                if let Some(a) = &mut self.ajout {
                    if let Some(c) = a.prim.champs().get(i) {
                        a.valeurs.insert(c.cle, V::N(v));
                    }
                }
                return self.relancer_ajout();
            }
            Champ::ModifN(i) => {
                if let Some(k) = self.objet_modifie() {
                    self.commander_modif(k, CmdModif::Fixer(i, "n", J::Num(v.round().clamp(1.0, 200.0))));
                }
                return;
            }
            Champ::ModifDecalage(i, a) => {
                if let Some(k) = self.objet_modifie() {
                    let mut d = modifs::liste(&self.projet.objets[k]).get(i).and_then(|m| m.get("decalage")).and_then(J::nombres::<3>).unwrap_or([0.0; 3]);
                    d[a] = v;
                    self.commander_modif(k, CmdModif::Fixer(i, "decalage", J::Liste(d.iter().map(|x| J::Num(*x)).collect())));
                }
                return;
            }
            _ => {}
        }
        if let Some(d) = self.derniere {
            let d = match (c, d) {
                (Champ::Coupes, Derniere::Subdivision { .. }) => Some(Derniere::Subdivision { coupes: (v as usize).clamp(1, 64) }),
                (Champ::Coupes, Derniere::Boucle { arete, glissement, uniforme, inverser, .. }) => Some(Derniere::Boucle { arete, coupes: (v as usize).clamp(1, 64), glissement, uniforme, inverser }),
                (Champ::Glissement, Derniere::Boucle { arete, coupes, uniforme, inverser, .. }) => Some(Derniere::Boucle { arete, coupes, glissement: v.clamp(-0.99, 0.99), uniforme, inverser }),
                (Champ::Angle, Derniere::Revolution { axe, vue, inverse, segments, fermer, .. }) => Some(Derniere::Revolution { axe, vue, angle: v.clamp(0.01, 360.0), inverse, segments, fermer }),
                (Champ::Segments, Derniere::Revolution { axe, vue, angle, inverse, fermer, .. }) => Some(Derniere::Revolution { axe, vue, angle, inverse, segments: (v as usize).clamp(1, 256), fermer }),
                (Champ::Tolerance, Derniere::Proches { .. }) => Some(Derniere::Proches { tolerance: v.max(0.0) }),
                (Champ::Decalage(k), Derniere::Translation { mut d }) => {
                    d[k] = v;
                    Some(Derniere::Translation { d })
                }
                (Champ::Facteur(k), Derniere::Echelle { mut s, pivot }) => {
                    s[k] = v.max(0.01); // comme au doigt : ni facteur nul, ni faces retournées
                    Some(Derniere::Echelle { s, pivot })
                }
                (Champ::Pivot(k), Derniere::Echelle { s, mut pivot }) => {
                    pivot[k] = v;
                    Some(Derniere::Echelle { s, pivot })
                }
                (Champ::Pivot(k), Derniere::Rotation { degres, axe, vue, mut pivot }) => {
                    pivot[k] = v;
                    Some(Derniere::Rotation { degres, axe, vue, pivot })
                }
                (Champ::Angle, Derniere::Rotation { axe, vue, pivot, .. }) => Some(Derniere::Rotation { degres: v, axe, vue, pivot }),
                _ => None,
            };
            if let Some(d) = d {
                return self.rejouer(d);
            }
        }
        if matches!(c, Champ::Position(_) | Champ::Rotation(_) | Champ::Dimension(_)) && self.selection_posee() {
            return;
        }
        match c {
            Champ::Position(k) => {
                let pts = self.points();
                if pts.is_empty() {
                    return;
                }
                let d = v - ed::centre(&pts)[k];
                if d == 0.0 {
                    return;
                }
                let mut t = [0.0; 3];
                t[k] = d;
                self.deplacer_selection(t);
            }
            Champ::Rotation(k) => {
                let Some(i) = self.seul() else { return };
                let mut deg = ed::euler(self.projet.objets[i].rotation);
                if deg[k] == v {
                    return;
                }
                deg[k] = v;
                self.memoriser();
                let s = ed::suivis(&self.projet.objets, &[i]);
                self.projet.objets[i].rotation = ed::depuis_euler(deg);
                ed::suivre(&mut self.projet.objets, &s);
            }
            Champ::Dimension(k) => {
                let Some(i) = self.seul() else { return };
                let base = self.base(i)[k];
                let o = &self.projet.objets[i];
                if v <= 0.0 || base <= 1e-9 || (v / base - o.echelle[k]).abs() < 1e-12 {
                    return;
                }
                let f = v / base;
                self.memoriser();
                let s = ed::suivis(&self.projet.objets, &[i]);
                let o = &mut self.projet.objets[i];
                if o.armature.is_some() {
                    o.echelle = [f; 3];
                } else {
                    o.echelle[k] = f;
                }
                ed::suivre(&mut self.projet.objets, &s);
            }
            Champ::OsLongueur => self.saisir_longueur_os(v),
            Champ::OsLimite(k, max) => self.saisir_limite(k, max, v),
            Champ::PasTranslation => self.pas.translation = (1e-6..=1e6).contains(&v).then_some(v),
            Champ::PasAngle => self.pas.angle = v.clamp(0.1, 180.0),
            Champ::PasEchelle => self.pas.echelle = v.clamp(0.001, 10.0),
            _ => {}
        }
        self.a_sauver = true;
    }

    /// OK : applique la saisie si elle a changé, puis passe au champ suivant du groupe.
    /// Une saisie qui n'est pas un nombre laisse le pavé ouvert.
    fn valider_pave(&mut self) {
        let Some(p) = self.pave.take() else { return };
        let suivant = self.champ_suivant(p.cible);
        if p.texte != p.depart {
            if p.cible == Champ::Nom {
                if let Some(k) = self.seul() {
                    self.renommer(k, &p.texte);
                }
            } else if p.cible == Champ::Projet {
                self.nom = p.texte.trim().to_string();
                self.a_sauver = true;
            } else if p.cible == Champ::OsNom {
                self.renommer_os(&p.texte);
            } else if p.cible == Champ::PoseNom {
                self.renommer_pose(&p.texte);
            } else if p.cible == Champ::SetNom {
                if let PageListe::Set(n) = self.page_liste.clone() {
                    self.renommer_set(&n, &p.texte);
                }
            } else if p.cible == Champ::MatNom {
                if let Some(n) = self.materiau_actif() {
                    self.renommer_materiau(&n, &p.texte);
                }
            } else if let Champ::MatCode(k) = p.cible {
                let Some(c) = theme::lire_hex(&p.texte) else {
                    self.pave = Some(p);
                    return self.dire("Code attendu : #rrggbb".into());
                };
                if let Some(n) = self.materiau_actif() {
                    self.mat_saisie = false;
                    self.regler_materiau(&n, |m| *couleur_mat_mut(m, k) = theme::hex(c));
                    self.mat_saisie = false;
                    self.mat_couleur = Some((n, k, theme::vers_tsl(c)));
                }
            } else if let Champ::Couleur(i) = p.cible {
                let Some(c) = theme::lire_hex(&p.texte) else {
                    self.pave = Some(p);
                    return self.dire("Code attendu : #rrggbb".into());
                };
                self.choisir_couleur(i, Some(c));
                self.couleur_ouverte = Some((i, theme::vers_tsl(c)));
                self.reglages_changes = true;
            } else {
                match p.texte.replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite()) {
                    Some(v) => self.appliquer_champ(p.cible, v),
                    None => {
                        self.pave = Some(p);
                        return self.dire("Nombre attendu".into());
                    }
                }
            }
        }
        if let Some(c) = suivant {
            self.saisir(c);
        }
    }

    /// Objets affichés au monde, chacun teinté de son matériau.
    fn liste_export(&self, selection: bool) -> Vec<(String, Maillage)> {
        let p = self.projet_range();
        let (cibles, mut cache) = (modifs::cibles(&p), self.evals.clone());
        // Une cible ne compte que pour ce qu'elle retranche, sauf si on la choisit
        p.objets
            .iter()
            .enumerate()
            .filter(|(k, o)| p.affiche(o) && o.armature.is_none() && (if selection { self.choix.contains(k) } else { !cibles.contains(k) }))
            .map(|(k, o)| {
                let mut m = ed::vers_monde_de(o, &modifs::evaluer(&p, k, &[], &mut cache));
                let mat: maillage::Mat = Arc::from(o.materiau.as_str());
                for f in &mut m.faces {
                    f.m = mat.clone();
                }
                (o.nom.clone(), m)
            })
            .collect()
    }

    fn nom_fichier(&self) -> &str {
        if self.nom.is_empty() { "scene" } else { &self.nom }
    }

    fn deposer(&mut self, nom: String, octets: Vec<u8>) {
        self.sorties.push(Sortie { nom, mime: MIME, octets });
    }

    /// Repart d'un cube seul ; annulable, sauf le nom du projet, comme dans GDA.
    fn nouvelle_scene(&mut self) {
        self.memoriser();
        let n = Projet::neuf();
        let p = &mut self.projet;
        (p.objets, p.sets, p.materiaux, p.actif) = (n.objets, n.sets, n.materiaux, n.actif);
        self.travail = None;
        self.os = None;
        self.version_geo = None;
        self.contexte_geo = None;
        self.boite = None;
        self.selection = Selection::Toucher;
        self.choix.clear();
        self.nom.clear();
        self.a_sauver = true;
        self.cam = self.cadre(&[]).unwrap_or(self.cam);
    }

    fn enregistrer(&mut self) {
        let (n, t) = (format!("{}.gda", self.nom_fichier()), self.projet_range().ecrire());
        self.deposer(n, t.into_bytes());
    }

    fn mats_mtl(&self) -> Vec<(String, maillage::MatMtl)> {
        self.projet.materiaux.iter().map(|(n, m)| (n.clone(), m.vers_mtl())).collect()
    }

    /// Matériaux et images de ce qui s'exporte. Une face sans UV d'un objet texturé
    /// passe à un matériau uni : ailleurs, elle prendrait un coin de l'image.
    fn textures_export(&self, liste: &mut [(String, Maillage)]) -> (Vec<(String, maillage::MatMtl)>, Vec<(String, Arc<Vec<u8>>)>) {
        let noms = texture::noms_fichiers(&self.projet.textures);
        let mut mats = self.mats_mtl();
        let mut images: Vec<(String, Arc<Vec<u8>>)> = Vec::new();
        for f in liste.iter_mut().flat_map(|(_, m)| &mut m.faces) {
            let id = self.projet.materiau(&f.m).and_then(|m| m.texture.as_ref()).filter(|id| noms.contains_key(*id));
            let Some(id) = id else { continue };
            if f.uv.is_empty() {
                let uni = format!("{}_sans_texture", f.m);
                if !mats.iter().any(|(n, _)| *n == uni) {
                    let copie = mats.iter().find(|(n, _)| **n == *f.m).map(|(_, m)| m.clone()).unwrap_or_default();
                    mats.push((uni.clone(), maillage::MatMtl { texture: None, ..copie }));
                }
                f.m = Arc::from(uni);
                continue;
            }
            let nom = &noms[id];
            if !images.iter().any(|(n, _)| n == nom) {
                images.push((nom.clone(), self.projet.textures[id].octets.clone()));
            }
            if let Some((_, m)) = mats.iter_mut().find(|(n, _)| **n == *f.m) {
                m.texture = Some(nom.clone());
            }
        }
        (mats, images)
    }

    /// Exporte la scène, ou les seuls objets choisis.
    fn exporter(&mut self, ext: &str, selection: bool) {
        let mut liste = self.liste_export(selection);
        if liste.iter().all(|(_, m)| m.faces.is_empty()) {
            return self.dire("Aucune face à exporter".into());
        }
        // Un objet seul donne son nom au fichier
        let titre = match (selection, &liste[..]) {
            (true, [(nom, _)]) => nom.clone(),
            (true, _) => format!("{}-selection", self.nom_fichier()),
            _ => self.nom_fichier().to_string(),
        }
        .replace('/', "-");
        let (mut mats, images) = self.textures_export(&mut liste);
        let tout = || Maillage::assembler(liste.iter().map(|(_, m)| m));
        let octets = match ext {
            // Sans son .mtl et ses images, un OBJ perdrait ses textures : ils le précèdent
            "obj" if !images.is_empty() => {
                let mtl = format!("{}.mtl", titre.split_whitespace().collect::<Vec<_>>().join("_"));
                let t = Maillage::objets_vers_obj(&liste, &titre, Some(&mtl)).into_bytes();
                mats.retain(|(n, _)| liste.iter().any(|(_, m)| m.faces.iter().any(|f| *f.m == **n)));
                for (n, o) in images {
                    self.deposer(n, (*o).clone());
                }
                self.deposer(mtl, Maillage::vers_mtl(&mats, &titre).into_bytes());
                t
            }
            "obj" => Maillage::objets_vers_obj(&liste, &titre, Some(MTL)).into_bytes(),
            "stl" => tout().vers_stl(&titre),
            _ => tout().vers_glb_images(&titre, &mats, &images),
        };
        self.deposer(format!("{titre}.{ext}"), octets);
    }

    fn exporter_mtl(&mut self) {
        let mut liste = self.liste_export(false);
        let (mats, images) = self.textures_export(&mut liste);
        for (n, o) in images {
            self.deposer(n, (*o).clone());
        }
        let t = Maillage::vers_mtl(&mats, self.nom_fichier());
        self.deposer(MTL.to_string(), t.into_bytes());
    }

    fn affiches(&self) -> Vec<usize> {
        (0..self.projet.objets.len()).filter(|&k| self.projet.affiche(&self.projet.objets[k])).collect()
    }

    /// Boîte au monde des objets donnés : leurs sommets, transformés.
    fn boite_monde(&self, liste: &[usize]) -> Option<Boite> {
        let mut b: Option<Boite> = None;
        let mut plus = |q: P3| {
            let x = b.get_or_insert(Boite { min: q, max: q });
            for c in 0..3 {
                x.min[c] = x.min[c].min(q[c]);
                x.max[c] = x.max[c].max(q[c]);
            }
        };
        for &k in liste {
            // Objet édité : ses sommets sont dans la copie de travail, au monde
            if let Some(t) = self.travail.as_ref().filter(|t| t.edites.contains(&k)) {
                t.m.sommets.iter().zip(&t.proprio).filter(|x| *x.1 == k).for_each(|x| plus(*x.0));
                continue;
            }
            let m = self.objet_vu(k).matrice();
            for p in &self.maillage_vu(k).sommets {
                plus(vue::appliquer(&m, *p));
            }
        }
        b
    }

    /// Caméra qui cadre les objets choisis, ou `liste` s'ils sont donnés.
    fn cadre(&self, liste: &[usize]) -> Option<Camera> {
        let tous = self.affiches();
        let b = self.boite_monde(if liste.is_empty() { &tous } else { liste })?;
        Some(self.cadre_boite(&b))
    }

    /// Caméra qui cadre une boîte du monde.
    fn cadre_boite(&self, b: &Boite) -> Camera {
        let cible = [0, 1, 2].map(|k| (b.min[k] + b.max[k]) / 2.0);
        let d = [b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]];
        let r = ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / 2.0).max(1e-3);
        let asp = (self.espace.w as f64 / self.espace.h.max(1.0) as f64).min(1.0);
        Camera { cible, distance: r / (CHAMP.to_radians() / 2.0).sin() / asp * 1.15, ..self.cam }
    }

    /// Pose la caméra : pour les aperçus.
    pub fn cadrer_cible(&mut self, cible: P3, distance: f64) {
        (self.cam.cible, self.cam.distance) = (cible, distance);
        self.vol = None;
    }

    pub fn recadrer(&mut self) {
        let liste = self.travail.as_ref().map_or(self.choix.clone(), |t| t.edites.clone());
        if let Some(arrivee) = self.cadre(&liste) {
            self.vol = Some(Vol { depart: self.cam, arrivee, t: 0.0 });
        }
    }

    pub fn frame(&mut self, cx: &mut Cx, screen: Rect, dt: f32) -> Scene {
        // La vue UV vit dans le mode Géométrie, et y prend la moitié de l'écran
        if self.contenu == Contenu::Uv && self.travail.is_none() {
            self.contenu = Contenu::Contexte;
            self.shell.end.open = false;
        }
        if self.contenu == Contenu::Os && self.travail.is_some() {
            self.contenu = Contenu::Contexte;
            self.shell.end.open = false;
        }
        self.shell.large = self.contenu == Contenu::Uv;
        // Un os posé ne fait que tourner
        if self.os.as_ref().is_some_and(|e| e.pose) {
            self.mode = Mode::Rotation;
        }
        let l = self.shell.layout(screen, dt);
        let w = l.workspace;
        self.espace = w;
        self.gestes(cx, w, dt);
        self.voler(dt);
        if let Some((_, reste)) = &mut self.message {
            *reste -= dt;
            if *reste <= 0.0 {
                self.message = None;
            }
        }

        self.origines(cx, w);
        self.dessiner_reperes(cx, w);
        self.dessiner_limites(cx, w);
        self.dessiner_poignee(cx, w);
        self.flottants(cx, w);
        self.panneau_debut(cx, &l);
        self.panneau_fin(cx, &l);
        self.poignees(cx, &l, screen);
        if self.geste.is_some() {
            cx.ui.region_add(id("pendant-geste"), screen, Sense::Sink);
        }
        self.dessiner_pave(cx, screen);
        self.scene(cx.dp, w)
    }

    fn voler(&mut self, dt: f32) {
        let Some(v) = &mut self.vol else { return };
        v.t = (v.t + dt / ANIM_CAMERA).min(1.0);
        let k = 1.0 - (1.0 - v.t as f64).powi(3); // ralentit à l'arrivée
        let (a, b) = (v.depart, v.arrivee);
        self.cam.distance = a.distance + (b.distance - a.distance) * k;
        for c in 0..3 {
            self.cam.cible[c] = a.cible[c] + (b.cible[c] - a.cible[c]) * k;
        }
        if v.t >= 1.0 {
            self.vol = None;
        }
    }

    fn repere(&self) -> (P3, P3, P3, P3) {
        let c = &self.cam;
        let (sl, cl, sh, ch) = (c.lacet.sin(), c.lacet.cos(), c.hauteur.sin(), c.hauteur.cos());
        let dos = [sl * ch, sh, cl * ch];
        let oeil = [c.cible[0] + dos[0] * c.distance, c.cible[1] + dos[1] * c.distance, c.cible[2] + dos[2] * c.distance];
        (oeil, [cl, 0.0, -sl], [-sl * sh, ch, -cl * sh], dos)
    }

    fn gestes(&mut self, cx: &mut Cx, w: Rect, dt: f32) {
        if let Some(p) = cx.ui.avancer(dt) {
            self.appui_long(cx, w, p);
        }
        if let Some(d) = cx.ui.take_glisse() {
            self.commencer(cx, w, d);
        }
        self.suivre(cx, w);
        let mut g = cx.ui.take_gesture();
        // Mode Boîte : un doigt trace, deux tournent la vue
        if self.selection == Selection::Boite {
            g.orbit = (g.pan.0 * 2.0, g.pan.1 * 2.0);
            g.pan = (0.0, 0.0);
        }
        if g.orbit != (0.0, 0.0) || g.pan != (0.0, 0.0) || g.zoom != 1.0 {
            self.vol = None;
        }
        let k = 2.0 * self.cam.distance * (CHAMP.to_radians() / 2.0).tan() / w.h.max(1.0) as f64;
        let (_, droite, haut, _) = self.repere();
        let c = &mut self.cam;
        c.lacet -= g.orbit.0 as f64 * 0.008;
        c.hauteur = (c.hauteur + g.orbit.1 as f64 * 0.006).clamp(-1.5, 1.5);
        c.distance = (c.distance / g.zoom as f64).clamp(1e-3, 1e7);
        for i in 0..3 {
            c.cible[i] += -g.pan.0 as f64 * k * droite[i] + g.pan.1 as f64 * k * haut[i];
        }
        if let Some(t) = cx.ui.take_tap() {
            self.appui(cx, w, t);
        }
    }

    /// L'objet `k` est une armature au premier plan, et le mode Scène la montre ainsi.
    fn devant(&self, k: usize) -> bool {
        self.travail.is_none() && self.os.is_none() && self.projet.objets.get(k).and_then(|o| o.armature.as_ref()).is_some_and(|a| a.devant())
    }

    /// Objet affiché sous le doigt, le plus proche de l'œil.
    fn objet_sous(&self, w: Rect, x: f32, y: f32) -> Option<usize> {
        let (oeil, droite, haut, dos) = self.repere();
        let t = (CHAMP.to_radians() / 2.0).tan();
        let nx = ((x - w.x) / w.w * 2.0 - 1.0) as f64 * t * (w.w / w.h.max(1.0)) as f64;
        let ny = (1.0 - (y - w.y) / w.h * 2.0) as f64 * t;
        let d = [0, 1, 2].map(|k| droite[k] * nx + haut[k] * ny - dos[k]);
        // Une armature au premier plan passe devant les autres objets
        let mut mieux: Option<(usize, bool, f64)> = None;
        for k in self.affiches() {
            if let Some(t) = vue::toucher(&self.objet_vu(k), &self.maillage_vu(k), oeil, d) {
                let devant = self.devant(k);
                if mieux.is_none_or(|m| (!devant, t) < (!m.1, m.2)) {
                    mieux = Some((k, devant, t));
                }
            }
        }
        mieux.map(|m| m.0)
    }

    fn matrices(&self, w: Rect) -> (Mat4, Mat4) {
        let (oeil, _, _, _) = self.repere();
        let c = &self.cam;
        let f = |p: P3| p.map(|x| x as f32);
        let v = math::look_at(f(oeil), f(c.cible), [0.0, 1.0, 0.0]);
        let (pres, loin) = ((c.distance / 200.0) as f32, (c.distance * 50.0) as f32);
        let p = math::perspective((CHAMP as f32).to_radians(), w.w.max(1.0) / w.h.max(1.0), pres, loin);
        (v, p)
    }

    /// Origines des objets affichés : points ronds, vus à travers tout.
    fn origines(&mut self, cx: &mut Cx, w: Rect) {
        let (v, p) = self.matrices(w);
        let vp = math::mul(&p, &v);
        let d = cx.dp;
        cx.clip(Some(w));
        for k in self.affiches() {
            let o = &self.projet.objets[k];
            if o.armature.is_some() || self.travail.is_some() {
                continue;
            }
            let q = self.objet_vu(k).position.map(|x| x as f32);
            let c = [
                vp[0] * q[0] + vp[4] * q[1] + vp[8] * q[2] + vp[12],
                vp[1] * q[0] + vp[5] * q[1] + vp[9] * q[2] + vp[13],
                vp[3] * q[0] + vp[7] * q[1] + vp[11] * q[2] + vp[15],
            ];
            if c[2] <= 0.0 {
                continue;
            }
            let (x, y) = (w.x + (c[0] / c[2] + 1.0) / 2.0 * w.w, w.y + (1.0 - c[1] / c[2]) / 2.0 * w.h);
            cx.dl.rrect(Rect::new(x - 7.0 * d, y - 7.0 * d, 14.0 * d, 14.0 * d), 7.0 * d, [0, 0, 0, 255]);
            cx.dl.rrect(Rect::new(x - 4.8 * d, y - 4.8 * d, 9.6 * d, 9.6 * d), 4.8 * d, coul(C::Origine));
        }
        cx.clip(None);
    }

    /// Curseur 3D, numéros des sommets choisis, boîte en cours de tracé.
    fn dessiner_reperes(&mut self, cx: &mut Cx, w: Rect) {
        let vue = self.vue_ecran(w);
        let d = cx.dp;
        cx.clip(Some(w));
        if let Some(e) = vue.ecran(self.projet.curseur) {
            let (x, y) = (w.x + e[0] as f32, w.y + e[1] as f32);
            let r = 16.0 * d;
            for k in 0..16 {
                let (a, b) = (k as f32 * std::f32::consts::TAU / 16.0, (k + 1) as f32 * std::f32::consts::TAU / 16.0);
                let c = if k % 2 == 0 { coul(C::Curseur) } else { coul(C::Surface) };
                cx.dl.line([x + r * a.cos(), y + r * a.sin()], [x + r * b.cos(), y + r * b.sin()], 3.0 * d, c);
            }
            for (ux, uy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                cx.dl.line([x + ux * r * 0.6, y + uy * r * 0.6], [x + ux * r * 1.35, y + uy * r * 1.35], 2.0 * d, coul(C::Reticule));
            }
        }
        if let Some(t) = self.travail.as_ref().filter(|t| t.filtres[0]) {
            for (n, i) in t.sel.sommets.iter().take(MAX_NUMEROS).enumerate() {
                if let Some(e) = vue.ecran(t.m.sommets[*i]) {
                    let s = (n + 1).to_string();
                    let (x, y) = (w.x + e[0] as f32 + 9.0 * d, w.y + e[1] as f32 - 11.0 * d);
                    cx.dl.rrect(Rect::new(x - 2.0 * d, y - 9.0 * d, (s.len() as f32 + 1.0) * 8.0 * d, 18.0 * d), 5.0 * d, coul(C::Surface));
                    cx.text_at(x + 2.0 * d, y, &s, 12.5, coul(C::Numero));
                }
            }
        }
        if let Some(b) = self.boite {
            let c = if self.boite_retire { coul(C::AxeX) } else { coul(C::Accent) };
            let ([x0, y0, x1, y1], e) = (b, 2.0 * d);
            cx.dl.line([x0, y0], [x1, y0], e, c);
            cx.dl.line([x1, y0], [x1, y1], e, c);
            cx.dl.line([x1, y1], [x0, y1], e, c);
            cx.dl.line([x0, y1], [x0, y0], e, c);
        }
        cx.clip(None);
    }

    /// En Géométrie, les limites de l'os actif : pour chaque axe limité, un secteur de sa
    /// couleur à la tête de l'os, sur la plage permise depuis le repos.
    fn dessiner_limites(&mut self, cx: &mut Cx, w: Rect) {
        let Some((e, o, a)) = self.armature_os() else { return };
        if e.pose || e.bouts.is_empty() {
            return;
        }
        let Some(x) = e.actif.as_ref().and_then(|i| a.trouver(i)) else { return };
        let l = x.limites();
        if !l.iter().any(|v| matches!(v, Limite::Bornee(..))) {
            return;
        }
        let Some(&(t, q)) = a.repos().get(&x.id) else { return };
        let (vue, mx, d) = (self.vue_ecran(w), o.matrice(), cx.dp);
        let px = |p: P3| vue.ecran(vue::appliquer(&mx, p)).map(|e| [w.x + e[0] as f32, w.y + e[1] as f32]);
        let Some(c0) = px(t) else { return };
        cx.clip(Some(w));
        for (k, v) in l.iter().enumerate() {
            let Limite::Bornee(mn, mxa) = *v else { continue };
            // Ce qui tourne : l'os pour les plis, son axe X pour la vrille
            let (axe, part, rayon) = match k {
                1 => ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], 0.25),
                0 => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.45),
                _ => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.45),
            };
            let n = (((mxa - mn) / 5.0).ceil() as usize).clamp(2, 72);
            let pts: Vec<Option<[f32; 2]>> = (0..=n)
                .map(|i| {
                    let th = (mn + (mxa - mn) * i as f64 / n as f64).to_radians();
                    let u = ed::qappliquer(ed::qmul(q, ed::qaxe(axe, th)), part);
                    px(std::array::from_fn(|j| t[j] + u[j] * x.longueur * rayon))
                })
                .collect();
            let c = coul(COUL_AXES[k]);
            let pale = [c[0], c[1], c[2], 50];
            for p in pts.iter().flatten() {
                cx.dl.line(c0, *p, 2.0 * d, pale);
            }
            for s in pts.windows(2) {
                if let [Some(a), Some(b)] = [s[0], s[1]] {
                    cx.dl.line(a, b, 3.0 * d, c);
                }
            }
            for p in [pts[0], pts[n]].into_iter().flatten() {
                cx.dl.line(c0, p, 2.5 * d, c);
            }
        }
        cx.clip(None);
    }

    /// Poignée de la sélection, en traits dans l'espace de travail.
    fn dessiner_poignee(&mut self, cx: &mut Cx, w: Rect) {
        let vue = self.vue_ecran(w);
        let d = cx.dp;
        let Some(p) = self.poignee(&vue, d) else { return };
        let tenue = self.geste.as_ref().map(|g| g.prise);
        let px = |q: P3| vue.ecran(q).map(|e| [w.x + e[0] as f32, w.y + e[1] as f32]);
        let voile = |c: Color, a: f32| [c[0], c[1], c[2], (c[3] as f32 * a) as u8];
        let Some(o) = px(p.o) else { return };
        cx.clip(Some(w));
        let trace = |cx: &mut Cx, pts: Vec<P3>, ep: f32, c: Color| {
            let e: Vec<_> = pts.into_iter().map(px).collect();
            for s in e.windows(2) {
                if let [Some(a), Some(b)] = [s[0], s[1]] {
                    cx.dl.line(a, b, ep, c);
                }
            }
        };
        if p.mode == Mode::Rotation {
            let limites = self.limites_pose();
            for k in (0..3).filter(|k| p.axes[*k]) {
                let a = if tenue.is_none() || tenue == Some(Prise::Axe(k)) { 0.95 } else { 0.3 };
                let c = coul(COUL_AXES[k]);
                let Some((l, angles)) = limites.filter(|(l, _)| matches!(l[k], Limite::Bornee(..))) else {
                    trace(cx, p.anneau(k, 64), 3.0 * d, voile(c, a));
                    continue;
                };
                let Limite::Bornee(mn, mx) = l[k] else { continue };
                // Anneau limité : pâle, avec le secteur encore permis depuis l'angle du
                // moment, comme en Géométrie ; il part de l'os, ou de son axe X pour la vrille
                trace(cx, p.anneau(k, 64), 2.0 * d, voile(c, a * 0.3));
                let (axe, part) = match k {
                    1 => ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
                    0 => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
                    _ => ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
                };
                let n = (((mx - mn) / 5.0).ceil() as usize).clamp(2, 72);
                let pts: Vec<P3> = (0..=n)
                    .map(|i| {
                        let th = (mn - angles[k] + (mx - mn) * i as f64 / n as f64).to_radians();
                        let u = ed::qappliquer(ed::qmul(p.repere, ed::qaxe(axe, th)), part);
                        std::array::from_fn(|j| p.o[j] + u[j] * p.taille)
                    })
                    .collect();
                for q in pts.iter().filter_map(|q| px(*q)) {
                    cx.dl.line(o, q, 2.0 * d, voile(c, a * 0.2));
                }
                for q in [pts[0], pts[n]].into_iter().filter_map(px) {
                    cx.dl.line(o, q, 2.5 * d, voile(c, a));
                }
                trace(cx, pts, 4.0 * d, voile(c, a));
            }
        } else {
            for k in 0..3 {
                let a = if !p.axes[k] {
                    0.18
                } else if tenue.is_some_and(|t| t != Prise::Axe(k) && t != Prise::O) {
                    0.45
                } else {
                    1.0
                };
                let c = voile(coul(COUL_AXES[k]), a);
                let Some(b) = px(p.bout(k)) else { continue };
                cx.dl.line(o, b, 3.5 * d, c);
                if p.mode == Mode::Echelle {
                    let s = 15.0 * d;
                    cx.dl.rrect(Rect::new(b[0] - s / 2.0, b[1] - s / 2.0, s, s), 2.0 * d, c);
                } else {
                    // Pointe : un chevron, dirigé selon l'axe à l'écran
                    let (dx, dy) = (b[0] - o[0], b[1] - o[1]);
                    let l = dx.hypot(dy).max(1.0);
                    let (ux, uy) = (dx / l * 11.0 * d, dy / l * 11.0 * d);
                    let bout = [b[0] + ux, b[1] + uy];
                    cx.dl.line(bout, [b[0] - uy * 0.8 - ux * 0.3, b[1] + ux * 0.8 - uy * 0.3], 4.0 * d, c);
                    cx.dl.line(bout, [b[0] + uy * 0.8 - ux * 0.3, b[1] - ux * 0.8 - uy * 0.3], 4.0 * d, c);
                }
            }
        }
        if p.a_anneau_vue() {
            let a = if tenue.is_none() || tenue == Some(Prise::V) { 0.95 } else { 0.3 };
            trace(cx, p.anneau_vue(&vue, 64), 2.5 * d, voile(coul(C::Secondaire), a));
        }
        let s = 15.0 * d;
        let a = if tenue.is_some_and(|t| t != Prise::O) { 0.45 } else { 1.0 };
        cx.dl.rrect(Rect::new(o[0] - s / 2.0, o[1] - s / 2.0, s, s), 3.0 * d, voile(coul(C::Poignee), a));
        cx.clip(None);
    }

    fn basculer(&mut self, c: Contenu) {
        if self.shell.end.open && self.contenu == c {
            self.shell.end.open = false;
        } else {
            self.contenu = c;
            self.shell.end.open = true;
        }
    }

    /// Taille et écart des dix pastilles : sur écran étroit, l'écart cède avant la taille.
    fn mesure_pastilles(largeur: f32, d: f32) -> (f32, f32) {
        let libre = largeur - 28.0 * d;
        let ecart = ((libre - 10.0 * 36.0 * d) / 9.0).clamp(3.0 * d, 10.0 * d);
        (((libre - 9.0 * ecart) / 10.0).clamp(34.0 * d, 52.0 * d), ecart)
    }

    fn flottants(&mut self, cx: &mut Cx, w: Rect) {
        let d = cx.dp;
        let m = 14.0 * d;
        let (taille, ecart) = Self::mesure_pastilles(w.w, d);
        let a = |i: f32| Rect::new(w.x + m + i * (taille + ecart), w.y + m, taille, taille);
        let (fin, contenu) = (self.shell.end.open, self.contenu);
        if cx.pastille(OBJETS, a(0.0), Icon::List, self.shell.start.open) {
            self.shell.start.toggle();
        }
        if cx.pastille(SCENE, a(1.0), Icon::Globe, fin && contenu == Contenu::Scene) {
            self.basculer(Contenu::Scene);
        }
        if cx.pastille(CONTEXTE, a(2.0), Icon::Contexte, fin && contenu == Contenu::Contexte) {
            self.basculer(Contenu::Contexte);
        }
        // Le panneau Os, en Scène et sur les os ; pas sur la géométrie d'un maillage
        if self.travail.is_some() {
            cx.pastille_eteinte(OS, a(3.0), Icon::Os);
        } else if cx.pastille(OS, a(3.0), Icon::Os, fin && contenu == Contenu::Os) {
            self.basculer(Contenu::Os);
        }
        // Les modifieurs se règlent en mode Scène : en Géométrie, on édite le maillage de base
        if self.travail.is_some() || self.os.is_some() {
            cx.pastille_eteinte(MODIFIEURS, a(4.0), Icon::Cle);
        } else if cx.pastille(MODIFIEURS, a(4.0), Icon::Cle, fin && contenu == Contenu::Modifieurs) {
            self.basculer(Contenu::Modifieurs);
        }
        if cx.pastille(FICHIERS, a(5.0), Icon::Dossier, fin && contenu == Contenu::Fichier) {
            self.basculer(Contenu::Fichier);
        }
        if cx.pastille(REGLAGES, a(6.0), Icon::Sliders, fin && contenu == Contenu::Reglages) {
            self.basculer(Contenu::Reglages);
        }
        if cx.pastille(AIDE, a(7.0), Icon::Help, fin && contenu == Contenu::Aide) {
            self.basculer(Contenu::Aide);
        }
        for (i, cle, icone, possible) in [(8.0, ANNULER, Icon::Annuler, !self.histoire.annuler.is_empty()), (9.0, RETABLIR, Icon::Retablir, !self.histoire.retablir.is_empty())] {
            if !possible {
                cx.pastille_eteinte(cle, a(i), icone);
            } else if cx.pastille(cle, a(i), icone, false) {
                if cle == ANNULER { self.annuler() } else { self.retablir() }
            }
        }
        self.barre_haute(cx, w, w.y + m + taille + ecart);
        let h = 44.0 * d;
        let y = self.barre(cx, w);
        let y = self.barre_rapide(cx, w, y);
        if let Some((t, _)) = &self.message {
            let l = cx.text.measure(t, (15.0 * d).round()) + 36.0 * d;
            let r = Rect::new(w.cx() - l / 2.0, y - h - ecart, l, h);
            cx.dl.rrect(r, h / 2.0, coul(C::Texte));
            cx.text_at(r.x + 18.0 * d, r.cy(), t, 15.0, coul(C::Surface));
        }
    }

    /// Pilules disposées en rangées qui passent à la ligne, depuis (`x`, `y`) sur la
    /// largeur `l`, centrées ou non. Rend celle qui est touchée et celle tenue longtemps.
    fn rangees(cx: &mut Cx, items: &[Pilule], x: f32, y: f32, l: f32, centre: bool) -> (Option<usize>, Option<usize>) {
        let (h, ecart) = (44.0 * cx.dp, 8.0 * cx.dp);
        let (mut appui, mut long) = (None, None);
        for (n, (a, b, largeur)) in Self::coupes(items, l, ecart).into_iter().enumerate() {
            let mut px = if centre { x + (l - largeur) / 2.0 } else { x };
            let py = y + n as f32 * (h + ecart);
            for (i, p) in items.iter().enumerate().take(b).skip(a) {
                let r = Rect::new(px, py, p.l, h);
                px += p.l + ecart;
                let touche = match p.icone {
                    Some(c) => cx.pill_icone(p.cle, r, c, p.nomme.then_some(p.nom.as_str()), p.actif, p.couleur),
                    None => cx.pill_teinte(p.cle, r, &p.nom, p.actif, p.couleur),
                };
                if touche {
                    appui = Some(i);
                }
                if p.long && cx.ui.long_press(p.cle) {
                    long = Some(i);
                }
            }
        }
        (appui, long)
    }

    /// Rangées : début, fin, largeur.
    fn coupes(items: &[Pilule], l: f32, ecart: f32) -> Vec<(usize, usize, f32)> {
        let mut r: Vec<(usize, usize, f32)> = vec![(0, 0, 0.0)];
        for (i, p) in items.iter().enumerate() {
            let c = r.last_mut().unwrap();
            if c.2 > 0.0 && c.2 + ecart + p.l > l {
                r.push((i, i + 1, p.l));
            } else {
                c.2 += if c.2 > 0.0 { ecart + p.l } else { p.l };
                c.1 = i + 1;
            }
        }
        r
    }

    /// Les modes : sous les pastilles.
    fn barre_haute(&mut self, cx: &mut Cx, w: Rect, y: f32) {
        let m = 14.0 * cx.dp;
        let cle = id("barre-haute");
        let edition = self.travail.is_some();
        let pose = self.os.as_ref().is_some_and(|e| e.pose);
        let geo = edition || (self.os.is_some() && !pose);
        // La pilule Pose, quand une armature est active ou posée
        let avec_pose = self.os.is_some() || (self.travail.is_none() && self.projet.actif.and_then(|k| self.projet.objets.get(k)).is_some_and(|o| o.armature.is_some()));
        let etroit = w.w < 760.0 * cx.dp;
        let mut l = vec![
            Pilule::nommee(cx, sub_id(cle, 0), "Scène", &icones::SCENE, self.travail.is_none() && self.os.is_none(), etroit),
            Pilule::nommee(cx, sub_id(cle, 1), "Géométrie", &icones::PIVOT_ACTIF, geo, etroit),
        ];
        if avec_pose {
            l.push(Pilule::nommee(cx, sub_id(cle, 5), "Pose", &icones::OS, pose, etroit));
        }
        match Self::rangees(cx, &l, w.x + m, y, w.w - 2.0 * m, false).0 {
            Some(0) => self.sortir_geometrie(),
            Some(1) => self.entrer_geometrie(),
            Some(2) => self.entrer_pose(),
            _ => {}
        }
    }

    /// Ce qui règle la sélection, dans la barre d'outils : son mode, l'effet de la
    /// boîte, et en Géométrie les types d'éléments qui se touchent. Leurs clés restent
    /// celles de la barre du haut, où ils étaient.
    fn pilules_selection(&self, cx: &mut Cx) -> Vec<Pilule> {
        let cle = id("barre-haute");
        let sel = match self.selection { Selection::Toucher => &icones::SELECTION_TOUCHER, Selection::Boite => &icones::SELECTION_BOITE, Selection::Chemin => &icones::SELECTION_CHEMIN };
        let mut l = vec![Pilule::icone(cx, sub_id(cle, 2), "Sélection", sel, false)];
        if self.selection == Selection::Boite {
            let mut p = Pilule::new(cx, sub_id(cle, 3), if self.boite_retire { "−" } else { "+" }, self.boite_retire);
            p.l = 44.0 * cx.dp;
            l.push(p);
        }
        if let Some(t) = &self.travail {
            for k in 0..3 {
                let mut p = Pilule::icone(cx, sub_id(cle, 10 + k), NOMS_FILTRES[k], ICONES_FILTRES[k], t.filtres[k]);
                p.long = true;
                l.push(p);
            }
        }
        l
    }

    /// Appui sur la `i`-ième pilule de la sélection ; `long` : appui long.
    fn appui_selection(&mut self, i: usize, long: bool) {
        let boite = self.selection == Selection::Boite;
        let base = 1 + boite as usize;
        if i >= base {
            let k = i - base;
            if long {
                // Appui long : ce type s'ajoute ou se retire, les autres restent
                if let Some(Err(m)) = self.travail.as_mut().map(|t| t.basculer_filtre(TYPES[k])) {
                    self.dire(m);
                }
            } else if let Some(t) = &mut self.travail {
                // Appui bref : ce type seul
                let mut f = [false; 3];
                f[k] = true;
                t.regler_filtres(f);
            }
            return;
        }
        if long {
            return;
        }
        if i == 1 {
            self.boite_retire = !self.boite_retire;
            return self.dire(if self.boite_retire { "La boîte retire de la sélection" } else { "La boîte ajoute à la sélection" }.to_string());
        }
        self.selection = match (self.selection, self.travail.is_some()) {
            (Selection::Toucher, _) => Selection::Boite,
            (Selection::Boite, true) => Selection::Chemin,
            _ => Selection::Toucher,
        };
        self.dire(match self.selection {
            Selection::Toucher => "Sélection : Toucher".into(),
            Selection::Boite => "Sélection : Boîte — un doigt trace la boîte, deux tournent la vue et pincent".to_string(),
            Selection::Chemin => "Sélection : Chemin — touchez un élément, puis un autre du même type".to_string(),
        });
    }

    /// Barre d'outils au bas de l'espace de travail : elle passe à la ligne sur les
    /// écrans étroits. Rend le haut de la barre.
    fn barre(&mut self, cx: &mut Cx, w: Rect) -> f32 {
        const OUTILS: [(Mode, &str, &Icone); 3] = [(Mode::Translation, "Déplacer", &icones::DEPLACER), (Mode::Rotation, "Tourner", &icones::TOURNER), (Mode::Echelle, "Échelle", &icones::ECHELLE)];
        let d = cx.dp;
        let (h, ecart, m) = (44.0 * d, 8.0 * d, 14.0 * d);
        // Un seul bouton pour les trois modes : chaque appui passe au suivant
        let i = OUTILS.iter().position(|o| o.0 == self.mode).unwrap_or(0);
        let mut l = vec![Pilule::icone(cx, sub_id(BARRE, 0), OUTILS[i].1, OUTILS[i].2, false)];
        for k in 0..3 {
            l.push(Pilule { cle: sub_id(BARRE, 3 + k), nom: NOMS_AXES[k].to_string(), l: h, actif: self.axes[k], couleur: coul(COUL_AXES[k]), long: false, icone: None, nomme: false });
        }
        l.push(Pilule::icone(cx, sub_id(BARRE, 6), "Pas", &icones::PAS, self.pas.actif));
        l.push(Pilule::icone(cx, sub_id(BARRE, 7), NOMS_PIVOTS[self.pivot], ICONES_PIVOTS[self.pivot], false));
        let outils = l.len();
        l.extend(self.pilules_selection(cx));
        let n = Self::coupes(&l, w.w - 2.0 * m, ecart).len() as f32;
        let haut = w.bottom() - m - n * h - (n - 1.0) * ecart;
        let (appui, long) = Self::rangees(cx, &l, w.x + m, haut, w.w - 2.0 * m, true);
        if let Some(i) = long.filter(|i| *i >= outils) {
            self.appui_selection(i - outils, true);
        }
        match appui {
            Some(i) if i >= outils => self.appui_selection(i - outils, false),
            Some(0) => {
                let (mode, nom, _) = OUTILS[(i + 1) % OUTILS.len()];
                self.mode = mode;
                self.dire(nom.to_string());
            }
            Some(k @ 1..=3) => {
                // Au moins un axe reste coché
                let mut a = self.axes;
                a[k - 1] = !a[k - 1];
                if a.contains(&true) {
                    self.axes = a;
                }
            }
            Some(4) => self.pas.actif = !self.pas.actif,
            Some(5) => {
                self.pivot = (self.pivot + 1) % PIVOTS.len();
                self.dire(format!("Pivot : {}", NOMS_PIVOTS[self.pivot]));
            }
            _ => {}
        }
        haut
    }

    /// Pavé numérique, par-dessus tout : il prend tous les doigts.
    fn dessiner_pave(&mut self, cx: &mut Cx, ecran: Rect) {
        let Some(p) = &self.pave else { return };
        let (titre, texte, neuf, alpha, maj, libre) = (p.cible.titre(), p.texte.clone(), p.neuf, p.alpha, p.maj, p.cible.libre());
        let avant: String = texte.chars().take(p.curseur).collect();
        let suivant = self.champ_suivant(p.cible).is_some();
        let d = cx.dp;
        if cx.ui.click(sub_id(PAVE, 100), ecran, None) {
            self.pave = None;
            return;
        }
        let large = if alpha { 520.0 } else { 300.0 };
        let (l, tete, t, ecart) = ((large * d).min(ecran.w - 16.0 * d), 92.0 * d, 50.0 * d, 8.0 * d);
        let h = tete + 5.0 * (t + ecart) + ecart;
        let marge = [(ecran.w - l).max(0.0), (ecran.h - h).max(0.0)];
        let mut c = Rect::new(ecran.x + self.pave_place[0] * marge[0], ecran.y + self.pave_place[1] * marge[1], l, h);

        // La barre du haut déplace le pavé : le doigt garde son point de prise
        let barre = Rect::new(c.x, c.y, l, 40.0 * d);
        cx.ui.region_add(sub_id(PAVE, 101), c, Sense::Sink);
        match cx.ui.drag(sub_id(PAVE, 102), barre) {
            Some(g) => {
                let prise = *self.pave_prise.get_or_insert((g.depart.0 - c.x, g.depart.1 - c.y));
                c.x = (g.x - prise.0).clamp(ecran.x, ecran.x + marge[0]);
                c.y = (g.y - prise.1).clamp(ecran.y, ecran.y + marge[1]);
                self.pave_place = [0, 1].map(|k| if marge[k] > 0.0 { ([c.x, c.y][k] - [ecran.x, ecran.y][k]) / marge[k] } else { 0.5 });
            }
            None if self.pave_prise.take().is_some() => self.reglages_changes = true,
            None => {}
        }

        cx.dl.shadow(Rect::new(c.x, c.y + 3.0 * d, l, h), 18.0 * d, 14.0 * d, [0, 0, 0, 70]);
        cx.dl.rrect(c, 18.0 * d, coul(C::Surface));
        let x0 = c.x + ecart;
        cx.text_at(x0 + 6.0 * d, c.y + 22.0 * d, &titre, 14.0, coul(C::Secondaire));
        // Six points : la barre se saisit
        let teinte = if self.pave_prise.is_some() { coul(C::Accent) } else { coul(C::Secondaire) };
        for i in 0..3 {
            for j in 0..2 {
                let p = Rect::new(c.right() - (24.0 + 7.0 * i as f32) * d, c.y + (16.0 + 7.0 * j as f32) * d, 3.0 * d, 3.0 * d);
                cx.dl.rrect(p, 1.5 * d, teinte);
            }
        }
        // Le curseur se déplace par les deux chevrons, de part et d'autre de la saisie
        let (bt, pas_) = (44.0 * d, 50.0 * d);
        let mut touche = None;
        for (rang, x, icone, nom) in [(105, x0, Icon::Gauche, "Gauche"), (106, c.right() - ecart - bt, Icon::Droite, "Droite")] {
            if cx.pastille(sub_id(PAVE, rang), Rect::new(x, c.y + 40.0 * d, bt, bt), icone, false) {
                touche = Some(nom);
            }
        }
        let lecture = Rect::new(x0 + pas_, c.y + 40.0 * d, l - 2.0 * ecart - 2.0 * pas_, 44.0 * d);
        cx.dl.rrect(lecture, 10.0 * d, coul(C::Fond));
        // Calée à droite ; trop longue, elle glisse pour garder le curseur en vue
        let px = (19.0 * d).round();
        let (m, ma, bord) = (cx.text.measure(&texte, px), cx.text.measure(&avant, px), 12.0 * d);
        let debut = lecture.right() - bord - m;
        let x = debut + ((lecture.x + bord) - (debut + ma)).max(0.0);
        cx.clip(Some(lecture));
        // Valeur reprise : surlignée, le premier chiffre la remplace
        if neuf && !texte.is_empty() {
            let mut f = coul(C::Accent);
            f[3] = 50;
            cx.dl.rrect(Rect::new(x - 4.0 * d, lecture.y + 8.0 * d, m + 8.0 * d, lecture.h - 16.0 * d), 5.0 * d, f);
        }
        cx.text_at(x, lecture.cy(), &texte, 19.0, coul(C::Texte));
        if !neuf {
            cx.dl.rrect(Rect::new(x + ma - d, lecture.y + 9.0 * d, 2.0 * d, lecture.h - 18.0 * d), d, coul(C::Accent));
        }
        cx.clip(None);
        // Le switch : un champ de texte peut aussi prendre des chiffres
        if libre {
            let (m, c0) = (58.0 * d, c.right() - 50.0 * d - 2.0 * 58.0 * d);
            for (i, (nom, mode)) in [("123", false), ("ABC", true)].into_iter().enumerate() {
                if cx.pill(sub_id(PAVE, 103 + i), Rect::new(c0 + i as f32 * m, c.y + 4.0 * d, m - 4.0 * d, 32.0 * d), nom, alpha == mode) {
                    if let Some(p) = &mut self.pave { p.alpha = mode; }
                }
            }
        }
        let lc = (l - 4.0 * ecart) / 3.0;
        let case = |i: usize, j: usize| Rect::new(x0 + j as f32 * (lc + ecart), c.y + tete + i as f32 * (t + ecart), lc, t);
        const TOUCHES: [&str; 12] = ["7", "8", "9", "4", "5", "6", "1", "2", "3", "-", "0", ","];
        if alpha {
            let e = 5.0 * d;
            for (r, rangee) in CLAVIER.iter().enumerate() {
                let total: f32 = rangee.iter().map(|k| k.1).sum();
                let u = (l - 2.0 * ecart - (rangee.len() - 1) as f32 * e) / total;
                let mut x = x0;
                for (n, (k, poids)) in rangee.iter().enumerate() {
                    let w = u * poids + (poids - 1.0) * e;
                    let vue = if maj && k.chars().count() == 1 { k.to_uppercase() } else { k.to_string() };
                    let r2 = Rect::new(x, c.y + tete + r as f32 * (t + ecart), w - (poids - 1.0) * e, t);
                    if cx.pill(sub_id(PAVE, 200 + 20 * r + n), r2, &vue, *k == "Maj" && maj) {
                        touche = Some(*k);
                    }
                    x += u * poids + e;
                }
            }
        } else {
            for (n, s) in TOUCHES.iter().enumerate() {
                if cx.pill(sub_id(PAVE, n), case(n / 3, n % 3), s, false) {
                    touche = Some(*s);
                }
            }
        }
        if cx.pill(sub_id(PAVE, 20), case(4, 0), "Effacer", false) {
            touche = Some("Effacer");
        }
        if let (Some(k), Some(p)) = (touche, &mut self.pave) {
            p.touche(k);
        }
        if cx.pill(sub_id(PAVE, 21), case(4, 1), "Annuler", false) {
            self.pave = None;
        }
        if cx.pill(sub_id(PAVE, 22), case(4, 2), if suivant { "Suivant" } else { "OK" }, true) {
            self.valider_pave();
        }
    }

    fn cadre_panneau(&self, cx: &mut Cx, id: Id, r: Rect, cote: Side, paysage: bool) {
        cx.ui.region_add(id, r, crate::ui::Sense::Sink);
        cx.dl.rrect(r, 0.0, coul(C::Surface));
        let t = cx.dp.max(1.0);
        let ligne = match (cote, paysage) {
            (Side::Start, true) => Rect::new(r.right() - t, r.y, t, r.h),
            (Side::End, true) => Rect::new(r.x, r.y, t, r.h),
            (Side::Start, false) => Rect::new(r.x, r.bottom() - t, r.w, t),
            (Side::End, false) => Rect::new(r.x, r.y, r.w, t),
        };
        cx.dl.rrect(ligne, 0.0, coul(C::Bordures));
    }

    /// Filet sous l'en-tête d'un panneau ; rend la zone du dessous.
    fn sous_entete(cx: &mut Cx, r: Rect, tete: f32) -> Rect {
        let d = cx.dp;
        cx.dl.rrect(Rect::new(r.x, r.y + tete - d, r.w, d.max(1.0)), 0.0, coul(C::Bordures));
        Rect::new(r.x, r.y + tete, r.w, r.h - tete)
    }

    /// Nom du projet, à renommer, et ce qu'il contient.
    fn entete_projet(&mut self, cx: &mut Cx, r: Rect) -> Rect {
        let d = cx.dp;
        let f = Rect::new(r.x + 12.0 * d, r.y + 10.0 * d, r.w - 24.0 * d, 44.0 * d);
        if cx.ui.click(NOM_PROJET, f, None) {
            self.saisir(Champ::Projet);
        }
        cx.dl.rrect(f, 10.0 * d, if cx.ui.pressed(NOM_PROJET) { coul(C::Bordures) } else { coul(C::Fond) });
        cx.clip(Some(f));
        let (nom, couleur) = if self.nom.is_empty() { ("Sans nom", coul(C::Secondaire)) } else { (self.nom.as_str(), coul(C::Texte)) };
        cx.text_at(f.x + 12.0 * d, f.cy(), nom, 17.0, couleur);
        cx.clip(None);
        let faces: usize = self.projet.objets.iter().filter(|o| o.armature.is_none()).map(|o| o.maillage.faces.len()).sum();
        let n = self.projet.objets.len();
        cx.text_at(f.x + 2.0 * d, f.bottom() + 17.0 * d, &format!("{n} objet{} · {faces} faces", if n > 1 { "s" } else { "" }), 14.0, coul(C::Secondaire));
        Self::sous_entete(cx, r, 84.0 * d)
    }

    fn panneau_debut(&mut self, cx: &mut Cx, l: &Layout) {
        if self.shell.start.anim == 0.0 {
            return;
        }
        let r = l.start;
        self.cadre_panneau(cx, id("panneau-debut"), r, Side::Start, l.landscape);
        let corps = self.entete_projet(cx, r);
        let z = id("liste-objets");
        // Un set ou des objets disparus, par une annulation, ramènent à la liste
        let page = match self.page_liste.clone() {
            PageListe::Set(n) if self.projet.set(&n).is_none() => PageListe::Objets,
            PageListe::Deplacer(l) => {
                let l: Vec<usize> = l.into_iter().filter(|k| *k < self.projet.objets.len()).collect();
                if l.is_empty() { PageListe::Objets } else { PageListe::Deplacer(l) }
            }
            p => p,
        };
        self.page_liste = page.clone();
        match page {
            PageListe::Set(n) => {
                let mut b = cx.body(sub_id(z, 1_000_001), corps, &self.scrolls);
                self.page_set(cx, &mut b, &n);
                return cx.body_end(b, &mut self.scrolls);
            }
            PageListe::Deplacer(l) => {
                let mut b = cx.body(sub_id(z, 1_000_002), corps, &self.scrolls);
                self.page_deplacer(cx, &mut b, &l);
                return cx.body_end(b, &mut self.scrolls);
            }
            PageListe::Objets => {}
        }
        let mut b = cx.body(z, corps, &self.scrolls);
        for e in 0..self.projet.sets.len() {
            let (nom, visible, replie) = { let s = &self.projet.sets[e]; (s.nom.clone(), s.visible, s.replie) };
            let membres = self.projet.objets.iter().filter(|o| o.set == nom).count();
            let (plier, oeil, menu) = cx.tete_set(&mut b, sub_id(id("set"), e), &nom, membres, replie, visible);
            if plier {
                self.projet.sets[e].replie = !replie;
                self.a_sauver = true;
            }
            if oeil {
                self.projet.sets[e].visible = !visible;
                self.visibilite_changee();
            }
            if menu {
                self.page_liste = PageListe::Set(nom.clone());
            }
            if replie {
                continue;
            }
            if membres == 0 {
                cx.paragraph(&mut b, "Vide : un appui long sur un objet l'y déplace.");
            }
            for k in 0..self.projet.objets.len() {
                let o = &self.projet.objets[k];
                if o.set != nom {
                    continue;
                }
                let c = self.projet.materiau(&o.materiau).map_or(coul(C::Bordures), |m| octets_hex(&m.couleur));
                let choisi = self.choix.contains(&k);
                if cx.row(&mut b, sub_id(z, k), &o.nom, Some(c), choisi) {
                    if self.travail.is_none() && self.os.is_none() {
                        self.basculer_objet(k);
                    }
                }
                if cx.ui.long_press(sub_id(z, k)) {
                    // Un objet de la sélection emmène les autres
                    self.page_liste = PageListe::Deplacer(if choisi { self.choix.clone() } else { vec![k] });
                }
            }
        }
        if cx.button(&mut b, id("set-nouveau"), "Nouveau set") {
            self.nouveau_set(&[]);
        }
        cx.body_end(b, &mut self.scrolls);
    }

    /// Choix du set où ranger des objets.
    fn page_deplacer(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, l: &[usize]) {
        let titre = match l {
            [k] => format!("DÉPLACER « {} » VERS", self.projet.objets[*k].nom.to_uppercase()),
            _ => format!("DÉPLACER {} OBJETS VERS", l.len()),
        };
        cx.section(b, &titre);
        for e in 0..self.projet.sets.len() {
            let (nom, visible) = (self.projet.sets[e].nom.clone(), self.projet.sets[e].visible);
            let la = l.iter().all(|k| self.projet.objets[*k].set == nom);
            let texte = if visible { nom.clone() } else { format!("{nom} (masqué)") };
            if cx.row(b, sub_id(id("set-vers"), e), &texte, None, la) {
                self.deplacer_objets(l, &nom);
                self.page_liste = PageListe::Objets;
            }
        }
        if cx.button(b, id("set-vers-nouveau"), "Nouveau set") {
            self.nouveau_set(l);
        }
        if cx.button(b, id("set-annuler"), "Annuler") {
            self.page_liste = PageListe::Objets;
        }
    }

    /// Nom d'un set, fusion et suppression. Le premier reste : il recueille les objets.
    fn page_set(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, nom: &str) {
        cx.section(b, "SET");
        if cx.champ(b, id("set-nom"), "Nom", coul(C::Texte), nom) {
            self.saisir(Champ::SetNom);
        }
        let premier = self.projet.sets[0].nom.clone();
        if nom == premier {
            cx.paragraph(b, "Le premier set reste toujours : il recueille les objets des sets retirés.");
        } else {
            let n = self.projet.objets.iter().filter(|o| o.set == nom).count();
            cx.section(b, "FUSIONNER DANS");
            for e in 0..self.projet.sets.len() {
                let dans = self.projet.sets[e].nom.clone();
                if dans != nom && cx.row(b, sub_id(id("set-dans"), e), &dans, None, false) {
                    self.fusionner_set(nom, &dans);
                }
            }
            cx.section(b, "SUPPRIMER");
            if cx.button(b, id("set-seul"), &format!("Le set seul (objets vers « {premier} »)")) {
                self.fusionner_set(nom, &premier);
            }
            if cx.button(b, id("set-tout"), &format!("Le set et ses {n} objet{}", if n > 1 { "s" } else { "" })) {
                self.supprimer_set(nom);
            }
        }
        if cx.button(b, id("set-retour"), "Retour") {
            self.page_liste = PageListe::Objets;
        }
    }

    /// Les objets rangés dans un set masqué quittent la sélection, comme à l'écran.
    fn deplacer_objets(&mut self, l: &[usize], set: &str) {
        let a: Vec<usize> = l.iter().copied().filter(|k| self.projet.objets[*k].set != set).collect();
        if a.is_empty() || self.projet.set(set).is_none() {
            return;
        }
        self.memoriser();
        for &k in &a {
            self.projet.objets[k].set = set.to_string();
        }
        self.apres_sets();
        self.dire(match a[..] {
            [k] => format!("« {} » → {set}", self.projet.objets[k].nom),
            _ => format!("{} objets → {set}", a.len()),
        });
    }

    /// Crée un set, y range les objets donnés, et ouvre sa page pour le nommer.
    fn nouveau_set(&mut self, l: &[usize]) {
        self.memoriser();
        let nom = self.projet.nom_set_libre("set");
        self.projet.sets.push(crate::projet::Set { nom: nom.clone(), visible: true, replie: false });
        for &k in l {
            self.projet.objets[k].set = nom.clone();
        }
        self.apres_sets();
        self.page_liste = PageListe::Set(nom);
    }

    fn renommer_set(&mut self, ancien: &str, nom: &str) {
        let nom = nom.trim();
        if nom.is_empty() {
            return self.dire("Nom vide".into());
        }
        if nom == ancien {
            return;
        }
        if self.projet.set(nom).is_some() {
            return self.dire("Ce nom est déjà pris".into());
        }
        self.memoriser();
        for o in self.projet.objets.iter_mut().filter(|o| o.set == ancien) {
            o.set = nom.to_string();
        }
        if let Some(e) = self.projet.sets.iter_mut().find(|e| e.nom == ancien) {
            e.nom = nom.to_string();
        }
        self.page_liste = PageListe::Set(nom.to_string());
        self.apres_sets();
    }

    /// Les objets de `de` passent dans `dans`, puis `de` disparaît.
    fn fusionner_set(&mut self, de: &str, dans: &str) {
        if self.projet.sets.first().is_some_and(|e| e.nom == de) {
            return;
        }
        self.memoriser();
        for o in self.projet.objets.iter_mut().filter(|o| o.set == de) {
            o.set = dans.to_string();
        }
        self.projet.sets.retain(|e| e.nom != de);
        self.page_liste = PageListe::Objets;
        self.apres_sets();
    }

    /// Le set et ses objets. Repasse en mode Scène : ils peuvent être en cours d'édition.
    fn supprimer_set(&mut self, nom: &str) {
        if self.projet.sets.first().is_some_and(|e| e.nom == nom) {
            return;
        }
        self.sortir_geometrie();
        self.memoriser();
        let l: Vec<usize> = (0..self.projet.objets.len()).filter(|k| self.projet.objets[*k].set == nom).collect();
        self.retirer_objets(&l);
        self.projet.sets.retain(|e| e.nom != nom);
        self.page_liste = PageListe::Objets;
        self.a_sauver = true;
    }

    fn apres_sets(&mut self) {
        let p = &self.projet;
        if self.travail.is_none() {
            let choix: Vec<usize> = self.choix.iter().copied().filter(|k| p.affiche(&p.objets[*k])).collect();
            self.choix = choix;
        }
        self.a_sauver = true;
    }

    fn panneau_fin(&mut self, cx: &mut Cx, l: &Layout) {
        if self.shell.end.anim == 0.0 {
            return;
        }
        let r = l.end;
        self.cadre_panneau(cx, id("panneau-fin"), r, Side::End, l.landscape);
        let d = cx.dp;
        let (onglets, corps) = (Rect::new(r.x + 12.0 * d, r.y + 11.0 * d, r.w - 24.0 * d, 38.0 * d), Self::sous_entete(cx, r, 60.0 * d));
        match self.contenu {
            Contenu::Reglages => {
                self.onglet = cx.tabs(id("onglets-reglages"), onglets, &ONGLETS, self.onglet);
                let z = sub_id(id("corps-reglages"), self.onglet);
                let mut b = cx.body(z, corps, &self.scrolls);
                match self.onglet {
                    ONGLET_COULEURS => self.onglet_couleurs(cx, &mut b),
                    _ => self.onglet_barre(cx, &mut b),
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Ajout => {
                let titre = self.ajout.as_ref().map_or("Ajouter", |a| a.prim.titre());
                cx.tabs(id("onglets-ajout"), onglets, &[titre], 0);
                let mut b = cx.body(id("corps-ajout"), corps, &self.scrolls);
                self.panneau_ajout(cx, &mut b);
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Uv => {
                cx.tabs(id("onglets-uv"), onglets, &["UV"], 0);
                self.panneau_uv(cx, corps);
            }
            Contenu::Scene => {
                self.onglet_scene = cx.tabs(id("onglets-scene"), onglets, &["Scène", "Matériaux"], self.onglet_scene);
                let mut b = cx.body(sub_id(id("corps-scene"), self.onglet_scene), corps, &self.scrolls);
                if self.onglet_scene == ONGLET_MATERIAUX {
                    self.onglet_materiaux(cx, &mut b);
                } else {
                    self.panneau_scene(cx, &mut b);
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Os => {
                // En pose : la pose en cours, et le catalogue des poses gardées
                let catalogue = if self.os.as_ref().is_some_and(|e| e.pose) {
                    self.onglet_os = cx.tabs(id("onglets-os"), onglets, &["Pose", "Catalogue"], self.onglet_os);
                    self.onglet_os == 1
                } else {
                    cx.tabs(id("onglets-os"), onglets, &["Os"], 0);
                    false
                };
                let mut b = cx.body(if catalogue { id("corps-catalogue") } else { id("corps-os") }, corps, &self.scrolls);
                if catalogue {
                    self.panneau_catalogue(cx, &mut b);
                } else {
                    self.panneau_os(cx, &mut b);
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Modifieurs => {
                cx.tabs(id("onglets-modifieurs"), onglets, &["Modifieurs"], 0);
                let mut b = cx.body(id("corps-modifieurs"), corps, &self.scrolls);
                if self.os.is_some() {
                    cx.paragraph(&mut b, "Une armature n'a pas de modifieurs. Revenez en mode Scène pour régler ceux des autres objets.");
                } else if self.travail.is_some() {
                    cx.paragraph(&mut b, "Le mode Géométrie édite le maillage de base, sans ses modifieurs. Revenez en mode Scène pour les régler.");
                } else {
                    self.panneau_modifs(cx, &mut b);
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Contexte => {
                let (geo, os) = (self.travail.is_some(), self.os.is_some());
                self.onglet_contexte = cx.tabs(id("onglets-contexte"), onglets, &[if os { "Armature" } else if geo { "Géométrie" } else { "Objet" }, "Dernière opération"], self.onglet_contexte);
                let mut b = cx.body(sub_id(id("corps-contexte"), self.onglet_contexte), corps, &self.scrolls);
                match self.onglet_contexte {
                    ONGLET_DERNIERE => self.reglages_derniere(cx, &mut b),
                    _ if os => self.fiche_os(cx, &mut b),
                    _ if geo => self.fiche_geo(cx, &mut b),
                    _ if self.liaison.is_some() => self.page_liaison(cx, &mut b),
                    _ => self.fiche(cx, &mut b),
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Fichier => {
                self.onglet_fichier = cx.tabs(id("onglets-fichier"), onglets, &["Projet", "Importer", "Exporter"], self.onglet_fichier);
                let mut b = cx.body(sub_id(id("corps-fichier"), self.onglet_fichier), corps, &self.scrolls);
                match self.onglet_fichier {
                    0 => self.onglet_projet(cx, &mut b),
                    1 => self.operations(cx, &mut b, Menu::Importer),
                    _ => self.onglet_exporter(cx, &mut b),
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Aide => {
                self.onglet_aide = cx.tabs(id("onglets-aide"), onglets, &["Gestes", "Opérations"], self.onglet_aide);
                let mut b = cx.body(sub_id(id("corps-aide"), self.onglet_aide), corps, &self.scrolls);
                if self.onglet_aide == 1 {
                    self.aide_operations(cx, &mut b);
                    cx.body_end(b, &mut self.scrolls);
                    return;
                }
                cx.section(&mut b, "GESTES");
                cx.paragraph(&mut b, "Un doigt tourne la vue, deux doigts zooment et déplacent. Un double appui dans le vide recadre.");
                cx.paragraph(&mut b, "Un appui sur un objet l'ajoute à la sélection ; le dernier choisi est l'objet actif, un nouvel appui sur lui le retire.");
                cx.paragraph(&mut b, "Un appui long choisit l'objet seul ; dans le vide, il vide la sélection.");
                cx.section(&mut b, "TRANSFORMER");
                cx.paragraph(&mut b, "Le premier bouton de la barre du bas passe de la translation à la rotation puis à l'échelle ; glissez ensuite depuis la poignée ou depuis un objet choisi. X, Y et Z limitent les axes. L'icône en escalier, le pas, arrondit à l'incrément réglable dans Contexte › Objet. L'icône suivante change le pivot, centre de rotation et d'échelle : son nom s'affiche à chaque appui.");
                cx.section(&mut b, "GÉOMÉTRIE");
                cx.paragraph(&mut b, "Un double appui sur un objet l'édite, avec les autres objets choisis. Les icônes de sommet, d'arête et de face, dans la barre du bas, disent ce qui se touche : un appui bref garde ce type seul, un appui long l'ajoute ou le retire.");
                cx.paragraph(&mut b, "Glisser depuis la sélection ou la poignée la transforme. Contexte › Géométrie regroupe les opérations. Le bouton de sélection, dans la barre du bas, passe de Toucher à Boîte, qui trace un cadre, puis à Chemin, qui relie deux éléments.");
                cx.section(&mut b, "ARMATURES");
                cx.paragraph(&mut b, "La géométrie d'une armature, ce sont ses os : la pilule Géométrie, l'action Éditer la géométrie ou un double appui sur une armature ouvrent ce mode sur elle. Quand une armature est active, la pilule Pose apparaît : les os y tournent autour de leur tête, selon les axes de l'os actif posé, à la poignée comme aux trois curseurs de Os actif, et le panneau Os ne garde que ce qui sert à la pose ; en mode Scène, il montre l'armature active, ses objets liés, ses poids, et la remet au repos. Les autres objets sont estompés et inertes. Un appui sur un bout, tête ou queue, le prend avec ce qui lui est soudé ; un appui sur le corps d'un os prend ses deux bouts ; un appui long les prend seuls. Glisser depuis la sélection ou la poignée déplace les bouts, et les enfants restent en place. Le panneau Os, sous sa pastille, regroupe l'os actif, les opérations et l'arbre des os ; Contexte › Armature donne la position des bouts choisis.");
                cx.section(&mut b, "DERNIÈRE OPÉRATION");
                cx.paragraph(&mut b, "Contexte › Dernière opération récapitule la dernière transformation au doigt ou la dernière opération réglable. Changer une valeur la refait depuis l'état d'avant, sans ajouter d'étape d'annulation. Elle disparaît à la modification suivante ou à l'annulation.");
                cx.section(&mut b, "BARRE RAPIDE");
                cx.paragraph(&mut b, "Au-dessus des outils, elle montre les opérations qui s'appliquent à la sélection ; en Géométrie, celles du type de l'élément actif d'abord. Glissez-la si elle ne tient pas. Réglages › Barre rapide choisit ce qui y figure.");
                cx.section(&mut b, "PAVÉ NUMÉRIQUE");
                cx.paragraph(&mut b, "Il reprend la valeur du champ, surlignée : un chiffre la remplace, Effacer et le signe la modifient, et les chevrons déplacent le curseur pour écrire ou effacer au milieu. Suivant valide et passe au champ d'après dans le groupe, comme X, Y puis Z. Sur un nom, 123 et ABC changent de clavier. Glissez sa barre du haut pour le déplacer : il garde cette place. Un appui ailleurs le referme sans appliquer la saisie en cours.");
                cx.section(&mut b, "COULEURS");
                cx.paragraph(&mut b, "Réglages › Couleurs change l'interface, la vue 3D, la géométrie, la sélection et les objets. Touchez une couleur pour la régler à la teinte, à la saturation et à la luminosité, ou par son code. La flèche la remet par défaut. Les couleurs laissées par défaut suivent celles dont elles dépendent, comme l'accent.");
                cx.section(&mut b, "OUVRIR UN PROJET");
                cx.paragraph(&mut b, "Depuis un gestionnaire de fichiers : Ouvrir avec, ou Partager, puis Rustcraft. Les formats .gda, OBJ, STL et GLB sont lus. La pastille Fichier réunit Projet, Importer et Exporter ; l'export prend la scène complète ou les seuls objets choisis.");
                cx.section(&mut b, "ÉTAT DU PORTAGE");
                cx.paragraph(&mut b, "Modes Scène, Géométrie et Armature, modifieurs booléen et répétition, visserie. Poses, contraintes et liaison des pièces des armatures conservées dans le projet, pas encore éditables.");
                cx.body_end(b, &mut self.scrolls);
            }
        }
    }

    fn onglet_projet(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let nom = if self.nom.is_empty() { "Sans nom" } else { self.nom.as_str() }.to_string();
        if cx.champ(b, id("f-nom"), "Nom", coul(C::Texte), &nom) {
            self.saisir(Champ::Projet);
        }
        self.operations(cx, b, Menu::Projet);
        cx.paragraph(b, "Un projet .gda garde toute la scène, sans perte. Il arrive dans Téléchargements, un nouveau fichier à chaque fois. Votre travail est aussi gardé d'un lancement à l'autre.");
    }

    fn onglet_exporter(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        self.operations(cx, b, Menu::Exporter);
        cx.paragraph(b, "Les fichiers arrivent dans Téléchargements, un nouveau à chaque fois.");
    }

    fn panneau_scene(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let geo = self.travail.is_some();
        cx.section(b, "RENDU");
        if geo {
            if cx.row(b, id("rendu-solide"), "Solide", None, !self.rayons) {
                self.rayons = false;
                self.version_geo = None;
            }
            if cx.row(b, id("rendu-rayons"), "Rayons X", None, self.rayons) {
                self.rayons = true;
                self.version_geo = None;
            }
        } else {
            if cx.row(b, id("rendu-solide"), "Solide", None, self.rendu == Rendu::Solide) {
                self.rendu = Rendu::Solide;
            }
            if cx.row(b, id("rendu-jeu"), "En jeu", None, self.rendu == Rendu::Jeu) {
                self.rendu = Rendu::Jeu;
            }
        }
        self.operations(cx, b, Menu::Vue);
        if !geo && self.os.is_none() {
            self.operations(cx, b, Menu::Scene);
        }
    }

    /// Boutons d'un menu du catalogue, par rubrique, grisés quand ils ne s'appliquent
    /// pas. L'action attend la fin de la liste : elle peut changer ce qui s'y affiche.
    fn operations(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, menu: Menu) {
        self.operations_si(cx, b, menu, |_| true);
    }

    fn operations_si(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, menu: Menu, garder: impl Fn(A) -> bool) {
        let c = self.contexte();
        let (mut rubrique, mut choisie) = ("", None);
        for o in actions::du_menu(menu).into_iter().filter(|o| garder(o.action)) {
            if o.rubrique != rubrique {
                rubrique = o.rubrique;
                cx.section(b, &rubrique.to_uppercase());
            }
            if cx.bouton(b, sub_id(id("operation"), o.action as usize), o.libelle, c.possible(o.action)) {
                choisie = Some(o.action);
            }
            if let Some(n) = c.note(o.action) {
                cx.paragraph(b, &n);
            }
        }
        if let Some(a) = choisie {
            self.agir(a);
        }
    }

    /// Ce que le catalogue doit savoir de la scène et de la sélection.
    fn contexte(&mut self) -> Contexte {
        // Pendant un glissé, la sélection ne change pas et le calcul serait à chaque image
        if let (Some(t), Some((v, c))) = (&self.travail, &self.contexte_geo) {
            if *v == t.version || self.geste.is_some() {
                return c.clone();
            }
        }
        let p = &self.projet;
        let mut c = Contexte {
            geo: self.travail.is_some(),
            choisis: self.choix.len(),
            affiches: self.affiches().len(),
            echelle: self.choix.iter().any(|k| p.objets[*k].armature.is_none() && p.objets[*k].echelle != [1.0; 3]),
            editable: p.actif.is_some_and(|k| k < p.objets.len()),
            ..Default::default()
        };
        c.liables = p.objets.iter().any(|o| o.armature.is_some()) && self.choix.iter().any(|k| p.objets[*k].armature.is_none());
        c.lies = self.choix.iter().any(|k| p.objets[*k].liaison.is_some());
        c.os = self.os.is_some();
        if let Some((e, _, a)) = self.armature_os() {
            let choisis = e.os_choisis(a);
            c.os_ouvert = true;
            c.pose = e.pose;
            c.posee = !a.pose.is_empty();
            c.bouts = e.bouts.len();
            c.os_choisis = choisis.len();
            c.queues = e.queues_choisies(a).len();
            c.os_actif = e.actif.is_some();
            c.os_enfants = e.actif.as_ref().is_some_and(|i| a.enfants(Some(i)).next().is_some());
            c.rattachables = choisis.iter().any(|i| Some(i) != e.actif.as_ref());
            c.attaches = choisis.iter().any(|i| a.trouver(i).is_some_and(|b| b.parent.is_some()));
        }
        if let Some(t) = &self.travail {
            c.sommets = t.sel.sommets.len();
            c.aretes = t.sel.aretes.len();
            c.faces = t.sel.faces.len();
            c.grande_face = t.sel.faces.iter().any(|f| t.m.faces[*f].s.len() > 3);
            c.anneau = t.arete_depart().and_then(|[i, j]| t.m.anneau(i, j)).is_some();
            c.dissolution = (c.aretes > 0).then(|| t.m.dissolution(&t.sel.aretes).map(|_| ()));
            c.actif = t.sel.dernier.is_some();
            c.type_actif = t.sel.dernier.map(|e| match e {
                El::S(_) => Groupe::Sommets,
                El::A(_) => Groupe::Aretes,
                El::F(_) => Groupe::Faces,
            });
            self.contexte_geo = Some((t.version, c.clone()));
        }
        c
    }

    /// Barre rapide, au-dessus de `bas` : une rangée qui défile, absente quand rien ne
    /// s'applique. Rend son haut.
    fn barre_rapide(&mut self, cx: &mut Cx, w: Rect, bas: f32) -> f32 {
        if !self.barre_visible {
            return bas;
        }
        let groupes = actions::barre(&self.contexte(), &self.acces);
        if groupes.is_empty() {
            self.defile = 0.0;
            return bas;
        }
        let d = cx.dp;
        let (h, ecart, m, trait_) = (44.0 * d, 8.0 * d, 14.0 * d, 17.0 * d);
        let mut largeurs = Vec::new();
        for (_, l) in &groupes {
            largeurs.push(l.iter().map(|o| { let b = o.barre.unwrap(); cx.largeur_operation(b.glyphe, b.court) }).collect::<Vec<f32>>());
        }
        let n: usize = largeurs.iter().map(Vec::len).sum();
        let total = largeurs.iter().flatten().sum::<f32>() + (n - groupes.len()) as f32 * ecart + (groupes.len() - 1) as f32 * trait_;
        let zone = Rect::new(w.x + m, bas - ecart - h, w.w - 2.0 * m, h);
        let depasse = (total - zone.w).max(0.0);
        let debut = if depasse > 0.0 { zone.x } else { zone.x + (zone.w - total) / 2.0 };
        cx.ui.region_add(BARRE_RAPIDE, Rect::new(debut, zone.y, total.min(zone.w), h), Sense::Scroll);
        self.defile = (self.defile - cx.ui.take_scroll_h(BARRE_RAPIDE)).clamp(0.0, depasse);
        cx.clip(Some(Rect::new(zone.x, zone.y - ecart, zone.w, h + 2.0 * ecart)));
        let (mut x, mut choisie) = (debut - self.defile, None);
        for (g, (_, l)) in groupes.iter().enumerate() {
            if g > 0 {
                x += trait_ - ecart;
                cx.dl.rrect(Rect::new(x - (trait_ - ecart) / 2.0 - 0.75 * d, zone.y + 10.0 * d, 1.5 * d, h - 20.0 * d), 0.75 * d, coul(C::Secondaire));
            }
            for (o, l) in l.iter().zip(&largeurs[g]) {
                let b = o.barre.unwrap();
                if cx.pill_operation(sub_id(BARRE_RAPIDE, o.action as usize), Rect::new(x, zone.y, *l, h), b.glyphe, b.court, BARRE_RAPIDE) {
                    choisie = Some(o.action);
                }
                x += l + ecart;
            }
        }
        cx.clip(None);
        if let Some(a) = choisie {
            self.agir(a);
        }
        zone.y
    }

    pub fn choisir_couleur(&mut self, i: usize, c: Option<Color>) {
        self.couleurs[i] = c;
        theme::appliquer(&self.couleurs);
    }

    /// Onglet Réglages › Couleurs. Toucher une couleur la déplie.
    fn onglet_couleurs(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let (mut groupe, mut tenu) = ("", false);
        for (i, l) in theme::LIGNES.iter().enumerate() {
            if l.groupe != groupe {
                groupe = l.groupe;
                cx.section(b, &groupe.to_uppercase());
            }
            let (ouvre, retour) = cx.ligne_couleur(b, sub_id(id("couleur"), i), sub_id(id("couleur-raz"), i), l.libelle, coul(l.c), self.couleurs[i].is_some());
            if retour {
                self.choisir_couleur(i, None);
                self.couleurs_changees = true;
                self.couleur_ouverte = self.couleur_ouverte.filter(|o| o.0 != i);
            } else if ouvre {
                self.couleur_ouverte = if self.couleur_ouverte.is_some_and(|o| o.0 == i) { None } else { Some((i, theme::vers_tsl(coul(l.c)))) };
            }
            let Some((_, tsl)) = self.couleur_ouverte.filter(|o| o.0 == i) else { continue };
            let mut neuf = tsl;
            for (k, (nom, max, unite)) in [("Teinte", 360.0, "°"), ("Saturation", 100.0, " %"), ("Luminosité", 100.0, " %")].into_iter().enumerate() {
                let z = sub_id(id("couleur-tsl"), k);
                cx.slider(b, z, nom, &format!("{:.0}{unite}", neuf[k]), &mut neuf[k], 0.0, max);
                tenu |= cx.ui.pressed(z);
            }
            if neuf != tsl {
                self.choisir_couleur(i, Some(theme::depuis_tsl(neuf)));
                self.couleur_ouverte = Some((i, neuf));
                self.couleurs_changees = true;
            }
            if cx.champ(b, id("couleur-code"), "Code", coul(C::Texte), &theme::hex(coul(l.c))) {
                self.saisir(Champ::Couleur(i));
            }
        }
        cx.section(b, "");
        if cx.button(b, id("couleurs-raz"), "Tout remettre par défaut") {
            self.couleurs = [None; theme::NB];
            theme::appliquer(&self.couleurs);
            self.couleur_ouverte = None;
            self.couleurs_changees = true;
        }
        // Gardées une fois le doigt levé, pas à chaque image d'un glissé
        if self.couleurs_changees && !tenu {
            self.couleurs_changees = false;
            self.reglages_changes = true;
        }
    }

    /// Onglet Réglages › Barre rapide.
    fn onglet_barre(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let avant = self.barre_visible;
        cx.toggle(b, id("barre-visible"), "Afficher la barre rapide", &mut self.barre_visible);
        self.reglages_changes |= avant != self.barre_visible;
        cx.paragraph(b, "Opérations à afficher au-dessus des outils. Elles n'y apparaissent que si elles s'appliquent à la sélection.");
        for (g, titre) in GROUPES {
            cx.section(b, &titre.to_uppercase());
            for o in CATALOGUE {
                let Some(e) = o.barre.filter(|e| e.groupe == g) else { continue };
                let (avant, mut v) = (e.retenue(&self.acces), e.retenue(&self.acces));
                cx.toggle(b, sub_id(id("acces"), o.action as usize), o.libelle, &mut v);
                if v != avant {
                    self.acces.insert(e.cle(), v);
                    self.reglages_changes = true;
                }
            }
        }
        if cx.bouton(b, id("acces-raz"), "Revenir aux opérations par défaut", !self.acces.is_empty()) {
            self.acces.clear();
            self.reglages_changes = true;
        }
    }

    /// Réglages à enregistrer, s'ils ont changé.
    pub fn prendre_reglages(&mut self) -> Option<String> {
        std::mem::take(&mut self.reglages_changes).then(|| {
            let mut l: Vec<String> = self.acces.iter().map(|(k, v)| format!("{k}\t{}", *v as u8)).collect();
            for (p, v) in &self.primitives {
                l.extend(v.iter().map(|(k, x)| {
                    let x = match x {
                        V::N(n) => format!("n{n}"),
                        V::T(t) => format!("t{t}"),
                        V::B(b) => format!("b{}", *b as u8),
                    };
                    format!("primitive.{}.{k}\t{x}", p.cle())
                }));
            }
            l.extend(theme::LIGNES.iter().zip(&self.couleurs).filter_map(|(c, v)| v.map(|v| format!("couleur.{}\t{}", c.cle, theme::hex(v)))));
            l.sort();
            l.insert(0, format!("pave\t{} {}", self.pave_place[0], self.pave_place[1]));
            l.insert(0, format!("barre\t{}", self.barre_visible as u8));
            l.join("\n") + "\n"
        })
    }

    /// Reprend les réglages enregistrés. Une ligne illisible est ignorée.
    pub fn reprendre_reglages(&mut self, texte: &str) {
        for (k, v) in texte.lines().filter_map(|l| l.split_once('\t')) {
            if k == "barre" {
                self.barre_visible = v != "0";
            } else if k == "pave" {
                let mut n = v.split(' ').filter_map(|x| x.parse::<f32>().ok()).filter(|x| x.is_finite());
                if let (Some(x), Some(y)) = (n.next(), n.next()) {
                    self.pave_place = [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)];
                }
            } else if let Some(cle) = k.strip_prefix("primitive.") {
                self.reprendre_primitive(cle, v);
            } else if let Some(cle) = k.strip_prefix("couleur.") {
                if let (Some(i), Some(c)) = (theme::LIGNES.iter().position(|l| l.cle == cle), theme::lire_hex(v)) {
                    self.couleurs[i] = Some(c);
                }
            } else {
                self.acces.insert(k.to_string(), v != "0");
            }
        }
        theme::appliquer(&self.couleurs);
    }

    /// Reprend un réglage de primitive ; un champ inconnu ou d'un autre genre est ignoré.
    fn reprendre_primitive(&mut self, cle: &str, v: &str) {
        let Some((p, k)) = cle.split_once('.') else { return };
        let Some(prim) = primitives::TOUTES.iter().find(|x| x.cle() == p).copied() else { return };
        let valeurs = self.primitives.entry(prim).or_insert_with(|| prim.defauts());
        let Some((k, actuel)) = valeurs.iter_mut().find(|x| *x.0 == k) else { return };
        let _ = k;
        let (genre, reste) = v.split_at(v.is_char_boundary(1).then_some(1).unwrap_or(0));
        match (genre, actuel) {
            ("n", V::N(x)) => *x = reste.parse().ok().filter(|n: &f64| n.is_finite()).unwrap_or(*x),
            ("t", V::T(x)) => *x = reste.to_string(),
            ("b", V::B(x)) => *x = reste != "0",
            _ => {}
        }
    }

    /// Exécute une opération du catalogue, si elle s'applique à la sélection.
    fn agir(&mut self, a: A) {
        if !self.contexte().possible(a) {
            return;
        }
        match a {
            A::Ouvrir => self.selecteur = Some(Selecteur::Ouvrir),
            A::Importer => self.selecteur = Some(Selecteur::Importer),
            A::Enregistrer => self.enregistrer(),
            A::ExporterObj => self.exporter("obj", false),
            A::ExporterStl => self.exporter("stl", false),
            A::ExporterGlb => self.exporter("glb", false),
            A::ExporterSelectionObj => self.exporter("obj", true),
            A::ExporterSelectionStl => self.exporter("stl", true),
            A::ExporterSelectionGlb => self.exporter("glb", true),
            A::NouvelleScene => self.nouvelle_scene(),
            A::ExporterMtl => self.exporter_mtl(),
            A::Recadrer => self.recadrer(),
            A::CurseurOrigine => {
                self.projet.curseur = [0.0; 3];
                self.a_sauver = true;
            }
            A::CurseurSelection => self.curseur_vers_selection(),
            A::SelectionCurseur => self.selection_vers_curseur(),
            A::AjouterCube => self.ouvrir_ajout(Prim::Cube),
            A::AjouterSphere => self.ouvrir_ajout(Prim::Sphere),
            A::AjouterCylindre => self.ouvrir_ajout(Prim::Cylindre),
            A::AjouterCone => self.ouvrir_ajout(Prim::Cone),
            A::AjouterIcosphere => self.ouvrir_ajout(Prim::Icosphere),
            A::AjouterVis => self.ouvrir_ajout(Prim::Vis),
            A::AjouterEcrou => self.ouvrir_ajout(Prim::Ecrou),
            A::AjouterRondelle => self.ouvrir_ajout(Prim::Rondelle),
            A::AjouterTige => self.ouvrir_ajout(Prim::Tige),
            A::ChoisirImage => self.choisir_image(),
            A::MettreEnPage => self.mettre_en_page(),
            A::MarquerCoutures => self.operer(|t| t.marquer_coutures(true)),
            A::EffacerCoutures => self.operer(|t| t.marquer_coutures(false)),
            A::CouturesDepuisUv => self.coutures_depuis_uv(),
            A::Deplier => self.deplier(),
            A::DeplacerSet => {
                self.page_liste = PageListe::Deplacer(self.choix.clone());
                self.shell.start.open = true;
            }
            A::EditerUv => self.basculer(Contenu::Uv),
            A::ProjeterVue => {
                let (_, droite, haut, _) = self.repere();
                self.projeter(uv::Projection::Vue { droite, haut });
            }
            A::ProjeterCube => self.projeter(uv::Projection::Cube),
            A::ProjeterCylindre => self.projeter(uv::Projection::Cylindre { axe: 1 }),
            A::ProjeterSphere => self.projeter(uv::Projection::Sphere { axe: 1 }),
            A::ModifBooleen => self.ajouter_modif("booleen"),
            A::ModifRepetition => self.ajouter_modif("repetition"),
            A::ToutChoisir => {
                self.choix = self.affiches();
                self.projet.actif = self.choix.last().copied().or(self.projet.actif);
            }
            A::RienChoisir => self.choix.clear(),
            A::Dupliquer => self.dupliquer(),
            A::Supprimer => self.supprimer(),
            A::AppliquerEchelle => self.appliquer_echelle(),
            A::LierArmature => self.ouvrir_liaison(),
            A::Delier => self.delier(),
            A::EditerGeometrie => self.entrer_geometrie(),
            A::AjouterArmature => self.ajouter_armature("armature", Armature::nouvelle()),
            A::AjouterHumanoide => self.ajouter_armature("humanoide", Armature::humanoide()),
            A::AjouterOs => self.ajouter_os(),
            A::ExtruderOs => self.extruder_os(),
            A::SupprimerOs => self.supprimer_os(),
            A::ParenterOs => self.parenter_os(),
            A::DetacherOs => self.detacher_os(),
            A::TousOs => self.tous_os(true),
            A::AucunOs => self.tous_os(false),
            A::DescendantsOs => self.descendants_os(),
            A::ReposOs => self.repos_os(),
            A::CreerFace => self.operer(Travail::creer_face),
            A::Inverser => self.operer(Travail::inverser),
            A::Trianguler => self.operer(Travail::trianguler),
            A::Separer => self.separer(),
            A::SupprimerFaces => self.operer(Travail::supprimer_faces),
            A::CreerArete => self.operer(Travail::creer_arete),
            A::Subdiviser => self.lancer(Derniere::Subdivision { coupes: 1 }),
            A::Boucle => {
                if let Some(arete) = self.travail.as_ref().and_then(Travail::arete_depart) {
                    self.lancer(Derniere::Boucle { arete, coupes: 1, glissement: 0.0, uniforme: false, inverser: false });
                }
            }
            A::Dissoudre => self.operer(Travail::dissoudre),
            A::SupprimerAretes => self.operer(Travail::supprimer_aretes),
            A::AjouterSommet => {
                let c = self.projet.curseur;
                let a = self.projet.actif.filter(|k| t_edite(&self.travail, *k)).or(self.travail.as_ref().and_then(|t| t.edites.first().copied()));
                if let Some(a) = a {
                    self.operer(|t| t.ajouter_sommet(c, a));
                }
            }
            A::Fusionner => self.lancer(Derniere::Fusion { cible: self.fusion }),
            A::Confondus => self.lancer(Derniere::Proches { tolerance: 0.0001 }),
            A::Revolution => self.revolution(self.espace),
            A::SupprimerSommets => self.operer(Travail::supprimer_sommets),
            A::Lies => self.selectionner(Travail::lies),
            A::TousSommets => self.selectionner(|t| { t.tout(T::S); Ok(None) }),
            A::ToutesAretes => self.selectionner(|t| { t.tout(T::A); Ok(None) }),
            A::ToutesFaces => self.selectionner(|t| { t.tout(T::F); Ok(None) }),
            A::AucunSommet => self.selectionner(|t| { t.aucun(Some(T::S)); Ok(None) }),
            A::AucuneArete => self.selectionner(|t| { t.aucun(Some(T::A)); Ok(None) }),
            A::AucuneFace => self.selectionner(|t| { t.aucun(Some(T::F)); Ok(None) }),
            A::ToutDeselectionner => self.selectionner(|t| { t.aucun(None); Ok(None) }),
            A::FacesSommets => self.selectionner(|t| t.convertir(T::F, T::S)),
            A::FacesAretes => self.selectionner(|t| t.convertir(T::F, T::A)),
            A::AretesSommets => self.selectionner(|t| t.convertir(T::A, T::S)),
            A::AretesFaces => self.selectionner(|t| t.convertir(T::A, T::F)),
            A::SommetsAretes => self.selectionner(|t| t.convertir(T::S, T::A)),
            A::SommetsFaces => self.selectionner(|t| t.convertir(T::S, T::F)),
        }
    }

    /// Onglet Opérations de l'aide, tiré du catalogue : un nom touché déplie sa description.
    fn aide_operations(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let mut tete = None;
        for o in CATALOGUE {
            if tete != Some((o.menu, o.rubrique)) {
                tete = Some((o.menu, o.rubrique));
                let titre = if o.menu.titre() == o.rubrique { o.rubrique.to_string() } else { format!("{} · {}", o.menu.titre(), o.rubrique) };
                cx.section(b, &titre.to_uppercase());
            }
            let ouvert = self.deplie == Some(o.action);
            if cx.row(b, sub_id(id("aide-operation"), o.action as usize), o.libelle, None, ouvert) {
                self.deplie = (!ouvert).then_some(o.action);
            }
            if ouvert {
                cx.paragraph(b, &phrase(o.aide));
            }
        }
    }

    /// Réglages de la dernière opération : chaque changement la rejoue.
    fn reglages_derniere(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some(d) = self.derniere else {
            return cx.paragraph(b, "Rien à régler pour l'instant. Après une transformation au doigt ou une opération réglable, ses valeurs s'affichent ici.");
        };
        cx.section(b, &d.titre().to_uppercase());
        let mut nouveau = d;
        match &mut nouveau {
            Derniere::Subdivision { coupes } => {
                if cx.champ(b, id("r-coupes"), "Coupes", coul(C::Texte), &coupes.to_string()) {
                    self.saisir(Champ::Coupes);
                }
            }
            Derniere::Boucle { coupes, glissement, uniforme, inverser, .. } => {
                if cx.champ(b, id("r-coupes"), "Coupes", coul(C::Texte), &coupes.to_string()) {
                    self.saisir(Champ::Coupes);
                }
                if cx.champ(b, id("r-gliss"), "Glissement", coul(C::Texte), &chiffres(*glissement)) {
                    self.saisir(Champ::Glissement);
                }
                cx.toggle(b, id("r-uniforme"), "Uniforme", uniforme);
                if *uniforme {
                    cx.toggle(b, id("r-inverser"), "Depuis l'autre côté", inverser);
                }
            }
            Derniere::Fusion { cible } => {
                for (c, n) in [(Cible::Centre, "Centre"), (Cible::Premier, "Premier"), (Cible::Dernier, "Dernier")] {
                    if cx.row(b, id(n), n, None, *cible == c) {
                        *cible = c;
                        self.fusion = c;
                    }
                }
            }
            Derniere::Revolution { axe, angle, inverse, segments, fermer, .. } => {
                for (k, n) in AXES_REVOLUTION.iter().enumerate() {
                    if cx.row(b, sub_id(id("r-axe"), k), &format!("Axe {n}"), None, *axe == k) {
                        *axe = k;
                    }
                }
                if cx.champ(b, id("r-angle"), "Angle", coul(C::Texte), &chiffres(*angle)) {
                    self.saisir(Champ::Angle);
                }
                if cx.champ(b, id("r-seg"), "Segments", coul(C::Texte), &segments.to_string()) {
                    self.saisir(Champ::Segments);
                }
                cx.toggle(b, id("r-inverse"), "Sens inverse", inverse);
                if *angle == 360.0 {
                    cx.toggle(b, id("r-fermer"), "Fermer (souder)", fermer);
                }
            }
            Derniere::Projection { .. } => cx.paragraph(b, "L'axe se règle dans la vue UV."),
            Derniere::MiseEnPage { .. } | Derniere::Depliage { .. } => cx.paragraph(b, "La marge, la densité et la rotation se règlent dans la vue UV."),
            Derniere::Proches { tolerance } => {
                if cx.champ(b, id("r-tol"), "Tolérance", coul(C::Texte), &format!("{tolerance}")) {
                    self.saisir(Champ::Tolerance);
                }
            }
            Derniere::Translation { d } => self.champs(cx, b, "Décalage", id("r-decalage"), *d, Champ::Decalage),
            Derniere::Rotation { degres, axe, vue, pivot } => {
                if cx.champ(b, id("r-angle"), "Angle", coul(C::Texte), &chiffres(*degres)) {
                    self.saisir(Champ::Angle);
                }
                for (k, n) in AXES_REVOLUTION.iter().enumerate().take(if vue.is_some() { 4 } else { 3 }) {
                    if cx.row(b, sub_id(id("r-axe"), k), &format!("Axe {n}"), None, *axe == k) {
                        *axe = k;
                    }
                }
                self.champs(cx, b, "Pivot", id("r-pivot"), *pivot, Champ::Pivot);
            }
            Derniere::Echelle { s, pivot } => {
                self.champs(cx, b, "Facteurs", id("r-facteur"), *s, Champ::Facteur);
                self.champs(cx, b, "Pivot", id("r-pivot"), *pivot, Champ::Pivot);
            }
        }
        if nouveau != d {
            self.rejouer(nouveau);
        }
    }

    /// Fiche de la sélection en mode Armature : où sont les bouts choisis.
    fn fiche_os(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some((e, _, a)) = self.armature_os() else {
            return cx.paragraph(b, "Aucune armature ouverte.");
        };
        let (nb, n) = (a.os.len(), e.os_choisis(a).len());
        cx.paragraph(b, &format!("{nb} os · {n} choisi{}", if n > 1 { "s" } else { "" }));
        let (pose, pts) = (e.pose, self.points());
        if pose {
            cx.paragraph(b, "En pose : les os tournent autour de leur tête.");
        } else if !pts.is_empty() {
            self.champs(cx, b, "Position", id("c-pos"), ed::centre(&pts), Champ::Position);
        }
        self.reglages_pas(cx, b);
    }

    /// Panneau Os : l'os actif, les opérations, l'arbre des os.
    fn panneau_os(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        if self.os.is_none() {
            return self.panneau_os_scene(cx, b);
        }
        cx.section(b, "ARMATURES");
        let ouverte = self.os.as_ref().and_then(|e| e.objet);
        let liste: Vec<(usize, String)> = self.affiches().into_iter().filter(|k| self.projet.objets[*k].armature.is_some()).map(|k| (k, self.projet.objets[k].nom.clone())).collect();
        // En pose, sans os choisi : l'armature entière revient au repos, sur sa ligne
        let tout_au_repos = self.armature_os().is_some_and(|(e, _, a)| e.pose && e.bouts.is_empty() && !a.pose.is_empty());
        let actif_os = self.os.as_ref().and_then(|e| e.actif.clone());
        match self.arbre_armatures(cx, b, &liste, ouverte, actif_os, tout_au_repos) {
            // Toucher l'armature ouverte vide la sélection des os
            Some(ClicArbre::Armature(k)) if ouverte == Some(k) => self.tous_os(false),
            Some(ClicArbre::Armature(k)) => self.ouvrir_armature(k),
            Some(ClicArbre::Tout(k, os)) => {
                if ouverte != Some(k) {
                    self.ouvrir_armature(k);
                }
                if let Some((s, a)) = self.os.as_mut().zip(self.projet.objets[k].armature.as_ref()) {
                    let ids: Vec<String> = match &os {
                        Some(i) => std::iter::once(i.clone()).chain(a.descendants(i)).collect(),
                        None => a.os.iter().map(|x| x.id.clone()).collect(),
                    };
                    s.actif = os.or_else(|| a.ordre().first().map(|x| x.id.clone()));
                    s.selectionner(a, &ids, false);
                }
            }
            Some(ClicArbre::Os(k, i)) => {
                if ouverte != Some(k) {
                    self.ouvrir_armature(k);
                }
                if let Some((s, a)) = self.os.as_mut().zip(self.projet.objets[k].armature.as_ref()) {
                    s.actif = Some(i.clone());
                    s.selectionner(a, std::slice::from_ref(&i), false);
                }
            }
            Some(ClicArbre::Repos) => self.repos_os(),
            None => {}
        }
        // En pose, on pose : l'ajout reste au repos et en Scène, où est aussi l'humanoïde
        if !self.os.as_ref().is_some_and(|e| e.pose) && cx.lien(b, id("os-nouvelle"), "Ajouter") {
            self.agir(A::AjouterArmature);
        }
        let Some((e, o, a)) = self.armature_os() else {
            return cx.paragraph(b, "Ajoutez une armature sur le curseur 3D, ou touchez-en une, ici ou dans la vue.");
        };
        let (e, a, echelle) = (e.clone(), a.clone(), o.echelle[0]);
        if e.pose {
            let n = a.os.iter().filter(|x| a.pose.get(&x.id).is_some()).count();
            let texte = match n { 0 => "Au repos : touchez un os, puis tournez-le.".to_string(), 1 => "1 os posé".to_string(), n => format!("{n} os posés") };
            if cx.legende_lien(b, &texte, (n > 0).then_some((id("pose-sauver"), "Sauvegarder la pose"))) {
                self.sauver_pose();
            }
        }
        cx.toggle(b, id("os-poids"), "Poids de l'os actif", &mut self.voir_poids);
        if self.voir_poids {
            cx.paragraph(b, if self.calcul_peau.is_some() { "Calcul des poids…" } else if e.actif.is_some() { "Bleu : aucun effet · rouge : plein effet de l'os actif." } else { "Touchez un os pour voir ses poids." });
        }
        // Os actif : seulement quand des os sont choisis
        let actif = e.actif.as_ref().and_then(|i| a.trouver(i)).filter(|_| !e.bouts.is_empty());
        if actif.is_some() {
            cx.section(b, "OS ACTIF");
        }
        if let Some(x) = actif {
            if e.pose {
                cx.paragraph(b, &x.nom);
                let r = a.pose.get(&x.id).map_or([0.0, 0.0, 0.0, 1.0], |p| p.r);
                self.angles_os(cx, b, &x.id, r, x.limites());
                // Ce lien ne concerne que l'os actif
                if a.pose.contains_key(&x.id) && cx.lien(b, id("os-repos-actif"), "Remettre au repos") {
                    let i = x.id.clone();
                    self.changer_os(|a, _| a.remettre(&[i]));
                }
            } else if cx.champ(b, id("os-nom"), "Nom", coul(C::Texte), &x.nom) {
                self.saisir(Champ::OsNom);
            }
            let parent = x.parent.as_ref().and_then(|p| a.trouver(p)).map_or(AUCUN_PARENT, |p| p.nom.as_str());
            if !e.pose && cx.champ(b, id("os-parent"), "Parent", coul(C::Texte), parent) {
                self.parents_ouverts = !self.parents_ouverts;
            }
            if self.parents_ouverts && !e.pose {
                let exclus = a.descendants(&x.id);
                if cx.row(b, id("os-sans-parent"), AUCUN_PARENT, None, x.parent.is_none()) {
                    self.changer_parent_os(&x.id, None);
                }
                for (k, p) in a.os.iter().enumerate().filter(|(_, p)| p.id != x.id && !exclus.contains(&p.id)) {
                    if cx.row(b, sub_id(id("os-parent-de"), k), &p.nom, None, x.parent.as_ref() == Some(&p.id)) {
                        self.changer_parent_os(&x.id, Some(&p.id));
                    }
                }
            }
            if !e.pose && cx.champ(b, id("os-longueur"), "Longueur", coul(C::Texte), &chiffres(x.longueur * echelle)) {
                self.saisir(Champ::OsLongueur);
            }
            if !e.pose {
                self.limites_os(cx, b, x);
            }
        } else {
            cx.paragraph(b, "Touchez un os, ici ou dans la vue, pour le régler.");
        }
        // Chaque mode ne montre que ses opérations, le repos ses os ; le retour au repos
        // est en lien, sous l'os actif ou sous l'armature
        // La sélection se fait dans l'arbre : ses opérations restent à la barre rapide
        self.operations_si(cx, b, Menu::Os, |a| !matches!(a, A::ReposOs | A::TousOs | A::AucunOs | A::DescendantsOs) && actions::du_mode_pose(a).is_none_or(|p| p == e.pose));
    }

    /// Onglet Catalogue du mode Pose : les poses gardées de l'armature ouverte. Un appui
    /// en applique une ; la pose choisie se renomme, se remplace ou se supprime.
    fn panneau_catalogue(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some((_, o, a)) = self.armature_os() else {
            return cx.paragraph(b, "Ouvrez une armature pour voir ses poses.");
        };
        let (nom, poses) = (o.nom.clone(), a.poses_nommees());
        cx.section(b, &format!("POSES DE {}", nom.to_uppercase()));
        if poses.is_empty() {
            return cx.paragraph(b, "Aucune pose gardée : réglez les os, puis Sauvegarder la pose dans l'onglet Pose.");
        }
        if self.pose_choisie.is_some_and(|i| i >= poses.len()) {
            self.pose_choisie = None;
        }
        for (i, (n, _)) in poses.iter().enumerate() {
            if cx.row(b, sub_id(id("pose-nommee"), i), n, None, self.pose_choisie == Some(i)) {
                self.appliquer_pose(i);
            }
        }
        let Some(i) = self.pose_choisie else {
            return cx.paragraph(b, "Touchez une pose pour la donner à l'armature.");
        };
        cx.section(b, "POSE CHOISIE");
        if cx.champ(b, id("pose-nom"), "Nom", coul(C::Texte), &poses[i].0) {
            self.saisir(Champ::PoseNom);
        }
        if cx.lien(b, id("pose-remplacer"), "Remplacer par la pose actuelle") {
            self.changer_os(|a, _| {
                let mut l = a.poses_nommees();
                l[i].1 = a.pose.clone();
                a.regler_poses(&l);
            });
            self.dire(format!("Pose « {} » remplacée", poses[i].0));
        }
        if cx.lien(b, id("pose-supprimer"), "Supprimer") {
            self.changer_os(|a, _| {
                let mut l = a.poses_nommees();
                l.remove(i);
                a.regler_poses(&l);
            });
            self.pose_choisie = None;
            self.dire(format!("Pose « {} » supprimée", poses[i].0));
        }
    }

    /// Garde la pose en cours dans le catalogue de l'armature ouverte.
    pub fn sauver_pose(&mut self) {
        let Some((_, _, a)) = self.armature_os() else { return };
        let l = a.poses_nommees();
        let nom = (1..).map(|k| format!("Pose {k}")).find(|n| l.iter().all(|p| &p.0 != n)).unwrap_or_default();
        let n = l.len();
        let x = nom.clone();
        self.changer_os(move |a, _| {
            let mut l = a.poses_nommees();
            l.push((x, a.pose.clone()));
            a.regler_poses(&l);
        });
        self.pose_choisie = Some(n);
        self.dire(format!("« {nom} » gardée dans le catalogue"));
    }

    /// Donne à l'armature ouverte la pose `i` du catalogue.
    pub fn appliquer_pose(&mut self, i: usize) {
        let Some(p) = self.armature_os().and_then(|(_, _, a)| a.poses_nommees().into_iter().nth(i)) else { return };
        self.changer_os(|a, _| a.pose = p.1);
        self.pose_choisie = Some(i);
    }

    fn renommer_pose(&mut self, nom: &str) {
        let nom = nom.trim().to_string();
        if nom.is_empty() {
            return self.dire("Nom vide".into());
        }
        let Some(i) = self.pose_choisie else { return };
        self.changer_os(|a, _| {
            let mut l = a.poses_nommees();
            if let Some(p) = l.get_mut(i) {
                p.0 = nom;
            }
            a.regler_poses(&l);
        });
    }

    /// Armatures et, sous celle qui est ouverte, l'arbre de ses os : chaque ligne qui a
    /// des enfants se plie par son chevron. Rend ce qui a été touché.
    fn arbre_armatures(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, liste: &[(usize, String)], ouverte: Option<usize>, actif_os: Option<String>, repos: bool) -> Option<ClicArbre> {
        let mut clic = None;
        // Les os choisis, et l'appui long qui prend une descendance : en édition des os
        let edition = self.os.is_some();
        let choisis: Vec<String> = self.armature_os().map(|(e, _, a)| e.os_choisis(a)).unwrap_or_default();
        for (k, nom) in liste {
            let k = *k;
            let Some(a) = self.projet.objets[k].armature.clone() else { continue };
            let cle = format!("a:{}", self.projet.objets[k].id);
            let deplie = ouverte == Some(k) && !self.os_plies.contains(&cle);
            let lien = (repos && ouverte == Some(k)).then_some((id("os-repos-armature"), "Tout au repos"));
            let (ligne, chevron, appui, desc) = cx.ligne_arbre(b, sub_id(id("os-armature"), k), sub_id(id("os-chevron"), k), nom, 0, Some(!deplie), ouverte == Some(k), false, lien, edition);
            if appui {
                clic = Some(ClicArbre::Repos);
            }
            if desc {
                clic = Some(ClicArbre::Tout(k, None));
            }
            if chevron && ouverte == Some(k) {
                if !self.os_plies.remove(&cle) {
                    self.os_plies.insert(cle);
                }
            } else if (chevron || ligne) && ouverte != Some(k) {
                self.os_plies.remove(&cle);
                clic = Some(ClicArbre::Armature(k));
            } else if ligne {
                clic = Some(ClicArbre::Armature(k));
            }
            if !deplie {
                continue;
            }
            let ordre = a.ordre();
            let mut cache: Option<usize> = None;
            for (r, x) in ordre.iter().enumerate() {
                let p = a.profondeur(&x.id);
                match cache {
                    Some(c) if p > c => continue,
                    _ => cache = None,
                }
                let enfants = ordre.get(r + 1).is_some_and(|y| a.profondeur(&y.id) > p);
                let plie = self.os_plies.contains(&x.id);
                let (ligne, chevron, _, desc) = cx.ligne_arbre(b, sub_id(id("os-rang"), r), sub_id(id("os-chevron-os"), r), &x.nom, p + 1, enfants.then_some(plie), actif_os.as_ref() == Some(&x.id), choisis.contains(&x.id), None, edition);
                if chevron && !self.os_plies.remove(&x.id) {
                    self.os_plies.insert(x.id.clone());
                }
                if desc {
                    clic = Some(ClicArbre::Tout(k, Some(x.id.clone())));
                }
                if ligne {
                    clic = Some(ClicArbre::Os(k, x.id.clone()));
                }
                if enfants && plie {
                    cache = Some(p);
                }
            }
        }
        clic
    }

    /// Limites de rotation de l'os actif, repliées derrière un lien : une ligne par axe,
    /// son état, et ses bornes quand il est limité.
    fn limites_os(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, x: &Os) {
        let l = x.limites();
        let n = l.iter().filter(|v| **v != Limite::Libre).count();
        let texte = match n { 0 => "Rotation libre".to_string(), 1 => "1 axe limité".to_string(), n => format!("{n} axes limités") };
        if cx.legende_lien(b, &texte, Some((id("os-limites"), if self.limites_ouvertes { "Replier" } else { "Limites" }))) {
            self.limites_ouvertes = !self.limites_ouvertes;
        }
        if !self.limites_ouvertes {
            return;
        }
        for k in 0..3 {
            let (etat, bornes) = match l[k] {
                Limite::Libre => ("Libre", None),
                Limite::Bornee(a, b) => ("Limité", Some((format!("{}°", chiffres(a)), format!("{}°", chiffres(b))))),
                Limite::Bloquee => ("Bloqué", None),
            };
            let nom = ["Rotation X", "Vrille Y", "Rotation Z"][k];
            let bornes = bornes.as_ref().map(|(a, b)| (a.as_str(), b.as_str()));
            let (e, mn, mx) = cx.ligne_limite(b, sub_id(id("os-limite"), k), nom, coul(COUL_AXES[k]), etat, bornes);
            if e {
                self.changer_etat_limite(k);
            }
            if mn {
                self.saisir(Champ::OsLimite(k, false));
            }
            if mx {
                self.saisir(Champ::OsLimite(k, true));
            }
        }
    }

    /// Trois curseurs : la rotation de l'os actif depuis son repos, en degrés, autour
    /// des axes X, Y puis Z de l'os.
    fn angles_os(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, os: &str, r: ed::Q, l: [Limite; 3]) {
        let base = match &self.angles_tenus {
            Some((i, v)) if i == os => *v,
            _ => angles_os(r),
        };
        let (mut nouveau, mut tenu) = (base, false);
        for k in 0..3 {
            let z = sub_id(id("os-angle"), k);
            let mut v = base[k] as f32;
            let (plage, libre) = match l[k] {
                Limite::Libre => (None, true),
                Limite::Bornee(a, b) => (Some((a as f32, b as f32)), true),
                Limite::Bloquee => (None, false),
            };
            let nom = ["Rotation X", "Vrille Y", "Rotation Z"][k];
            let montre = if libre { format!("{:.0}°", base[k]) } else { format!("{:.0}° · bloqué", base[k]) };
            cx.slider_borne(b, z, nom, &montre, &mut v, -180.0, 180.0, plage, libre);
            tenu |= cx.ui.pressed(z);
            if v != base[k] as f32 {
                nouveau[k] = (v as f64).round();
            }
        }
        if nouveau != base {
            self.regler_angles(os, nouveau);
        }
        if !tenu {
            self.angles_tenus = None;
            self.angle_saisie = false;
        }
    }

    /// Donne à l'os `id` la rotation de ces angles, depuis son repos.
    pub fn regler_angles(&mut self, id: &str, angles: [f64; 3]) {
        if !self.angle_saisie {
            self.memoriser();
            self.angle_saisie = true;
        }
        let Some(e) = &mut self.os else { return };
        let Some(o) = e.objet.and_then(|k| self.projet.objets.get_mut(k)) else { return };
        let Some(a) = &mut o.armature else { return };
        // Dans les limites de l'os ; un axe bloqué garde sa valeur
        let avant = angles_os(a.pose.get(id).map_or([0.0, 0.0, 0.0, 1.0], |p| p.r));
        let l = a.trouver(id).map_or([Limite::Libre; 3], |x| x.limites());
        let angles = [0, 1, 2].map(|k| l[k].borner(angles[k], avant[k]));
        self.angles_tenus = Some((id.to_string(), angles));
        let r = depuis_angles_os(angles);
        let t = a.pose.get(id).map_or([0.0; 3], |p| p.t);
        if angles == [0.0; 3] && t == [0.0; 3] {
            a.pose.remove(id);
        } else {
            a.pose.insert(id.to_string(), Ecart { r, t });
        }
        o.maillage = Arc::new(a.enveloppe_pose());
        e.version += 1;
        self.a_sauver = true;
    }

    /// Panneau Os en mode Scène : l'armature active, sa pose, ses poids et ses os.
    fn panneau_os_scene(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        cx.section(b, "ARMATURES");
        let actif = self.projet.actif.filter(|k| self.projet.objets.get(*k).is_some_and(|o| o.armature.is_some()));
        let liste: Vec<(usize, String)> = self.affiches().into_iter().filter(|k| self.projet.objets[*k].armature.is_some()).map(|k| (k, self.projet.objets[k].nom.clone())).collect();
        let os_vu = actif.and_then(|k| {
            let i = &self.projet.objets[k].id;
            self.os_scene.as_ref().filter(|(a, _)| a == i).map(|(_, x)| x.clone())
        });
        let posee = actif.and_then(|k| self.projet.objets[k].armature.as_ref()).is_some_and(|a| !a.pose.is_empty());
        match self.arbre_armatures(cx, b, &liste, actif, os_vu, posee) {
            Some(ClicArbre::Armature(k)) => {
                self.projet.actif = Some(k);
                self.choix = vec![k];
            }
            Some(ClicArbre::Os(k, i)) => {
                self.projet.actif = Some(k);
                self.choix = vec![k];
                self.os_scene = Some((self.projet.objets[k].id.clone(), i));
            }
            Some(ClicArbre::Repos) => {
                if let Some(k) = actif {
                    self.repos_armature(k);
                }
            }
            Some(ClicArbre::Tout(..)) | None => {}
        }
        if cx.lien(b, id("os-nouvelle"), "Ajouter") {
            self.agir(A::AjouterArmature);
        }
        let Some(k) = actif else {
            return cx.paragraph(b, "Touchez une armature, ici ou dans la vue, pour voir ses réglages.");
        };
        let o = &self.projet.objets[k];
        let Some(a) = o.armature.clone() else { return };
        let (nom, id_a) = (o.nom.clone(), o.id.clone());
        cx.section(b, &nom.to_uppercase());
        let lies = self.projet.objets.iter().filter(|x| x.liaison.as_ref().is_some_and(|l| l.armature == id_a)).count();
        let poses = a.os.iter().filter(|x| a.pose.get(&x.id).is_some()).count();
        cx.paragraph(b, &format!(
            "{} os · {} · {}",
            a.os.len(),
            match lies { 0 => "aucun objet lié".to_string(), 1 => "1 objet lié".to_string(), n => format!("{n} objets liés") },
            match poses { 0 => "au repos".to_string(), 1 => "1 os posé".to_string(), n => format!("{n} os posés") },
        ));
        if cx.champ(b, id("os-armature-nom"), "Nom", coul(C::Texte), &nom) {
            self.choix = vec![k];
            self.saisir(Champ::Nom);
        }
        if cx.lien(b, id("os-editer"), "Éditer les os") {
            self.choix = vec![k];
            self.entrer_geometrie();
            return;
        }
        cx.toggle(b, id("os-poids"), "Poids de l'os actif", &mut self.voir_poids);
        let os_actif = self.os_scene.as_ref().filter(|(x, _)| *x == id_a).map(|(_, x)| x.clone()).filter(|x| a.trouver(x).is_some());
        if self.voir_poids {
            cx.paragraph(b, if self.calcul_peau.is_some() { "Calcul des poids…" } else if os_actif.is_some() { "Bleu : aucun effet · rouge : plein effet de l'os actif." } else { "Touchez un os de l'arbre pour voir ses poids." });
        }
    }

    /// Fiche de la sélection en mode Géométrie : les opérations par type d'élément.
    fn fiche_geo(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let Some(t) = &self.travail else { return };
        let resume = if t.sel.vide() { "Rien de sélectionné".to_string() } else { t.resume() };
        cx.paragraph(b, &resume);
        let pts = self.points();
        if !pts.is_empty() {
            self.champs(cx, b, "Position", id("c-pos"), ed::centre(&pts), Champ::Position);
        }
        self.operations(cx, b, Menu::Geometrie);
        self.reglages_pas(cx, b);
    }

    fn reglages_pas(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        cx.section(b, "PAS");
        let t = self.pas_translation();
        for (cle, nom, v, c) in [(id("p-t"), "Translation", t, Champ::PasTranslation), (id("p-a"), "Angle", self.pas.angle, Champ::PasAngle), (id("p-e"), "Échelle", self.pas.echelle, Champ::PasEchelle)] {
            if cx.champ(b, cle, nom, coul(C::Texte), &chiffres(v)) {
                self.saisir(c);
            }
        }
    }

    fn champs(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, titre: &str, cle: Id, v: [f64; 3], c: fn(usize) -> Champ) {
        cx.section(b, &titre.to_uppercase());
        for k in 0..3 {
            if cx.champ(b, sub_id(cle, k), NOMS_AXES[k], coul(COUL_AXES[k]), &chiffres(v[k])) {
                self.saisir(c(k));
            }
        }
    }

    /// Fiche de la sélection.
    /// Page « Lier à une armature » : l'armature, puis ses os à cocher.
    fn page_liaison(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let l = self.cibles_liaison();
        let (Some(mut page), false) = (self.liaison.clone(), l.is_empty()) else {
            self.liaison = None;
            return self.fiche(cx, b);
        };
        let p = &self.projet;
        let titre = if let [k] = l[..] { p.objets[k].nom.clone() } else { format!("{} objets", l.len()) };
        cx.section(b, &format!("LIER {}", titre.to_uppercase()));
        let armatures: Vec<(&Objet, &Armature)> = p.objets.iter().filter_map(|o| Some((o, o.armature.as_ref()?))).collect();
        for (k, (o, a)) in armatures.iter().enumerate() {
            let choisie = page.armature.as_ref() == Some(&o.id);
            if cx.row(b, sub_id(id("liaison-armature"), k), &o.nom, None, choisie) && !choisie {
                page.armature = Some(o.id.clone());
                page.os = a.os.iter().map(|x| x.id.clone()).collect();
            }
        }
        let armature = page.armature.as_ref().and_then(|i| armatures.iter().find(|(o, _)| o.id == *i)).map(|(_, a)| *a);
        // Au-dessus de l'arbre : un humanoïde a 19 os
        let pret = armature.is_some_and(|a| page.os.iter().any(|i| a.trouver(i).is_some()));
        let fin = cx.rangee(b, id("liaison-fin"), &[("Lier", pret), ("Annuler", true)]);
        if let Some(a) = armature {
            page.os.retain(|i| a.trouver(i).is_some());
            cx.paragraph(b, &match page.os.len() {
                0 => "Touchez les os à lier.".to_string(),
                1 => "Lié à cet os seul, l'objet le suivra rigidement.".to_string(),
                n => format!("Les sommets se répartiront entre ces {n} os."),
            });
            cx.section(b, "OS");
            match cx.rangee(b, id("liaison-tous"), &[("Tous les os", page.os.len() < a.os.len()), ("Aucun", !page.os.is_empty())]) {
                Some(0) => page.os = a.os.iter().map(|x| x.id.clone()).collect(),
                Some(1) => page.os.clear(),
                _ => {}
            }
            for (k, x) in a.ordre().into_iter().enumerate() {
                let coche = page.os.contains(&x.id);
                let nom = format!("{}{}", "    ".repeat(a.profondeur(&x.id)), x.nom);
                if cx.row(b, sub_id(id("liaison-os"), k), &nom, None, coche) {
                    if coche {
                        page.os.retain(|i| *i != x.id);
                    } else {
                        page.os.push(x.id.clone());
                    }
                }
            }
        } else {
            cx.paragraph(b, "Choisissez une armature.");
        }
        self.liaison = Some(page);
        match fin {
            Some(0) => self.lier(),
            Some(1) => self.liaison = None,
            _ => {}
        }
    }

    /// Ce qui relie un objet à une armature, pour sa fiche.
    fn texte_liaison(&self, o: &Objet) -> Option<String> {
        let p = &self.projet;
        if o.armature.is_some() {
            let n = p.objets.iter().filter(|x| x.liaison.as_ref().is_some_and(|l| l.armature == o.id)).count();
            return (n > 0).then(|| format!("{n} objet{} lié{}.", if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" }));
        }
        let l = o.liaison.as_ref()?;
        let x = p.objet_id(&l.armature)?;
        Some(match (&l.os[..], &x.armature) {
            ([i], Some(a)) => format!("Lié à « {} », os {}.", x.nom, a.trouver(i)?.nom),
            _ => format!("Lié à « {} », {} os.", x.nom, l.os.len()),
        })
    }

    fn fiche(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body) {
        let pts = self.points();
        if pts.is_empty() {
            cx.paragraph(b, "Aucun objet choisi.");
        } else {
            let n = pts.len();
            let titre = self.seul().map_or(format!("{n} objets"), |k| self.projet.objets[k].nom.clone());
            cx.section(b, &titre.to_uppercase());
            if let Some(k) = self.seul() {
                let o = &self.projet.objets[k];
                let (s, f, mat, nom) = (o.maillage.sommets.len(), o.maillage.faces.len(), o.materiau.clone(), o.nom.clone());
                match &o.armature {
                    Some(a) => cx.paragraph(b, &format!("Armature de {} os. Un double appui l'édite.", a.os.len())),
                    None => cx.paragraph(b, &format!("{s} sommets · {f} faces · matériau {mat}")),
                }
                if let Some(t) = self.texte_liaison(&self.projet.objets[k]) {
                    cx.paragraph(b, &t);
                }
                if cx.champ(b, id("c-nom"), "Nom", coul(C::Texte), &nom) {
                    self.saisir(Champ::Nom);
                }
            }
            self.champs(cx, b, "Position", id("c-pos"), ed::centre(&pts), Champ::Position);
            if let Some(k) = self.seul() {
                let o = &self.projet.objets[k];
                let (rot, ech) = (ed::euler(o.rotation), o.echelle);
                let base = self.base(k);
                self.champs(cx, b, "Rotation", id("c-rot"), rot, Champ::Rotation);
                self.champs(cx, b, "Dimensions", id("c-dim"), [0, 1, 2].map(|c| base[c] * ech[c]), Champ::Dimension);
                if let Some(a) = &self.projet.objets[k].armature {
                    let mut d = a.devant();
                    cx.toggle(b, id("armature-devant"), "Au premier plan", &mut d);
                    if d != a.devant() {
                        self.memoriser();
                        if let Some(a) = &mut self.projet.objets[k].armature {
                            a.regler_devant(d);
                        }
                        self.a_sauver = true;
                        self.dire(if d { "Armature vue à travers les objets, en mode Scène" } else { "Armature cachée par les objets placés devant" }.into());
                    }
                }
                let mut v = self.projet.objets[k].visible;
                cx.toggle(b, id("objet-visible"), "Visible", &mut v);
                if self.projet.objets[k].visible != v {
                    self.visibilite_changee();
                }
                self.projet.objets[k].visible = v;
            }
            self.operations(cx, b, Menu::Objet);
        }
        self.reglages_pas(cx, b);
    }

    fn poignees(&mut self, cx: &mut Cx, l: &Layout, screen: Rect) {
        let d = cx.dp;
        for (cote, cle, vu) in [(Side::Start, id("poignee-debut"), self.shell.start.anim), (Side::End, id("poignee-fin"), self.shell.end.anim)] {
            if vu < 1.0 {
                continue;
            }
            let bande = self.shell.handle(l, cote);
            if let Some(t) = cx.ui.drag(cle, bande) {
                self.shell.resize(cote, screen, t.x, t.y);
            }
            if cx.ui.double_tapped(cle) {
                self.shell.reset_size(cote, screen);
            }
            let couleur = if cx.ui.pressed(cle) { coul(C::Accent) } else { { let [r, v, b, _] = coul(C::Secondaire); [r, v, b, 140] } };
            let (long, ep) = (52.0 * d, 5.0 * d);
            let g = if l.landscape {
                Rect::new(bande.cx() - ep / 2.0, bande.cy() - long / 2.0, ep, long)
            } else {
                Rect::new(bande.cx() - long / 2.0, bande.cy() - ep / 2.0, long, ep)
            };
            cx.dl.rrect(g, ep / 2.0, couleur);
        }
    }

    fn couleurs(&self, k: usize, o: &Objet) -> ([f32; 3], [f32; 3], f32) {
        if self.os.as_ref().is_some_and(|e| e.objet == Some(k) || (e.objet.is_none() && o.armature.is_some())) {
            return (lin_octets(coul(C::Armature)), [0.0; 3], 1.0);
        }
        if self.travail.is_some() || self.os.is_some() {
            return (lin_octets(coul(C::Face)), [0.0; 3], OPACITE_PASSIF);
        }
        if self.rendu == Rendu::Jeu && o.armature.is_none() {
            let defaut = Materiau::neuf(None);
            let m = self.projet.materiau(&o.materiau).unwrap_or(&defaut);
            // Illumination 0 : couleur plate, sans lumière.
            return if m.illumination == 0.0 { ([0.0; 3], lin_hex(&m.couleur), m.opacite as f32) } else { (lin_hex(&m.couleur), lin_hex(&m.emission), m.opacite as f32) };
        }
        let c = if !self.choix.contains(&k) {
            coul(if o.armature.is_some() { C::Armature } else { C::Objet })
        } else if self.projet.actif == Some(k) {
            coul(C::ObjetActif)
        } else {
            coul(C::ObjetChoisi)
        };
        (lin_octets(c), [0.0; 3], 1.0)
    }

    /// Pas de la grille : puissance de 10 à l'échelle de la scène.
    fn pas_grille(&self) -> f32 {
        let t = self.boite_monde(&self.affiches()).map_or(1.0, |b| (0..3).map(|k| b.max[k] - b.min[k]).fold(0.0, f64::max));
        let t = if t > 0.0 { t } else { 1.0 };
        10f64.powf((t / 2.0).log10().floor()) as f32
    }

    /// Décode les images demandées pour la première fois : le rendu Solide n'en paie
    /// aucune. Une image illisible laisse la couleur du matériau.
    fn images_a_charger(&mut self, besoin: HashSet<String>) -> (Vec<(String, Image)>, Vec<String>) {
        let mut images = Vec::new();
        for id in besoin {
            if !self.images_vues.insert(id.clone()) {
                continue;
            }
            if id == texture::DAMIER {
                images.push((id, texture::damier()));
                continue;
            }
            if id == texture::RAMPE {
                images.push((id, texture::rampe()));
                continue;
            }
            let t = &self.projet.textures[&id];
            let nom = t.nom.clone();
            match texture::decoder(&t.octets) {
                Ok(i) => {
                    self.uv.aspects.insert(id.clone(), Aspect::de(i.l, i.h));
                    images.push((id, i));
                }
                Err(e) => self.dire(format!("Texture illisible : {nom} ({e})")),
            }
        }
        let retraits: Vec<String> = self.images_vues.iter().filter(|id| *id != texture::DAMIER && *id != texture::RAMPE && !self.projet.textures.contains_key(*id)).cloned().collect();
        for id in &retraits {
            self.images_vues.remove(id);
        }
        (images, retraits)
    }

    fn scene(&mut self, dp: f32, w: Rect) -> Scene {
        self.avancer_calculs();
        self.avancer_ajout();
        self.avancer_peaux();
        let (v, p) = self.matrices(w);
        let (oeil, droite, haut, dos) = self.repere();
        let l = if self.rendu == Rendu::Jeu { SOLEIL } else { [0, 1, 2].map(|k| droite[k] * LAMPE[0] + haut[k] * LAMPE[1] + dos[k] * LAMPE[2]) };
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        let mut objets = Vec::new();
        let (mut ajouts, mut vus) = (Vec::new(), HashSet::new());
        let mut besoin = HashSet::new();
        let cibles = modifs::cibles(&self.projet);
        let mut poids_vus = HashSet::new();
        for k in self.affiches() {
            if self.travail.as_ref().is_some_and(|t| t.edites.contains(&k)) {
                continue;
            }
            let m = self.objet_vu(k).matrice();
            let vu = self.maillage_vu(k);
            let poids = self.vue_poids(k, &vu);
            let o = &self.projet.objets[k];
            let (albedo, emission, opacite) = if poids.is_some() { ([1.0; 3], [0.0; 3], 1.0) } else { self.couleurs(k, o) };
            let colore = poids.is_some();
            let vu = poids.unwrap_or(vu);
            let cle = Arc::as_ptr(&vu) as usize;
            let contour = self.travail.is_none() && self.os.is_none() && self.choix.contains(&k);
            // Comme la couleur des matériaux : en rendu En jeu seulement
            let texture = if colore {
                poids_vus.insert(o.id.clone());
                Some(texture::RAMPE.to_string())
            } else if self.rendu == Rendu::Jeu && self.travail.is_none() && self.os.is_none() {
                self.projet.texture_de(o).cloned()
            } else {
                None
            };
            besoin.extend(texture.clone());
            let devant = self.devant(k);
            objets.push(ObjetVue { tampon: cle, modele: vue::mat4(&m), normale: vue::mat_normale(&m), albedo, emission, opacite, contour, filaire: cibles.contains(&k), texture, envers: None, devant });
            vus.insert(cle);
            if !self.connus.contains_key(&cle) {
                ajouts.push((cle, vue::tampons(&vu)));
                self.connus.insert(cle, vu);
            }
        }
        self.vues_poids.retain(|i, _| poids_vus.contains(i));
        // Aperçu de la primitive en cours d'ajout : comme un objet choisi, sur le curseur
        if let Some(m) = self.ajout.as_ref().and_then(|a| a.apercu.clone()) {
            let cle = Arc::as_ptr(&m) as usize;
            let c = self.projet.curseur;
            let mx = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, c[0], c[1], c[2], 1.0];
            objets.push(ObjetVue { tampon: cle, modele: vue::mat4(&mx), normale: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], albedo: lin_octets(coul(C::ObjetActif)), emission: [0.0; 3], opacite: 1.0, contour: true, filaire: false, texture: None, envers: None, devant: false });
            vus.insert(cle);
            if !self.connus.contains_key(&cle) {
                ajouts.push((cle, vue::tampons(&m)));
                self.connus.insert(cle, m);
            }
        }
        let retraits: Vec<usize> = self.connus.keys().filter(|c| !vus.contains(c)).copied().collect();
        for c in &retraits {
            self.connus.remove(c);
        }
        let mut retraits = retraits;
        // Le modèle montre la texture de la vue UV, ou le damier s'il n'en a pas
        let image_uv = if self.contenu == Contenu::Uv && self.shell.end.open && self.travail.is_some() { Some(self.faces_uv().1.unwrap_or(texture::DAMIER.to_string())) } else { None };
        if self.uv.texture != image_uv {
            self.uv.texture = image_uv.clone();
            self.version_geo = None;
        }
        besoin.extend(image_uv);
        // Vignette de l'onglet Matériaux
        if self.contenu == Contenu::Scene && self.onglet_scene == ONGLET_MATERIAUX {
            let i = self.materiau_actif().and_then(|n| self.projet.materiau(&n)?.texture.clone());
            besoin.extend(i.filter(|i| self.projet.textures.contains_key(i)));
        }
        let geo_vue = self.vue_geo(&mut objets, &mut ajouts, &mut retraits);
        let (images, images_retraits) = self.images_a_charger(besoin);
        Scene {
            workspace: w,
            vue_proj: math::mul(&p, &v),
            oeil: oeil.map(|x| x as f32),
            vue: v,
            lumiere: l.map(|x| (x / n) as f32),
            objets,
            pas_grille: self.pas_grille(),
            largeur_contour: 3.0 * dp,
            contour: lin_octets(coul(C::ContourChoisi)),
            couleurs: CouleursVue::maintenant(),
            ajouts,
            retraits,
            images,
            images_retraits,
            geo: geo_vue,
        }
    }

    /// Copie de travail vers le rendu : faces, fil et points, seulement s'ils ont changé.
    fn vue_geo(&mut self, objets: &mut Vec<ObjetVue>, ajouts: &mut Vec<(usize, Tampons)>, retraits: &mut Vec<usize>) -> Option<GeoVue> {
        if self.os.is_some() {
            let g = self.vue_os();
            if g.is_none() {
                self.version_geo = None;
            }
            return g;
        }
        let Some(t) = &self.travail else {
            if self.version_geo.take().is_some() {
                retraits.extend([CLE_FACES, CLE_CHOISIES]);
            }
            return None;
        };
        let opacite = if self.rayons { OPACITE_RAYONS } else { 1.0 };
        let id3: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let image = self.uv.texture.clone();
        let modele = vue::mat4(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        for (cle, c) in [(CLE_FACES, coul(C::Face)), (CLE_CHOISIES, coul(C::FaceChoisie))] {
            // Sous une texture, le choix n'est plus qu'une teinte légère
            let albedo = match (&image, cle == CLE_CHOISIES) {
                (None, _) => lin_octets(c),
                (Some(_), false) => [1.0; 3],
                (Some(_), true) => lin_octets(c).map(|x| 0.7 + 0.3 * x),
            };
            objets.push(ObjetVue { tampon: cle, modele, normale: id3, albedo, emission: [0.0; 3], opacite, contour: false, filaire: false, texture: image.clone(), envers: Some(lin_octets(coul(C::Envers))), devant: false });
        }
        if self.version_geo == Some(t.version) {
            return Some(GeoVue { maj: None, rayons: self.rayons });
        }
        self.version_geo = Some(t.version);
        let m = &t.m;
        let choisies: HashSet<usize> = if t.filtres[2] { t.sel.faces.iter().copied().collect() } else { HashSet::new() };
        // Seules les faces de cette image la portent
        let cherchee = image.as_ref().filter(|i| *i != texture::DAMIER);
        let porte: Vec<bool> = m.faces.iter().map(|f| image.is_none() || f.s.first().and_then(|i| t.proprio.get(*i)).and_then(|k| self.projet.texture_de(&self.projet.objets[*k])) == cherchee).collect();
        for (cle, garde) in [(CLE_FACES, false), (CLE_CHOISIES, true)] {
            ajouts.push((cle, Tampons { faces: vue::faces_si_uv(m, |k| choisies.contains(&k) == garde, |k| porte[k]), bandes: Vec::new(), bandes_idx: Vec::new(), fil: Vec::new() }));
        }
        let mut g = GeoTampons::default();
        let f3 = |p: P3| p.map(|x| x as f32);
        for [i, j] in m.aretes() {
            g.lignes.extend(f3(m.sommets[i]));
            g.lignes.extend(f3(m.sommets[j]));
        }
        if t.filtres[0] {
            g.points.extend(m.sommets.iter().flat_map(|p| f3(*p)));
            g.choisis.extend(t.sel.sommets.iter().flat_map(|i| f3(m.sommets[*i])));
        }
        // Ce qu'on touche dans la vue UV s'allume sur le modèle
        if self.contenu == Contenu::Uv {
            let mut vus = HashSet::new();
            for &(f, k) in &self.uv.sel {
                if let Some(&i) = m.faces.get(f).and_then(|x| x.s.get(k)) {
                    if vus.insert(i) {
                        g.choisis.extend(f3(m.sommets[i]));
                    }
                }
            }
        }
        for [i, j] in m.coutures_valides() {
            let (p, q) = (f3(m.sommets[i]), f3(m.sommets[j]));
            for (a, b, c) in [(p, q, 1.0), (p, q, -1.0), (q, p, -1.0), (p, q, -1.0), (q, p, 1.0), (q, p, -1.0)] {
                g.coutures.extend(a);
                g.coutures.extend(b);
                g.coutures.push(c);
            }
        }
        if t.filtres[1] {
            for [i, j] in &t.sel.aretes {
                let (p, q) = (f3(m.sommets[*i]), f3(m.sommets[*j]));
                for (a, b, c) in [(p, q, 1.0), (p, q, -1.0), (q, p, -1.0), (p, q, -1.0), (q, p, 1.0), (q, p, -1.0)] {
                    g.bandes.extend(a);
                    g.bandes.extend(b);
                    g.bandes.push(c);
                }
            }
        }
        Some(GeoVue { maj: Some(g), rayons: self.rayons })
    }
}

/// Pose de l'armature d'un objet : ses os de départ, avec ces bouts transformés au monde.
/// Les os tels qu'on les voit et les touche.
fn segments_os(e: &EditionOs, a: &Armature) -> Segments {
    if e.pose { a.segments_pose() } else { a.segments() }
}

pub use crate::armature::{angles_os, depuis_angles_os};

fn poser_os(o: &mut Objet, e: &EditionOs, depart: &Pose, v: Transfo) {
    let Transfo::Rotation { q, .. } = v else { return };
    let (r, ri) = (o.rotation, ed::qinv(ed::qnorm(o.rotation)));
    let Some(a) = &mut o.armature else { return };
    a.pose = depart.clone();
    let l = e.racines(a);
    a.tourner(&l, ed::qnorm(ed::qmul(ri, ed::qmul(q, r))));
    // L'os bute sur ses limites
    a.borner(depart);
    o.maillage = Arc::new(a.enveloppe_pose());
}

fn poser_bouts(o: &mut Objet, bouts: &[Bout], depart: &Segments, v: Transfo) {
    let mx = o.matrice();
    let mut table = depart.clone();
    for b in bouts {
        if let Some(s) = table.get_mut(&b.os) {
            let p = if b.queue { &mut s.queue } else { &mut s.tete };
            *p = geo::vers_local(o, ed::transformer(vue::appliquer(&mx, *p), v));
        }
    }
    let Some(mut a) = o.armature.take() else { return };
    a.appliquer_segments(&table);
    o.maillage = Arc::new(a.enveloppe());
    o.armature = Some(a);
}

/// Nombre pour un champ : trois décimales au plus, sans zéros inutiles.
fn chiffres(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.replace('.', ",") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::DrawList;
    use crate::text::Text;
    use crate::ui::{Event, Ui};

    const ECRAN: Rect = Rect::new(0., 0., 1920., 1200.);
    const CENTRE: (f32, f32) = (960.0, 600.0);
    const GDA: &str = r##"{"format":"gda","version":9,"scene":{"objets":[
      {"nom":"a","position":[5,0,0],"materiau":"bois","maillage":{"v":[[0,0,0],[1,0,0],[0,1,0]],"f":[["bois",0,1,2]]}},
      {"nom":"b","maillage":{"v":[[0,0,0],[1,0,0],[0,1,0]],"f":[["defaut",0,1,2]]}}]},
      "materiaux":{"bois":{"couleur":"#804020","opacite":0.5}}}"##;

    struct Banc {
        app: App,
        ui: Ui,
        text: Text,
        dl: DrawList,
    }

    impl Banc {
        fn new() -> Banc {
            let mut b = Banc { app: App::new(1.5), ui: Ui::new(1.5), text: Text::new(), dl: DrawList::default() };
            b.images(1);
            b
        }
        fn image(&mut self, dt: f32) -> Scene {
            self.dl.clear();
            let mut cx = Cx { ui: &mut self.ui, text: &mut self.text, dl: &mut self.dl, dp: 1.5 };
            let s = self.app.frame(&mut cx, ECRAN, dt);
            self.ui.end_frame();
            s
        }
        /// Panneaux fermés, le cube de départ cadré au milieu de l'écran.
        fn pret() -> Banc {
            let mut b = Banc::new();
            b.app.shell.start.open = false;
            b.images(60);
            b.app.remplacer(Projet::neuf(), "");
            b.images(1);
            b
        }
        /// Origine de la poignée, et bout de l'axe `k`, à l'écran.
        fn bout(&self, k: usize) -> ((f32, f32), (f32, f32)) {
            let w = self.app.espace;
            let v = self.app.vue_ecran(w);
            let p = self.app.poignee(&v, 1.5).unwrap();
            let e = |q: P3| { let e = v.ecran(q).unwrap(); (w.x + e[0] as f32, w.y + e[1] as f32) };
            (e(p.o), e(p.bout(k)))
        }
        /// Glisse d'un point à un autre ; rend le nombre d'étapes d'annulation ajoutées.
        fn glisser(&mut self, a: (f32, f32), b: (f32, f32)) -> usize {
            let n = self.app.histoire.annuler.len();
            self.ui.event(Event::Down { id: 0, x: a.0, y: a.1 }, 0.0);
            for k in 1..=4 {
                let t = k as f32 / 4.0;
                self.ui.event(Event::Move { id: 0, x: a.0 + (b.0 - a.0) * t, y: a.1 + (b.1 - a.1) * t }, 0.1);
                self.images(1);
            }
            self.ui.event(Event::Up { id: 0 }, 0.2);
            self.images(1);
            self.app.histoire.annuler.len() - n
        }
        fn images(&mut self, n: usize) -> Scene {
            let mut s = self.image(0.05);
            for _ in 1..n {
                s = self.image(0.05);
            }
            s
        }
        fn appui(&mut self, x: f32, y: f32) {
            self.ui.event(Event::Down { id: 0, x, y }, 0.0);
            self.ui.event(Event::Up { id: 0 }, 0.05);
            self.images(2);
        }
    }

    /// Deux cubes : `a` percé par `b`, plus petit et plus haut.
    fn perce() -> Banc {
        let mut b = Banc::pret();
        let mut p = Projet::neuf();
        p.objets[0].maillage = Arc::new(Maillage::cube(2.0));
        p.objets[0].nom = "a".into();
        let mut o = p.objets[0].clone();
        o.nom = "b".into();
        o.maillage = Arc::new(Maillage::cube(1.0));
        o.position = [0.0, 1.0, 0.0];
        p.objets.push(o);
        b.app.remplacer(p, "");
        b.app.choisir(Some(0));
        b.images(1);
        b.app.agir(crate::actions::Action::ModifBooleen);
        b.images(1);
        b
    }

    /// Le sommet de la copie de travail le plus proche de l'œil, à l'écran.
    fn sommet_proche(b: &Banc) -> (usize, (f32, f32)) {
        let w = b.app.espace;
        let v = b.app.vue_ecran(w);
        let t = b.app.travail.as_ref().unwrap();
        let d = |p: P3| (0..3).map(|k| (p[k] - v.oeil[k]).powi(2)).sum::<f64>();
        let i = (0..t.m.sommets.len()).min_by(|a, b| d(t.m.sommets[*a]).total_cmp(&d(t.m.sommets[*b]))).unwrap();
        let e = v.ecran(t.m.sommets[i]).unwrap();
        (i, (w.x + e[0] as f32, w.y + e[1] as f32))
    }

    fn en_geometrie() -> Banc {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(1);
        b
    }

    #[test]
    fn double_appui_sur_un_objet_entre_en_geometrie() {
        let mut b = Banc::pret();
        b.appui(CENTRE.0, CENTRE.1);
        b.appui(CENTRE.0, CENTRE.1);
        assert!(b.app.travail.is_some());
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets.len(), 8);
    }

    #[test]
    fn glisser_un_sommet_se_retrouve_a_la_sortie_et_s_annule() {
        let mut b = en_geometrie();
        let (i, e) = sommet_proche(&b);
        b.appui(e.0, e.1);
        assert_eq!(b.app.travail.as_ref().unwrap().sel.sommets, vec![i]);
        let depart = b.app.travail.as_ref().unwrap().m.sommets[i];
        assert_eq!(b.glisser(e, (e.0 + 120.0, e.1 - 60.0)), 1);
        let arrivee = b.app.travail.as_ref().unwrap().m.sommets[i];
        assert_ne!(depart, arrivee);

        // Un seul sommet a bougé, et l'objet le reçoit en sortant
        b.app.sortir_geometrie();
        assert!(b.app.travail.is_none());
        assert_eq!(b.app.choix, vec![0]);
        assert!(ed::vers_monde(&b.app.projet.objets[0]).sommets.contains(&arrivee));

        // L'annulation revient en mode Géométrie, avant le glissé
        b.app.annuler();
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets[i], depart);
        assert!(b.app.travail.as_ref().unwrap().sel.sommets.contains(&i));
    }

    #[test]
    fn le_rendu_suit_la_copie_de_travail() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.images(1);
        b.app.entrer_geometrie();
        let s = b.image(0.05);
        assert!(s.ajouts.iter().any(|a| a.0 == CLE_FACES));
        let g = s.geo.as_ref().unwrap().maj.as_ref().unwrap();
        assert_eq!(g.lignes.len(), 12 * 6);
        assert_eq!(g.points.len(), 8 * 3);
        assert!(g.choisis.is_empty());
        assert!(s.objets.iter().all(|o| o.tampon == CLE_FACES || o.tampon == CLE_CHOISIES));
        // Rien ne change : rien n'est renvoyé
        assert!(b.image(0.05).geo.unwrap().maj.is_none());

        b.app.travail.as_mut().unwrap().tout(T::F);
        let s = b.image(0.05);
        let tris = |c: usize| s.ajouts.iter().find(|a| a.0 == c).unwrap().1.faces.len();
        assert_eq!((tris(CLE_FACES), tris(CLE_CHOISIES)), (0, 12 * 3 * crate::vue::FACE_F32));

        b.app.travail.as_mut().unwrap().aucun(None);
        b.app.travail.as_mut().unwrap().tout(T::A);
        let s = b.image(0.05);
        assert_eq!(s.geo.unwrap().maj.unwrap().bandes.len(), 12 * 6 * 7);

        b.app.sortir_geometrie();
        let s = b.image(0.05);
        assert!(s.geo.is_none() && s.retraits.contains(&CLE_FACES));
        assert!(s.objets.iter().all(|o| o.contour));
    }

    #[test]
    fn la_boite_choisit_et_retire() {
        let mut b = en_geometrie();
        b.app.selection = Selection::Boite;
        let w = b.app.espace;
        let (a, z) = ((w.x + 5.0, w.y + 150.0), (w.right() - 5.0, w.bottom() - 92.0));
        assert_eq!(b.glisser(a, z), 0);
        // Solide convexe : un sommet se voit s'il appartient à une face tournée vers l'œil
        let t = b.app.travail.as_ref().unwrap();
        let oeil = b.app.vue_ecran(w).oeil;
        let mut vus = HashSet::new();
        for f in &t.m.faces {
            let (n, p) = (t.m.normale(&f.s), t.m.sommets[f.s[0]]);
            if (0..3).map(|k| n[k] * (oeil[k] - p[k])).sum::<f64>() > 0.0 {
                vus.extend(f.s.iter().copied());
            }
        }
        assert!(vus.len() < 8);
        assert_eq!(t.sel.sommets.iter().copied().collect::<HashSet<_>>(), vus);
        b.app.boite_retire = true;
        b.glisser(a, z);
        assert!(b.app.travail.as_ref().unwrap().sel.sommets.is_empty());

        // En scène, elle prend les objets
        b.app.sortir_geometrie();
        b.app.choix.clear();
        b.app.boite_retire = false;
        b.glisser(a, z);
        assert_eq!(b.app.choix, vec![0]);
    }

    #[test]
    fn la_derniere_operation_se_regle_sans_ajouter_d_etape() {
        let mut b = en_geometrie();
        let t = b.app.travail.as_mut().unwrap();
        let a = t.m.aretes()[0];
        t.basculer(El::A(a));
        let n = b.app.histoire.annuler.len();
        b.app.lancer(Derniere::Subdivision { coupes: 1 });
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets.len(), 9);
        b.app.saisir(Champ::Coupes);
        b.app.appliquer_champ(Champ::Coupes, 3.0);
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets.len(), 11);
        assert_eq!(b.app.derniere, Some(Derniere::Subdivision { coupes: 3 }));
        assert_eq!(b.app.histoire.annuler.len(), n + 1);
        b.app.annuler();
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets.len(), 8);
        assert!(b.app.derniere.is_none());
    }

    /// Banc en mode Scène, cube choisi, panneau Contexte ouvert, mode donné : rend un
    /// glissé depuis l'objet choisi, de la forme d'un glissé réel.
    fn glisse_sur_le_cube(mode: Mode) -> Banc {
        let mut b = Banc::pret();
        b.app.mode = mode;
        b.app.axes = [true; 3];
        b.app.shell.end.open = true;
        b.app.contenu = Contenu::Contexte;
        b.app.onglet_contexte = ONGLET_DERNIERE;
        b.images(60);
        let w = b.app.espace;
        let (x, y) = (w.cx(), w.cy());
        b.appui(x, y);
        b.images(30);
        assert_eq!(b.app.choix, vec![0]);
        b.glisser((x + 40.0, y + 30.0), (x + 160.0, y - 50.0));
        b.images(2);
        b
    }

    #[test]
    fn la_translation_au_doigt_se_regle_et_s_annule_d_un_coup() {
        let mut b = glisse_sur_le_cube(Mode::Translation);
        let Some(Derniere::Translation { d }) = b.app.derniere else { panic!("{:?}", b.app.derniere) };
        assert_ne!(d, [0.0; 3]);
        let n = b.app.histoire.annuler.len();
        assert!(b.ui.zone(sub_id(id("r-decalage"), 0)).is_some(), "le champ est dans l'onglet");
        b.app.saisir(Champ::Decalage(1));
        b.app.pave.as_mut().unwrap().texte = "2".into();
        b.app.valider_pave();
        assert_eq!(b.app.pave.as_ref().unwrap().cible, Champ::Decalage(2), "OK passe au champ suivant");
        b.app.pave = None;
        let p = b.app.projet.objets[0].position;
        assert_eq!(p, [d[0], 2.0, d[2]], "refaite depuis le départ");
        assert_eq!(b.app.histoire.annuler.len(), n);
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].position, [0.0; 3]);
        assert!(b.app.derniere.is_none());
    }

    #[test]
    fn la_rotation_au_doigt_se_regle_angle_axe_et_pivot() {
        let mut b = glisse_sur_le_cube(Mode::Rotation);
        let Some(Derniere::Rotation { pivot, .. }) = b.app.derniere else { panic!("{:?}", b.app.derniere) };
        b.app.appliquer_champ(Champ::Angle, 90.0);
        let Some(Derniere::Rotation { degres, axe, vue, .. }) = b.app.derniere else { panic!() };
        assert_eq!(degres, 90.0);
        let u = vue.filter(|_| axe == 3).unwrap_or([0, 1, 2].map(|k| if k == axe { 1.0 } else { 0.0 }));
        assert_eq!(b.app.projet.objets[0].rotation, ed::qaxe(u, 90f64.to_radians()));
        b.app.rejouer(Derniere::Rotation { degres: 90.0, axe: 1, vue: None, pivot });
        assert_eq!(b.app.projet.objets[0].rotation, ed::qaxe([0.0, 1.0, 0.0], 90f64.to_radians()));
        b.app.appliquer_champ(Champ::Pivot(0), 3.0);
        let p = b.app.projet.objets[0].position;
        assert!((p[0] - 3.0).abs() < 1e-9 && (p[2] - 3.0).abs() < 1e-9, "{p:?}");
    }

    #[test]
    fn l_echelle_au_doigt_se_regle_facteur_et_pivot() {
        let mut b = glisse_sur_le_cube(Mode::Echelle);
        assert!(matches!(b.app.derniere, Some(Derniere::Echelle { .. })), "{:?}", b.app.derniere);
        for (c, v) in [(Champ::Facteur(0), 3.0), (Champ::Facteur(1), 2.0), (Champ::Facteur(2), 0.0)] {
            b.app.appliquer_champ(c, v);
        }
        assert_eq!(b.app.projet.objets[0].echelle, [3.0, 2.0, 0.01]);
        b.app.appliquer_champ(Champ::Pivot(0), 1.0);
        assert_eq!(b.app.projet.objets[0].position[0], -2.0);
    }

    #[test]
    fn un_glisse_en_geometrie_se_regle_aussi() {
        let mut b = en_geometrie();
        let (i, e) = sommet_proche(&b);
        b.appui(e.0, e.1);
        let depart = b.app.travail.as_ref().unwrap().m.sommets[i];
        b.glisser(e, (e.0 + 120.0, e.1 - 60.0));
        let Some(Derniere::Translation { d }) = b.app.derniere else { panic!("{:?}", b.app.derniere) };
        b.app.appliquer_champ(Champ::Decalage(0), d[0] + 1.0);
        let t = b.app.travail.as_ref().unwrap();
        assert_eq!(t.m.sommets[i], [depart[0] + d[0] + 1.0, depart[1] + d[1], depart[2] + d[2]]);
        assert_eq!(t.sel.sommets, vec![i], "la sélection reste");
    }

    #[test]
    fn masquer_un_objet_retire_la_derniere_operation() {
        let mut b = glisse_sur_le_cube(Mode::Translation);
        assert!(b.app.derniere.is_some());
        b.app.visibilite_changee();
        assert!(b.app.derniere.is_none(), "la refaire rendrait l'objet visible");
    }

    #[test]
    fn la_pastille_contexte_montre_la_fiche_et_une_operation_ouvre_ses_reglages() {
        let mut b = en_geometrie();
        b.app.shell.end.open = false;
        b.images(30);
        let z = b.ui.zone(CONTEXTE).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(30);
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Contexte);
        assert!(b.ui.zone(sub_id(id("onglets-contexte"), 1)).is_some());
        assert!(b.ui.zone(sub_id(id("onglets-reglages"), 2)).is_none(), "plus d'onglet Objet en Réglages");
        b.app.travail.as_mut().unwrap().tout(T::A);
        b.app.lancer(Derniere::Subdivision { coupes: 1 });
        b.images(2);
        assert_eq!(b.app.onglet_contexte, ONGLET_DERNIERE);
        assert!(b.ui.zone(id("r-coupes")).is_some());
    }

    #[test]
    fn revolution_refusee_sans_sommet_ni_arete() {
        let mut b = en_geometrie();
        let n = b.app.histoire.annuler.len();
        b.app.revolution(b.app.espace);
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert!(b.app.derniere.is_none());
    }

    #[test]
    fn separer_cree_un_objet_et_s_annule() {
        let mut b = en_geometrie();
        b.app.travail.as_mut().unwrap().basculer(El::F(0));
        b.app.separer();
        assert_eq!(b.app.projet.objets.len(), 2);
        b.images(1);
        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), 1);
        assert_eq!(b.app.travail.as_ref().unwrap().m.faces.len(), 6);
        b.images(1);
    }

    #[test]
    fn le_projet_enregistre_contient_la_copie_de_travail() {
        let mut b = en_geometrie();
        b.app.travail.as_mut().unwrap().tout(T::S);
        b.app.deplacer_selection([1.0, 0.0, 0.0]);
        let p = b.app.projet_range();
        assert!(ed::vers_monde(&p.objets[0]).sommets.iter().all(|q| q[0] > 0.0));
        assert!(ed::vers_monde(&b.app.projet.objets[0]).sommets.iter().any(|q| q[0] < 0.0), "le projet reste tel quel jusqu'à la sortie");
    }

    #[test]
    fn appui_long_sur_un_filtre_le_retire_sans_toucher_les_autres() {
        let mut b = en_geometrie();
        b.images(1);
        let z = b.ui.zone(sub_id(id("barre-haute"), 10)).unwrap();
        let (x, y) = (z.cx(), z.cy());
        b.ui.event(Event::Down { id: 0, x, y }, 0.0);
        b.images(1);
        b.image(0.6);
        b.images(2);
        b.ui.event(Event::Up { id: 0 }, 0.7);
        b.images(2);
        assert_eq!(b.app.travail.as_ref().unwrap().filtres, [false, true, true]);
    }

    #[test]
    fn importer_ajoute_sur_le_curseur_et_s_annule() {
        let mut b = Banc::pret();
        b.app.projet.curseur = [1.0, 2.0, 3.0];
        let n = b.app.histoire.annuler.len();
        b.app.importer("essai.gda", GDA.as_bytes());
        assert_eq!(b.app.projet.objets.iter().map(|o| o.nom.as_str()).collect::<Vec<_>>(), ["cube", "a", "b"]);
        assert_eq!(b.app.projet.objets[1].position, [6.0, 2.0, 3.0]);
        assert_eq!(b.app.choix, [1, 2]);
        assert_eq!(b.app.projet.actif, Some(2));
        assert!(b.app.projet.materiau("bois").is_some());
        assert_eq!(b.app.histoire.annuler.len(), n + 1);
        assert!(!b.image(0.05).ajouts.is_empty());

        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), 1);
        assert!(b.app.projet.materiau("bois").is_none());
        b.images(1);
    }

    fn glb_texture() -> Vec<u8> {
        crate::projet::tests_textures::glb(&crate::projet::tests_textures::image(2))
    }

    #[test]
    fn importer_un_glb_texture_passe_en_jeu_et_charge_l_image_une_fois() {
        let mut b = Banc::pret();
        b.app.importer("x.glb", &glb_texture());
        assert_eq!(b.app.rendu, Rendu::Jeu);
        let s = b.images(1);
        assert_eq!(s.images.len(), 1);
        assert_eq!(s.objets.iter().filter(|o| o.texture.is_some()).count(), 1);
        assert!(b.images(1).images.is_empty());
        b.app.annuler();
        b.images(1);
        assert!(!b.app.projet.a_des_textures());
    }

    #[test]
    fn importer_plusieurs_fichiers_trouve_le_fichier_3d_et_dit_ce_qui_manque() {
        let mut b = Banc::pret();
        let n = b.app.projet.objets.len();
        b.app.importer_fichiers(vec![("a.png".into(), vec![1])]);
        assert_eq!(b.app.projet.objets.len(), n);
        assert!(b.app.message.as_ref().unwrap().0.contains("fichier 3D"));
        let mtl = b"newmtl m\nmap_Kd a.png\n".to_vec();
        let obj = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nusemtl m\nf 1/1 2/1 3/1\n".to_vec();
        b.app.importer_fichiers(vec![("a.mtl".into(), mtl), ("a.obj".into(), obj)]);
        assert_eq!(b.app.projet.objets.len(), n + 1);
        assert_eq!(b.app.rendu, Rendu::Solide);
        let m = &b.app.message.as_ref().unwrap().0;
        assert!(m.contains("Texture absente : a.png"), "{m}");
    }

    #[test]
    fn une_image_abimee_se_dit_une_fois_et_laisse_la_couleur() {
        let mut b = Banc::pret();
        b.app.importer("x.glb", &crate::projet::tests_textures::glb(b"\x89PNG abime"));
        let s = b.images(1);
        assert!(s.images.is_empty());
        let m = &b.app.message.as_ref().unwrap().0;
        assert!(m.contains("Texture illisible"), "{m}");
        b.app.message = None;
        b.images(2);
        assert!(b.app.message.is_none());
    }

    #[test]
    fn la_geometrie_garde_les_uv_et_la_scene_les_montre_a_la_sortie() {
        let mut b = Banc::pret();
        b.app.importer("x.glb", &glb_texture());
        let k = b.app.projet.objets.iter().position(|o| !o.maillage.faces.is_empty() && !o.maillage.faces[0].uv.is_empty()).unwrap();
        b.app.choix = HashSet::from([k]).into_iter().collect();
        b.app.projet.actif = Some(k);
        let uv = b.app.projet.objets[k].maillage.faces[0].uv.clone();
        b.app.entrer_geometrie();
        assert!(b.images(1).objets.iter().all(|o| o.texture.is_none()));
        b.app.sortir_geometrie();
        assert_eq!(b.app.projet.objets[k].maillage.faces[0].uv, uv);
        assert!(b.images(1).objets.iter().any(|o| o.texture.is_some()));
    }

    #[test]
    fn exporter_un_glb_texture_garde_uv_et_image_au_retour() {
        let mut b = Banc::pret();
        b.app.ouvrir("x.glb", &glb_texture());
        b.app.exporter("glb", false);
        let s = b.app.prendre_sorties();
        assert_eq!(s.len(), 1, "tout est dans le fichier");
        let (p, manquantes) = Projet::depuis_fichiers("y.glb", &s[0].octets, &[]).unwrap();
        assert!(manquantes.is_empty());
        assert_eq!(p.textures.values().next().unwrap().octets, b.app.projet.textures.values().next().unwrap().octets);
        let o = p.objets.iter().find(|o| p.texture_de(o).is_some()).unwrap();
        assert_eq!(o.maillage.faces[0].uv.len(), 3);
        assert_eq!(p.materiau("rouge").unwrap().couleur, "#ff0000");
    }

    #[test]
    fn exporter_un_obj_texture_depose_mtl_et_images_et_se_reimporte() {
        let mut b = Banc::pret();
        b.app.ouvrir("x y.glb", &glb_texture());
        b.app.exporter("obj", false);
        let s = b.app.prendre_sorties();
        assert_eq!(s.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), ["image.png", "x_y.mtl", "x y.obj"]);
        let annexes: Vec<_> = s[..2].iter().map(|x| (x.nom.clone(), x.octets.clone())).collect();
        let (p, manquantes) = Projet::depuis_fichiers(&s[2].nom, &s[2].octets, &annexes).unwrap();
        assert!(manquantes.is_empty(), "{manquantes:?}");
        assert!(p.a_des_textures());
        assert!(std::str::from_utf8(&s[2].octets).unwrap().contains("mtllib x_y.mtl"));
    }

    #[test]
    fn la_selection_n_exporte_que_ses_images() {
        let mut b = Banc::pret();
        b.app.ouvrir("x.glb", &glb_texture());
        let k = b.app.projet.objets.iter().position(|o| b.app.projet.texture_de(o).is_none()).unwrap();
        b.app.choix = vec![k];
        b.app.exporter("obj", true);
        let s = b.app.prendre_sorties();
        assert_eq!(s.len(), 1, "objet sans texture : le seul OBJ");
    }

    #[test]
    fn une_face_sans_uv_d_un_objet_texture_reste_unie_a_l_export() {
        let mut b = Banc::pret();
        b.app.ouvrir("x.glb", &glb_texture());
        let k = b.app.projet.objets.iter().position(|o| b.app.projet.texture_de(o).is_some()).unwrap();
        let mut m = (*b.app.projet.objets[k].maillage).clone();
        let mut f = m.faces[0].clone();
        f.uv.clear();
        f.s.reverse();
        m.faces.push(f);
        b.app.projet.objets[k].maillage = Arc::new(m);
        b.app.exporter("obj", false);
        let s = b.app.prendre_sorties();
        let mtl = std::str::from_utf8(&s[1].octets).unwrap();
        assert!(mtl.contains("newmtl bois_sans_texture"), "{mtl}");
        assert_eq!(mtl.matches("map_Kd").count(), 1, "{mtl}");
        assert!(std::str::from_utf8(&s[2].octets).unwrap().contains("usemtl bois_sans_texture"));
    }

    #[test]
    fn exporter_le_mtl_seul_nomme_et_depose_les_images() {
        let mut b = Banc::pret();
        b.app.ouvrir("x.glb", &glb_texture());
        b.app.exporter_mtl();
        let s = b.app.prendre_sorties();
        assert_eq!(s.len(), 2);
        assert!(std::str::from_utf8(&s[1].octets).unwrap().contains(&format!("map_Kd {}", s[0].nom)));
    }

    #[test]
    fn importer_un_fichier_illisible_ne_change_rien() {
        let mut b = Banc::pret();
        let n = b.app.histoire.annuler.len();
        b.app.importer("fichier", b"");
        b.app.importer("x.stl", b"pas un stl");
        assert_eq!(b.app.projet.objets.len(), 1);
        assert_eq!(b.app.histoire.annuler.len(), n);
    }

    #[test]
    fn importer_sort_du_mode_geometrie_et_reste_visible() {
        let mut b = en_geometrie();
        b.app.travail.as_mut().unwrap().tout(T::S);
        b.app.deplacer_selection([1.0, 0.0, 0.0]);
        b.app.projet.sets[0].visible = false;
        b.app.importer("essai.gda", GDA.as_bytes());
        assert!(b.app.travail.is_none());
        assert!(ed::vers_monde(&b.app.projet.objets[0]).sommets.iter().all(|q| q[0] > 0.0));
        assert!(b.app.projet.sets[0].visible);
        assert_eq!(b.app.choix.len(), 2);
    }

    #[test]
    fn les_boutons_du_panneau_demandent_le_selecteur() {
        let mut b = Banc::pret();
        assert!(b.app.prendre_selecteur().is_none());
        b.app.contenu = Contenu::Fichier;
        b.app.shell.end.open = true;
        b.images(60);
        let z = b.ui.zone(sub_id(id("operation"), A::Ouvrir as usize)).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.prendre_selecteur(), Some(Selecteur::Ouvrir));
        assert!(b.app.prendre_selecteur().is_none());
    }

    #[test]
    fn toute_action_s_execute_ou_ne_fait_rien() {
        for o in CATALOGUE {
            for geo in [false, true] {
                let mut b = if geo { en_geometrie() } else { Banc::pret() };
                b.app.choisir(Some(0));
                if let Some(t) = &mut b.app.travail {
                    [T::F, T::A, T::S].into_iter().for_each(|x| t.tout(x));
                }
                let (possible, n, p) = (b.app.contexte().possible(o.action), b.app.histoire.annuler.len(), b.app.projet.clone());
                b.app.agir(o.action);
                b.images(2);
                if !possible {
                    assert_eq!(b.app.histoire.annuler.len(), n, "{:?}", o.action);
                    assert_eq!(b.app.projet, p, "{:?}", o.action);
                }
            }
        }
    }

    #[test]
    fn les_panneaux_tires_du_catalogue_se_dessinent() {
        let mut b = en_geometrie();
        b.app.shell.end.open = true;
        b.app.travail.as_mut().unwrap().tout(T::A);
        for onglet in 0..3 {
            b.app.contenu = Contenu::Reglages;
            b.app.onglet = onglet;
            b.images(3);
            b.app.contenu = Contenu::Contexte;
            b.app.onglet_contexte = onglet;
            b.images(3);
            b.app.contenu = Contenu::Modifieurs;
            b.images(3);
            b.app.contenu = Contenu::Scene;
            b.app.onglet_scene = onglet % 2;
            b.images(3);
        }
        b.app.contenu = Contenu::Fichier;
        for onglet in 0..3 {
            b.app.onglet_fichier = onglet;
            b.images(2);
        }
        b.app.sortir_geometrie();
        b.app.contenu = Contenu::Aide;
        b.app.onglet_aide = 1;
        b.images(30);
        // Tout se déplie sans se chevaucher : une seule description à la fois
        for o in CATALOGUE {
            b.app.deplie = Some(o.action);
            b.images(1);
        }
        assert_eq!(phrase("relier les sommets"), "Relier les sommets.");
    }

    #[test]
    fn le_panneau_grise_ce_que_la_selection_ne_permet_pas() {
        let mut b = en_geometrie();
        let c = b.app.contexte();
        assert!(!c.possible(A::CreerFace) && !c.possible(A::Dissoudre) && c.possible(A::AjouterSommet));
        b.app.travail.as_mut().unwrap().tout(T::F);
        let c = b.app.contexte();
        assert!(c.possible(A::Inverser) && c.possible(A::Trianguler) && !c.possible(A::Fusionner));
        b.app.travail.as_mut().unwrap().aucun(None);
        b.app.sortir_geometrie();
        assert!(b.app.contexte().possible(A::EditerGeometrie));
        b.app.projet.actif = None;
        assert!(!b.app.contexte().possible(A::EditerGeometrie));
    }

    /// Centre de la pilule de la barre rapide, d'après les zones déclarées.
    fn pilule(b: &Banc, a: A) -> Option<(f32, f32)> {
        b.ui.zone(sub_id(BARRE_RAPIDE, a as usize)).map(|r| (r.cx(), r.cy()))
    }

    #[test]
    fn la_barre_rapide_execute_ce_qu_elle_montre() {
        let mut b = Banc::pret();
        b.app.choisir(None);
        b.images(2);
        assert!(pilule(&b, A::AjouterCube).is_some() && pilule(&b, A::Dupliquer).is_none());
        let (x, y) = pilule(&b, A::AjouterCube).unwrap();
        b.appui(x, y);
        // Le panneau d'ajout s'ouvre ; son bouton ajoute
        assert!(b.app.contenu == Contenu::Ajout);
        b.images(30);
        let r = b.ui.zone(id("p-ajouter")).expect("bouton Ajouter");
        b.appui(r.cx(), r.cy());
        assert_eq!(b.app.projet.objets.len(), 2);
        b.images(1);
        // Le nouvel objet est choisi : la barre change
        assert!(pilule(&b, A::Dupliquer).is_some() && pilule(&b, A::AjouterCube).is_none());
    }

    #[test]
    fn la_barre_rapide_defile_quand_elle_ne_tient_pas() {
        let mut b = en_geometrie();
        for c in CATALOGUE {
            if let Some(e) = c.barre {
                b.app.acces.insert(e.cle(), true);
            }
        }
        b.app.travail.as_mut().unwrap().tout(T::A);
        b.app.travail.as_mut().unwrap().tout(T::F);
        b.app.travail.as_mut().unwrap().tout(T::S);
        b.images(2);
        let (x, y) = pilule(&b, A::Inverser).unwrap();
        let n = b.app.histoire.annuler.len();
        b.glisser((x, y), (x - 300.0, y));
        // Le premier pas du doigt détache la pilule, les trois autres font défiler
        assert_eq!(b.app.defile, 225.0);
        assert_eq!(b.app.histoire.annuler.len(), n, "glisser n'a pas déclenché l'opération");
        assert_eq!(pilule(&b, A::Inverser).unwrap().0, x - 225.0);
        b.glisser((x, y), (x + 900.0, y));
        assert_eq!(b.app.defile, 0.0);
    }

    #[test]
    fn la_barre_rapide_masquee_ne_prend_pas_de_place() {
        let mut b = en_geometrie();
        b.app.travail.as_mut().unwrap().tout(T::F);
        b.images(2);
        assert!(b.ui.zone(BARRE_RAPIDE).is_some());
        b.app.barre_visible = false;
        b.images(2);
        assert!(b.ui.zone(BARRE_RAPIDE).is_none());
    }

    #[test]
    fn les_reglages_de_la_barre_se_gardent() {
        let mut b = Banc::pret();
        assert!(b.app.prendre_reglages().is_none());
        b.app.acces.insert("Ajout:Cube".into(), false);
        b.app.barre_visible = false;
        b.app.reglages_changes = true;
        let t = b.app.prendre_reglages().unwrap();
        assert!(b.app.prendre_reglages().is_none());
        let mut c = Banc::pret();
        c.app.reprendre_reglages(&t);
        c.app.reprendre_reglages("ligne illisible\n");
        assert!(!c.app.barre_visible);
        assert_eq!(c.app.acces, b.app.acces);
    }

    #[test]
    fn annuler_refait_le_contexte_de_la_geometrie() {
        let mut b = en_geometrie();
        b.app.travail.as_mut().unwrap().tout(T::F);
        assert!(b.app.contexte().possible(A::Inverser));
        b.app.agir(A::SupprimerFaces);
        assert!(!b.app.contexte().possible(A::Inverser));
        b.app.annuler();
        assert!(b.app.contexte().possible(A::Inverser));
    }

    #[test]
    fn les_pictogrammes_tiennent_dans_leur_carre() {
        let barre_haute = [&icones::SCENE, &icones::SELECTION_TOUCHER, &icones::SELECTION_BOITE, &icones::SELECTION_CHEMIN];
        for c in [&icones::DEPLACER, &icones::TOURNER, &icones::ECHELLE, &icones::PAS].into_iter().chain(ICONES_PIVOTS).chain(ICONES_FILTRES).chain(barre_haute) {
            assert!(!c.traits.is_empty());
            for (e, t) in c.traits {
                assert!(*e > 0.0 && t.len() >= 2 && t.iter().flatten().all(|v| (-0.5..=24.5).contains(v)), "{t:?}");
            }
        }
    }

    #[test]
    fn un_seul_bouton_fait_tourner_les_trois_modes() {
        let mut b = Banc::pret();
        b.images(30);
        let z = b.ui.zone(sub_id(BARRE, 0)).unwrap();
        assert!(b.ui.zone(sub_id(BARRE, 1)).is_none() && b.ui.zone(sub_id(BARRE, 2)).is_none());
        for (m, nom) in [(Mode::Rotation, "Tourner"), (Mode::Echelle, "Échelle"), (Mode::Translation, "Déplacer")] {
            b.appui(z.cx(), z.cy());
            b.images(30);
            assert!(b.app.mode == m);
            assert_eq!(b.app.message.as_ref().unwrap().0, nom);
        }
    }

    #[test]
    fn le_bouton_pivot_change_de_pictogramme_et_dit_son_nom() {
        let mut b = Banc::pret();
        b.images(30);
        assert!(b.ui.zone(sub_id(BARRE, 8)).is_none(), "Pivot est le dernier bouton");
        let z = b.ui.zone(sub_id(BARRE, 7)).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.pivot, 1);
        assert_eq!(b.app.message.as_ref().unwrap().0, "Pivot : Boîte");
    }

    fn onglet_couleurs() -> Banc {
        let mut b = Banc::pret();
        b.app.shell.end.open = true;
        b.app.contenu = Contenu::Reglages;
        b.app.onglet = ONGLET_COULEURS;
        b.images(60);
        b
    }

    #[test]
    fn une_couleur_se_regle_sur_place_et_se_retrouve_au_lancement_suivant() {
        let mut b = onglet_couleurs();
        b.app.prendre_reglages();
        let z = b.ui.zone(sub_id(id("couleur"), 0)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(3);
        let (i, tsl) = b.app.couleur_ouverte.unwrap();
        assert_eq!(i, 0);

        // Glisser la teinte vers le début change l'accent et la poignée, qui en dépend
        let z = b.ui.zone(sub_id(id("couleur-tsl"), 0)).unwrap();
        b.glisser((z.cx(), z.cy()), (z.x + 60.0, z.cy()));
        b.images(3);
        let neuf = b.app.couleur_ouverte.unwrap().1;
        assert!(neuf[0] < tsl[0] && neuf[1] == tsl[1], "{neuf:?}");
        assert_ne!(theme::hex(coul(C::Accent)), "#2a1fd1");
        assert_eq!(coul(C::Poignee), coul(C::Accent));
        let scene = b.images(1);
        assert_eq!(scene.couleurs.sommet_choisi, [0, 1, 2].map(|k| coul(C::Accent)[k] as f32 / 255.0));

        let texte = b.app.prendre_reglages().expect("gardée au lever du doigt");
        assert!(texte.contains("couleur.accent\t#"), "{texte}");
        let accent = coul(C::Accent);
        theme::appliquer(&[None; theme::NB]);
        let mut neuve = Banc::new();
        neuve.app.reprendre_reglages(&texte);
        assert_eq!(coul(C::Accent), accent);
        assert!(!neuve.app.acces.keys().any(|k| k.contains("couleur")));
        theme::appliquer(&[None; theme::NB]);
    }

    #[test]
    fn un_code_de_couleur_se_saisit_et_un_mauvais_laisse_le_pave_ouvert() {
        let mut b = onglet_couleurs();
        b.app.saisir(Champ::Couleur(C::Fond as usize));
        assert!(b.app.pave.as_ref().unwrap().alpha);
        b.app.pave.as_mut().unwrap().texte = "bleu".into();
        b.app.valider_pave();
        assert!(b.app.pave.is_some());
        b.app.pave.as_mut().unwrap().texte = "#102030".into();
        b.app.valider_pave();
        assert!(b.app.pave.is_none());
        assert_eq!(coul(C::Fond), [0x10, 0x20, 0x30, 255]);
        assert_eq!(coul(C::FondBord), [0x10, 0x20, 0x30, 255], "le bord du fond en dépend");
        b.images(2);
        let z = b.ui.zone(sub_id(id("couleur-raz"), C::Fond as usize)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
        assert_eq!(theme::hex(coul(C::Fond)), "#f1f0f6");
        assert!(b.app.couleurs[C::Fond as usize].is_none());
    }

    #[test]
    fn tout_remettre_par_defaut_oublie_tous_les_choix() {
        let mut b = onglet_couleurs();
        b.app.choisir_couleur(0, Some([1, 2, 3, 255]));
        b.app.choisir_couleur(5, Some([4, 5, 6, 255]));
        b.images(2);
        b.app.scrolls.insert(sub_id(id("corps-reglages"), ONGLET_COULEURS), (1e6, 1e7));
        b.images(3);
        let z = b.ui.zone(id("couleurs-raz")).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
        assert!(b.app.couleurs.iter().all(|c| c.is_none()));
        assert_eq!(theme::hex(coul(C::Accent)), "#2a1fd1");
    }

    #[test]
    fn la_pastille_fichier_ouvre_son_panneau_puis_le_ferme() {
        let mut b = Banc::pret();
        let w = b.app.espace;
        let (x, y) = (w.x + (14.0 + 5.0 * (52.0 + 10.0) + 26.0) * 1.5, w.y + (14.0 + 26.0) * 1.5);
        b.appui(x, y);
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Fichier);
        b.images(30);
        // Le panneau a réduit l'espace, pas la position des pastilles à gauche
        b.appui(x, y);
        assert!(!b.app.shell.end.open);
    }

    #[test]
    fn un_projet_ouvert_remplace_la_scene_et_recharge_les_tampons() {
        let mut b = Banc::new();
        assert!(b.image(0.05).ajouts.is_empty());
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        assert_eq!(b.app.projet.objets.len(), 2);
        assert_eq!(b.app.nom, "essai");
        let s = b.image(0.05);
        assert_eq!((s.ajouts.len(), s.retraits.len()), (2, 1)); // le cube de départ s'en va
        assert_eq!(s.objets.len(), 2);
    }

    #[test]
    fn un_fichier_illisible_laisse_la_scene_en_place() {
        let mut b = Banc::new();
        b.app.ouvrir("x.gda", b"{ pas du json");
        assert_eq!(b.app.projet.objets[0].nom, "cube");
        assert!(b.app.message.as_ref().unwrap().0.contains("illisible"));
    }

    #[test]
    fn on_ouvre_aussi_les_fichiers_3d() {
        let mut b = Banc::new();
        let c = maillage::Maillage::cube(1.0);
        b.app.ouvrir("piece.stl", &c.vers_stl("x"));
        assert_eq!(b.app.projet.objets[0].maillage.sommets.len(), 8);
        b.app.ouvrir("deux.obj", b"o a\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\no b\nv 0 0 1\nv 1 0 1\nv 0 1 1\nf 4 5 6\n");
        assert_eq!(b.app.projet.objets.len(), 2);
    }

    #[test]
    fn un_appui_choisit_le_cube_un_second_le_retire_et_le_vide_ne_change_rien() {
        let mut b = Banc::pret();
        b.appui(CENTRE.0, CENTRE.1);
        assert_eq!(b.app.choix, [0]);
        assert!(b.image(0.05).objets[0].contour);
        b.appui(30.0, 700.0);
        assert_eq!(b.app.choix, [0]);
        b.images(20);
        b.appui(CENTRE.0, CENTRE.1);
        assert!(b.app.choix.is_empty());
    }

    #[test]
    fn un_appui_ajoute_a_la_selection() {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.basculer_objet(0);
        b.app.basculer_objet(1);
        assert_eq!(b.app.choix, [0, 1]);
        assert_eq!(b.app.projet.actif, Some(1));
        b.app.basculer_objet(0); // déjà choisi, pas actif : il le devient
        assert_eq!(b.app.choix, [1, 0]);
        assert_eq!(b.app.projet.actif, Some(0));
    }

    #[test]
    fn un_appui_long_laisse_l_objet_seul_ou_vide_tout() {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.choix = vec![0, 1];
        b.ui.event(Event::Down { id: 0, x: 1900.0, y: 600.0 }, 0.0);
        assert!(b.ui.maintien());
        b.images(15);
        b.ui.event(Event::Up { id: 0 }, 1.0);
        b.images(1);
        assert!(b.app.choix.is_empty());
    }

    #[test]
    fn un_double_appui_dans_le_vide_recadre() {
        let mut b = Banc::pret();
        b.app.cam.cible = [40.0, 0.0, 0.0];
        for t in [0.0, 0.1] {
            b.ui.event(Event::Down { id: 0, x: 30.0, y: 700.0 }, t);
            b.ui.event(Event::Up { id: 0 }, t);
            b.images(1);
        }
        assert!(b.app.vol.is_some());
    }

    #[test]
    fn glisser_le_bout_d_un_axe_deplace_puis_s_annule() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.images(1);
        let (o, bout) = b.bout(0);
        assert!(b.glisser(bout, (bout.0 + 100.0, bout.1)) == 1);
        let p = b.app.projet.objets[0].position;
        assert!(p[0] > 0.0 && p[1] == 0.0 && p[2] == 0.0, "{p:?} depuis {o:?}");
        assert_eq!(b.app.histoire.annuler.len(), 1);
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].position, [0.0; 3]);
        b.app.retablir();
        assert_eq!(b.app.projet.objets[0].position, p);
    }

    #[test]
    fn glisser_un_objet_choisi_le_deplace_mais_pas_un_autre() {
        let mut b = Banc::pret();
        b.app.ajouter("autre", Maillage::cube(1.0));
        b.app.projet.objets[1].position = [2.0, 0.0, 0.0];
        b.app.choisir(Some(1));
        b.images(1);
        let lacet = b.app.cam.lacet;
        // Un coin du cube 0, loin de la poignée, non choisi : la vue tourne
        assert_eq!(b.glisser((CENTRE.0 - 40.0, CENTRE.1), (CENTRE.0 - 200.0, CENTRE.1)), 0);
        assert!(b.app.cam.lacet != lacet);
        assert_eq!(b.app.projet.objets[0].position, [0.0; 3]);
    }

    #[test]
    fn tourner_et_echelle_passent_par_la_poignee() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.mode = Mode::Rotation;
        b.images(1);
        let (_, droite) = b.bout(1);
        // L'anneau de vue passe au-dessus du centre, à 1,22 longueur d'axe
        let o = b.bout(0).0;
        let r = (o.1 - droite.1) / 0.92 * 1.22;
        assert_eq!(b.glisser((o.0, o.1 - r), (o.0 + r, o.1)), 1);
        assert_ne!(b.app.projet.objets[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        b.app.annuler();
        b.app.mode = Mode::Echelle;
        b.images(1);
        let (_, bout) = b.bout(0);
        b.glisser(bout, (o.0 + (bout.0 - o.0) * 2.0, o.1 + (bout.1 - o.1) * 2.0));
        let e = b.app.projet.objets[0].echelle;
        assert!(e[0] > 1.5 && e[1] == 1.0 && e[2] == 1.0, "{e:?}");
    }

    #[test]
    fn une_echelle_de_biais_remplace_le_maillage_et_son_tampon() {
        let mut b = Banc::pret();
        b.app.projet.objets[0].rotation = ed::qaxe([0.0, 1.0, 0.0], 0.5);
        b.app.choisir(Some(0));
        let avant = b.image(0.05);
        assert!(avant.ajouts.is_empty());
        let l = vec![0];
        let d = ed::depart(&b.app.projet.objets, &l);
        ed::poser(&mut b.app.projet.objets, &l, &d, Transfo::Echelle { s: [2.0, 1.0, 1.0], c: [0.0; 3] });
        let s = b.image(0.05);
        assert_eq!((s.ajouts.len(), s.retraits.len()), (1, 1));
        assert_eq!(s.objets[0].tampon, s.ajouts[0].0);
    }

    #[test]
    fn dupliquer_partage_le_maillage_jusqu_a_changement() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.dupliquer();
        assert_eq!(b.app.choix, [1]);
        assert_eq!(b.app.projet.objets[1].nom, "cube 2");
        assert_eq!(b.app.projet.actif, Some(1));
        assert!(b.image(0.05).ajouts.is_empty());
        b.app.projet.objets[1].echelle = [2.0; 3];
        ed::figer_echelle(&mut b.app.projet.objets[1]);
        assert_eq!(b.app.projet.objets[0].maillage.sommets[0], [-0.5; 3]);
        assert_eq!(b.image(0.05).ajouts.len(), 1);
    }

    #[test]
    fn supprimer_renumerote_l_actif_et_s_annule() {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.ajouter("c", Maillage::cube(1.0));
        b.app.choix = vec![0];
        b.app.projet.actif = Some(2);
        b.app.supprimer();
        assert_eq!(b.app.projet.objets.len(), 2);
        assert_eq!(b.app.projet.actif, Some(1));
        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), 3);
        assert_eq!(b.app.choix, [0]);
        b.app.choix = vec![2];
        b.app.projet.actif = Some(2);
        b.app.supprimer();
        assert_eq!(b.app.projet.actif, None);
    }

    #[test]
    fn annuler_vide_retablir_apres_une_nouvelle_action() {
        let mut b = Banc::pret();
        b.app.ajouter("a", Maillage::cube(1.0));
        b.app.annuler();
        assert_eq!(b.app.histoire.retablir.len(), 1);
        b.app.ajouter("b", Maillage::cube(1.0));
        assert!(b.app.histoire.retablir.is_empty());
    }

    fn pave_zone(b: &Banc) -> Rect {
        b.ui.zone(sub_id(PAVE, 101)).unwrap()
    }

    #[test]
    fn le_pave_se_deplace_par_sa_barre_et_garde_sa_place() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.saisir(Champ::Position(0));
        b.images(2);
        let c = pave_zone(&b);
        let n = (c.x + 60.0, c.y + 20.0);
        assert_eq!(b.glisser(n, (n.0 - 300.0, n.1 - 100.0)), 0);
        assert!(b.app.pave.is_some(), "glisser ne le referme pas");
        let d = pave_zone(&b);
        assert!((d.x - (c.x - 300.0)).abs() < 0.5 && (d.y - (c.y - 100.0)).abs() < 0.5, "{d:?}");

        // Les touches suivent, et une nouvelle ouverture retrouve la place
        let ok = b.ui.zone(sub_id(PAVE, 7)).unwrap();
        b.appui(ok.cx(), ok.cy());
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "2");
        b.app.pave = None;
        b.images(1);
        b.app.saisir(Champ::Position(1));
        b.images(2);
        assert_eq!(pave_zone(&b), d);
    }

    #[test]
    fn le_pave_reste_dans_l_ecran() {
        let mut b = pave_sur(Champ::Position(0));
        let c = pave_zone(&b);
        let n = (c.x + 60.0, c.y + 20.0);
        b.glisser(n, (-500.0, -500.0));
        let d = pave_zone(&b);
        assert_eq!((d.x, d.y), (0.0, 0.0));
        let n = (d.x + 60.0, d.y + 20.0);
        b.glisser(n, (5000.0, 5000.0));
        let d = pave_zone(&b);
        assert_eq!((d.right(), d.bottom()), (ECRAN.w, ECRAN.h));
    }

    #[test]
    fn la_place_du_pave_se_garde_dans_les_reglages() {
        let mut b = pave_sur(Champ::Position(0));
        let c = pave_zone(&b);
        let n = (c.x + 60.0, c.y + 20.0);
        b.glisser(n, (n.0 + 200.0, n.1));
        let t = b.app.prendre_reglages().expect("à enregistrer au lâcher");
        let mut e = Banc::pret();
        e.app.reprendre_reglages(&t);
        assert_eq!(e.app.pave_place, b.app.pave_place);
        e.app.reprendre_reglages("pave\tn'importe quoi\n");
        assert_eq!(e.app.pave_place, b.app.pave_place);
    }

    #[test]
    fn un_appui_hors_du_pave_le_referme() {
        let mut b = pave_sur(Champ::Position(0));
        b.appui(10.0, 10.0);
        assert!(b.app.pave.is_none());
    }

    fn pave_sur(c: Champ) -> Banc {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.images(1);
        b.app.projet.objets[0].position = [1.5, 0.0, 0.0];
        b.app.saisir(c);
        b.images(2);
        b
    }

    fn taper(b: &mut Banc, n: usize) {
        let z = b.ui.zone(sub_id(PAVE, n)).unwrap();
        b.appui(z.cx(), z.cy());
    }

    #[test]
    fn le_pave_reprend_la_valeur_que_le_premier_chiffre_remplace() {
        let mut b = pave_sur(Champ::Position(0));
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "1,5");
        taper(&mut b, 0); // 7
        taper(&mut b, 7); // 2
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "72");
    }

    #[test]
    fn le_signe_et_effacer_gardent_la_valeur_reprise() {
        let mut b = pave_sur(Champ::Position(0));
        taper(&mut b, 9); // signe
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "-1,5");
        taper(&mut b, 20);
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "-1,");
    }

    #[test]
    fn une_valeur_inchangee_ne_s_applique_pas() {
        // Trois décimales à l'écran : valider sans toucher n'arrondit pas la valeur
        let mut b = pave_sur(Champ::Position(0));
        b.app.pave = None;
        b.app.projet.objets[0].position = [1.23456, 0.0, 0.0];
        b.app.saisir(Champ::Position(0));
        let n = b.app.histoire.annuler.len();
        b.app.valider_pave();
        assert_eq!(b.app.projet.objets[0].position[0], 1.23456);
        assert_eq!(b.app.histoire.annuler.len(), n);
    }

    #[test]
    fn ok_passe_au_champ_suivant_du_groupe_puis_referme() {
        let mut b = pave_sur(Champ::Position(0));
        b.app.pave.as_mut().unwrap().texte = "4".into();
        b.app.valider_pave();
        assert_eq!(b.app.projet.objets[0].position[0], 4.0);
        assert_eq!(b.app.pave.as_ref().unwrap().cible, Champ::Position(1));
        b.app.valider_pave();
        assert_eq!(b.app.pave.as_ref().unwrap().cible, Champ::Position(2));
        b.app.valider_pave();
        assert!(b.app.pave.is_none());
    }

    #[test]
    fn le_groupe_suit_les_reglages_de_la_derniere_operation() {
        let mut b = en_geometrie();
        let t = b.app.travail.as_mut().unwrap();
        let a = t.m.aretes()[0];
        t.basculer(El::A(a));
        b.app.agir(A::Subdiviser);
        assert_eq!(b.app.champ_suivant(Champ::Coupes), None, "pas de glissement ici");
        assert_eq!(b.app.champ_suivant(Champ::Tolerance), None);
        assert_eq!(b.app.champ_suivant(Champ::PasTranslation), Some(Champ::PasAngle));
    }

    #[test]
    fn une_saisie_invalide_laisse_le_pave_ouvert() {
        let mut b = pave_sur(Champ::Position(0));
        b.app.pave.as_mut().unwrap().texte = "-".into();
        b.app.valider_pave();
        assert!(b.app.pave.is_some());
        assert_eq!(b.app.projet.objets[0].position[0], 1.5);
    }

    #[test]
    fn un_champ_de_texte_s_ouvre_en_alphanumerique_et_le_switch_le_change() {
        let mut b = pave_sur(Champ::Nom);
        let p = b.app.pave.as_ref().unwrap();
        assert!(p.alpha && p.neuf);
        assert_eq!(p.texte, "cube");
        let large = pave_zone(&b).w;
        taper(&mut b, 200 + 20); // q
        taper(&mut b, 200 + 40); // Maj
        taper(&mut b, 200 + 41); // w, en majuscule
        taper(&mut b, 200 + 42); // x, de nouveau en minuscule
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "qWx");
        taper(&mut b, 103); // 123
        assert!(!b.app.pave.as_ref().unwrap().alpha);
        b.images(1);
        assert!(pave_zone(&b).w < large);
        taper(&mut b, 0); // 7
        taper(&mut b, 9); // « - » s'écrit, ce n'est plus un signe
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "qWx7-");
        taper(&mut b, 104); // ABC
        assert!(b.app.pave.as_ref().unwrap().alpha);
    }

    /// Pavé sur `texte`, valeur déjà en main, curseur à la fin.
    fn saisie(cible: Champ, texte: &str) -> Pave {
        Pave { depart: String::new(), texte: texte.into(), neuf: false, curseur: texte.chars().count(), cible, alpha: false, maj: false }
    }

    #[test]
    fn le_curseur_ecrit_et_efface_au_milieu() {
        let mut p = saisie(Champ::Position(0), "125");
        p.touche("Gauche");
        p.touche("Gauche");
        p.touche("7");
        assert_eq!((p.texte.as_str(), p.curseur), ("1725", 2));
        p.touche("Effacer");
        assert_eq!((p.texte.as_str(), p.curseur), ("125", 1));
        p.touche("Gauche");
        p.touche("Gauche");
        p.touche("Effacer");
        assert_eq!((p.texte.as_str(), p.curseur), ("125", 0), "rien avant le curseur");
    }

    #[test]
    fn le_curseur_reste_dans_le_texte() {
        let mut p = saisie(Champ::Position(0), "12");
        p.touche("Droite");
        assert_eq!(p.curseur, 2);
        (0..5).for_each(|_| p.touche("Gauche"));
        assert_eq!(p.curseur, 0);
    }

    #[test]
    fn le_curseur_compte_les_caracteres_pas_les_octets() {
        let mut p = saisie(Champ::Nom, "été");
        p.touche("Gauche");
        p.touche("k");
        assert_eq!(p.texte, "étké");
        // Curseur entre « t » et « k » : Effacer retire le « t »
        p.touche("Gauche");
        p.touche("Effacer");
        assert_eq!(p.texte, "éké");
    }

    #[test]
    fn deplacer_le_curseur_garde_la_valeur_reprise() {
        let mut b = pave_sur(Champ::Position(0));
        taper(&mut b, 105);
        taper(&mut b, 6); // 1, avant le 5
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "1,15");
    }

    #[test]
    fn le_signe_suit_le_curseur_et_reste_devant() {
        let mut p = saisie(Champ::Position(0), "12");
        p.touche("Gauche");
        p.touche("-");
        assert_eq!((p.texte.as_str(), p.curseur), ("-12", 2));
        p.touche("-");
        assert_eq!((p.texte.as_str(), p.curseur), ("12", 1));
        p.touche("-");
        (0..3).for_each(|_| p.touche("Gauche"));
        p.touche("3");
        assert_eq!(p.texte, "-312", "un chiffre ne passe pas devant le signe");
    }

    #[test]
    fn la_virgule_tient_compte_du_curseur() {
        let mut p = saisie(Champ::Position(0), "12");
        p.touche("Gauche");
        p.touche(",");
        assert_eq!(p.texte, "1,2");
        p.touche(",");
        assert_eq!(p.texte, "1,2", "une seule virgule");
        let mut p = saisie(Champ::Position(0), "12");
        p.touche("Gauche");
        p.touche("Gauche");
        p.touche(",");
        assert_eq!(p.texte, "0,12");
    }

    #[test]
    fn le_nom_du_projet_se_change_depuis_l_entete_du_panneau() {
        let mut b = Banc::pret();
        b.app.shell.start.open = true;
        b.images(60);
        b.app.prendre_sauvegarde();
        let z = b.ui.zone(NOM_PROJET).unwrap();
        b.appui(z.cx(), z.cy());
        let p = b.app.pave.as_ref().unwrap();
        assert!(p.cible == Champ::Projet && p.alpha);
        b.app.pave.as_mut().unwrap().texte = "  boulon M8 ".into();
        b.app.valider_pave();
        assert!(b.app.pave.is_none());
        assert_eq!(b.app.nom_fichier(), "boulon M8");
        assert_eq!(b.app.prendre_sauvegarde().unwrap().0, "boulon M8");

        b.app.saisir(Champ::Projet);
        b.app.pave.as_mut().unwrap().texte = "   ".into();
        b.app.valider_pave();
        assert_eq!(b.app.nom_fichier(), "scene", "sans nom : celui des fichiers par défaut");
    }

    #[test]
    fn les_panneaux_se_ferment_par_leur_pastille() {
        let mut b = Banc::pret();
        b.app.shell.start.open = true;
        b.images(60);
        b.appui((14.0 + 26.0) * 1.5 + b.app.espace.x, (14.0 + 26.0) * 1.5);
        assert!(!b.app.shell.start.open);
    }

    #[test]
    fn un_champ_numerique_n_a_pas_de_switch() {
        let b = pave_sur(Champ::Position(0));
        assert!(b.ui.zone(sub_id(PAVE, 103)).is_none());
        assert_eq!(pave_zone(&b).w, 450.0);
    }

    #[test]
    fn renommer_garde_les_noms_uniques_et_s_annule() {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.projet.objets[1].modifs = crate::json::lire(r#"[{"type":"booleen","cible":"a"}]"#).unwrap();
        b.app.choisir(Some(0));
        b.images(1);
        b.app.saisir(Champ::Nom);
        b.app.pave.as_mut().unwrap().texte = "  b ".into();
        b.app.valider_pave();
        assert_eq!(b.app.projet.objets[0].nom, "b 2");
        assert!(b.app.projet.ecrire().contains("b 2"));
        let crate::json::J::Liste(l) = &b.app.projet.objets[1].modifs else { panic!() };
        assert_eq!(l[0].get("cible"), Some(&crate::json::J::Txt("b 2".into())));
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].nom, "a");

        // Un nom vide ne passe pas
        b.app.saisir(Champ::Nom);
        b.app.pave.as_mut().unwrap().texte = "   ".into();
        let n = b.app.histoire.annuler.len();
        b.app.valider_pave();
        assert_eq!(b.app.projet.objets[0].nom, "a");
        assert_eq!(b.app.histoire.annuler.len(), n);
    }

    #[test]
    fn le_pave_saisit_une_position() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.saisir(Champ::Position(2));
        b.images(1);
        b.app.pave.as_mut().unwrap().texte = "-2,5".into();
        b.app.valider_pave();
        assert_eq!(b.app.projet.objets[0].position, [0.0, 0.0, -2.5]);
        assert!(b.app.pave.is_none());
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].position, [0.0; 3]);
    }

    #[test]
    fn le_pave_retient_les_doigts() {
        let mut b = Banc::pret();
        b.app.saisir(Champ::PasAngle);
        b.images(2);
        // Touche 5 : au milieu du pavé
        b.appui(960.0, 600.0 - 30.0);
        assert_eq!(b.app.pave.as_ref().unwrap().texte, "5");
        b.appui(20.0, 20.0); // à côté : renonce
        assert!(b.app.pave.is_none());
        assert_eq!(b.app.pas.angle, 15.0);
    }

    #[test]
    fn dimensions_et_rotation_saisies() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.appliquer_champ(Champ::Dimension(0), 3.0);
        assert_eq!(b.app.projet.objets[0].echelle, [3.0, 1.0, 1.0]);
        b.app.appliquer_champ(Champ::Dimension(1), -1.0);
        assert_eq!(b.app.histoire.annuler.len(), 1);
        b.app.appliquer_champ(Champ::Rotation(2), 90.0);
        assert!((ed::euler(b.app.projet.objets[0].rotation)[2] - 90.0).abs() < 1e-9);
    }

    fn panneau_fichier(onglet: usize) -> Banc {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.shell.end.open = true;
        b.app.contenu = Contenu::Fichier;
        b.app.onglet_fichier = onglet;
        b.images(60);
        b
    }

    #[test]
    fn le_panneau_fichier_a_trois_onglets_qui_rangent_les_operations() {
        let op = |a: A| sub_id(id("operation"), a as usize);
        let mut b = panneau_fichier(0);
        assert!(b.ui.zone(op(A::Enregistrer)).is_some() && b.ui.zone(op(A::NouvelleScene)).is_some());
        assert!(b.ui.zone(op(A::Importer)).is_none() && b.ui.zone(op(A::ExporterObj)).is_none());
        let z = b.ui.zone(id("f-nom")).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.pave.as_ref().unwrap().cible == Champ::Projet);
        b.app.pave = None;
        b.images(2);

        let z = b.ui.zone(sub_id(id("onglets-fichier"), 1)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(3);
        assert!(b.ui.zone(op(A::Importer)).is_some() && b.ui.zone(op(A::Enregistrer)).is_none());

        let z = b.ui.zone(sub_id(id("onglets-fichier"), 2)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(3);
        assert!(b.ui.zone(op(A::ExporterObj)).is_some() && b.ui.zone(op(A::ExporterMtl)).is_some());
        assert!(b.ui.zone(op(A::ExporterSelectionObj)).is_none(), "grisé : rien de choisi");
        b.app.choix = vec![0];
        b.images(2);
        assert!(b.ui.zone(op(A::ExporterSelectionObj)).is_some());
    }

    #[test]
    fn exporter_la_selection_ne_prend_que_les_objets_choisis() {
        let mut b = panneau_fichier(2);
        assert!(b.app.choix.is_empty());
        assert!(!b.app.contexte().possible(A::ExporterSelectionObj), "rien de choisi");
        b.app.agir(A::ExporterSelectionObj);
        assert!(b.app.prendre_sorties().is_empty());

        // Un objet : le fichier porte son nom, l'autre n'y est pas
        b.app.choix = vec![0];
        b.app.agir(A::ExporterSelectionObj);
        let s = b.app.prendre_sorties();
        assert_eq!(s[0].nom, "a.obj");
        let obj = std::str::from_utf8(&s[0].octets).unwrap();
        assert!(obj.contains("o a") && !obj.contains("o b") && obj.contains("v 5 0 0"), "{obj}");

        // Plusieurs objets : le nom du projet, marqué
        b.app.choix = vec![0, 1];
        b.app.agir(A::ExporterSelectionStl);
        let s = b.app.prendre_sorties();
        assert_eq!(s[0].nom, "essai-selection.stl");
        assert_eq!(Maillage::depuis_fichier("x.stl", &s[0].octets).unwrap().faces.len(), 2);

        // La scène complète reste entière, quelle que soit la sélection
        b.app.choix = vec![0];
        b.app.agir(A::ExporterStl);
        let s = b.app.prendre_sorties();
        assert_eq!(s[0].nom, "essai.stl");
        assert_eq!(Maillage::depuis_fichier("x.stl", &s[0].octets).unwrap().faces.len(), 2);
    }

    #[test]
    fn un_nom_d_objet_avec_une_barre_ne_casse_pas_le_fichier() {
        let mut b = panneau_fichier(2);
        b.app.projet.objets[0].nom = "a/b".into();
        b.app.choix = vec![0];
        b.app.agir(A::ExporterSelectionStl);
        assert_eq!(b.app.prendre_sorties()[0].nom, "a-b.stl");
    }

    #[test]
    fn exporter_en_geometrie_prend_la_copie_de_travail() {
        let mut b = en_geometrie();
        let t = b.app.travail.as_mut().unwrap();
        let avant = t.m.faces.len();
        t.basculer(El::F(0));
        b.app.agir(A::SupprimerFaces);
        assert_eq!(b.app.travail.as_ref().unwrap().m.faces.len(), avant - 1);
        b.app.agir(A::ExporterSelectionStl);
        let s = b.app.prendre_sorties();
        assert_eq!(Maillage::depuis_fichier("x.stl", &s[0].octets).unwrap().faces.len(), 2 * (avant - 1));
    }

    #[test]
    fn nouvelle_scene_repart_d_un_cube_et_s_annule() {
        let mut b = panneau_fichier(0);
        let n = b.app.projet.objets.len();
        b.app.choix = vec![1];
        b.app.agir(A::NouvelleScene);
        assert_eq!(b.app.projet.objets.len(), 1);
        assert_eq!(b.app.projet.objets[0].nom, "cube");
        assert!(b.app.choix.is_empty() && b.app.nom.is_empty());
        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), n);
        assert_eq!(b.app.choix, vec![1]);

        // Depuis le mode Géométrie, l'annulation y revient
        let mut b = en_geometrie();
        b.app.agir(A::NouvelleScene);
        assert!(b.app.travail.is_none());
        b.app.annuler();
        assert!(b.app.travail.is_some());
    }

    #[test]
    fn exporter_donne_des_fichiers_relisibles() {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.exporter("obj", false);
        b.app.exporter("stl", false);
        b.app.exporter("glb", false);
        b.app.exporter_mtl();
        b.app.enregistrer();
        let s = b.app.prendre_sorties();
        assert_eq!(s.iter().map(|x| x.nom.as_str()).collect::<Vec<_>>(), ["essai.obj", "essai.stl", "essai.glb", "materiaux.mtl", "essai.gda"]);
        let obj = std::str::from_utf8(&s[0].octets).unwrap();
        assert!(obj.contains("mtllib materiaux.mtl") && obj.contains("usemtl bois"));
        // Au monde : l'objet « a » est en x = 5
        assert!(obj.contains("v 5 0 0"), "{obj}");
        assert_eq!(Maillage::depuis_fichier("x.stl", &s[1].octets).unwrap().faces.len(), 2);
        assert_eq!(Maillage::depuis_fichier("x.glb", &s[2].octets).unwrap().faces.len(), 2);
        let p = Projet::lire(std::str::from_utf8(&s[4].octets).unwrap()).unwrap();
        assert_eq!(p.objets.len(), 2);
        assert!(b.app.prendre_sorties().is_empty());
    }

    #[test]
    fn une_modification_demande_une_sauvegarde() {
        let mut b = Banc::pret();
        b.app.a_sauver = false;
        assert!(b.app.prendre_sauvegarde().is_none());
        b.app.ajouter("a", Maillage::cube(1.0));
        let (_, p) = b.app.prendre_sauvegarde().unwrap();
        assert_eq!(p.objets.len(), 2);
        assert!(b.app.prendre_sauvegarde().is_none());
        let mut c = Banc::new();
        c.app.reprendre("x", &p.ecrire());
        assert_eq!(c.app.projet.objets.len(), 2);
        assert_eq!(c.app.nom, "x");
        assert!(c.app.prendre_sauvegarde().is_none());
        c.app.reprendre("y", "illisible");
        assert_eq!(c.app.nom, "x");
    }

    #[test]
    fn la_barre_passe_a_la_ligne_sur_un_ecran_etroit() {
        let mut large = Banc::pret();
        let mut cx = Cx { ui: &mut large.ui, text: &mut large.text, dl: &mut large.dl, dp: 1.5 };
        let (w, e) = (Rect::new(0.0, 0.0, 1900.0, 1200.0), Rect::new(0.0, 0.0, 500.0, 1200.0));
        let (un, deux) = (large.app.barre(&mut cx, w), large.app.barre(&mut cx, e));
        assert!(deux < un);
    }

    #[test]
    fn glisser_tourne_la_vue_sans_choisir() {
        let mut b = Banc::new();
        b.app.shell.start.open = false;
        b.images(60);
        let lacet = b.app.cam.lacet;
        b.ui.event(Event::Down { id: 0, x: 900., y: 600. }, 0.0);
        b.ui.event(Event::Move { id: 0, x: 1000., y: 600. }, 0.1);
        b.ui.event(Event::Move { id: 0, x: 1100., y: 600. }, 0.1);
        b.ui.event(Event::Up { id: 0 }, 0.2);
        b.images(1);
        assert!(b.app.cam.lacet < lacet);
        assert!(b.app.choix.is_empty());
    }

    #[test]
    fn recadrer_vole_vers_la_scene_puis_s_arrete() {
        let mut b = Banc::new();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.cam.cible = [50.0, 0.0, 0.0];
        b.app.recadrer();
        assert!(b.app.animating());
        b.images(200);
        assert!(!b.app.animating());
        assert!((b.app.cam.cible[0] - 3.0).abs() < 1e-9);
    }

    #[test]
    fn masquer_un_set_retire_ses_objets() {
        let mut b = Banc::new();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.projet.sets[0].visible = false;
        assert!(b.image(0.05).objets.is_empty());
    }

    #[test]
    fn le_rendu_en_jeu_prend_le_materiau() {
        let mut b = Banc::new();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.rendu = Rendu::Jeu;
        let s = b.image(0.05);
        assert_eq!(s.objets[0].opacite, 0.5);
        assert_ne!(s.objets[0].albedo, s.objets[1].albedo);
    }

    #[test]
    fn rien_ne_redessine_au_repos() {
        let mut b = Banc::new();
        b.images(60);
        assert!(!b.app.animating());
    }

    #[test]
    fn un_booleen_montre_le_resultat_et_la_cible_en_fil_de_fer() {
        let mut b = perce();
        assert_eq!(modifs::cible(&modifs::liste(&b.app.projet.objets[0])[0]), "b");
        assert!(b.app.contenu == Contenu::Modifieurs && b.app.shell.end.open);
        b.app.attendre_calculs();
        let s = b.images(1);
        let base = Arc::as_ptr(&b.app.projet.objets[0].maillage) as usize;
        let vu = b.app.maillage_vu(0);
        assert_ne!(Arc::as_ptr(&vu) as usize, base);
        assert!(vu.sommets.len() > 8);
        assert_eq!(s.objets.iter().filter(|o| o.filaire).count(), 1);
        assert!(s.objets.iter().any(|o| o.tampon == Arc::as_ptr(&vu) as usize && !o.filaire));
        // Le maillage de base ne change pas
        assert_eq!(b.app.projet.objets[0].maillage.sommets.len(), 8);
    }

    #[test]
    fn un_resultat_perime_reste_affiche_pendant_le_calcul() {
        let mut b = perce();
        b.app.attendre_calculs();
        let avant = b.app.maillage_vu(0);
        b.app.commander_modif(0, CmdModif::Fixer(0, "op", J::Txt("intersection".into())));
        assert!(Arc::ptr_eq(&b.app.maillage_vu(0), &avant));
        b.app.attendre_calculs();
        assert!(!Arc::ptr_eq(&b.app.maillage_vu(0), &avant));
    }

    #[test]
    fn l_export_complet_prend_le_resultat_sans_la_cible() {
        let mut b = perce();
        let l = b.app.liste_export(false);
        assert_eq!(l.len(), 1);
        assert!(l[0].1.sommets.len() > 8);
        // Choisie, la cible s'exporte
        b.app.choix = [1].into_iter().collect();
        assert_eq!(b.app.liste_export(true).len(), 1);
    }

    /// Cube aux UV déroulées sur une bande, en Géométrie, vue UV ouverte et posée.
    fn banc_uv() -> Banc {
        let mut b = Banc::pret();
        let mut p = Projet::neuf();
        let mut m = Maillage::cube(2.0);
        for (i, f) in m.faces.iter_mut().enumerate() {
            let (x, y) = (i as f32 * 0.15 + 0.05, 0.3);
            f.uv = vec![[x, y], [x + 0.1, y], [x + 0.1, y + 0.1], [x, y + 0.1]];
        }
        p.objets[0].maillage = Arc::new(m);
        b.app.remplacer(p, "");
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.app.travail.as_mut().unwrap().aucun(None);
        b.images(1);
        b.app.agir(crate::actions::Action::EditerUv);
        b.images(60);
        b
    }

    fn sur_uv(b: &Banc, q: Uv) -> (f32, f32) {
        let e = b.app.uv.ecran(q);
        (e[0], e[1])
    }

    fn uv_0(b: &Banc) -> Uv {
        b.app.travail.as_ref().unwrap().m.faces[0].uv[0]
    }

    /// Sphère sans UV, en Géométrie.
    fn banc_sans_uv() -> Banc {
        let mut b = Banc::pret();
        let mut p = Projet::neuf();
        p.objets[0].maillage = Arc::new(Maillage::sphere_uv(1.0, 16, 8));
        b.app.remplacer(p, "");
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.app.travail.as_mut().unwrap().aucun(None);
        b.images(1);
        b
    }

    fn banc_mat() -> Banc {
        let mut b = Banc::pret();
        b.app.ouvrir("essai.gda", GDA.as_bytes());
        b.app.choisir(Some(0));
        b.app.contenu = Contenu::Scene;
        b.app.onglet_scene = ONGLET_MATERIAUX;
        b.app.shell.end.open = true;
        b.images(60);
        b
    }

    fn png_2_sur_1() -> Vec<u8> {
        crate::texture::tests::png_de(4, 2, |x, _| [x as u8 * 60, 200, 100, 255])
    }

    #[test]
    fn toucher_la_bibliotheque_applique_le_materiau_et_s_annule() {
        let mut b = banc_mat();
        let avant = b.app.projet.objets[0].materiau.clone();
        b.app.nouveau_materiau(None);
        let nom = b.app.projet.objets[0].materiau.clone();
        assert_ne!(nom, avant);
        assert_eq!(nom, "materiau");
        b.images(2);
        let k = b.app.projet.materiaux.iter().position(|x| x.0 == avant).unwrap();
        // La ligne peut sortir du cadre : le corps se fait défiler, puis on touche
        b.app.scrolls.insert(sub_id(id("corps-scene"), ONGLET_MATERIAUX), (1e6, 1e7));
        b.images(2);
        let z = b.ui.zone(sub_id(id("mat-ligne"), k)).expect("ligne");
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.projet.objets[0].materiau, avant);
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].materiau, nom);
    }

    #[test]
    fn renommer_suit_les_objets_et_refuse_un_nom_pris() {
        let mut b = banc_mat();
        let n = b.app.nouveau_materiau(None);
        b.app.renommer_materiau(&n, "  acier ");
        assert_eq!(b.app.projet.objets[0].materiau, "acier");
        assert!(b.app.projet.materiau(&n).is_none());
        b.app.nouveau_materiau(None);
        let e = b.app.projet.objets[0].materiau.clone();
        b.app.renommer_materiau(&e, "acier");
        assert_eq!(b.app.projet.objets[0].materiau, e);
        b.app.renommer_materiau(DEFAUT, "x");
        assert!(b.app.projet.materiau(DEFAUT).is_some());
    }

    #[test]
    fn supprimer_rend_le_defaut_et_le_defaut_reste() {
        let mut b = banc_mat();
        let n = b.app.nouveau_materiau(None);
        b.app.supprimer_materiau(&n);
        assert_eq!(b.app.projet.objets[0].materiau, DEFAUT);
        assert!(b.app.projet.materiau(&n).is_none());
        b.app.supprimer_materiau(DEFAUT);
        assert!(b.app.projet.materiau(DEFAUT).is_some());
        b.app.annuler();
        assert!(b.app.projet.materiau(&n).is_some());
    }

    #[test]
    fn dupliquer_copie_les_reglages_sous_un_nom_libre() {
        let mut b = banc_mat();
        let n = b.app.nouveau_materiau(None);
        b.app.regler_materiau(&n, |m| m.opacite = 0.25);
        b.app.mat_saisie = false;
        let c = b.app.nouveau_materiau(Some(&n));
        assert_eq!(c, "materiau 2");
        assert_eq!(b.app.projet.materiau(&c).unwrap().opacite, 0.25);
    }

    #[test]
    fn glisser_un_reglage_ne_fait_qu_une_etape_et_montre_le_rendu_en_jeu() {
        let mut b = banc_mat();
        b.app.nouveau_materiau(None);
        b.app.rendu = Rendu::Solide;
        b.images(2);
        let z = b.ui.zone(id("mat-opacite")).expect("curseur");
        let m = (z.x + z.w * 0.3, z.cy());
        assert_eq!(b.glisser((z.x + z.w * 0.8, z.cy()), m), 1);
        let n = b.app.projet.objets[0].materiau.clone();
        assert!(b.app.projet.materiau(&n).unwrap().opacite < 0.6);
        assert!(b.app.rendu == Rendu::Jeu);
    }

    #[test]
    fn une_couleur_s_ouvre_et_son_code_se_saisit() {
        let mut b = banc_mat();
        b.app.nouveau_materiau(None);
        b.images(2);
        let z = b.ui.zone(sub_id(id("mat-couleur"), 0)).expect("ligne");
        b.appui(z.cx(), z.cy());
        b.images(2);
        assert!(b.ui.zone(sub_id(id("mat-code"), 0)).is_some());
        b.app.saisir(Champ::MatCode(0));
        b.app.pave.as_mut().unwrap().texte = "#102030".into();
        b.app.valider_pave();
        let n = b.app.projet.objets[0].materiau.clone();
        assert_eq!(b.app.projet.materiau(&n).unwrap().couleur, "#102030");
    }

    #[test]
    fn le_bouton_demande_une_image_et_le_defaut_cree_un_materiau() {
        let mut b = banc_mat();
        b.app.projet.objets[0].materiau = DEFAUT.to_string();
        b.images(2);
        let z = b.ui.zone(id("mat-choisir")).expect("bouton");
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.prendre_selecteur(), Some(Selecteur::Image));
        b.app.recevoir_image("Pierre 1.png", &png_2_sur_1());
        let n = b.app.projet.objets[0].materiau.clone();
        assert_eq!(n, "Pierre 1");
        let m = b.app.projet.materiau(&n).unwrap();
        assert_eq!(m.couleur, "#ffffff");
        assert!(m.texture.is_some());
        assert!(b.app.projet.materiau(DEFAUT).unwrap().texture.is_none());
        assert!(b.app.rendu == Rendu::Jeu);
        // Gardée dans le projet, et la vignette se charge
        assert!(b.app.projet.ecrire().contains("Pierre 1.png"));
        let s = b.images(2);
        assert!(s.objets[0].texture.is_some());
        assert!(b.dl.images.iter().any(|i| Some(i) == b.app.projet.materiau(&n).unwrap().texture.as_ref()));
        b.app.annuler();
        assert_ne!(b.app.projet.objets[0].materiau, "Pierre 1");
    }

    #[test]
    fn retirer_l_image_en_geometrie_se_voit_aussitot() {
        let mut b = banc_sans_uv();
        b.app.ouvrir_uv();
        b.images(40);
        b.app.choisir_image();
        b.app.recevoir_image("mur.png", &png_2_sur_1());
        let s = b.images(3);
        assert!(s.objets.iter().any(|o| o.tampon == CLE_FACES && o.texture.is_some()));
        b.app.contenu = Contenu::Scene;
        b.app.onglet_scene = ONGLET_MATERIAUX;
        b.images(40);
        let z = b.ui.zone(sub_id(id("mat-image"), 1)).expect("Retirer");
        b.appui(z.cx(), z.cy());
        assert!(b.app.projet.materiau("mur").unwrap().texture.is_none());
        let s = b.images(3);
        assert!(!s.objets.iter().any(|o| o.tampon == CLE_FACES && o.texture.is_some()));
    }

    #[test]
    fn afficher_le_panneau_ne_change_pas_un_materiau_hors_bornes() {
        let mut b = banc_mat();
        let n = b.app.nouveau_materiau(None);
        b.app.regler_materiau(&n, |m| {
            m.refraction = 0.5;
            m.brillance = 2000.0;
        });
        b.app.mat_saisie = false;
        b.app.a_sauver = false;
        let e = b.app.histoire.annuler.len();
        b.images(4);
        assert_eq!(b.app.histoire.annuler.len(), e);
        let m = b.app.projet.materiau(&n).unwrap();
        assert_eq!((m.refraction, m.brillance), (0.5, 2000.0));
    }

    #[test]
    fn une_image_illisible_ne_change_rien() {
        let mut b = banc_mat();
        let n = b.app.nouveau_materiau(None);
        b.app.image_pour = Some(n.clone());
        let e = b.app.histoire.annuler.len();
        b.app.recevoir_image("x.png", b"pas une image");
        assert!(b.app.projet.materiau(&n).unwrap().texture.is_none());
        assert_eq!(b.app.histoire.annuler.len(), e);
        assert!(b.app.image_pour.is_none());
        // Sans demande en cours, un document arrivé n'a pas d'effet
        b.app.recevoir_image("y.png", &png_2_sur_1());
        assert!(b.app.projet.textures.is_empty());
    }

    #[test]
    fn l_image_arrive_dans_la_vue_uv_et_sa_forme_guide_la_projection() {
        let mut b = banc_sans_uv();
        b.app.ouvrir_uv();
        b.images(40);
        let z = b.ui.zone(sub_id(id("uv-mode"), 4)).expect("bouton Image");
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.prendre_selecteur(), Some(Selecteur::Image));
        b.app.recevoir_image("mur.png", &png_2_sur_1());
        assert_eq!(b.app.projet.objets[0].materiau, "mur");
        b.app.agir(crate::actions::Action::ProjeterCube);
        assert!(matches!(b.app.derniere, Some(Derniere::Projection { aspect: Aspect { l, h }, .. }) if l == 1.0 && h == 0.5));
        let s = b.images(3);
        let id_image = b.app.projet.materiau("mur").unwrap().texture.clone();
        assert!(s.objets.iter().any(|o| o.tampon == CLE_FACES && o.texture == id_image));
    }

    fn banc_page() -> Banc {
        let mut b = banc_sans_uv();
        b.app.agir(crate::actions::Action::ProjeterCube);
        b.images(40);
        b
    }

    fn uv_de(b: &Banc) -> Vec<Vec<[f32; 2]>> {
        b.app.travail.as_ref().unwrap().m.faces.iter().map(|f| f.uv.clone()).collect()
    }

    /// Trois objets, deux sets, la liste ouverte.
    fn banc_sets() -> Banc {
        let mut b = Banc::pret();
        let mut p = Projet::neuf();
        let o = p.objets[0].clone();
        for n in ["b", "c"] {
            let mut x = o.clone();
            x.nom = n.into();
            p.objets.push(x);
        }
        p.objets[0].nom = "a".into();
        p.objets[2].set = "décor".into();
        p.sets.push(crate::projet::Set { nom: "décor".into(), visible: true, replie: false });
        b.app.remplacer(p, "");
        b.app.shell.start.open = true;
        b.images(40);
        b
    }

    fn tenir(b: &mut Banc, z: Rect) {
        b.ui.event(Event::Down { id: 0, x: z.cx(), y: z.cy() }, 0.0);
        b.images(1);
        b.image(0.6);
        b.images(2);
        b.ui.event(Event::Up { id: 0 }, 0.7);
        b.images(2);
    }

    fn sets_de(b: &Banc) -> Vec<String> {
        b.app.projet.objets.iter().map(|o| o.set.clone()).collect()
    }

    fn touche(b: &mut Banc, z: Id) {
        let r = b.ui.zone(z).expect("zone");
        b.appui(r.cx(), r.cy());
        b.images(3);
    }

    #[test]
    fn un_appui_long_deplace_l_objet_vers_un_set_et_s_annule() {
        let mut b = banc_sets();
        let r = b.ui.zone(sub_id(id("liste-objets"), 0)).unwrap();
        tenir(&mut b, r);
        assert_eq!(b.app.page_liste, PageListe::Deplacer(vec![0]));
        assert!(b.app.choix.is_empty(), "l'appui long ne choisit pas");
        touche(&mut b, sub_id(id("set-vers"), 1));
        assert_eq!(sets_de(&b)[0], "décor");
        assert_eq!(b.app.page_liste, PageListe::Objets);
        b.app.annuler();
        assert_eq!(sets_de(&b)[0], crate::projet::SET_DEFAUT);
    }

    #[test]
    fn la_selection_se_deplace_ensemble_et_son_propre_set_ne_fait_rien() {
        let mut b = banc_sets();
        b.app.choix = vec![0, 1];
        b.images(2);
        let r = b.ui.zone(sub_id(id("liste-objets"), 1)).unwrap();
        tenir(&mut b, r);
        assert_eq!(b.app.page_liste, PageListe::Deplacer(vec![0, 1]));
        let n = b.app.histoire.annuler.len();
        touche(&mut b, sub_id(id("set-vers"), 0));
        assert_eq!(b.app.histoire.annuler.len(), n);
        b.app.agir(crate::actions::Action::DeplacerSet);
        b.images(30);
        touche(&mut b, sub_id(id("set-vers"), 1));
        assert_eq!(sets_de(&b), ["décor"; 3]);
    }

    #[test]
    fn vers_un_nouveau_set_ouvre_sa_page_et_le_nom_se_regle() {
        let mut b = banc_sets();
        let r = b.ui.zone(sub_id(id("liste-objets"), 0)).unwrap();
        tenir(&mut b, r);
        touche(&mut b, id("set-vers-nouveau"));
        assert_eq!(sets_de(&b)[0], "set");
        assert_eq!(b.app.page_liste, PageListe::Set("set".into()));
        b.app.saisir(Champ::SetNom);
        b.app.pave.as_mut().unwrap().texte = "décor".into();
        b.app.valider_pave();
        assert_eq!(sets_de(&b)[0], "set", "nom déjà pris");
        b.app.pave = None;
        b.app.saisir(Champ::SetNom);
        b.app.pave.as_mut().unwrap().texte = " props ".into();
        b.app.valider_pave();
        assert_eq!(sets_de(&b)[0], "props");
        assert!(b.app.projet.set("props").is_some() && b.app.projet.set("set").is_none());
    }

    #[test]
    fn fusionner_supprimer_et_garder_le_premier_set() {
        let mut b = banc_sets();
        let n0 = b.app.projet.objets.len();
        touche(&mut b, sub_id(sub_id(id("set"), 1), 2));
        assert_eq!(b.app.page_liste, PageListe::Set("décor".into()));
        touche(&mut b, id("set-seul"));
        assert_eq!(b.app.projet.sets.len(), 1);
        assert!(sets_de(&b).iter().all(|s| s == crate::projet::SET_DEFAUT));
        b.app.annuler();
        b.images(3);
        assert_eq!(b.app.projet.sets.len(), 2);
        b.app.choix = vec![0, 2];
        b.app.projet.actif = Some(2);
        b.app.page_liste = PageListe::Set("décor".into());
        b.images(3);
        touche(&mut b, id("set-tout"));
        assert_eq!(b.app.projet.objets.len(), n0 - 1);
        assert_eq!(b.app.choix, [0]);
        assert_eq!(b.app.projet.actif, None);
        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), n0);
        // Le premier set n'a ni fusion ni suppression
        b.app.page_liste = PageListe::Set(crate::projet::SET_DEFAUT.into());
        b.images(3);
        assert!(b.ui.zone(id("set-tout")).is_none());
    }

    #[test]
    fn supprimer_un_set_garde_l_actif_sur_son_objet() {
        let mut b = banc_sets();
        b.app.projet.objets.swap(0, 2);
        b.app.projet.actif = Some(2);
        b.app.choix = vec![2];
        b.app.supprimer_set("décor");
        assert_eq!(b.app.projet.actif, Some(1));
        assert_eq!(b.app.choix, [1]);
        assert_eq!(b.app.projet.objets[1].nom, "a");
    }

    #[test]
    fn plier_masquer_et_ranger_dans_un_set_masque() {
        let mut b = banc_sets();
        touche(&mut b, sub_id(sub_id(id("set"), 0), 0));
        assert!(b.app.projet.sets[0].replie);
        assert!(b.ui.zone(sub_id(id("liste-objets"), 0)).is_none());
        b.app.annuler();
        touche(&mut b, sub_id(sub_id(id("set"), 1), 1));
        assert!(!b.app.projet.sets[1].visible);
        b.app.choix = vec![1];
        b.app.deplacer_objets(&[1], "décor");
        assert!(b.app.choix.is_empty());
        assert_eq!(b.app.projet.ecrire().matches("décor").count() >= 2, true);
        let p = Projet::lire(&b.app.projet.ecrire()).unwrap();
        assert_eq!(p.objets[1].set, "décor");
    }

    #[test]
    fn une_annulation_qui_retire_le_set_ramene_a_la_liste() {
        let mut b = banc_sets();
        touche(&mut b, id("set-nouveau"));
        assert_eq!(b.app.page_liste, PageListe::Set("set".into()));
        b.app.annuler();
        b.images(2);
        assert_eq!(b.app.page_liste, PageListe::Objets);
    }

    #[test]
    fn deplacer_en_geometrie_garde_la_copie_de_travail() {
        let mut b = banc_sets();
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(3);
        b.app.deplacer_objets(&[0], "décor");
        assert!(b.app.travail.is_some());
        assert_eq!(sets_de(&b)[0], "décor");
        b.app.supprimer_set("décor");
        assert!(b.app.travail.is_none());
    }

    /// Une boîte, ses arêtes verticales et le tour du dessus marqués par le panneau.
    fn banc_boite() -> Banc {
        let mut b = Banc::pret();
        let mut p = Projet::neuf();
        p.objets[0].maillage = Arc::new(Maillage::boite(2.0, 2.0, 2.0));
        b.app.remplacer(p, "");
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.app.travail.as_mut().unwrap().aucun(None);
        b.app.ouvrir_uv();
        b.images(40);
        b
    }

    fn coutures_de(b: &Banc) -> Vec<Arete> {
        b.app.travail.as_ref().unwrap().m.coutures_valides()
    }

    fn choisir_toutes_les_aretes(b: &mut Banc) {
        let t = b.app.travail.as_mut().unwrap();
        t.sel.aretes = t.m.aretes();
    }

    #[test]
    fn marquer_effacer_et_annuler_une_couture_depuis_le_panneau() {
        let mut b = banc_boite();
        let z = b.ui.zone(sub_id(id("uv-couture"), 0)).expect("bouton");
        b.appui(z.cx(), z.cy());
        assert!(coutures_de(&b).is_empty());
        choisir_toutes_les_aretes(&mut b);
        b.appui(z.cx(), z.cy());
        assert_eq!(coutures_de(&b).len(), 12);
        let z1 = b.ui.zone(sub_id(id("uv-couture"), 1)).unwrap();
        b.app.travail.as_mut().unwrap().sel.aretes.truncate(2);
        b.appui(z1.cx(), z1.cy());
        assert_eq!(coutures_de(&b).len(), 10);
        b.app.annuler();
        assert_eq!(coutures_de(&b).len(), 12);
    }

    #[test]
    fn deplier_annuler_et_regler_la_marge_sans_nouvelle_etape() {
        let mut b = banc_boite();
        choisir_toutes_les_aretes(&mut b);
        let n0 = b.app.histoire.annuler.len();
        b.app.operer(|t| t.marquer_coutures(true));
        b.app.travail.as_mut().unwrap().sel.aretes.clear();
        b.app.agir(crate::actions::Action::Deplier);
        assert_eq!(b.app.histoire.annuler.len(), n0 + 2);
        assert!(b.app.travail.as_ref().unwrap().m.faces.iter().all(|f| f.uv.len() == f.s.len()));
        b.images(30);
        let (n, u) = (b.app.histoire.annuler.len(), uv_de(&b));
        let z = b.ui.zone(id("uv-marge")).expect("marge");
        b.glisser((z.x + z.w * 0.2, z.cy()), (z.x + z.w * 0.8, z.cy()));
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert_ne!(uv_de(&b), u);
        b.app.annuler();
        assert!(b.app.travail.as_ref().unwrap().m.faces.iter().all(|f| f.uv.is_empty()));
    }

    #[test]
    fn deplier_un_morceau_ferme_laisse_tout_en_place() {
        let mut b = banc_boite();
        let n = b.app.histoire.annuler.len();
        b.app.agir(crate::actions::Action::Deplier);
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert!(b.app.travail.as_ref().unwrap().m.faces.iter().all(|f| f.uv.is_empty()));
    }

    #[test]
    fn les_coutures_viennent_des_uv_et_reviennent_du_projet() {
        let mut b = banc_boite();
        b.app.agir(crate::actions::Action::ProjeterCube);
        b.app.agir(crate::actions::Action::CouturesDepuisUv);
        assert_eq!(coutures_de(&b).len(), 12);
        b.app.sortir_geometrie();
        assert_eq!(b.app.projet.objets[0].maillage.coutures_valides().len(), 12);
        let p = Projet::lire(&b.app.projet.ecrire()).unwrap();
        assert_eq!(p.objets[0].maillage.coutures_valides().len(), 12);
    }

    #[test]
    fn la_vue_montre_les_coutures_et_une_boucle_les_garde() {
        let mut b = banc_boite();
        choisir_toutes_les_aretes(&mut b);
        b.app.operer(|t| t.marquer_coutures(true));
        let a = coutures_de(&b)[0];
        let n = coutures_de(&b).len();
        let t = b.app.travail.as_mut().unwrap();
        t.boucle(Some(a), 1, 0.0, false, false).unwrap();
        assert_eq!(coutures_de(&b).len(), n + 4);
        let t = b.app.travail.as_mut().unwrap();
        t.sel.aretes = vec![coutures_de_liste(t)];
        t.subdiviser(1).unwrap();
        assert_eq!(coutures_de(&b).len(), n + 5);
    }

    fn coutures_de_liste(t: &Travail) -> Arete {
        t.m.coutures_valides()[0]
    }

    #[test]
    fn la_mise_en_page_se_lance_du_panneau_et_s_annule() {
        let mut b = banc_page();
        let avant = uv_de(&b);
        let (n, z) = (b.app.histoire.annuler.len(), b.ui.zone(sub_id(id("uv-page"), 0)).expect("bouton"));
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.histoire.annuler.len(), n + 1);
        assert_ne!(uv_de(&b), avant);
        assert!(matches!(b.app.derniere, Some(Derniere::MiseEnPage { .. })));
        b.app.annuler();
        assert_eq!(uv_de(&b), avant);
    }

    #[test]
    fn changer_un_reglage_rejoue_sans_nouvelle_etape() {
        let mut b = banc_page();
        b.app.agir(crate::actions::Action::MettreEnPage);
        b.images(30);
        let (n, u) = (b.app.histoire.annuler.len(), uv_de(&b));
        let z = b.ui.zone(sub_id(id("uv-page"), 1)).expect("densité");
        b.appui(z.cx(), z.cy());
        assert!(!b.app.uv.page.egaliser);
        assert_eq!(b.app.histoire.annuler.len(), n);
        b.images(3);
        let z = b.ui.zone(id("uv-marge")).expect("marge");
        b.glisser((z.x + z.w * 0.2, z.cy()), (z.x + z.w * 0.8, z.cy()));
        assert!(b.app.uv.page.marge > 0.02);
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert_ne!(uv_de(&b), u);
    }

    #[test]
    fn un_reglage_change_avant_coup_ne_touche_a_rien() {
        let mut b = banc_page();
        let u = uv_de(&b);
        let z = b.ui.zone(sub_id(id("uv-page"), 2)).expect("rotation");
        b.appui(z.cx(), z.cy());
        assert!(b.app.uv.page.tourner);
        assert_eq!(uv_de(&b), u);
    }

    #[test]
    fn sans_uv_la_mise_en_page_le_dit_et_seules_les_faces_de_la_vue_bougent() {
        let mut b = banc_sans_uv();
        let n = b.app.histoire.annuler.len();
        b.app.agir(crate::actions::Action::MettreEnPage);
        assert_eq!(b.app.histoire.annuler.len(), n);
        let mut b = banc_page();
        let u = uv_de(&b);
        b.app.travail.as_mut().unwrap().sel.faces = vec![0, 1];
        b.app.agir(crate::actions::Action::MettreEnPage);
        let v = uv_de(&b);
        assert_ne!(u[0], v[0]);
        assert_eq!(u[2..], v[2..]);
    }

    #[test]
    fn projeter_donne_des_uv_ouvre_la_vue_et_s_annule() {
        let mut b = banc_sans_uv();
        assert!(b.app.faces_uv().0.is_empty());
        let n = b.app.histoire.annuler.len();
        b.app.agir(crate::actions::Action::ProjeterSphere);
        assert_eq!(b.app.histoire.annuler.len(), n + 1);
        assert!(b.app.contenu == Contenu::Uv && b.app.shell.end.open);
        let f = b.app.travail.as_ref().unwrap().m.faces.len();
        assert_eq!(b.app.faces_uv().0.len(), f);
        // Le panneau se dessine, et le modèle montre le damier
        let s = b.images(40);
        assert!(s.objets.iter().any(|o| o.tampon == CLE_FACES && o.texture.as_deref() == Some(texture::DAMIER)));
        assert!(b.dl.images.iter().any(|i| i == texture::DAMIER));
        assert!(s.images.is_empty() || s.images.iter().all(|i| i.0 == texture::DAMIER));
        b.app.annuler();
        assert!(b.app.faces_uv().0.is_empty());
    }

    #[test]
    fn changer_l_axe_rejoue_sans_nouvelle_etape() {
        let mut b = banc_sans_uv();
        b.app.agir(crate::actions::Action::ProjeterCylindre);
        b.images(40);
        let (n, avant) = (b.app.histoire.annuler.len(), b.app.travail.as_ref().unwrap().m.faces[3].uv.clone());
        let z = b.ui.zone(sub_id(id("uv-axe"), 2)).expect("axes proposés");
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert!(matches!(b.app.derniere, Some(Derniere::Projection { p: uv::Projection::Cylindre { axe: 2 }, .. })));
        assert_ne!(b.app.travail.as_ref().unwrap().m.faces[3].uv, avant);
        // Déplacer ensuite met fin au réglage de la projection
        b.images(2);
        let a = sur_uv(&b, b.app.travail.as_ref().unwrap().m.faces[3].uv[0]);
        b.glisser(a, (a.0 + 40.0, a.1));
        b.images(2);
        assert!(b.app.derniere.is_none() && b.ui.zone(sub_id(id("uv-axe"), 2)).is_none());
    }

    #[test]
    fn seules_les_faces_choisies_se_projettent() {
        let mut b = banc_sans_uv();
        let t = b.app.travail.as_mut().unwrap();
        t.sel.ajouter(crate::geometrie::El::F(5));
        b.app.agir(crate::actions::Action::ProjeterCube);
        let m = &b.app.travail.as_ref().unwrap().m;
        assert!(uv::complete(m, 5) && !uv::complete(m, 4));
    }

    #[test]
    fn une_projection_suit_la_forme_de_l_image() {
        let mut b = banc_sans_uv();
        // L'image du modèle est lue même si la vue ne l'a pas encore montrée
        let octets = include_bytes!("../tests/donnees/cube-texture.glb");
        let mut c = Banc::pret();
        c.app.ouvrir("cube-texture.glb", octets);
        c.app.choisir(Some(0));
        c.app.entrer_geometrie();
        c.images(1);
        c.app.agir(crate::actions::Action::ProjeterCube);
        assert!(matches!(c.app.derniere, Some(Derniere::Projection { .. })));
        assert!(!c.app.uv.aspects.is_empty());
        // Sans texture : tuile carrée
        b.app.agir(crate::actions::Action::ProjeterCube);
        assert!(matches!(b.app.derniere, Some(Derniere::Projection { aspect: Aspect::CARRE, .. })));
    }

    #[test]
    fn la_vue_uv_prend_la_moitie_de_l_ecran_et_quitte_la_geometrie_avec_elle() {
        let mut b = banc_uv();
        assert!(b.app.shell.large && b.app.contenu == Contenu::Uv);
        assert!(b.ui.zone(TOILE_UV).is_some());
        b.app.sortir_geometrie();
        b.images(2);
        assert!(!b.app.shell.large && !b.app.shell.end.open);
    }

    #[test]
    fn toucher_choisit_le_point_et_le_vide_efface() {
        let mut b = banc_uv();
        let (x, y) = sur_uv(&b, [0.05, 0.3]);
        b.appui(x, y);
        assert_eq!(b.app.uv.sel.len(), 1);
        assert!(b.app.uv.sel.contains(&(0, 0)));
        // Toucher encore le retire, et le vide efface tout
        b.appui(x, y);
        assert!(b.app.uv.sel.is_empty());
        b.appui(x, y);
        let (x, y) = sur_uv(&b, [0.5, 0.9]);
        b.appui(x, y);
        assert!(b.app.uv.sel.is_empty());
    }

    #[test]
    fn glisser_un_point_le_deplace_et_s_annule() {
        let mut b = banc_uv();
        let (a, z) = (sur_uv(&b, [0.05, 0.3]), sur_uv(&b, [0.25, 0.5]));
        assert_eq!(b.glisser(a, z), 1);
        let q = uv_0(&b);
        assert!((q[0] - 0.25).abs() < 1e-3 && (q[1] - 0.5).abs() < 1e-3, "{q:?}");
        // Les trois autres coins de la face n'ont pas bougé
        assert_eq!(b.app.travail.as_ref().unwrap().m.faces[0].uv[1], [0.15, 0.3]);
        b.app.annuler();
        b.images(1);
        assert_eq!(uv_0(&b), [0.05, 0.3]);
    }

    #[test]
    fn un_ilot_se_deplace_d_un_bloc_et_le_pas_s_applique() {
        let mut b = banc_uv();
        b.app.uv.ilots = true;
        b.app.pas.actif = true;
        let (a, z) = (sur_uv(&b, [0.1, 0.35]), sur_uv(&b, [0.2, 0.4]));
        b.glisser(a, z);
        let f = b.app.travail.as_ref().unwrap().m.faces[0].uv.clone();
        assert!((f[0][0] - (0.05 + 0.09375)).abs() < 1e-3, "{f:?}"); // 3/32
        assert!((f[2][0] - f[0][0] - 0.1).abs() < 1e-4);
        assert_eq!(b.app.travail.as_ref().unwrap().m.faces[1].uv[0], [0.2, 0.3]);
    }

    #[test]
    fn la_boite_ajoute_et_un_axe_bloque_l_autre() {
        let mut b = banc_uv();
        let (a, z) = (sur_uv(&b, [0.0, 0.2]), sur_uv(&b, [0.17, 0.5]));
        assert_eq!(b.glisser(a, z), 0);
        assert_eq!(b.app.uv.sel.len(), 4);
        b.app.uv.axes = [false, true];
        let (a, z) = (sur_uv(&b, [0.05, 0.3]), sur_uv(&b, [0.5, 0.4]));
        b.glisser(a, z);
        let q = uv_0(&b);
        assert!(q[0] == 0.05 && (q[1] - 0.4).abs() < 1e-3, "{q:?}");
    }

    #[test]
    fn tourner_garde_le_centre_de_la_selection() {
        let mut b = banc_uv();
        b.app.uv.ilots = true;
        let (x, y) = sur_uv(&b, [0.1, 0.35]);
        b.appui(x, y);
        b.app.uv.outil = OutilUv::Tourner;
        // Du bord droit au bord bas : un quart de tour autour du centre
        let (a, z) = (sur_uv(&b, [0.3, 0.35]), sur_uv(&b, [0.1, 0.55]));
        assert_eq!(b.glisser(a, z), 1);
        let f = b.app.travail.as_ref().unwrap().m.faces[0].uv.clone();
        assert!((f[0][0] - 0.15).abs() < 1e-3 && (f[0][1] - 0.3).abs() < 1e-3, "{f:?}");
    }

    #[test]
    fn deux_doigts_zooment_sans_rien_deplacer() {
        let mut b = banc_uv();
        let (z0, a) = (b.app.uv.vu[2], sur_uv(&b, [0.5, 0.5]));
        let n = b.app.histoire.annuler.len();
        b.ui.event(Event::Down { id: 0, x: a.0 - 50.0, y: a.1 }, 0.0);
        b.ui.event(Event::Down { id: 1, x: a.0 + 50.0, y: a.1 }, 0.0);
        b.images(1);
        b.ui.event(Event::Move { id: 1, x: a.0 + 150.0, y: a.1 }, 0.1);
        b.images(1);
        b.ui.event(Event::Up { id: 1 }, 0.2);
        b.ui.event(Event::Up { id: 0 }, 0.2);
        b.images(1);
        assert!(b.app.uv.vu[2] > z0 * 1.5);
        assert_eq!(b.app.histoire.annuler.len(), n);
        assert!(b.app.uv.sel.is_empty());
    }

    #[test]
    fn les_faces_choisies_disent_ce_que_la_vue_montre() {
        let mut b = banc_uv();
        assert_eq!(b.app.faces_uv().0.len(), 6);
        b.app.travail.as_mut().unwrap().sel.ajouter(crate::geometrie::El::F(2));
        assert_eq!(b.app.faces_uv().0, vec![2]);
    }

    #[test]
    fn les_uv_editees_restent_apres_la_geometrie() {
        let mut b = banc_uv();
        let (a, z) = (sur_uv(&b, [0.05, 0.3]), sur_uv(&b, [0.25, 0.5]));
        b.glisser(a, z);
        b.app.sortir_geometrie();
        b.images(1);
        let u = b.app.projet.objets[0].maillage.faces[0].uv[0];
        assert!((u[0] - 0.25).abs() < 1e-3);
        assert!(b.app.projet.ecrire().contains("\"uv\""));
    }

    #[test]
    fn la_texture_passe_sur_le_modele_et_dans_la_vue() {
        let mut b = Banc::pret();
        b.app.ouvrir("cube-texture.glb", include_bytes!("../tests/donnees/cube-texture.glb"));
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(1);
        b.app.ouvrir_uv();
        let s = b.images(40);
        let id = b.app.faces_uv().1.expect("texture");
        assert!(s.objets.iter().any(|o| o.tampon == CLE_FACES && o.texture.as_ref() == Some(&id)));
        assert!(b.dl.images.contains(&id));
        assert!(b.dl.cmds.iter().any(|c| c.image.is_some()));
        // Vue fermée : le modèle reprend sa couleur de travail
        b.app.shell.end.open = false;
        let s = b.images(2);
        assert!(s.objets.iter().all(|o| o.texture.is_none()));
    }

    #[test]
    fn sans_uv_la_vue_propose_les_projections() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(1);
        b.app.ouvrir_uv();
        b.images(40);
        assert!(b.app.faces_uv().0.is_empty());
    }

    #[test]
    fn la_pastille_scene_ouvre_son_panneau_dans_les_deux_modes() {
        let mut b = Banc::pret();
        b.app.shell.end.open = false;
        b.images(30);
        let z = b.ui.zone(SCENE).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Scene);
        b.images(30);
        assert!(b.ui.zone(id("rendu-solide")).is_some() && b.ui.zone(id("rendu-jeu")).is_some());
        b.app.entrer_geometrie();
        b.images(3);
        assert!(b.ui.zone(id("rendu-rayons")).is_some());
    }

    #[test]
    fn la_pastille_ouvre_et_ferme_le_panneau_des_modifieurs() {
        let mut b = perce();
        b.images(30);
        let z = b.ui.zone(MODIFIEURS).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(!b.app.shell.end.open);
        b.images(30);
        b.appui(z.cx(), z.cy());
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Modifieurs);
        // Éteinte en Géométrie
        b.app.entrer_geometrie();
        b.images(2);
        b.appui(z.cx(), z.cy());
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Modifieurs);
    }

    #[test]
    fn neuf_pastilles_tiennent_sur_un_telephone_etroit() {
        // 360 et 393 dp : téléphones courants, le Redmi Note 14 parmi eux
        for largeur in [360.0, 393.0, 432.0] {
            let (t, e) = App::mesure_pastilles(largeur, 1.0);
            assert!(14.0 + 9.0 * t + 8.0 * e <= largeur - 14.0 + 0.01, "{t} {e}");
            assert!(t >= 34.0);
        }
        assert_eq!(App::mesure_pastilles(1280.0, 1.0), (52.0, 10.0));
    }

    #[test]
    fn deplacer_la_cible_refait_le_resultat() {
        let mut b = perce();
        b.app.attendre_calculs();
        let avant = b.app.maillage_vu(0);
        b.app.projet.objets[1].position = [0.2, 1.0, 0.0];
        b.app.attendre_calculs();
        let apres = b.app.maillage_vu(0);
        assert!(!Arc::ptr_eq(&avant, &apres));
        assert_ne!(avant.sommets, apres.sommets);
    }

    #[test]
    fn appliquer_remplace_le_maillage_et_s_annule() {
        let mut b = perce();
        b.app.attendre_calculs();
        b.app.commander_modif(0, CmdModif::Appliquer(0));
        assert!(modifs::liste(&b.app.projet.objets[0]).is_empty());
        assert!(b.app.projet.objets[0].maillage.sommets.len() > 8);
        b.images(1);
        assert_eq!(b.images(1).objets.iter().filter(|o| o.filaire).count(), 0);
        b.app.annuler();
        assert_eq!(b.app.projet.objets[0].maillage.sommets.len(), 8);
        assert_eq!(modifs::liste(&b.app.projet.objets[0]).len(), 1);
    }

    #[test]
    fn la_geometrie_edite_la_base() {
        let mut b = perce();
        b.app.attendre_calculs();
        b.app.entrer_geometrie();
        b.images(1);
        assert_eq!(b.app.travail.as_ref().unwrap().m.sommets.len(), 8);
    }

    #[test]
    fn le_panneau_d_ajout_regle_previsualise_et_ajoute() {
        let mut b = Banc::pret();
        let n = b.app.projet.objets.len();
        b.app.agir(crate::actions::Action::AjouterCube);
        assert!(b.app.contenu == Contenu::Ajout);
        assert!(b.app.shell.end.open);
        let s = b.images(2);
        assert_eq!(b.app.projet.objets.len(), n);
        let m = b.app.ajout.as_ref().unwrap().apercu.clone().unwrap();
        assert!(s.objets.iter().any(|o| o.tampon == Arc::as_ptr(&m) as usize));
        // Hors bornes : plus d'aperçu, Ajouter sans effet
        b.app.appliquer_champ(Champ::Primitive(0, "x"), 0.0);
        assert!(b.app.ajout.as_ref().unwrap().apercu.is_none());
        b.app.valider_ajout();
        assert_eq!(b.app.projet.objets.len(), n);
        b.app.ouvrir_ajout(Prim::Cube);
        b.app.appliquer_champ(Champ::Primitive(0, "x"), 3.0);
        b.app.valider_ajout();
        assert_eq!(b.app.projet.objets.len(), n + 1);
        assert!(b.app.ajout.is_none() && !b.app.shell.end.open);
        // Les réglages reviennent à l'ajout suivant
        b.app.ouvrir_ajout(Prim::Cube);
        let a = b.app.ajout.as_ref().unwrap();
        let (cle, v) = (a.prim.champs()[0].cle, a.valeurs.clone());
        assert!(matches!(v.get(cle), Some(V::N(x)) if *x == 3.0));
    }

    #[test]
    fn fermer_le_panneau_abandonne_l_ajout() {
        let mut b = Banc::pret();
        b.app.agir(crate::actions::Action::AjouterSphere);
        b.images(1);
        b.app.shell.end.open = false;
        b.images(1);
        assert!(b.app.ajout.is_none());
    }

    #[test]
    fn une_vis_se_calcule_a_part_et_se_cadre() {
        let mut b = Banc::pret();
        b.app.agir(crate::actions::Action::AjouterVis);
        b.app.attendre_calculs();
        let a = b.app.ajout.as_ref().unwrap();
        assert!(a.apercu.as_ref().is_some_and(|m| m.sommets.len() > 100), "{:?}", a.info);
        assert!(b.app.vol.is_some());
        b.app.valider_ajout();
        assert!(b.app.projet.objets.last().unwrap().maillage.sommets.len() > 100);
    }

    #[test]
    fn les_reglages_de_primitive_reviennent_apres_relance() {
        let mut b = Banc::pret();
        b.app.primitives.insert(Prim::Vis, Prim::Vis.defauts());
        b.app.primitives.get_mut(&Prim::Vis).unwrap().insert("longueur", V::N(33.0));
        b.app.reglages_changes = true;
        let t = b.app.prendre_reglages().unwrap();
        let mut c = Banc::pret();
        c.app.reprendre_reglages(&t);
        assert!(matches!(c.app.primitives[&Prim::Vis].get("longueur"), Some(V::N(x)) if *x == 33.0));
        // Entrée d'une version plus ancienne : ignorée
        c.app.reprendre_reglages("primitive.vis.inconnu\tn3\nprimitive.inconnue.x\tn3");
    }

    // ----- Armatures ---------------------------------------------------------

    fn armature(b: &Banc) -> &Armature {
        b.app.projet.objets[b.app.os.as_ref().unwrap().objet.unwrap()].armature.as_ref().unwrap()
    }

    /// Une armature à l'écran, en mode Armature, cadrée.
    fn en_os(humain: bool) -> Banc {
        let mut b = Banc::pret();
        b.app.agir(if humain { A::AjouterHumanoide } else { A::AjouterArmature });
        b.app.entrer_geometrie();
        assert!(!b.app.shell.end.open, "le panneau Os ne s'ouvre pas de lui-même");
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.app.recadrer();
        b.images(40);
        // Assez loin pour qu'aucun bout ne passe sous les barres
        b.app.cam.distance *= 2.0;
        b.images(1);
        assert!(b.app.os.is_some());
        b
    }

    /// Position à l'écran d'un bout d'os.
    fn point_os(b: &Banc, id: &str, queue: bool) -> (f32, f32) {
        let w = b.app.espace;
        let v = b.app.vue_ecran(w);
        let o = &b.app.projet.objets[b.app.os.as_ref().unwrap().objet.unwrap()];
        let s = o.armature.as_ref().unwrap().segments();
        let p = vue::appliquer(&o.matrice(), if queue { s[id].queue } else { s[id].tete });
        let e = v.ecran(p).unwrap();
        (w.x + e[0] as f32, w.y + e[1] as f32)
    }

    fn milieu_os(b: &Banc, id: &str) -> (f32, f32) {
        let (t, q) = (point_os(b, id, false), point_os(b, id, true));
        ((t.0 + q.0) / 2.0, (t.1 + q.1) / 2.0)
    }

    #[test]
    fn on_ajoute_une_armature_ou_un_humanoide_sur_le_curseur() {
        let mut b = Banc::pret();
        b.app.projet.curseur = [1.0, 2.0, 3.0];
        let n = b.app.projet.objets.len();
        b.app.agir(A::AjouterArmature);
        let o = &b.app.projet.objets[n];
        assert_eq!((o.nom.as_str(), o.position), ("armature", [1.0, 2.0, 3.0]));
        assert_eq!(o.maillage.faces.len(), 8);
        assert_eq!(b.app.choix, [n]);
        // La géométrie d'une armature, ce sont ses os
        assert!(b.app.contexte().possible(A::EditerGeometrie));
        b.app.agir(A::EditerGeometrie);
        assert!(b.app.os.as_ref().is_some_and(|e| e.objet == Some(n)) && b.app.travail.is_none());
        assert!(!b.app.contexte().possible(A::EditerGeometrie));
        b.app.sortir_geometrie();
        b.app.agir(A::AjouterHumanoide);
        assert_eq!(b.app.projet.objets[n + 1].armature.as_ref().unwrap().os.len(), 19);
        assert_eq!(b.app.projet.objets[n + 1].nom, "humanoide");
        b.images(2);
        b.app.annuler();
        b.app.annuler();
        assert_eq!(b.app.projet.objets.len(), n);
    }

    #[test]
    fn un_double_appui_sur_une_armature_entre_en_mode_armature() {
        let mut b = Banc::pret();
        b.app.agir(A::AjouterArmature);
        b.app.recadrer();
        b.images(40);
        let k = b.app.choix[0];
        b.app.projet.objets[0].visible = false;
        b.images(1);
        b.app.os = Some(EditionOs { objet: Some(k), ..Default::default() });
        let c = milieu_os(&b, "o1");
        b.app.os = None;
        b.appui(c.0, c.1);
        b.appui(c.0, c.1);
        assert_eq!(b.app.os.as_ref().and_then(|e| e.objet), Some(k));
        assert!(b.app.travail.is_none());
        b.app.sortir_geometrie();
        assert!(b.app.os.is_none());
    }

    #[test]
    fn toucher_un_bout_prend_le_soudure_et_glisser_le_deplace_puis_s_annule() {
        let mut b = en_os(false);
        b.app.os.as_mut().unwrap().bouts = vec![Bout::queue("o1")];
        b.app.agir(A::ExtruderOs);
        assert_eq!(armature(&b).os.len(), 2);
        let f = armature(&b).os[1].id.clone();
        b.app.tous_os(false);
        b.images(1);
        b.app.recadrer();
        b.images(40);
        let e = point_os(&b, "o1", true);
        b.appui(e.0, e.1);
        assert_eq!(b.app.os.as_ref().unwrap().bouts, [Bout::queue("o1"), Bout::tete(&f)]);
        let avant = armature(&b).segments();
        assert_eq!(b.glisser(e, (e.0 + 150.0, e.1)), 1);
        let apres = armature(&b).segments();
        assert_ne!(apres["o1"].queue, avant["o1"].queue);
        assert!((0..3).all(|k| (apres["o1"].queue[k] - apres[&f].tete[k]).abs() < 1e-9), "soudure");
        assert!((0..3).all(|k| (apres[&f].queue[k] - avant[&f].queue[k]).abs() < 1e-9), "la queue de l'enfant reste");
        assert_eq!(b.app.projet.objets[b.app.os.as_ref().unwrap().objet.unwrap()].maillage.faces.len(), 16);
        b.app.annuler();
        assert_eq!(armature(&b).segments()["o1"].queue, avant["o1"].queue);
    }

    #[test]
    fn le_corps_prend_les_deux_bouts_et_l_appui_long_seul() {
        let mut b = en_os(true);
        let id = armature(&b).os.iter().find(|x| x.nom == "cuisse.G").unwrap().id.clone();
        let c = milieu_os(&b, &id);
        b.appui(c.0, c.1);
        let e = b.app.os.as_ref().unwrap();
        assert_eq!(e.os_choisis(armature(&b)), [id.clone()]);
        assert_eq!(e.actif.as_ref(), Some(&id));
        // Double appui dans le vide : recadre, sans rien choisir
        let w = b.app.espace;
        let n = b.app.os.as_ref().unwrap().bouts.len();
        let vide = (w.x + 0.12 * w.w, w.y + 0.5 * w.h);
        b.app.cam.distance *= 3.0;
        b.appui(vide.0, vide.1);
        b.appui(vide.0, vide.1);
        assert!(b.app.vol.is_some());
        assert_eq!(b.app.os.as_ref().unwrap().bouts.len(), n);
    }

    #[test]
    fn extruder_supprimer_parenter_detacher() {
        let mut b = en_os(false);
        assert!(!b.app.contexte().possible(A::ExtruderOs));
        b.app.tous_os(true);
        assert!(b.app.contexte().possible(A::ExtruderOs));
        // Le dernier os reste
        b.app.supprimer_os();
        assert_eq!(armature(&b).os.len(), 1);
        assert!(b.app.message.as_ref().unwrap().0.contains("au moins un os"));
        b.app.agir(A::ExtruderOs);
        let f = armature(&b).os[1].id.clone();
        assert_eq!(armature(&b).os[1].parent.as_deref(), Some("o1"));
        assert_eq!(b.app.os.as_ref().unwrap().actif.as_ref(), Some(&f));
        // Boucle refusée : l'actif descend de l'os choisi
        b.app.tous_os(true);
        b.app.agir(A::ParenterOs);
        assert!(b.app.message.as_ref().unwrap().0.contains("1 os refusé"));
        assert_eq!(armature(&b).trouver("o1").unwrap().parent, None);
        b.app.agir(A::DetacherOs);
        assert_eq!(armature(&b).os[1].parent, None);
        b.app.agir(A::ParenterOs);
        assert_eq!(armature(&b).trouver("o1").unwrap().parent.as_deref(), Some(f.as_str()));
        b.app.os.as_mut().unwrap().actif = Some("o1".into());
        b.app.tous_os(false);
        b.app.os.as_mut().unwrap().actif = Some(f.clone());
        b.app.agir(A::DescendantsOs);
        assert_eq!(b.app.os.as_ref().unwrap().os_choisis(armature(&b)), ["o1"]);
        b.app.agir(A::SupprimerOs);
        assert_eq!(armature(&b).os.len(), 1);
        assert_eq!(b.app.os.as_ref().unwrap().actif, None);
        b.app.annuler();
        assert_eq!(armature(&b).os.len(), 2);
    }

    #[test]
    fn ajouter_un_os_le_pose_sur_le_curseur_de_la_longueur_de_l_actif() {
        let mut b = en_os(false);
        let k = b.app.os.as_ref().unwrap().objet.unwrap();
        b.app.projet.objets[k].position = [1.0, 0.0, 0.0];
        b.app.projet.objets[k].echelle = [2.0; 3];
        b.app.projet.curseur = [3.0, 0.0, 0.0];
        b.app.os.as_mut().unwrap().actif = Some("o1".into());
        b.app.agir(A::AjouterOs);
        let n = &armature(&b).os[1];
        assert_eq!((n.position, n.longueur, n.parent.clone()), ([1.0, 0.0, 0.0], 1.0, None));
        assert_eq!(b.app.os.as_ref().unwrap().os_choisis(armature(&b)), [n.id.clone()]);
    }

    #[test]
    fn nom_longueur_et_parent_se_reglent_dans_la_fiche() {
        let mut b = en_os(false);
        let k = b.app.os.as_ref().unwrap().objet.unwrap();
        b.app.projet.objets[k].echelle = [2.0; 3];
        b.app.tous_os(true);
        b.app.agir(A::ExtruderOs);
        let f = armature(&b).os[1].id.clone();
        b.app.os.as_mut().unwrap().actif = Some("o1".into());
        assert_eq!(b.app.valeur_champ(Champ::OsLongueur).unwrap(), "2");
        b.app.saisir(Champ::OsLongueur);
        b.app.appliquer_champ(Champ::OsLongueur, 3.0);
        b.app.pave = None;
        // Longueur au monde : l'enfant soudé suit sa tête, sa queue reste
        assert!((armature(&b).trouver("o1").unwrap().longueur - 1.5).abs() < 1e-9);
        let s = armature(&b).segments();
        assert!((s[&f].tete[1] - 1.5).abs() < 1e-9 && (s[&f].queue[1] - 2.0).abs() < 1e-9);
        b.app.annuler();
        assert_eq!(armature(&b).trouver("o1").unwrap().longueur, 1.0);
        b.app.renommer_os("  bras ");
        assert_eq!(armature(&b).trouver("o1").unwrap().nom, "bras");
        b.app.os.as_mut().unwrap().actif = Some(f.clone());
        b.app.renommer_os("bras");
        assert_eq!(b.app.message.as_ref().unwrap().0, "Ce nom est déjà pris");
        b.app.renommer_os(" ");
        assert_eq!(b.app.message.as_ref().unwrap().0, "Nom vide");
        b.app.changer_parent_os(&f, None);
        assert_eq!(armature(&b).trouver(&f).unwrap().parent, None);
        b.app.changer_parent_os("o1", Some(&f));
        b.app.changer_parent_os(&f, Some("o1"));
        assert_eq!(b.app.message.as_ref().unwrap().0, "Cet os descend de celui-ci");
    }

    #[test]
    fn la_fiche_touche_les_os_et_la_translation_passe_par_le_panneau() {
        let mut b = en_os(true);
        // L'arbre est en bas du panneau : on y descend au doigt
        let bord = b.ui.zone(id("corps-os")).unwrap();
        for _ in 0..6 {
            if b.ui.zone(sub_id(id("os-rang"), 0)).unwrap().bottom() < bord.bottom() {
                break;
            }
            b.glisser((bord.cx(), bord.bottom() - 40.0), (bord.cx(), bord.y + 40.0));
            b.images(10);
        }
        let z = b.ui.zone(sub_id(id("os-rang"), 0)).unwrap();
        assert!(z.bottom() < bord.bottom());
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.os.as_ref().unwrap().actif.as_deref(), Some(armature(&b).ordre()[0].id.as_str()));
        assert_eq!(b.app.os.as_ref().unwrap().os_choisis(armature(&b)).len(), 1);
        b.images(2);
        assert!(b.ui.zone(id("os-nom")).is_some());
        let y = ed::centre(&b.app.points())[1];
        b.app.appliquer_champ(Champ::Position(1), y + 1.0);
        assert!((ed::centre(&b.app.points())[1] - y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn en_mode_armature_le_reste_de_la_scene_est_inerte() {
        let mut b = en_os(false);
        let c = b.app.contexte();
        assert!(c.os && !c.possible(A::Dupliquer) && !c.possible(A::AjouterCube) && !c.possible(A::ModifBooleen) && !c.possible(A::EditerGeometrie));
        assert!(c.possible(A::Enregistrer) && c.possible(A::Recadrer) && c.possible(A::AjouterOs));
        let g: Vec<Groupe> = actions::barre(&c, &Choix::new()).iter().map(|x| x.0).collect();
        assert_eq!(g, [Groupe::Os]);
        let (armature, cube) = (b.app.os.as_ref().unwrap().objet.unwrap(), 0);
        assert!(armature != cube);
        let (_, _, o) = b.app.couleurs(cube, &b.app.projet.objets[cube]);
        assert_eq!(o, OPACITE_PASSIF);
        assert_eq!(b.app.couleurs(armature, &b.app.projet.objets[armature]).2, 1.0);
        // Hors de l'armature, un appui ne choisit rien
        let w = b.app.espace;
        b.appui(w.x + 30.0, w.y + 30.0);
        assert_eq!(b.app.choix, [armature]);
        // Ajouter un objet ressort du mode, comme en Géométrie ; le panneau Os reste, en Scène
        b.app.ajouter("x", Maillage::cube(1.0));
        assert!(b.app.os.is_none());
        b.images(2);
        assert!(b.app.contenu == Contenu::Os && b.app.shell.end.open);
        assert!(b.ui.zone(id("os-armature-nom")).is_none(), "le cube est actif");
    }

    /// Pilule d'un mode, par sa place dans la barre : Scène, Géométrie.
    fn mode(b: &mut Banc, cle: usize) {
        let z = b.ui.zone(sub_id(id("barre-haute"), cle)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
    }

    #[test]
    fn la_geometrie_d_une_armature_ce_sont_ses_os() {
        let mut b = Banc::pret();
        let n = b.app.projet.objets.len();
        b.app.agir(A::AjouterArmature);
        b.images(2);
        // Géométrie ouvre les os de l'armature active ; la pilule Pose est là aussi
        assert!(b.ui.zone(sub_id(id("barre-haute"), 5)).is_some());
        mode(&mut b, 1);
        assert_eq!(b.app.os.as_ref().unwrap().objet, Some(n));
        assert!(b.app.travail.is_none() && !b.app.shell.end.open, "le panneau Os ne s'ouvre pas de lui-même");
        // Un nouvel appui reste dans les os, Scène en sort
        mode(&mut b, 1);
        assert!(b.app.os.is_some() && b.app.travail.is_none());
        mode(&mut b, 0);
        assert!(b.app.os.is_none());
        // L'actif décide : sur un cube, la géométrie du cube
        b.app.choix = vec![0, n];
        b.app.projet.actif = Some(0);
        b.images(1);
        mode(&mut b, 1);
        assert!(b.app.travail.is_some() && b.app.os.is_none());
        mode(&mut b, 0);
        b.app.choix = vec![0, n];
        b.app.projet.actif = Some(n);
        b.images(1);
        mode(&mut b, 1);
        assert!(b.app.travail.is_none() && b.app.os.as_ref().unwrap().objet == Some(n));
    }

    #[test]
    fn le_panneau_et_la_vue_changent_d_armature() {
        let mut b = Banc::pret();
        let n = b.app.projet.objets.len();
        b.app.agir(A::AjouterArmature);
        b.app.projet.curseur = [3.0, 0.0, 0.0];
        b.app.agir(A::AjouterArmature);
        b.app.entrer_armature();
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.app.recadrer();
        b.images(40);
        assert_eq!(b.app.os.as_ref().unwrap().objet, Some(n + 1));
        let z = b.ui.zone(sub_id(id("os-armature"), n)).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.os.as_ref().unwrap().objet, Some(n));
        assert_eq!(b.app.choix, [n]);
        // Rien d'ouvert : un appui dans la vue prend l'armature touchée
        b.app.os = Some(EditionOs::default());
        b.app.version_geo = None;
        b.images(2);
        b.app.os = Some(EditionOs { objet: Some(n + 1), ..Default::default() });
        let c = milieu_os(&b, "o1");
        b.app.os = Some(EditionOs::default());
        b.appui(c.0, c.1);
        assert_eq!(b.app.os.as_ref().unwrap().objet, Some(n + 1));
    }

    #[test]
    fn l_appui_long_dans_l_arbre_choisit_toute_une_descendance() {
        let mut b = en_os(true);
        b.images(2);
        // La rubrique Sélection a disparu du panneau
        assert!(b.ui.zone(sub_id(id("operation"), A::TousOs as usize)).is_none());
        let a = armature(&b).clone();
        let ordre: Vec<String> = a.ordre().iter().map(|x| x.id.clone()).collect();
        let r = ordre.iter().position(|i| a.trouver(i).unwrap().nom == "bras.G").unwrap();
        let z = b.ui.zone(sub_id(id("os-rang"), r)).unwrap();
        tenir(&mut b, z);
        let mut attendu: Vec<String> = std::iter::once(ordre[r].clone()).chain(a.descendants(&ordre[r])).collect();
        let mut vu = b.app.os.as_ref().unwrap().os_choisis(&a);
        attendu.sort();
        vu.sort();
        assert_eq!(vu, attendu);
        assert_eq!(b.app.os.as_ref().unwrap().actif.as_ref(), Some(&ordre[r]));
        // Sur l'armature, l'appui long choisit tout
        let k = b.app.os.as_ref().unwrap().objet.unwrap();
        let z = b.ui.zone(sub_id(id("os-armature"), k)).unwrap();
        tenir(&mut b, z);
        assert_eq!(b.app.os.as_ref().unwrap().os_choisis(&a).len(), a.os.len());
        // Toucher le nom de l'armature vide la sélection
        let z = b.ui.zone(sub_id(id("os-armature"), k)).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.os.as_ref().unwrap().bouts.is_empty());
    }

    #[test]
    fn les_poses_se_gardent_au_catalogue_et_se_rappellent() {
        let (mut b, bras) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.images(2);
        let a = |b: &Banc| b.app.projet.objets[1].armature.clone().unwrap();
        let levee = a(&b).pose.clone();
        // Sauvegarder la pose, sur la ligne des os posés : l'arbre replié pour la voir
        let cle = format!("a:{}", b.app.projet.objets[1].id);
        b.app.os_plies.insert(cle);
        b.images(2);
        let z = b.ui.zone(id("pose-sauver")).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(a(&b).poses_nommees(), vec![("Pose 1".to_string(), levee.clone())]);
        // Une autre pose, gardée aussi
        b.app.angle_saisie = false;
        b.app.regler_angles(&bras, [0.0, 0.0, -30.0]);
        b.app.angle_saisie = false;
        b.app.sauver_pose();
        assert_eq!(a(&b).poses_nommees().len(), 2);
        // Le catalogue : la première revient d'un appui
        b.app.onglet_os = 1;
        b.images(2);
        let z = b.ui.zone(sub_id(id("pose-nommee"), 0)).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(a(&b).pose, levee);
        // Renommer, puis garder dans le fichier
        b.app.saisir(Champ::PoseNom);
        b.app.pave.as_mut().unwrap().texte = "Salut".into();
        b.app.valider_pave();
        let relu = Projet::lire(&b.app.projet.ecrire()).unwrap();
        let p = relu.objets[1].armature.as_ref().unwrap().poses_nommees();
        assert_eq!(p[0].0, "Salut");
        assert_eq!(p[0].1.len(), levee.len());
        // Supprimer, puis annuler
        b.images(2);
        let z = b.ui.zone(id("pose-supprimer")).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(a(&b).poses_nommees().len(), 1);
        b.app.annuler();
        assert_eq!(a(&b).poses_nommees().len(), 2);
    }

    #[test]
    fn la_pilule_pose_apparait_avec_une_armature_active() {
        let mut b = Banc::pret();
        b.images(2);
        assert!(b.ui.zone(sub_id(id("barre-haute"), 5)).is_none(), "pas d'armature active");
        let n = b.app.projet.objets.len();
        b.app.agir(A::AjouterArmature);
        b.images(2);
        // Scène → Pose
        mode(&mut b, 5);
        assert!(b.app.os.as_ref().is_some_and(|e| e.pose && e.objet == Some(n)));
        assert!(b.app.contexte().possible(A::TousOs) && !b.app.contexte().possible(A::AjouterOs));
        let a = b.app.projet.objets[n].armature.as_mut().unwrap();
        let o1 = a.os[0].id.clone();
        a.tourner(&[o1], ed::qaxe([0.0, 0.0, 1.0], 0.5));
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.images(2);
        assert!(b.ui.zone(sub_id(id("operation"), A::AjouterOs as usize)).is_none(), "le panneau de pose n'ajoute pas d'os");
        assert!(b.ui.zone(id("os-repos-armature")).is_some(), "sans os choisi, toute l'armature");
        assert!(b.ui.zone(id("os-repos-actif")).is_none() && b.ui.zone(sub_id(id("os-angle"), 0)).is_none());
        // Un os choisi : sa rubrique, ses angles, son seul retour au repos
        let o1 = b.app.projet.objets[n].armature.as_ref().unwrap().os[0].id.clone();
        {
            let e = b.app.os.as_mut().unwrap();
            e.actif = Some(o1.clone());
            e.selectionner(b.app.projet.objets[n].armature.as_ref().unwrap(), std::slice::from_ref(&o1), false);
        }
        b.images(2);
        assert!(b.ui.zone(id("os-repos-armature")).is_none());
        let z = b.ui.zone(id("os-repos-actif")).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.projet.objets[n].armature.as_ref().unwrap().pose.is_empty());
        b.app.annuler();
        assert!(!b.app.projet.objets[n].armature.as_ref().unwrap().pose.is_empty());
        assert!(b.ui.zone(id("os-pose")).is_none() && b.ui.zone(id("os-nom")).is_none() && b.ui.zone(id("os-nouvelle")).is_none());
        // Pose → Géométrie : les os au repos, mêmes armature et panneau
        mode(&mut b, 1);
        assert!(b.app.os.as_ref().is_some_and(|e| !e.pose && e.objet == Some(n)));
        assert!(b.ui.zone(sub_id(id("operation"), A::AjouterOs as usize)).is_some());
        assert!(b.ui.zone(id("os-repos-armature")).is_none() && b.ui.zone(id("os-repos-actif")).is_none());
        // Géométrie → Pose → Scène
        mode(&mut b, 5);
        assert!(b.app.os.as_ref().is_some_and(|e| e.pose));
        mode(&mut b, 0);
        assert!(b.app.os.is_none());
        // Sur un maillage actif, pas de pilule Pose
        b.app.choisir(Some(0));
        b.images(2);
        assert!(b.ui.zone(sub_id(id("barre-haute"), 5)).is_none());
    }

    #[test]
    fn une_armature_au_premier_plan_se_voit_et_se_touche_a_travers_les_objets() {
        let mut b = Banc::pret();
        let n = b.app.projet.objets.len();
        b.app.agir(A::AjouterArmature);
        b.app.projet.objets[0].echelle = [0.1; 3];
        b.app.recadrer();
        b.images(40);
        // Le cube enferme l'armature, l'œil reste dehors
        b.app.projet.objets[0].position = [0.0, 0.5, 0.0];
        b.app.projet.objets[0].echelle = [1.4 / Maillage::cube(1.0).boite_englobante().unwrap().max[1] / 2.0; 3];
        b.images(1);
        b.app.os = Some(EditionOs { objet: Some(n), ..Default::default() });
        let c = milieu_os(&b, "o1");
        b.app.os = None;
        b.app.choix.clear();
        b.app.projet.actif = None;
        b.appui(c.0, c.1);
        assert_eq!(b.app.choix, [0], "caché par le cube");
        // L'interrupteur de Contexte › Objet
        b.app.choix = vec![n];
        b.app.projet.actif = Some(n);
        b.app.contenu = Contenu::Contexte;
        b.app.onglet_contexte = 0;
        b.app.shell.end.open = true;
        b.images(30);
        let z = b.ui.zone(id("armature-devant")).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.projet.objets[n].armature.as_ref().unwrap().devant());
        let s = b.images(1);
        assert_eq!(s.objets.iter().filter(|o| o.devant).count(), 1);
        b.app.choix.clear();
        b.app.projet.actif = None;
        b.app.shell.end.open = false;
        b.images(30);
        b.appui(c.0, c.1);
        assert_eq!(b.app.choix, [n], "touchée à travers le cube");
        // Gardé dans le fichier ; en Géométrie, l'armature redevient ordinaire
        let relu = Projet::lire(&b.app.projet.ecrire()).unwrap();
        assert!(relu.objets[n].armature.as_ref().unwrap().devant());
        b.app.entrer_geometrie();
        assert!(b.images(1).objets.iter().all(|o| !o.devant));
        b.app.sortir_geometrie();
        b.app.annuler();
        assert!(!b.app.projet.objets[n].armature.as_ref().unwrap().devant());
    }

    #[test]
    fn une_armature_cachee_ne_s_ouvre_pas() {
        let mut b = Banc::pret();
        b.app.agir(A::AjouterArmature);
        let k = b.app.choix[0];
        b.app.projet.objets[k].visible = false;
        b.app.entrer_armature();
        assert!(b.app.os.is_none());
        assert_eq!(b.app.message.as_ref().unwrap().0, "Objet caché");
    }

    #[test]
    fn la_pastille_os_ouvre_son_panneau_et_s_eteint_hors_du_mode() {
        // En Scène, le panneau Os s'ouvre ; sur la géométrie d'un maillage, non
        let mut b = Banc::pret();
        let r = b.ui.zone(OS).unwrap();
        b.appui(r.cx(), r.cy());
        assert!(b.app.contenu == Contenu::Os && b.app.shell.end.open);
        b.images(2);
        assert!(b.ui.zone(id("os-nouvelle")).is_some());
        b.app.shell.end.open = false;
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(2);
        b.appui(r.cx(), r.cy());
        assert!(b.app.contenu != Contenu::Os);
        let mut b = en_os(false);
        let r = b.ui.zone(OS).unwrap();
        b.appui(r.cx(), r.cy());
        assert!(!b.app.shell.end.open, "refermé");
        b.appui(r.cx(), r.cy());
        assert!(b.app.contenu == Contenu::Os && b.app.shell.end.open);
        assert!(b.ui.zone(sub_id(id("operation"), A::AjouterOs as usize)).is_some());
        // Contexte reste celui de la sélection, sans les opérations
        b.app.contenu = Contenu::Contexte;
        b.app.onglet_contexte = 0;
        b.images(30);
        assert!(b.ui.zone(sub_id(id("operation"), A::AjouterOs as usize)).is_none());
    }

    #[test]
    fn le_rendu_montre_les_bouts_et_suit_la_selection() {
        let mut b = en_os(false);
        b.app.version_geo = None;
        let s = b.image(0.05);
        let g = s.geo.as_ref().unwrap();
        assert!(g.rayons);
        let m = g.maj.as_ref().unwrap();
        assert_eq!((m.points.len(), m.choisis.len(), m.bandes.len()), (6, 0, 0));
        assert!(b.image(0.05).geo.unwrap().maj.is_none());
        b.app.tous_os(true);
        let s = b.image(0.05);
        let m = s.geo.as_ref().unwrap().maj.as_ref().unwrap();
        assert_eq!((m.choisis.len(), m.bandes.len()), (6, 6 * 7));
        b.app.sortir_geometrie();
        assert!(b.image(0.05).geo.is_none());
    }

    #[test]
    fn une_boite_prend_des_bouts_et_les_retire() {
        let mut b = en_os(false);
        let w = b.app.espace;
        b.app.selection = Selection::Boite;
        let e = point_os(&b, "o1", true);
        let ((x, y), bas) = ((e.0 - 30.0, e.1 - 30.0), (e.0 + 30.0, e.1 + 30.0));
        assert!(w.contains(x, y));
        b.glisser((x, y), bas);
        assert_eq!(b.app.os.as_ref().unwrap().bouts, [Bout::queue("o1")]);
        // La poignée est là : le tracé passe directement
        b.app.boite_retire = true;
        b.app.boite = Some([x, y, bas.0, bas.1]);
        b.app.tracer_boite(w);
        assert!(b.app.os.as_ref().unwrap().bouts.is_empty());
    }

    #[test]
    fn la_derniere_translation_se_refait_sur_les_os() {
        let mut b = en_os(false);
        let e = point_os(&b, "o1", true);
        b.appui(e.0, e.1);
        let q = armature(&b).segments()["o1"].queue;
        b.glisser(e, (e.0 + 100.0, e.1));
        assert!(matches!(b.app.derniere, Some(Derniere::Translation { .. })));
        b.app.rejouer(Derniere::Translation { d: [0.0, 1.0, 0.0] });
        assert_eq!(b.app.os.as_ref().unwrap().bouts, [Bout::queue("o1")]);
        let r = armature(&b).segments()["o1"].queue;
        assert!((r[1] - q[1] - 1.0).abs() < 1e-9 && (r[0] - q[0]).abs() < 1e-9);
    }

    #[test]
    fn un_projet_garde_les_armatures_et_n_ecrit_pas_leur_dessin() {
        let t = r##"{"format":"gda","version":9,"scene":{"objets":[{"nom":"s","position":[1,0,0],"armature":{"os":[
          {"id":"a","nom":"a","parent":null,"position":[0,0,0],"rotation":[0,0,0,1],"longueur":2,"contraintes":{"r":[{"etat":"bloque"},{"etat":"libre"},{"etat":"libre"}],"t":[]},"couleur":"#ff0000"},
          {"id":"b","nom":"b","parent":"a","position":[0,2,0],"rotation":[0,0,0,1],"longueur":1}],"pose":{"b":{"r":[0,0,0,1],"t":[0,0,0]}},"poses":[],"ancre":"a"},
          "maillage":{"v":[],"f":[]}}]},"materiaux":{}}"##;
        let mut b = Banc::pret();
        b.app.remplacer(Projet::lire(t).unwrap(), "");
        b.images(2);
        let o = &b.app.projet.objets[0];
        assert_eq!(o.maillage.faces.len(), 16);
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.app.tous_os(true);
        b.app.poser_selection(Transfo::Translation([0.0, 1.0, 0.0]));
        b.app.sortir_geometrie();
        let ecrit = crate::json::lire(&b.app.projet.ecrire()).unwrap();
        let o = &ecrit.get("scene").unwrap().get("objets").unwrap().liste().unwrap()[0];
        assert_eq!(o.get("maillage").unwrap().get("f").unwrap().liste().unwrap().len(), 0);
        let a = o.get("armature").unwrap();
        assert_eq!(a.get("ancre").unwrap().txt(), Some("a"));
        assert!(a.get("pose").unwrap().get("b").is_some());
        let os = &a.get("os").unwrap().liste().unwrap()[0];
        assert_eq!(os.get("couleur").unwrap().txt(), Some("#ff0000"));
        assert!(os.get("contraintes").is_some());
        assert_eq!(os.get("position").unwrap().nombres::<3>(), Some([0.0, 1.0, 0.0]));
        let relu = Projet::lire(&b.app.projet.ecrire()).unwrap();
        assert_eq!(relu.objets[0].armature, b.app.projet.objets[0].armature);
        assert_eq!(relu.objets[0].maillage.faces.len(), 16);
    }

    #[test]
    fn un_export_ne_contient_pas_les_armatures() {
        let mut b = Banc::pret();
        let f = b.app.liste_export(false).len();
        b.app.agir(A::AjouterHumanoide);
        assert_eq!(b.app.liste_export(false).len(), f);
    }

    /// Un cube choisi et un humanoïde.
    fn banc_lie() -> Banc {
        let mut b = Banc::pret();
        b.app.ajouter_armature("corps", Armature::humanoide());
        b.app.choisir(Some(0));
        b
    }

    fn liaison_de(b: &Banc, k: usize) -> Option<crate::projet::Liaison> {
        b.app.projet.objets[k].liaison.clone()
    }

    #[test]
    fn la_page_propose_la_seule_armature_et_tous_ses_os() {
        let mut b = banc_lie();
        let n = b.app.projet.objets[1].armature.as_ref().unwrap().os.len();
        assert!(b.app.contexte().possible(A::LierArmature) && !b.app.contexte().possible(A::Delier));
        b.app.agir(A::LierArmature);
        b.images(2);
        assert!(b.app.shell.end.open && b.app.contenu == Contenu::Contexte);
        let page = b.app.liaison.clone().unwrap();
        assert_eq!(page.armature.as_ref(), Some(&b.app.projet.objets[1].id));
        assert_eq!(page.os.len(), n);
        b.app.lier();
        assert!(b.app.liaison.is_none());
        assert_eq!(liaison_de(&b, 0).unwrap().os.len(), n);
        assert!(b.app.contexte().possible(A::Delier));
        // Refaire l'opération reprend le groupe en place
        let un = b.app.projet.objets[1].armature.as_ref().unwrap().os[3].id.clone();
        b.app.projet.objets[0].liaison.as_mut().unwrap().os = vec![un.clone()];
        b.app.agir(A::LierArmature);
        assert_eq!(b.app.liaison.as_ref().unwrap().os, [un]);
    }

    #[test]
    fn lier_et_delier_s_annulent() {
        let mut b = banc_lie();
        b.app.agir(A::LierArmature);
        b.app.lier();
        assert!(liaison_de(&b, 0).is_some());
        b.app.annuler();
        assert!(liaison_de(&b, 0).is_none());
        b.app.retablir();
        assert!(liaison_de(&b, 0).is_some());
        b.app.agir(A::Delier);
        assert!(liaison_de(&b, 0).is_none());
        b.app.annuler();
        assert!(liaison_de(&b, 0).is_some());
    }

    #[test]
    fn sans_armature_ni_objet_a_lier_l_operation_s_efface() {
        let mut b = Banc::pret();
        b.app.choisir(Some(0));
        assert!(!b.app.contexte().possible(A::LierArmature));
        let mut b = banc_lie();
        b.app.choisir(Some(1));
        assert!(!b.app.contexte().possible(A::LierArmature));
        b.app.entrer_armature();
        assert!(!b.app.contexte().possible(A::LierArmature));
    }

    #[test]
    fn la_selection_decide_de_ce_qui_est_lie() {
        let mut b = banc_lie();
        let c = b.app.projet.objets[0].clone();
        b.app.projet.objets.push(c);
        b.app.projet.objets[2].id = "deux".into();
        b.app.choix = vec![0, 1, 2];
        b.app.agir(A::LierArmature);
        // L'armature choisie est retenue, et elle-même n'est pas liée
        assert_eq!(b.app.liaison.as_ref().unwrap().armature.as_ref(), Some(&b.app.projet.objets[1].id));
        b.app.lier();
        assert!(liaison_de(&b, 0).is_some() && liaison_de(&b, 2).is_some() && liaison_de(&b, 1).is_none());
    }

    #[test]
    fn supprimer_un_os_ou_l_armature_rend_la_liaison_coherente() {
        let mut b = banc_lie();
        let a = b.app.projet.objets[1].clone();
        let (i, j) = (a.armature.as_ref().unwrap().os[1].id.clone(), a.armature.as_ref().unwrap().os[2].id.clone());
        b.app.projet.objets[0].liaison = Some(crate::projet::Liaison { armature: a.id.clone(), os: vec![i.clone(), j.clone()] });
        b.app.choisir(Some(1));
        b.app.entrer_armature();
        let (x, y) = (Bout::tete(&i), Bout::queue(&i));
        b.app.os.as_mut().unwrap().bouts = vec![x, y].into_iter().collect();
        b.app.supprimer_os();
        assert_eq!(liaison_de(&b, 0).unwrap().os, [j.clone()]);
        b.app.annuler();
        assert_eq!(liaison_de(&b, 0).unwrap().os.len(), 2);
        b.app.sortir_geometrie();
        b.app.choisir(Some(1));
        b.app.agir(A::Supprimer);
        assert!(liaison_de(&b, 0).is_none());
        b.app.annuler();
        assert!(liaison_de(&b, 0).is_some());
    }

    #[test]
    fn l_armature_emporte_ses_objets() {
        let mut b = banc_lie();
        b.app.agir(A::LierArmature);
        b.app.lier();
        b.app.projet.objets[0].position = [2.0, 1.0, 0.0];
        let p = b.app.projet.objets[1].position;
        let rel = [2.0 - p[0], 1.0 - p[1], 0.0 - p[2]];
        b.app.choisir(Some(1));
        b.app.poser_selection(Transfo::Translation([1.0, 0.0, 0.0]));
        assert_eq!(b.app.projet.objets[0].position, [3.0, 1.0, 0.0]);
        // Un quart de tour autour de l'armature, puis une échelle double
        let c = b.app.projet.objets[1].position;
        let q = ed::qaxe([0.0, 1.0, 0.0], std::f64::consts::FRAC_PI_2);
        b.app.poser_selection(Transfo::Rotation { q, c });
        b.app.poser_selection(Transfo::Echelle { s: [2.0; 3], c });
        let o = &b.app.projet.objets[0];
        let attendu = ed::qappliquer(q, rel.map(|v| v * 2.0));
        for k in 0..3 {
            assert!((o.position[k] - c[k] - attendu[k]).abs() < 1e-9, "{:?}", o.position);
        }
        assert_eq!(o.echelle, [2.0; 3]);
        assert!(o.rotation.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-9));
    }

    #[test]
    fn un_objet_choisi_avec_son_armature_ne_bouge_qu_une_fois() {
        let mut b = banc_lie();
        b.app.agir(A::LierArmature);
        b.app.lier();
        b.app.choix = vec![0, 1];
        let p = b.app.projet.objets[0].position;
        b.app.poser_selection(Transfo::Translation([1.0, 0.0, 0.0]));
        assert_eq!(b.app.projet.objets[0].position[0], p[0] + 1.0);
        // Un objet déplacé seul n'emporte pas son armature
        let a = b.app.projet.objets[1].position;
        b.app.choisir(Some(0));
        b.app.poser_selection(Transfo::Translation([1.0, 0.0, 0.0]));
        assert_eq!(b.app.projet.objets[1].position, a);
    }

    #[test]
    fn une_saisie_chiffree_emporte_aussi_les_objets() {
        let mut b = banc_lie();
        b.app.agir(A::LierArmature);
        b.app.lier();
        b.app.choisir(Some(1));
        let d = b.app.base(1)[0];
        b.app.appliquer_champ(Champ::Dimension(0), d * 3.0);
        assert!(b.app.projet.objets[0].echelle.iter().all(|e| (e - 3.0).abs() < 1e-9));
    }

    #[test]
    fn dupliquer_donne_un_nouvel_identifiant_et_suit_l_armature_copiee() {
        let mut b = banc_lie();
        b.app.agir(A::LierArmature);
        b.app.lier();
        // L'objet seul reste sur l'originale
        b.app.agir(A::Dupliquer);
        let p = &b.app.projet;
        assert_ne!(p.objets[2].id, p.objets[0].id);
        assert_eq!(p.objets[2].liaison.as_ref().unwrap().armature, p.objets[1].id);
        // Copiés ensemble, ils restent ensemble
        b.app.choix = vec![0, 1];
        b.app.agir(A::Dupliquer);
        let p = &b.app.projet;
        assert_eq!(p.objets[3].liaison.as_ref().unwrap().armature, p.objets[4].id);
        assert_eq!(p.objets[0].liaison.as_ref().unwrap().armature, p.objets[1].id);
    }

    /// Un cube lié au seul bras gauche d'un humanoïde, ce dernier ouvert en pose et choisi.
    fn banc_pose() -> (Banc, String) {
        let mut b = banc_lie();
        let a = b.app.projet.objets[1].clone();
        let bras = a.armature.as_ref().unwrap().os.iter().find(|x| x.nom == "bras.G").unwrap().id.clone();
        b.app.projet.objets[0].liaison = Some(crate::projet::Liaison { armature: a.id, os: vec![bras.clone()] });
        b.app.projet.objets[0].position = [1.0, 1.4, 0.0];
        b.app.choisir(Some(1));
        b.app.entrer_armature();
        b.app.basculer_pose();
        let e = b.app.os.as_mut().unwrap();
        let x = b.app.projet.objets[1].armature.as_ref().unwrap();
        e.selectionner(x, std::slice::from_ref(&bras), false);
        (b, bras)
    }

    fn tourner_bras(b: &mut Banc, q: ed::Q) {
        b.app.memoriser();
        b.app.poser_selection(Transfo::Rotation { q, c: [0.0; 3] });
    }

    #[test]
    fn les_limites_se_reglent_en_geometrie_et_bornent_la_pose() {
        let (mut b, bras) = banc_pose();
        let limites = |b: &Banc| b.app.projet.objets[1].armature.as_ref().unwrap().trouver(&bras).unwrap().limites();
        // En Géométrie, repliées derrière un lien sous la longueur
        b.app.basculer_pose();
        let x = b.app.projet.objets[1].armature.clone().unwrap();
        let e = b.app.os.as_mut().unwrap();
        e.selectionner(&x, std::slice::from_ref(&bras), false);
        e.actif = Some(bras.clone());
        b.app.os_plies.insert(format!("a:{}", b.app.projet.objets[1].id));
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.images(2);
        assert!(b.ui.zone(sub_id(sub_id(id("os-limite"), 0), 0)).is_none());
        let z = b.ui.zone(id("os-limites")).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
        // L'état passe de Libre à Limité, puis à Bloqué
        let z = b.ui.zone(sub_id(sub_id(id("os-limite"), 2), 0)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
        assert_eq!(limites(&b)[2], Limite::Bornee(-45.0, 45.0));
        assert!(b.ui.zone(sub_id(sub_id(id("os-limite"), 2), 1)).is_some());
        // Les bornes au pavé ; un minimum qui passe le maximum l'emmène
        b.app.saisir(Champ::OsLimite(2, true));
        b.app.appliquer_champ(Champ::OsLimite(2, true), 30.0);
        b.app.saisir(Champ::OsLimite(2, false));
        b.app.appliquer_champ(Champ::OsLimite(2, false), -20.0);
        b.app.pave = None;
        assert_eq!(limites(&b)[2], Limite::Bornee(-20.0, 30.0));
        b.app.annuler();
        assert_eq!(limites(&b)[2], Limite::Bornee(-45.0, 30.0));
        b.app.changer_etat_limite(0);
        b.app.changer_etat_limite(0);
        assert_eq!(limites(&b)[0], Limite::Bloquee);
        // En pose : les curseurs et le geste butent
        b.app.basculer_pose();
        // L'axe bloqué n'a plus d'anneau à saisir
        let w = b.app.espace;
        let vue = b.app.vue_ecran(w);
        let p = b.app.poignee(&vue, 1.0).unwrap();
        assert!(!p.axes[0] && p.axes[2]);
        b.app.regler_angles(&bras, [25.0, 10.0, 60.0]);
        b.app.angle_saisie = false;
        let v = angles_os(b.app.projet.objets[1].armature.as_ref().unwrap().pose[&bras].r);
        assert!(v.iter().zip([0.0, 10.0, 30.0]).all(|(x, y)| (x - y).abs() < 1e-6), "{v:?}");
        let o = &b.app.projet.objets[1];
        let (_, q) = o.armature.as_ref().unwrap().matrices_pose()[&bras];
        let z = ed::qappliquer(ed::qnorm(ed::qmul(o.rotation, q)), [0.0, 0.0, 1.0]);
        tourner_bras(&mut b, ed::qaxe(z, 1.0));
        let v = angles_os(b.app.projet.objets[1].armature.as_ref().unwrap().pose[&bras].r);
        assert!((v[2] - 30.0).abs() < 1e-6 && v[0].abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn les_angles_d_un_os_vont_et_reviennent() {
        for d in [[0.0, 0.0, 0.0], [30.0, -45.0, 60.0], [-170.0, 120.0, 10.0], [90.0, 0.0, -89.0]] {
            let a = angles_os(depuis_angles_os(d));
            assert!(a.iter().zip(d).all(|(x, y)| (x - y).abs() < 1e-6), "{d:?} → {a:?}");
        }
        // La vrille garde la direction de l'os
        let dir = |d| ed::qappliquer(depuis_angles_os(d), [0.0, 1.0, 0.0]);
        let (u, v) = (dir([20.0, 0.0, 50.0]), dir([20.0, 80.0, 50.0]));
        assert!((0..3).all(|k| (u[k] - v[k]).abs() < 1e-9));
    }

    #[test]
    fn en_pose_les_curseurs_et_la_poignee_suivent_le_repere_de_l_os() {
        let (mut b, bras) = banc_pose();
        b.app.os.as_mut().unwrap().actif = Some(bras.clone());
        let segment = |b: &Banc| b.app.projet.objets[1].armature.as_ref().unwrap().segments_pose()[&bras];
        let dir = |s: crate::armature::Segment| {
            let d = [0, 1, 2].map(|k| s.queue[k] - s.tete[k]);
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            d.map(|v| v / l)
        };
        let proche = |u: P3, v: P3| (0..3).all(|k| (u[k] - v[k]).abs() < 1e-6);
        let avant = segment(&b);
        // Les curseurs : angles depuis le repos, autour des axes de l'os
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.images(2);
        assert!((0..3).all(|k| b.ui.zone(sub_id(id("os-angle"), k)).is_some()));
        b.app.regler_angles(&bras, [0.0, 0.0, 60.0]);
        b.app.angle_saisie = false;
        let e = b.app.projet.objets[1].armature.as_ref().unwrap().pose[&bras];
        assert!(angles_os(e.r).iter().zip([0.0, 0.0, 60.0]).all(|(a, b)| (a - b).abs() < 1e-6));
        let apres = segment(&b);
        assert!(proche(apres.tete, avant.tete) && !proche(dir(apres), dir(avant)), "l'os tourne autour de sa tête");
        // Autour de son propre axe Y, l'os ne fait que vriller
        b.app.regler_angles(&bras, [0.0, 45.0, 60.0]);
        b.app.angle_saisie = false;
        assert!(proche(dir(segment(&b)), dir(apres)));
        // La poignée prend le repère de l'os posé : son axe Y suit l'os
        let w = b.app.espace;
        let vue = b.app.vue_ecran(w);
        let p = b.app.poignee(&vue, 1.0).unwrap();
        let o = &b.app.projet.objets[1];
        let monde = |v: P3| ed::qappliquer(ed::qnorm(o.rotation), v);
        assert!(proche(p.axe(1), monde(dir(segment(&b)))));
        let mut g = Geste::new(Mode::Rotation, Prise::Axe(1), 0.0, 0.0, vue, [true; 3], Vec::new(), Vec::new(), p.o, p.o);
        g.orienter(p.repere);
        assert!(proche(g.rotation().0, p.axe(1)));
        // Le geste autour de cet axe vrille l'os sans le déplacer
        let d0 = dir(segment(&b));
        b.app.memoriser();
        b.app.poser_selection(Transfo::Rotation { q: ed::qaxe(p.axe(1), 0.7), c: p.o });
        assert!(proche(dir(segment(&b)), d0));
        // Au repos, les axes du monde
        b.app.basculer_pose();
        assert_eq!(b.app.poignee(&vue, 1.0).unwrap().repere, [0.0, 0.0, 0.0, 1.0]);
        // L'annulation reprend les angles
        b.app.basculer_pose();
        b.app.annuler();
        b.app.annuler();
        b.app.annuler();
        assert!(proche(dir(segment(&b)), dir(avant)));
    }

    #[test]
    fn le_panneau_os_regle_l_armature_en_mode_scene() {
        let (mut b, bras) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        b.app.os.as_mut().unwrap().actif = Some(bras.clone());
        b.app.sortir_geometrie();
        assert!(b.app.os.is_none() && b.app.projet.actif == Some(1));
        b.app.contenu = Contenu::Os;
        b.app.shell.end.open = true;
        b.images(2);
        // L'os actif de l'édition reste choisi dans l'arbre
        assert_eq!(b.app.os_scene.as_ref().map(|x| x.1.as_str()), Some(bras.as_str()));
        assert!(b.ui.zone(id("os-armature-nom")).is_some());
        // Un autre os de l'arbre devient l'os actif de la vue des poids
        let z = b.ui.zone(sub_id(id("os-rang"), 0)).unwrap();
        b.appui(z.cx(), z.cy());
        let racine = b.app.projet.objets[1].armature.as_ref().unwrap().ordre()[0].id.clone();
        assert_eq!(b.app.os_vu().unwrap().1, Some(racine.clone()));
        // Tout au repos, puis annulation
        assert!(!b.app.projet.objets[1].armature.as_ref().unwrap().pose.is_empty());
        let z = b.ui.zone(id("os-repos-armature")).unwrap();
        b.appui(z.cx(), z.cy());
        assert!(b.app.projet.objets[1].armature.as_ref().unwrap().pose.is_empty());
        b.app.annuler();
        assert!(!b.app.projet.objets[1].armature.as_ref().unwrap().pose.is_empty());
        // Le chevron replie les os ; Éditer les os remonte sous la liste
        b.images(2);
        assert!(b.ui.zone(sub_id(id("os-rang"), 0)).is_some());
        let z = b.ui.zone(sub_id(id("os-chevron"), 1)).unwrap();
        b.appui(z.cx(), z.cy());
        b.images(2);
        assert!(b.ui.zone(sub_id(id("os-rang"), 0)).is_none());
        // Éditer les os : retour à l'édition, sur l'os choisi en Scène
        let z = b.ui.zone(id("os-editer")).unwrap();
        b.appui(z.cx(), z.cy());
        assert_eq!(b.app.os.as_ref().unwrap().actif, Some(racine));
        // En Géométrie sur un maillage, la pastille Os s'éteint
        b.app.sortir_geometrie();
        b.app.choisir(Some(0));
        b.app.entrer_geometrie();
        b.images(2);
        assert!(b.app.travail.is_some() && b.app.contenu != Contenu::Os);
    }

    #[test]
    fn un_objet_lie_a_un_os_le_suit_sans_changer_ses_donnees() {
        let (mut b, bras) = banc_pose();
        let q = ed::qaxe([0.0, 0.0, 1.0], 0.8);
        let tete = b.app.projet.objets[1].armature.as_ref().unwrap().segments()[&bras].tete;
        tourner_bras(&mut b, q);
        // Tourne autour de la tête du bras
        let (position, rotation) = b.app.place_posee(0).unwrap();
        let r = ed::qappliquer(q, [1.0 - tete[0], 1.4 - tete[1], -tete[2]]);
        for k in 0..3 {
            assert!((position[k] - tete[k] - r[k]).abs() < 1e-9, "{position:?}");
        }
        assert!(rotation.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-9));
        assert_eq!(b.app.projet.objets[0].position, [1.0, 1.4, 0.0]);
    }

    #[test]
    fn le_suivi_tient_compte_de_la_pose_de_l_armature_dans_la_scene() {
        let (mut b, bras) = banc_pose();
        let q = ed::qaxe([0.0, 0.0, 1.0], 0.8);
        tourner_bras(&mut b, q);
        b.app.sortir_geometrie();
        let avant = b.app.place_posee(0).unwrap();
        // Armature déplacée et tournée : l'objet suit, comme un point de l'os
        b.app.choisir(Some(1));
        let (t, r) = ([1.0, 2.0, 3.0], ed::qaxe([0.0, 1.0, 0.0], 1.1));
        b.app.poser_selection(Transfo::Translation(t));
        let c = b.app.projet.objets[1].position;
        b.app.poser_selection(Transfo::Rotation { q: r, c });
        b.app.poser_selection(Transfo::Echelle { s: [2.0; 3], c });
        let (p, q2) = b.app.place_posee(0).unwrap();
        let p0 = [0, 1, 2].map(|k| avant.0[k] + t[k] - c[k]);
        let attendu = ed::qappliquer(r, p0.map(|v| v * 2.0));
        for k in 0..3 {
            assert!((p[k] - c[k] - attendu[k]).abs() < 1e-9, "{p:?}");
        }
        let voulu = ed::qmul(r, avant.1);
        assert!(q2.iter().zip(voulu).all(|(a, b)| (a - b).abs() < 1e-9));
        let _ = bras;
    }

    #[test]
    fn plusieurs_os_laissent_l_objet_au_repos_pour_l_instant() {
        let (mut b, bras) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        let autre = b.app.projet.objets[1].armature.as_ref().unwrap().os[0].id.clone();
        b.app.projet.objets[0].liaison.as_mut().unwrap().os = vec![bras, autre];
        assert!(b.app.place_posee(0).is_none());
    }

    #[test]
    fn au_repos_l_armature_ouverte_et_ses_objets_reviennent_puis_repartent() {
        let (mut b, _) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        let enveloppe = |b: &Banc| b.app.projet.objets[1].maillage.sommets.clone();
        let posee = enveloppe(&b);
        assert!(b.app.place_posee(0).is_some());
        b.app.basculer_pose();
        assert_ne!(enveloppe(&b), posee);
        assert!(b.app.place_posee(0).is_none());
        b.app.basculer_pose();
        assert_eq!(enveloppe(&b), posee);
        // Hors du mode, la pose se voit
        b.app.basculer_pose();
        b.app.sortir_geometrie();
        assert_eq!(enveloppe(&b), posee);
        assert!(b.app.place_posee(0).is_some());
    }

    #[test]
    fn la_pose_s_annule_et_reste_dans_le_fichier() {
        let (mut b, bras) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        let t = b.app.projet.ecrire();
        b.app.annuler();
        assert!(b.app.projet.objets[1].armature.as_ref().unwrap().pose.is_empty());
        assert!(b.app.place_posee(0).is_none());
        assert!(b.app.os.as_ref().unwrap().pose, "on reste en pose");
        b.app.retablir();
        assert!(b.app.place_posee(0).is_some());
        let p = Projet::lire(&t).unwrap();
        assert!(p.objets[1].armature.as_ref().unwrap().pose.contains_key(&bras));
        assert!(p.place_posee(&p.objets[0]).is_some());
        assert_eq!(*p.objets[1].maillage, p.objets[1].armature.as_ref().unwrap().enveloppe_pose());
    }

    #[test]
    fn remettre_au_repos_les_os_choisis_ou_tous() {
        let (mut b, _) = banc_pose();
        let q = ed::qaxe([0.0, 0.0, 1.0], 0.8);
        tourner_bras(&mut b, q);
        assert!(b.app.contexte().possible(A::ReposOs));
        b.app.agir(A::ReposOs);
        assert!(b.app.projet.objets[1].armature.as_ref().unwrap().pose.is_empty());
        assert!(!b.app.contexte().possible(A::ReposOs));
        assert_eq!(*b.app.projet.objets[1].maillage, b.app.projet.objets[1].armature.as_ref().unwrap().enveloppe());
        b.app.annuler();
        b.app.tous_os(false);
        b.app.agir(A::ReposOs);
        assert!(b.app.place_posee(0).is_none(), "aucun choisi : tous");
    }

    #[test]
    fn en_pose_on_ne_change_ni_la_hierarchie_ni_le_mode() {
        let (mut b, _) = banc_pose();
        let c = b.app.contexte();
        assert!(c.pose && !c.possible(A::AjouterOs) && !c.possible(A::SupprimerOs) && !c.possible(A::ExtruderOs) && !c.possible(A::ParenterOs) && !c.possible(A::SelectionCurseur));
        assert!(c.possible(A::TousOs) && c.possible(A::AucunOs));
        b.app.mode = Mode::Translation;
        b.images(1);
        assert!(b.app.mode == Mode::Rotation);
        b.app.contenu = Contenu::Contexte;
        b.images(2);
        b.app.contenu = Contenu::Os;
        b.images(2);
    }

    #[test]
    fn en_pose_une_translation_ne_fait_rien_et_les_points_sont_les_tetes() {
        let (mut b, bras) = banc_pose();
        let p = b.app.projet.objets[1].armature.as_ref().unwrap().pose.clone();
        b.app.poser_selection(Transfo::Translation([1.0, 0.0, 0.0]));
        assert_eq!(b.app.projet.objets[1].armature.as_ref().unwrap().pose, p);
        let t = b.app.projet.objets[1].armature.as_ref().unwrap().segments()[&bras].tete;
        let o = b.app.projet.objets[1].matrice();
        assert_eq!(b.app.points(), vec![vue::appliquer(&o, t)]);
    }

    #[test]
    fn un_objet_pose_ne_se_deplace_pas() {
        let (mut b, _) = banc_pose();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.8));
        b.app.sortir_geometrie();
        b.app.choisir(Some(0));
        b.app.appliquer_champ(Champ::Position(0), 9.0);
        assert_eq!(b.app.projet.objets[0].position, [1.0, 1.4, 0.0]);
        // La boîte et le centre de l'objet suivent la pose
        let (p, _) = b.app.place_posee(0).unwrap();
        assert_eq!(b.app.points(), vec![p]);
    }

    #[test]
    fn toucher_un_os_en_pose_puis_glisser_le_tourne_et_le_lacher_garde() {
        let mut b = en_os(true);
        b.app.basculer_pose();
        b.images(1);
        let id = armature(&b).os.iter().find(|x| x.nom == "cuisse.G").unwrap().id.clone();
        let c = milieu_os(&b, &id);
        b.appui(c.0, c.1);
        assert_eq!(b.app.os.as_ref().unwrap().os_choisis(armature(&b)), [id.clone()]);
        let repos = armature(&b).segments();
        assert_eq!(b.glisser(c, (c.0 + 120.0, c.1 + 40.0)), 1);
        let a = armature(&b);
        let p = a.segments_pose();
        assert!(a.pose.contains_key(&id));
        assert!((0..3).all(|k| (p[&id].tete[k] - repos[&id].tete[k]).abs() < 1e-9), "la tête reste");
        assert!((0..3).any(|k| (p[&id].queue[k] - repos[&id].queue[k]).abs() > 1e-3), "la queue tourne");
        assert_eq!(a.segments(), repos, "le repos ne change pas");
        b.app.annuler();
        assert!(armature(&b).pose.is_empty());
    }

    /// Un tube de l'épaule à la main, lié au bras et à l'avant-bras gauches, armature en pose.
    fn banc_peau() -> (Banc, String, String) {
        let mut b = banc_lie();
        let x = b.app.projet.objets[1].clone();
        let a = x.armature.as_ref().unwrap();
        let i = |n: &str| a.os.iter().find(|o| o.nom == n).unwrap().id.clone();
        let (bras, avant) = (i("bras.G"), i("avant-bras.G"));
        let s = a.segments();
        let (d, f) = (s[&bras].tete, s[&avant].queue);
        let (anneaux, cotes) = (24, 8);
        let mut sommets = Vec::new();
        for r in 0..=anneaux {
            let t = r as f64 / anneaux as f64;
            for c in 0..cotes {
                let an = c as f64 / cotes as f64 * std::f64::consts::TAU;
                sommets.push([d[0] + (f[0] - d[0]) * t, d[1] + (f[1] - d[1]) * t + 0.05 * an.cos(), d[2] + (f[2] - d[2]) * t + 0.05 * an.sin()]);
            }
        }
        let mut faces = Vec::new();
        for r in 0..anneaux {
            for c in 0..cotes {
                let n = (c + 1) % cotes;
                faces.push(maillage::Face { s: vec![r * cotes + c, r * cotes + n, (r + 1) * cotes + n, (r + 1) * cotes + c], m: maillage::defaut(), uv: Vec::new() });
            }
        }
        b.app.projet.objets[0].maillage = Arc::new(Maillage::new(sommets, faces, Vec::new()));
        b.app.projet.objets[0].liaison = Some(crate::projet::Liaison { armature: x.id.clone(), os: vec![bras.clone(), avant.clone()] });
        b.app.choisir(Some(1));
        b.app.entrer_armature();
        b.app.basculer_pose();
        let e = b.app.os.as_mut().unwrap();
        let a = b.app.projet.objets[1].armature.as_ref().unwrap();
        e.actif = Some(avant.clone());
        e.selectionner(a, std::slice::from_ref(&avant), false);
        (b, bras, avant)
    }

    fn pret(b: &mut Banc) {
        b.images(1);
        b.app.attendre_calculs();
        b.images(1);
    }

    #[test]
    fn un_groupe_deforme_l_objet_sans_toucher_ses_donnees() {
        let (mut b, _, _) = banc_peau();
        let base = b.app.projet.objets[0].maillage.clone();
        pret(&mut b);
        assert!(Arc::ptr_eq(&b.app.maillage_vu(0), &base), "sans pose, le maillage de base");
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 1.2));
        pret(&mut b);
        let vu = b.app.maillage_vu(0);
        assert!(!Arc::ptr_eq(&vu, &base));
        let bouge = |i: usize| (0..3).map(|c| (vu.sommets[i][c] - base.sommets[i][c]).abs()).fold(0.0, f64::max);
        let n = base.sommets.len();
        assert!(bouge(0) < 1e-6, "l'épaule reste : {}", bouge(0));
        assert!(bouge(n - 1) > 0.1, "la main suit : {}", bouge(n - 1));
        assert_eq!(vu.faces.len(), base.faces.len());
        assert!(Arc::ptr_eq(&b.app.projet.objets[0].maillage, &base), "le maillage enregistré ne change pas");
        // Au repos pour l'édition, puis de nouveau en pose
        b.app.basculer_pose();
        pret(&mut b);
        assert!(Arc::ptr_eq(&b.app.maillage_vu(0), &base));
        b.app.basculer_pose();
        pret(&mut b);
        assert!(!Arc::ptr_eq(&b.app.maillage_vu(0), &base));
    }

    #[test]
    fn la_peau_ne_se_recalcule_que_si_le_maillage_ou_les_os_changent() {
        let (mut b, _, _) = banc_peau();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 1.2));
        pret(&mut b);
        let l = b.app.peaux[&b.app.projet.objets[0].id].clone();
        // Une autre pose, ou l'armature emportant son objet : même peau
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 0.3));
        b.app.sortir_geometrie();
        b.app.choisir(Some(1));
        b.app.poser_selection(Transfo::Translation([1.0, 2.0, 0.0]));
        pret(&mut b);
        assert!(Arc::ptr_eq(&l, &b.app.peaux[&b.app.projet.objets[0].id]));
        // Le maillage change : nouvelle peau
        let mut m = (*b.app.projet.objets[0].maillage).clone();
        m.sommets[0][1] += 0.01;
        b.app.projet.objets[0].maillage = Arc::new(m);
        pret(&mut b);
        assert!(!Arc::ptr_eq(&l, &b.app.peaux[&b.app.projet.objets[0].id]));
    }

    #[test]
    fn un_objet_deforme_ne_se_deplace_pas_et_s_exporte_au_repos() {
        let (mut b, _, _) = banc_peau();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 1.2));
        pret(&mut b);
        b.app.sortir_geometrie();
        pret(&mut b);
        b.app.choisir(Some(0));
        let p = b.app.projet.objets[0].position;
        b.app.appliquer_champ(Champ::Position(0), 9.0);
        assert_eq!(b.app.projet.objets[0].position, p);
        let l = b.app.liste_export(true);
        assert_eq!(l[0].1.sommets, b.app.projet.objets[0].maillage.sommets);
    }

    #[test]
    fn annuler_la_pose_rend_le_maillage_de_base() {
        let (mut b, _, _) = banc_peau();
        tourner_bras(&mut b, ed::qaxe([0.0, 0.0, 1.0], 1.2));
        pret(&mut b);
        b.app.annuler();
        pret(&mut b);
        assert!(Arc::ptr_eq(&b.app.maillage_vu(0), &b.app.projet.objets[0].maillage));
    }

    #[test]
    fn les_poids_de_l_os_actif_se_lisent_en_couleur() {
        let (mut b, bras, avant) = banc_peau();
        b.app.voir_poids = true;
        pret(&mut b);
        let vu = b.app.maillage_vu(0);
        let v = b.app.vue_poids(0, &vu).unwrap();
        let u = |m: &Maillage, f: usize| m.faces[f].uv[0][0];
        assert!(v.faces[0].uv.len() == 4 && u(&v, 0) < 0.1, "l'épaule ne dépend pas de l'avant-bras");
        assert!(u(&v, v.faces.len() - 1) > 0.9, "la main en dépend entièrement");
        // Même résultat tant que rien ne change ; l'os actif change la vue
        assert!(Arc::ptr_eq(&v, &b.app.vue_poids(0, &vu).unwrap()));
        b.app.os.as_mut().unwrap().actif = Some(bras);
        let w = b.app.vue_poids(0, &vu).unwrap();
        assert!(u(&w, 0) > 0.9 && u(&w, w.faces.len() - 1) < 0.1);
        // Les deux poids se complètent
        let (p, q) = (&v.faces[40].uv[0][0], &w.faces[40].uv[0][0]);
        assert!((p + q - 1.0).abs() < 0.03, "{p} + {q}");
        let s = b.images(1);
        assert!(s.objets.iter().any(|o| o.texture.as_deref() == Some(texture::RAMPE)));
        let _ = avant;
        // En Scène, l'armature active garde la vue des poids et son os actif
        let actif = b.app.os.as_ref().unwrap().actif.clone().unwrap();
        b.app.sortir_geometrie();
        assert!(b.app.voir_poids);
        let a = b.app.projet.actif.unwrap();
        assert!(b.app.projet.objets[a].armature.is_some());
        assert_eq!(b.app.os_vu().unwrap().1, Some(actif));
        assert!(b.app.vue_poids(0, &vu).is_some());
        // Sur un autre objet actif, plus rien
        b.app.projet.actif = Some(0);
        assert!(b.app.vue_poids(0, &vu).is_none());
    }

    #[test]
    fn un_objet_lie_a_un_seul_os_prend_tout_ou_rien() {
        let (mut b, bras) = banc_pose();
        b.app.voir_poids = true;
        let vu = b.app.maillage_vu(0);
        b.app.os.as_mut().unwrap().actif = Some(bras);
        assert!(b.app.vue_poids(0, &vu).unwrap().faces[0].uv[0][0] > 0.9);
        b.app.os.as_mut().unwrap().actif = Some("autre".into());
        assert!(b.app.vue_poids(0, &vu).unwrap().faces[0].uv[0][0] < 0.1);
    }
}
