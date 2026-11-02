//! Contexte d'édition 3D : la scène d'un projet, sa caméra, ses panneaux.
//!
//! Modes Scène et Géométrie : poignées, transformations, annulation, opérations,
//! enregistrement et export. Les calculs sont dans `edition` et `geometrie`.

use crate::theme::{self, coul, C};
use crate::icones::{self, Icone};
use crate::draw::{Color, Rect};
use crate::actions::{self, Action as A, Choix, Contexte, Groupe, Menu, CATALOGUE, GROUPES};
use crate::edition::{self as ed, Etape, Geste, Historique, Mode, Pas, Pivot, Poignee, Prise, Transfo, Vue, PIVOTS};
use crate::geometrie::{self as geo, Bilan, El, RegleRevolution, Travail, T};
use crate::json::J;
use crate::maillage::{self, Arete, Boite, Cible, Maillage, P3};
use crate::math::{self, Mat4};
use crate::modifs;
use crate::primitives::{self, Genre, Prim, Valeurs, V};
use crate::projet::{Materiau, Objet, Projet};
use crate::shell::{Layout, Shell, Side};
use crate::texture::{self, Image};
use crate::ui::{id, sub_id, Id, Sense};
use crate::vue::{self, Tampons};
use crate::widgets::{Cx, Icon, Scrolls};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

const OBJETS: Id = id("objets");
const BARRE_RAPIDE: Id = id("barre-rapide");
const NOM_PROJET: Id = id("nom-projet");
const FICHIERS: Id = id("pastille-fichier");
const SCENE: Id = id("pastille-scene");
const CONTEXTE: Id = id("pastille-contexte");
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
const ONGLET_COULEURS: usize = 0;
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
}

/// Fil, points et arêtes choisies de la copie de travail.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeoTampons {
    pub lignes: Vec<f32>,
    pub points: Vec<f32>,
    pub choisis: Vec<f32>,
    /// Bandes : position, autre bout, côté.
    pub bandes: Vec<f32>,
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
            Champ::Projet => "Nom du projet".into(),
        }
    }

    /// Champ qui accepte du texte, pas seulement un nombre.
    fn libre(self) -> bool {
        matches!(self, Champ::Nom | Champ::Projet | Champ::Couleur(_))
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
}

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
        self.shell.animating() || self.vol.is_some() || self.message.is_some() || self.calcul.is_some() || self.ajout.as_ref().is_some_and(|a| a.calcul.is_some())
    }

    /// Maillage que l'objet montre : le modifié, ou son dernier résultat en attendant.
    fn maillage_vu(&self, k: usize) -> Arc<Maillage> {
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

    /// Attend la fin des calculs : pour les aperçus hors écran et les vérifications.
    pub fn attendre_calculs(&mut self) {
        loop {
            self.avancer_calculs();
            self.avancer_ajout();
            if self.calcul.is_none() && self.ajout.as_ref().is_none_or(|a| a.calcul.is_none()) {
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
        if self.ajout.is_some() && (self.contenu != Contenu::Ajout || !self.shell.end.open || self.travail.is_some()) {
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

    /// Passe en mode Géométrie : les objets choisis, l'actif compris.
    pub fn entrer_geometrie(&mut self) {
        if self.travail.is_some() {
            return;
        }
        let Some(a) = self.projet.actif.filter(|k| *k < self.projet.objets.len()) else {
            return self.dire("Aucun objet actif : touchez un objet".into());
        };
        if self.projet.objets[a].armature.is_some() {
            return self.dire("Une armature n’a pas de géométrie".into());
        }
        let l: Vec<usize> = self.choix.iter().copied().filter(|k| self.projet.affiche(&self.projet.objets[*k])).collect();
        self.travail = Some(Travail::ouvrir(&self.projet.objets, &l, a, self.filtres));
        self.fin_geste();
        self.version_geo = None;
    }

    pub fn sortir_geometrie(&mut self) {
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
        if pts.is_empty() {
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
        if let Some(t) = &mut self.travail {
            let l = t.impliques();
            let depart: Vec<P3> = l.iter().map(|i| t.m.sommets[*i]).collect();
            t.poser(&l, &depart, v);
        } else {
            let l = self.choix.clone();
            let departs = ed::depart(&self.projet.objets, &l);
            ed::poser(&mut self.projet.objets, &l, &departs, v);
        }
    }

    fn etape(&self) -> Etape {
        let p = &self.projet;
        Etape { objets: p.objets.clone(), sets: p.sets.clone(), materiaux: p.materiaux.clone(), actif: p.actif, choix: self.choix.clone(), travail: self.travail.clone() }
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
        self.version_geo = None;
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
        match &self.travail {
            Some(t) => t.points(),
            None => self.choix.iter().map(|k| self.projet.objets[*k].position).collect(),
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
        Some(Poignee::new(self.mode, ed::centre(&pts), pivot, self.axes, vue, dp as f64))
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
        if prise.is_none() && !boite && poignee.is_some() {
            let dessus = match &self.travail {
                Some(t) => t.sur_selection(px, py, &vue, self.rayon(cx.dp), self.rayons),
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
        self.memoriser();
        let pts = self.points();
        let pivot = self.pivot_courant(&pts);
        self.pas_t = self.pas_translation();
        let (indices, departs) = match &self.travail {
            Some(_) => (Vec::new(), Vec::new()),
            None => (self.choix.clone(), ed::depart(&self.projet.objets, &self.choix)),
        };
        let mut g = Geste::new(self.mode, prise, px, py, vue, self.axes, indices, departs, ed::centre(&pts), pivot);
        if let Some(t) = &self.travail {
            g.indices = t.impliques();
            g.sommets = pts;
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
        let n = self.travail.as_ref().map_or(self.projet.objets.len(), |t| t.m.sommets.len());
        if self.geste.as_ref().is_some_and(|g| g.indices.iter().any(|k| *k >= n)) {
            self.geste = None;
        }
        let mut faite = None;
        if let Some(g) = &mut self.geste {
            let v = g.suivre((x - w.x) as f64, (y - w.y) as f64, &self.pas, self.pas_t);
            match &mut self.travail {
                Some(t) => t.poser(&g.indices, &g.sommets, v),
                None => ed::poser(&mut self.projet.objets, &g.indices, &g.departs, v),
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

    /// Copies sur place : les maillages restent partagés jusqu'à ce que l'un change.
    fn dupliquer(&mut self) {
        if self.choix.is_empty() {
            return self.dire("Aucun objet sélectionné".into());
        }
        self.memoriser();
        let p = &mut self.projet;
        self.choix = self
            .choix
            .iter()
            .map(|&k| {
                let mut o = p.objets[k].clone();
                o.nom = ed::nom_libre(&p.objets, &o.nom);
                p.objets.push(o);
                p.objets.len() - 1
            })
            .collect();
        p.actif = self.choix.last().copied();
        self.a_sauver = true;
        self.dire("Dupliqué : glissez pour écarter la copie".into());
    }

    fn supprimer(&mut self) {
        if self.choix.is_empty() {
            return self.dire("Aucun objet sélectionné".into());
        }
        self.memoriser();
        let (p, retires) = (&mut self.projet, std::mem::take(&mut self.choix));
        p.actif = p.actif.filter(|a| !retires.contains(a)).map(|a| a - retires.iter().filter(|k| **k < a).count());
        let mut k = 0;
        p.objets.retain(|_| {
            k += 1;
            !retires.contains(&(k - 1))
        });
        self.a_sauver = true;
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
        self.maillage_vu(k).boite_englobante().map_or([0.0; 3], |b| [0, 1, 2].map(|c| b.max[c] - b.min[c]))
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
                self.projet.objets[i].rotation = ed::depuis_euler(deg);
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
                let o = &mut self.projet.objets[i];
                if o.armature.is_some() {
                    o.echelle = [f; 3];
                } else {
                    o.echelle[k] = f;
                }
            }
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
            let m = self.projet.objets[k].matrice();
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

    fn recadrer(&mut self) {
        let liste = self.travail.as_ref().map_or(self.choix.clone(), |t| t.edites.clone());
        if let Some(arrivee) = self.cadre(&liste) {
            self.vol = Some(Vol { depart: self.cam, arrivee, t: 0.0 });
        }
    }

    pub fn frame(&mut self, cx: &mut Cx, screen: Rect, dt: f32) -> Scene {
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

    /// Objet affiché sous le doigt, le plus proche de l'œil.
    fn objet_sous(&self, w: Rect, x: f32, y: f32) -> Option<usize> {
        let (oeil, droite, haut, dos) = self.repere();
        let t = (CHAMP.to_radians() / 2.0).tan();
        let nx = ((x - w.x) / w.w * 2.0 - 1.0) as f64 * t * (w.w / w.h.max(1.0)) as f64;
        let ny = (1.0 - (y - w.y) / w.h * 2.0) as f64 * t;
        let d = [0, 1, 2].map(|k| droite[k] * nx + haut[k] * ny - dos[k]);
        let mut mieux: Option<(usize, f64)> = None;
        for k in self.affiches() {
            if let Some(t) = vue::toucher(&self.projet.objets[k], &self.maillage_vu(k), oeil, d) {
                if mieux.is_none_or(|m| t < m.1) {
                    mieux = Some((k, t));
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
            let q = o.position.map(|x| x as f32);
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
            for k in (0..3).filter(|k| p.axes[*k]) {
                let a = if tenue.is_none() || tenue == Some(Prise::Axe(k)) { 0.95 } else { 0.3 };
                trace(cx, p.anneau(k, 64), 3.0 * d, voile(coul(COUL_AXES[k]), a));
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

    /// Taille et écart des neuf pastilles : sur écran étroit, l'écart cède avant la taille.
    fn mesure_pastilles(largeur: f32, d: f32) -> (f32, f32) {
        let libre = largeur - 28.0 * d;
        let ecart = ((libre - 9.0 * 36.0 * d) / 8.0).clamp(3.0 * d, 10.0 * d);
        (((libre - 8.0 * ecart) / 9.0).clamp(34.0 * d, 52.0 * d), ecart)
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
        // Les modifieurs se règlent en mode Scène : en Géométrie, on édite le maillage de base
        if self.travail.is_some() {
            cx.pastille_eteinte(MODIFIEURS, a(3.0), Icon::Cle);
        } else if cx.pastille(MODIFIEURS, a(3.0), Icon::Cle, fin && contenu == Contenu::Modifieurs) {
            self.basculer(Contenu::Modifieurs);
        }
        if cx.pastille(FICHIERS, a(4.0), Icon::Dossier, fin && contenu == Contenu::Fichier) {
            self.basculer(Contenu::Fichier);
        }
        if cx.pastille(REGLAGES, a(5.0), Icon::Sliders, fin && contenu == Contenu::Reglages) {
            self.basculer(Contenu::Reglages);
        }
        if cx.pastille(AIDE, a(6.0), Icon::Help, fin && contenu == Contenu::Aide) {
            self.basculer(Contenu::Aide);
        }
        for (i, cle, icone, possible) in [(7.0, ANNULER, Icon::Annuler, !self.histoire.annuler.is_empty()), (8.0, RETABLIR, Icon::Retablir, !self.histoire.retablir.is_empty())] {
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

    /// Mode, filtres et sélection : sous les pastilles.
    fn barre_haute(&mut self, cx: &mut Cx, w: Rect, y: f32) {
        let m = 14.0 * cx.dp;
        let cle = id("barre-haute");
        let geo = self.travail.is_some();
        let etroit = w.w < 760.0 * cx.dp;
        let sel = match self.selection { Selection::Toucher => &icones::SELECTION_TOUCHER, Selection::Boite => &icones::SELECTION_BOITE, Selection::Chemin => &icones::SELECTION_CHEMIN };
        let mut l = vec![
            Pilule::nommee(cx, sub_id(cle, 0), "Scène", &icones::SCENE, !geo, etroit),
            // Sur écran étroit, le nom laisserait les filtres passer à la ligne
            if etroit && geo { Pilule::icone(cx, sub_id(cle, 1), "Géométrie", &icones::PIVOT_ACTIF, true) } else { Pilule::nommee(cx, sub_id(cle, 1), "Géométrie", &icones::PIVOT_ACTIF, geo, etroit) },
            Pilule::icone(cx, sub_id(cle, 2), "Sélection", sel, false),
        ];
        if self.selection == Selection::Boite {
            let mut p = Pilule::new(cx, sub_id(cle, 3), if self.boite_retire { "−" } else { "+" }, self.boite_retire);
            p.l = 44.0 * cx.dp;
            l.push(p);
        }
        let base = l.len();
        if let Some(t) = &self.travail {
            for k in 0..3 {
                let mut p = Pilule::icone(cx, sub_id(cle, 10 + k), NOMS_FILTRES[k], ICONES_FILTRES[k], t.filtres[k]);
                p.long = true;
                l.push(p);
            }
        }
        let (appui, long) = Self::rangees(cx, &l, w.x + m, y, w.w - 2.0 * m, false);
        if let Some(i) = long.filter(|i| *i >= base) {
            // Appui long : ce type s'ajoute ou se retire, les autres restent
            let r = self.travail.as_mut().map(|t| t.basculer_filtre(TYPES[i - base]));
            if let Some(Err(m)) = r {
                self.dire(m);
            }
        }
        match appui {
            Some(0) => self.sortir_geometrie(),
            Some(1) => self.entrer_geometrie(),
            Some(2) => {
                self.selection = match (self.selection, geo) {
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
            Some(3) if self.selection == Selection::Boite => {
                self.boite_retire = !self.boite_retire;
                self.dire(if self.boite_retire { "La boîte retire de la sélection" } else { "La boîte ajoute à la sélection" }.to_string());
            }
            Some(i) if i >= base => {
                // Appui bref : ce type seul
                let mut f = [false; 3];
                f[i - base] = true;
                if let Some(t) = &mut self.travail {
                    t.regler_filtres(f);
                }
            }
            _ => {}
        }
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
        let n = Self::coupes(&l, w.w - 2.0 * m, ecart).len() as f32;
        let haut = w.bottom() - m - n * h - (n - 1.0) * ecart;
        match Self::rangees(cx, &l, w.x + m, haut, w.w - 2.0 * m, true).0 {
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
        let faces: usize = self.projet.objets.iter().map(|o| o.maillage.faces.len()).sum();
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
        let mut b = cx.body(z, corps, &self.scrolls);
        for e in 0..self.projet.sets.len() {
            let nom = self.projet.sets[e].nom.clone();
            let mut visible = self.projet.sets[e].visible;
            cx.toggle(&mut b, sub_id(id("set"), e), &nom, &mut visible);
            if self.projet.sets[e].visible != visible {
                self.visibilite_changee();
            }
            self.projet.sets[e].visible = visible;
            for k in 0..self.projet.objets.len() {
                let o = &self.projet.objets[k];
                if o.set != nom {
                    continue;
                }
                let c = self.projet.materiau(&o.materiau).map_or(coul(C::Bordures), |m| octets_hex(&m.couleur));
                let choisi = self.choix.contains(&k);
                if cx.row(&mut b, sub_id(z, k), &o.nom, Some(c), choisi) {
                    if self.travail.is_none() {
                        self.basculer_objet(k);
                    }
                }
            }
        }
        cx.body_end(b, &mut self.scrolls);
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
            Contenu::Scene => {
                cx.tabs(id("onglets-scene"), onglets, &["Scène"], 0);
                let mut b = cx.body(id("corps-scene"), corps, &self.scrolls);
                self.panneau_scene(cx, &mut b);
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Modifieurs => {
                cx.tabs(id("onglets-modifieurs"), onglets, &["Modifieurs"], 0);
                let mut b = cx.body(id("corps-modifieurs"), corps, &self.scrolls);
                if self.travail.is_some() {
                    cx.paragraph(&mut b, "Le mode Géométrie édite le maillage de base, sans ses modifieurs. Revenez en mode Scène pour les régler.");
                } else {
                    self.panneau_modifs(cx, &mut b);
                }
                cx.body_end(b, &mut self.scrolls);
            }
            Contenu::Contexte => {
                let geo = self.travail.is_some();
                self.onglet_contexte = cx.tabs(id("onglets-contexte"), onglets, &[if geo { "Géométrie" } else { "Objet" }, "Dernière opération"], self.onglet_contexte);
                let mut b = cx.body(sub_id(id("corps-contexte"), self.onglet_contexte), corps, &self.scrolls);
                match self.onglet_contexte {
                    ONGLET_DERNIERE => self.reglages_derniere(cx, &mut b),
                    _ if geo => self.fiche_geo(cx, &mut b),
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
                cx.paragraph(&mut b, "Un double appui sur un objet l'édite, avec les autres objets choisis. Les icônes de sommet, d'arête et de face, en haut, disent ce qui se touche : un appui bref garde ce type seul, un appui long l'ajoute ou le retire.");
                cx.paragraph(&mut b, "Glisser depuis la sélection ou la poignée la transforme. Contexte › Géométrie regroupe les opérations. Le bouton de sélection, en haut, passe de Toucher à Boîte, qui trace un cadre, puis à Chemin, qui relie deux éléments.");
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
                cx.paragraph(&mut b, "Modes Scène et Géométrie, modifieurs booléen et répétition, visserie. Armatures conservées, ni affichées ni appliquées.");
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
        if !geo {
            self.operations(cx, b, Menu::Scene);
        }
    }

    /// Boutons d'un menu du catalogue, par rubrique, grisés quand ils ne s'appliquent
    /// pas. L'action attend la fin de la liste : elle peut changer ce qui s'y affiche.
    fn operations(&mut self, cx: &mut Cx, b: &mut crate::widgets::Body, menu: Menu) {
        let c = self.contexte();
        let (mut rubrique, mut choisie) = ("", None);
        for o in actions::du_menu(menu) {
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
            editable: p.actif.and_then(|k| p.objets.get(k)).is_some_and(|o| o.armature.is_none()),
            ..Default::default()
        };
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
            A::EditerGeometrie => self.entrer_geometrie(),
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
                cx.paragraph(b, &format!("{s} sommets · {f} faces · matériau {mat}"));
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
                let o = &self.projet.objets[k];
                if o.armature.is_some() {
                    cx.paragraph(b, "Armature conservée, pas encore affichée.");
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
        if self.travail.is_some() {
            return (lin_octets(coul(C::Face)), [0.0; 3], OPACITE_PASSIF);
        }
        if self.rendu == Rendu::Jeu {
            let defaut = Materiau::neuf(None);
            let m = self.projet.materiau(&o.materiau).unwrap_or(&defaut);
            // Illumination 0 : couleur plate, sans lumière.
            return if m.illumination == 0.0 { ([0.0; 3], lin_hex(&m.couleur), m.opacite as f32) } else { (lin_hex(&m.couleur), lin_hex(&m.emission), m.opacite as f32) };
        }
        let c = if !self.choix.contains(&k) {
            coul(C::Objet)
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
            let t = &self.projet.textures[&id];
            let nom = t.nom.clone();
            match texture::decoder(&t.octets) {
                Ok(i) => images.push((id, i)),
                Err(e) => self.dire(format!("Texture illisible : {nom} ({e})")),
            }
        }
        let retraits: Vec<String> = self.images_vues.iter().filter(|id| !self.projet.textures.contains_key(*id)).cloned().collect();
        for id in &retraits {
            self.images_vues.remove(id);
        }
        (images, retraits)
    }

    fn scene(&mut self, dp: f32, w: Rect) -> Scene {
        self.avancer_calculs();
        self.avancer_ajout();
        let (v, p) = self.matrices(w);
        let (oeil, droite, haut, dos) = self.repere();
        let l = if self.rendu == Rendu::Jeu { SOLEIL } else { [0, 1, 2].map(|k| droite[k] * LAMPE[0] + haut[k] * LAMPE[1] + dos[k] * LAMPE[2]) };
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        let mut objets = Vec::new();
        let (mut ajouts, mut vus) = (Vec::new(), HashSet::new());
        let mut besoin = HashSet::new();
        let cibles = modifs::cibles(&self.projet);
        for k in self.affiches() {
            if self.travail.as_ref().is_some_and(|t| t.edites.contains(&k)) {
                continue;
            }
            let o = &self.projet.objets[k];
            let m = o.matrice();
            let (albedo, emission, opacite) = self.couleurs(k, o);
            let vu = self.maillage_vu(k);
            let cle = Arc::as_ptr(&vu) as usize;
            let contour = self.travail.is_none() && self.choix.contains(&k);
            // Comme la couleur des matériaux : en rendu En jeu seulement
            let texture = if self.rendu == Rendu::Jeu && self.travail.is_none() { self.projet.texture_de(o).cloned() } else { None };
            besoin.extend(texture.clone());
            objets.push(ObjetVue { tampon: cle, modele: vue::mat4(&m), normale: vue::mat_normale(&m), albedo, emission, opacite, contour, filaire: cibles.contains(&k), texture, envers: None });
            vus.insert(cle);
            if !self.connus.contains_key(&cle) {
                ajouts.push((cle, vue::tampons(&vu)));
                self.connus.insert(cle, vu);
            }
        }
        // Aperçu de la primitive en cours d'ajout : comme un objet choisi, sur le curseur
        if let Some(m) = self.ajout.as_ref().and_then(|a| a.apercu.clone()) {
            let cle = Arc::as_ptr(&m) as usize;
            let c = self.projet.curseur;
            let mx = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, c[0], c[1], c[2], 1.0];
            objets.push(ObjetVue { tampon: cle, modele: vue::mat4(&mx), normale: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], albedo: lin_octets(coul(C::ObjetActif)), emission: [0.0; 3], opacite: 1.0, contour: true, filaire: false, texture: None, envers: None });
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
        let Some(t) = &self.travail else {
            if self.version_geo.take().is_some() {
                retraits.extend([CLE_FACES, CLE_CHOISIES]);
            }
            return None;
        };
        let opacite = if self.rayons { OPACITE_RAYONS } else { 1.0 };
        let id3: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let modele = vue::mat4(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        for (cle, c) in [(CLE_FACES, coul(C::Face)), (CLE_CHOISIES, coul(C::FaceChoisie))] {
            objets.push(ObjetVue { tampon: cle, modele, normale: id3, albedo: lin_octets(c), emission: [0.0; 3], opacite, contour: false, filaire: false, texture: None, envers: Some(lin_octets(coul(C::Envers))) });
        }
        if self.version_geo == Some(t.version) {
            return Some(GeoVue { maj: None, rayons: self.rayons });
        }
        self.version_geo = Some(t.version);
        let m = &t.m;
        let choisies: HashSet<usize> = if t.filtres[2] { t.sel.faces.iter().copied().collect() } else { HashSet::new() };
        for (cle, garde) in [(CLE_FACES, false), (CLE_CHOISIES, true)] {
            ajouts.push((cle, Tampons { faces: vue::faces_si(m, |k| choisies.contains(&k) == garde), bandes: Vec::new(), bandes_idx: Vec::new(), fil: Vec::new() }));
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
    fn la_barre_rapide_vide_ou_masquee_ne_prend_pas_de_place() {
        let mut b = en_geometrie();
        assert!(b.ui.zone(BARRE_RAPIDE).is_none());
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
        let (x, y) = (w.x + (14.0 + 4.0 * (52.0 + 10.0) + 26.0) * 1.5, w.y + (14.0 + 26.0) * 1.5);
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
}
