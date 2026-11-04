//! Catalogue des opérations, repris de `_operations` de GDA : une seule source pour les
//! panneaux, l'aide et, plus tard, la barre rapide. Sans interface : ce module dit ce
//! qui existe et ce qui est possible, `editeur` l'affiche et l'exécute.

use std::collections::HashMap;

/// Une opération. Chaque variante a une seule entrée dans `CATALOGUE`, dans le même ordre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Enregistrer,
    Ouvrir,
    NouvelleScene,
    Importer,
    ExporterObj,
    ExporterStl,
    ExporterGlb,
    ExporterSelectionObj,
    ExporterSelectionStl,
    ExporterSelectionGlb,
    ExporterMtl,
    Recadrer,
    CurseurOrigine,
    CurseurSelection,
    SelectionCurseur,
    CurseurVue,
    CurseurScene,
    VueCurseur,
    AjouterCube,
    AjouterSphere,
    AjouterCylindre,
    AjouterCone,
    AjouterIcosphere,
    AjouterVis,
    AjouterEcrou,
    AjouterRondelle,
    AjouterTige,
    ToutChoisir,
    RienChoisir,
    Dupliquer,
    Supprimer,
    AppliquerEchelle,
    EditerGeometrie,
    ModifBooleen,
    ModifRepetition,
    CreerFace,
    Inverser,
    Trianguler,
    Separer,
    SupprimerFaces,
    CreerArete,
    Subdiviser,
    Boucle,
    Dissoudre,
    SupprimerAretes,
    AjouterSommet,
    Fusionner,
    Confondus,
    Revolution,
    SupprimerSommets,
    Lies,
    TousSommets,
    ToutesAretes,
    ToutesFaces,
    AucunSommet,
    AucuneArete,
    AucuneFace,
    ToutDeselectionner,
    FacesSommets,
    FacesAretes,
    AretesSommets,
    AretesFaces,
    SommetsAretes,
    SommetsFaces,
    EditerUv,
    ProjeterVue,
    ProjeterCube,
    ProjeterCylindre,
    ProjeterSphere,
    ChoisirImage,
    MettreEnPage,
    MarquerCoutures,
    EffacerCoutures,
    CouturesDepuisUv,
    Deplier,
    DeplacerSet,
    AjouterArmature,
    AjouterHumanoide,
    LierArmature,
    Delier,
    ReposOs,
    AjouterOs,
    ExtruderOs,
    SupprimerOs,
    ParenterOs,
    DetacherOs,
    TousOs,
    AucunOs,
    DescendantsOs,
    ExporterSage,
}

/// Panneau où figure l'opération.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    /// Panneau Fichier.
    Projet,
    Importer,
    Exporter,
    /// Onglet Scène, dans tous les modes.
    Vue,
    /// Onglet Scène, en mode Scène.
    Scene,
    /// Onglet Objet.
    Objet,
    /// Onglet Géométrie.
    Geometrie,
    /// Panneau Modifieurs.
    Modifieurs,
    /// Onglet Os, en mode Armature.
    Os,
}

impl Menu {
    pub fn titre(self) -> &'static str {
        match self {
            Menu::Projet => "Projet",
            Menu::Importer => "Importer",
            Menu::Exporter => "Exporter",
            Menu::Vue => "Vue",
            Menu::Scene => "Scène",
            Menu::Objet => "Objet",
            Menu::Geometrie => "Géométrie",
            Menu::Modifieurs => "Modifieurs",
            Menu::Os => "Os",
        }
    }
}

/// Groupes de la barre rapide (`ED_ACCES` de GDA).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Groupe {
    Ajout,
    Objet,
    Scene,
    ElementActif,
    Faces,
    Aretes,
    Sommets,
    Geo,
    Os,
    Vue,
}

/// Groupes dans l'ordre de l'onglet Réglages › Barre rapide.
pub const GROUPES: [(Groupe, &str); 10] = [
    (Groupe::Ajout, "Scène, aucun objet choisi"),
    (Groupe::Objet, "Objets choisis"),
    (Groupe::Scene, "Scène, toujours"),
    (Groupe::ElementActif, "Géométrie : élément actif"),
    (Groupe::Faces, "Faces sélectionnées"),
    (Groupe::Aretes, "Arêtes sélectionnées"),
    (Groupe::Sommets, "Sommets sélectionnés"),
    (Groupe::Geo, "Géométrie, toujours"),
    (Groupe::Os, "Armature"),
    (Groupe::Vue, "Vue et curseur 3D (tous les modes)"),
];

/// Choix de l'utilisateur : opération, par `Barre::cle`, gardée ou retirée de la barre.
pub type Choix = HashMap<String, bool>;

/// Comment l'opération s'offre à la barre rapide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Barre {
    pub groupe: Groupe,
    pub glyphe: &'static str,
    pub court: &'static str,
    /// Dans la barre tant que l'utilisateur n'a rien choisi.
    pub defaut: bool,
}

impl Barre {
    /// Identifiant durable, gardé dans les réglages (« groupe:court » de GDA).
    pub fn cle(&self) -> String {
        format!("{:?}:{}", self.groupe, self.court)
    }

    pub fn retenue(&self, choix: &Choix) -> bool {
        choix.get(&self.cle()).copied().unwrap_or(self.defaut)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Op {
    pub action: Action,
    pub menu: Menu,
    pub rubrique: &'static str,
    pub libelle: &'static str,
    /// Description fixe, indépendante de la sélection, sans majuscule ni point final.
    pub aide: &'static str,
    pub barre: Option<Barre>,
}

/// Ce dont dépend la disponibilité des opérations, à un instant donné.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contexte {
    /// En mode Géométrie.
    pub geo: bool,
    /// Objets choisis, et objets affichés.
    pub choisis: usize,
    pub affiches: usize,
    /// Un objet choisi a une échelle à figer.
    pub echelle: bool,
    /// L'objet actif a une géométrie.
    pub editable: bool,
    /// Éléments sélectionnés.
    pub sommets: usize,
    pub aretes: usize,
    pub faces: usize,
    /// Une face sélectionnée a plus de 3 sommets.
    pub grande_face: bool,
    /// L'arête active traverse un anneau de quadrilatères.
    pub anneau: bool,
    /// Bilan de la dissolution des arêtes sélectionnées, s'il y en a.
    pub dissolution: Option<Result<(), String>>,
    /// Il y a un élément actif.
    pub actif: bool,
    /// Groupe du type de l'élément actif.
    pub type_actif: Option<Groupe>,
    /// Un objet choisi peut se lier : il a une géométrie, et le projet une armature.
    pub liables: bool,
    /// Un objet choisi est lié.
    pub lies: bool,
    /// L'armature ouverte est en pose ; elle a des os écartés de leur repos.
    pub pose: bool,
    pub posee: bool,
    /// En mode Armature, et une armature y est ouverte.
    pub os: bool,
    pub os_ouvert: bool,
    /// Bouts d'os sélectionnés.
    pub bouts: usize,
    /// Os aux deux bouts sélectionnés, et os à la queue sélectionnée.
    pub os_choisis: usize,
    pub queues: usize,
    /// Il y a un os actif, et il a des enfants.
    pub os_actif: bool,
    pub os_enfants: bool,
    /// Un os choisi autre que l'actif ; un os choisi qui a un parent.
    pub rattachables: bool,
    pub attaches: bool,
    /// Une armature à exporter pour S.A.G.E.
    pub personnage: bool,
}

/// Le mode où une opération des os s'applique : `Some(false)` au repos seulement
/// (mode Géométrie), `Some(true)` en pose seulement (mode Pose), `None` dans les deux.
pub fn du_mode_pose(a: Action) -> Option<bool> {
    use Action::*;
    match a {
        AjouterOs | ExtruderOs | SupprimerOs | ParenterOs | DetacherOs | SelectionCurseur => Some(false),
        ReposOs => Some(true),
        _ => None,
    }
}

impl Contexte {
    /// Vrai si l'opération s'applique à la sélection du moment.
    pub fn possible(&self, a: Action) -> bool {
        use Action::*;
        let (n, na, nf) = (self.sommets, self.aretes, self.faces);
        if self.os {
            if du_mode_pose(a).is_some_and(|p| p != self.pose) {
                return false;
            }
            return match a {
                Ouvrir | Enregistrer | Importer | ExporterObj | ExporterStl | ExporterGlb | ExporterMtl | NouvelleScene | Recadrer | CurseurOrigine | CurseurVue | CurseurScene | VueCurseur | AjouterArmature | AjouterHumanoide => true,
                ExporterSage => self.personnage,
                AjouterOs | TousOs => self.os_ouvert,
                ExporterSelectionObj | ExporterSelectionStl | ExporterSelectionGlb => self.choisis > 0,
                CurseurSelection | SelectionCurseur | AucunOs => self.bouts > 0,
                ExtruderOs => self.queues > 0,
                SupprimerOs => self.os_choisis > 0,
                ParenterOs => self.os_actif && self.rattachables,
                DetacherOs => self.attaches,
                DescendantsOs => self.os_enfants,
                ReposOs => self.posee,
                _ => false,
            };
        }
        match a {
            Ouvrir | Enregistrer | Importer | ExporterObj | ExporterStl | ExporterGlb | ExporterMtl | NouvelleScene | Recadrer | CurseurOrigine | CurseurVue | CurseurScene | VueCurseur => true,
            ExporterSage => self.personnage,
            ExporterSelectionObj | ExporterSelectionStl | ExporterSelectionGlb => self.choisis > 0,
            CurseurSelection | SelectionCurseur => {
                if self.geo {
                    n + na + nf > 0
                } else {
                    self.choisis > 0
                }
            }
            AjouterCube | AjouterSphere | AjouterCylindre | AjouterCone | AjouterIcosphere | AjouterVis | AjouterEcrou | AjouterRondelle | AjouterTige | AjouterArmature | AjouterHumanoide => !self.geo,
            ModifBooleen | ModifRepetition => !self.geo && self.choisis > 0,
            ToutChoisir => !self.geo && self.affiches > 0,
            RienChoisir | Dupliquer | Supprimer | DeplacerSet => !self.geo && self.choisis > 0,
            AppliquerEchelle => !self.geo && self.echelle,
            LierArmature => !self.geo && self.liables,
            Delier => !self.geo && self.lies,
            EditerGeometrie => !self.geo && !self.os && self.editable,
            AjouterOs | ExtruderOs | SupprimerOs | ParenterOs | DetacherOs | TousOs | AucunOs | DescendantsOs | ReposOs => false,
            CreerFace => self.geo && n >= 3,
            CreerArete => self.geo && n == 2,
            Fusionner | SommetsAretes => self.geo && n >= 2,
            SommetsFaces => self.geo && n >= 3,
            SupprimerSommets | AucunSommet => self.geo && n > 0,
            Subdiviser | SupprimerAretes | AucuneArete | AretesSommets | AretesFaces => self.geo && na > 0,
            Inverser | Separer | SupprimerFaces | AucuneFace | FacesSommets | FacesAretes => self.geo && nf > 0,
            Trianguler => self.geo && self.grande_face,
            Boucle => self.geo && self.anneau,
            ChoisirImage => self.actif,
            EditerUv | ProjeterVue | ProjeterCube | ProjeterCylindre | ProjeterSphere | MettreEnPage | CouturesDepuisUv | Deplier => self.geo,
            MarquerCoutures | EffacerCoutures => self.geo && na > 0,
            Dissoudre => self.geo && matches!(self.dissolution, Some(Ok(()))),
            Revolution => self.geo && n + na > 0,
            Lies => self.geo && self.actif,
            ToutDeselectionner => self.geo && n + na + nf > 0,
            AjouterSommet | Confondus | TousSommets | ToutesAretes | ToutesFaces => self.geo,
        }
    }

    /// Pourquoi l'opération ne peut pas se faire, quand la raison mérite d'être dite.
    pub fn note(&self, a: Action) -> Option<String> {
        match (a, &self.dissolution) {
            (Action::Dissoudre, Some(Err(e))) => Some(format!("Dissolution impossible : {e}")),
            // Dernier bouton de la rubrique : la raison s'y lit sous les trois
            (Action::ExporterSelectionGlb, _) if self.choisis == 0 => Some("Aucun objet choisi.".into()),
            _ => None,
        }
    }
}

/// Les opérations d'un menu, dans l'ordre d'affichage.
pub fn du_menu(m: Menu) -> impl Iterator<Item = &'static Op> {
    CATALOGUE.iter().filter(move |o| o.menu == m)
}

/// Contenu de la barre rapide (`_acces` de GDA) : les opérations retenues qui s'appliquent
/// à la sélection, par groupe. En Géométrie, le type de l'élément actif passe devant.
pub fn barre(c: &Contexte, choix: &Choix) -> Vec<(Groupe, Vec<&'static Op>)> {
    let ordre: Vec<Groupe> = if c.os {
        if c.os_ouvert { vec![Groupe::Os, Groupe::Vue] } else { vec![Groupe::Ajout, Groupe::Vue] }
    } else if c.geo {
        let mut o = vec![Groupe::ElementActif];
        o.extend(c.type_actif);
        o.extend([Groupe::Faces, Groupe::Aretes, Groupe::Sommets, Groupe::Geo, Groupe::Vue]);
        o.dedup_by_key(|g| *g);
        let mut vus = Vec::new();
        o.retain(|g| !vus.contains(g) && { vus.push(*g); true });
        o
    } else {
        vec![if c.choisis > 0 { Groupe::Objet } else { Groupe::Ajout }, Groupe::Scene, Groupe::Vue]
    };
    // Les opérations du curseur valent pour les objets comme pour la géométrie
    let groupe = |g: Groupe| match g {
        Groupe::Objet if c.os => Groupe::Os,
        Groupe::Objet if c.geo => Groupe::Geo,
        g => g,
    };
    ordre
        .into_iter()
        .map(|g| (g, CATALOGUE.iter().filter(|o| o.barre.is_some_and(|b| groupe(b.groupe) == g && b.retenue(choix)) && c.possible(o.action)).collect::<Vec<_>>()))
        .filter(|x| !x.1.is_empty())
        .collect()
}

pub const CATALOGUE: &[Op] = &[
    Op {
        action: Action::Enregistrer,
        menu: Menu::Projet,
        rubrique: "Projet",
        libelle: "Enregistrer le projet",
        aide: "écrire un fichier .gda dans Téléchargements, toute la scène sans perte, un nouveau fichier à chaque fois ; le travail est aussi gardé d’un lancement à l’autre",
        barre: None,
    },
    Op {
        action: Action::Ouvrir,
        menu: Menu::Projet,
        rubrique: "Projet",
        libelle: "Ouvrir (remplace la scène)",
        aide: "choisir un projet ou un fichier avec le sélecteur d’Android ; il remplace la scène et vide l’annulation",
        barre: None,
    },
    Op {
        action: Action::NouvelleScene,
        menu: Menu::Projet,
        rubrique: "Scène",
        libelle: "Nouvelle scène",
        aide: "repartir d’un cube seul, dans le set par défaut, avec la seule bibliothèque de matériaux par défaut (annulable)",
        barre: Some(Barre { groupe: Groupe::Vue, glyphe: "✱", court: "Nouvelle scène", defaut: false }),
    },
    Op {
        action: Action::Importer,
        menu: Menu::Importer,
        rubrique: "Fichier de l'appareil",
        libelle: "Ajouter à la scène",
        aide: "ajouter à la scène un fichier choisi avec le sélecteur d’Android (OBJ, STL, glTF, GLB ou .gda) : son origine se place sur le curseur 3D, les objets ajoutés sont choisis (annulable) ; pour un objet texturé, choisissez ensemble le .obj, son .mtl et ses images ; un .glb contient les siennes, et les textures se voient en rendu En jeu",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "⤓", court: "Fichier", defaut: true }),
    },
    Op {
        action: Action::ExporterObj,
        menu: Menu::Exporter,
        rubrique: "Exporter la scène",
        libelle: "OBJ",
        aide: "exporter les objets affichés en OBJ dans Téléchargements ; les arêtes libres y sont gardées ; s’ils ont des textures, le .mtl et les images l’accompagnent",
        barre: None,
    },
    Op {
        action: Action::ExporterStl,
        menu: Menu::Exporter,
        rubrique: "Exporter la scène",
        libelle: "STL",
        aide: "exporter les objets affichés en STL dans Téléchargements",
        barre: None,
    },
    Op {
        action: Action::ExporterGlb,
        menu: Menu::Exporter,
        rubrique: "Exporter la scène",
        libelle: "GLB (glTF)",
        aide: "exporter les objets affichés en GLB dans Téléchargements, textures comprises",
        barre: None,
    },
    Op {
        action: Action::ExporterSelectionObj,
        menu: Menu::Exporter,
        rubrique: "Exporter la sélection",
        libelle: "OBJ",
        aide: "exporter les seuls objets choisis en OBJ, dans le repère du monde ; en mode Géométrie, les objets édités ; s’ils ont des textures, le .mtl et les images l’accompagnent",
        barre: None,
    },
    Op {
        action: Action::ExporterSelectionStl,
        menu: Menu::Exporter,
        rubrique: "Exporter la sélection",
        libelle: "STL",
        aide: "exporter les seuls objets choisis en STL, dans le repère du monde ; en mode Géométrie, les objets édités",
        barre: None,
    },
    Op {
        action: Action::ExporterSelectionGlb,
        menu: Menu::Exporter,
        rubrique: "Exporter la sélection",
        libelle: "GLB (glTF)",
        aide: "exporter les seuls objets choisis en GLB, dans le repère du monde, textures comprises ; en mode Géométrie, les objets édités",
        barre: None,
    },
    Op {
        action: Action::ExporterMtl,
        menu: Menu::Exporter,
        rubrique: "Matériaux (bibliothèque des OBJ)",
        libelle: "MTL",
        aide: "exporter les matériaux en .mtl dans Téléchargements, à côté d’un OBJ, avec leurs images",
        barre: None,
    },
    Op {
        action: Action::Recadrer,
        menu: Menu::Vue,
        rubrique: "Vue",
        libelle: "Recadrer",
        aide: "cadrer la vue sur l’objet édité, sinon sur les objets sélectionnés ou toute la scène (aussi : double appui dans le vide)",
        barre: Some(Barre { groupe: Groupe::Vue, glyphe: "⛶", court: "Recadrer", defaut: false }),
    },
    Op {
        action: Action::CurseurOrigine,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Curseur → origine",
        aide: "ramener le curseur 3D au point 0, 0, 0",
        barre: Some(Barre { groupe: Groupe::Vue, glyphe: "⌖", court: "Curs. → 0", defaut: false }),
    },
    Op {
        action: Action::CurseurSelection,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Curseur → sélection",
        aide: "placer le curseur 3D au centre de la sélection : origines des objets choisis, ou sommets sélectionnés en mode Géométrie",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⌖", court: "Curs. → sél.", defaut: false }),
    },
    Op {
        action: Action::SelectionCurseur,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Sélection → curseur",
        aide: "déplacer la sélection pour amener son centre sur le curseur 3D",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⇥", court: "Sél. → curs.", defaut: false }),
    },
    Op {
        action: Action::CurseurVue,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Curseur → centre de la vue",
        aide: "placer le curseur 3D sur le point que la vue regarde, autour duquel elle tourne",
        barre: Some(Barre { groupe: Groupe::Vue, glyphe: "⊙", court: "Curs. → vue", defaut: false }),
    },
    Op {
        action: Action::CurseurScene,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Curseur → centre de la scène",
        aide: "placer le curseur 3D au centre de la boîte des objets affichés, ou de l’objet édité en mode Géométrie",
        barre: None,
    },
    Op {
        action: Action::VueCurseur,
        menu: Menu::Vue,
        rubrique: "Curseur 3D",
        libelle: "Vue → curseur",
        aide: "amener la vue sur le curseur 3D : elle le regarde et tourne autour de lui",
        barre: Some(Barre { groupe: Groupe::Vue, glyphe: "◎", court: "Vue → curs.", defaut: false }),
    },
    Op {
        action: Action::AjouterCube,
        menu: Menu::Scene,
        rubrique: "Ajouter sur le curseur",
        libelle: "Cube…",
        aide: "régler et ajouter un cube ou un pavé sur le curseur 3D, dans le set de l’objet actif",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "+", court: "Cube", defaut: true }),
    },
    Op {
        action: Action::AjouterSphere,
        menu: Menu::Scene,
        rubrique: "Ajouter sur le curseur",
        libelle: "Sphère…",
        aide: "régler et ajouter une sphère UV sur le curseur 3D : quadrilatères, triangles autour des pôles",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "◍", court: "Sphère", defaut: false }),
    },
    Op {
        action: Action::AjouterCylindre,
        menu: Menu::Scene,
        rubrique: "Ajouter sur le curseur",
        libelle: "Cylindre…",
        aide: "régler et ajouter un cylindre sur le curseur 3D, axe Y",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "⌀", court: "Cylindre", defaut: false }),
    },
    Op {
        action: Action::AjouterCone,
        menu: Menu::Scene,
        rubrique: "Ajouter sur le curseur",
        libelle: "Cône…",
        aide: "régler et ajouter un cône ou un tronc de cône sur le curseur 3D, axe Y",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "△", court: "Cône", defaut: false }),
    },
    Op {
        action: Action::AjouterIcosphere,
        menu: Menu::Scene,
        rubrique: "Ajouter sur le curseur",
        libelle: "Icosphère…",
        aide: "régler et ajouter une icosphère sur le curseur 3D : triangles réguliers, quatre fois plus à chaque subdivision",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "◍", court: "Icosphère", defaut: false }),
    },
    Op {
        action: Action::AjouterVis,
        menu: Menu::Scene,
        rubrique: "Visserie (1 unité = 1 mm)",
        libelle: "Vis…",
        aide: "régler et ajouter une vis ou un boulon ISO, de M2 à M24 : tête, longueur, filetage, paramètres détaillés pour l’impression 3D ; la vue se cadre sur l’aperçu",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "⚙", court: "Vis", defaut: false }),
    },
    Op {
        action: Action::AjouterEcrou,
        menu: Menu::Scene,
        rubrique: "Visserie (1 unité = 1 mm)",
        libelle: "Écrou…",
        aide: "régler et ajouter un écrou hexagonal ISO, de même hélice que la vis de sa taille",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "⬡", court: "Écrou", defaut: false }),
    },
    Op {
        action: Action::AjouterRondelle,
        menu: Menu::Scene,
        rubrique: "Visserie (1 unité = 1 mm)",
        libelle: "Rondelle…",
        aide: "régler et ajouter une rondelle plate ISO",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "◎", court: "Rondelle", defaut: false }),
    },
    Op {
        action: Action::AjouterTige,
        menu: Menu::Scene,
        rubrique: "Visserie (1 unité = 1 mm)",
        libelle: "Tige filetée…",
        aide: "régler et ajouter une tige filetée, centrée sur le curseur 3D",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "≡", court: "Tige", defaut: false }),
    },
    Op {
        action: Action::ToutChoisir,
        menu: Menu::Scene,
        rubrique: "Sélection",
        libelle: "Tout choisir",
        aide: "choisir tous les objets affichés",
        barre: Some(Barre { groupe: Groupe::Scene, glyphe: "▣", court: "Tout choisir", defaut: false }),
    },
    Op {
        action: Action::RienChoisir,
        menu: Menu::Scene,
        rubrique: "Sélection",
        libelle: "Tout désélectionner",
        aide: "désélectionner tous les objets",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "▢", court: "Rien", defaut: false }),
    },
    Op {
        action: Action::Dupliquer,
        menu: Menu::Objet,
        rubrique: "Actions",
        libelle: "Dupliquer",
        aide: "copier les objets sélectionnés, sur place : glissez pour les écarter",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⧉", court: "Dupliquer", defaut: true }),
    },
    Op {
        action: Action::Supprimer,
        menu: Menu::Objet,
        rubrique: "Actions",
        libelle: "Supprimer",
        aide: "supprimer les objets sélectionnés",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "✕", court: "Supprimer", defaut: true }),
    },
    Op {
        action: Action::AppliquerEchelle,
        menu: Menu::Objet,
        rubrique: "Actions",
        libelle: "Appliquer l’échelle",
        aide: "figer l’échelle des objets choisis dans leur maillage : leur forme ne change pas, leur échelle revient à 1",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⤢", court: "Appl. échelle", defaut: false }),
    },
    Op {
        action: Action::EditerGeometrie,
        menu: Menu::Objet,
        rubrique: "Actions",
        libelle: "Éditer la géométrie",
        aide: "passer en mode Géométrie sur les objets choisis (l’actif compris) ; sur une armature, ce sont ses os qui s’éditent, au repos ou en pose ; aussi : double appui sur un objet",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "✎", court: "Géométrie", defaut: true }),
    },
    Op {
        action: Action::ModifBooleen,
        menu: Menu::Modifieurs,
        rubrique: "Ajouter un modifieur",
        libelle: "Booléen",
        aide: "ajouter à l’objet actif un modifieur booléen : différence, union ou intersection avec un autre objet, la cible, montrée en fil de fer ; le maillage de base ne change pas tant que le modifieur n’est pas appliqué",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "◐", court: "Booléen", defaut: false }),
    },
    Op {
        action: Action::ModifRepetition,
        menu: Menu::Modifieurs,
        rubrique: "Ajouter un modifieur",
        libelle: "Répétition",
        aide: "ajouter à l’objet actif un modifieur de répétition : des copies décalées, éventuellement unies",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⁂", court: "Répétition", defaut: false }),
    },
    Op {
        action: Action::CreerFace,
        menu: Menu::Geometrie,
        rubrique: "Faces",
        libelle: "Créer une face",
        aide: "relier les sommets sélectionnés, dans l’ordre (sens inverse des aiguilles d’une montre vu de l’extérieur) ; la nouvelle face est sélectionnée",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "◭", court: "Face", defaut: true }),
    },
    Op {
        action: Action::Inverser,
        menu: Menu::Geometrie,
        rubrique: "Faces",
        libelle: "Inverser",
        aide: "retourner les faces sélectionnées ; l’envers est teinté",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "⇄", court: "Inverser", defaut: true }),
    },
    Op {
        action: Action::Trianguler,
        menu: Menu::Geometrie,
        rubrique: "Faces",
        libelle: "Trianguler",
        aide: "découper en triangles les faces sélectionnées de plus de 3 sommets (concaves comprises)",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "◿", court: "Trianguler", defaut: true }),
    },
    Op {
        action: Action::Separer,
        menu: Menu::Geometrie,
        rubrique: "Faces",
        libelle: "Séparer (nouvel objet)",
        aide: "faire des faces sélectionnées un nouvel objet (avec les sommets qu’elles utilisent ; ceux du bord restent aussi à l’objet d’origine), à la même place, du même matériau et set ; l’objet d’origine perd ces faces et les sommets qui ne servaient qu’à elles, et il est supprimé s’il est vidé ; refusé sur un objet à modifieurs",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "⇱", court: "Séparer", defaut: true }),
    },
    Op {
        action: Action::SupprimerFaces,
        menu: Menu::Geometrie,
        rubrique: "Faces",
        libelle: "Supprimer (sommets gardés)",
        aide: "supprimer les faces sélectionnées ; leurs sommets restent",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "✕", court: "Faces", defaut: true }),
    },
    Op {
        action: Action::CreerArete,
        menu: Menu::Geometrie,
        rubrique: "Arêtes",
        libelle: "Créer une arête",
        aide: "relier les deux sommets sélectionnés (seulement deux) : s’ils appartiennent à une même face sans en être voisins, la face est coupée en deux le long de la nouvelle arête ; sinon, arête libre (sans face : exportée en OBJ, ignorée par le STL, le glTF et les booléens) ; la nouvelle arête est sélectionnée",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "⟋", court: "Arête", defaut: true }),
    },
    Op {
        action: Action::Subdiviser,
        menu: Menu::Geometrie,
        rubrique: "Arêtes",
        libelle: "Subdiviser",
        aide: "couper les arêtes sélectionnées en segments égaux (1 coupe, nombre réglable ensuite dans l’onglet « Dernière opération ») ; les faces voisines reçoivent les nouveaux sommets, les morceaux restent sélectionnés",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "⋮", court: "Subdiviser", defaut: true }),
    },
    Op {
        action: Action::Boucle,
        menu: Menu::Geometrie,
        rubrique: "Arêtes",
        libelle: "Boucle de coupe",
        aide: "couper en travers l’anneau de quadrilatères que traverse l’arête active (la dernière touchée, ou la seule sélectionnée) : de face en face par l’arête opposée, jusqu’à une face qui n’est pas un quadrilatère, un bord ou le tour complet ; la nouvelle boucle d’arêtes est sélectionnée ; dans l’onglet « Dernière opération » : nombre de coupes, glissement (−1 à 1) vers l’un ou l’autre côté, Uniforme (même distance le long de chaque arête que sur l’arête active : la boucle garde la forme d’une boucle voisine) et Depuis l’autre côté (l’autre boucle voisine)",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "⫴", court: "Boucle", defaut: true }),
    },
    Op {
        action: Action::Dissoudre,
        menu: Menu::Geometrie,
        rubrique: "Arêtes",
        libelle: "Dissoudre (fusionner les faces)",
        aide: "fusionner les deux faces que borde chaque arête sélectionnée, dans le sens des faces (refusé sur une arête de bord, des faces d’orientations contraires ou si la face obtenue aurait un trou) ; la face obtenue est sélectionnée",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "◫", court: "Dissoudre", defaut: true }),
    },
    Op {
        action: Action::SupprimerAretes,
        menu: Menu::Geometrie,
        rubrique: "Arêtes",
        libelle: "Supprimer (et leurs faces)",
        aide: "supprimer les arêtes sélectionnées et les faces qu’elles bordent ; les sommets restent",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "✕", court: "Arêtes", defaut: true }),
    },
    Op {
        action: Action::AjouterSommet,
        menu: Menu::Geometrie,
        rubrique: "Sommets",
        libelle: "Ajouter (sur le curseur 3D)",
        aide: "ajouter un sommet sur le curseur 3D, à l’objet du dernier sommet sélectionné",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "+", court: "Sommet", defaut: false }),
    },
    Op {
        action: Action::Fusionner,
        menu: Menu::Geometrie,
        rubrique: "Sommets",
        libelle: "Fusionner",
        aide: "réunir les sommets sélectionnés en un seul ; dans l’onglet « Dernière opération », sa position : Centre (moyenne), Premier ou Dernier sélectionné (choix retenu pour la fois suivante)",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "⊙", court: "Fusionner", defaut: true }),
    },
    Op {
        action: Action::Confondus,
        menu: Menu::Geometrie,
        rubrique: "Sommets",
        libelle: "Fusionner les confondus",
        aide: "réunir, à leur position moyenne, les sommets à moins d’une tolérance réglée ensuite dans l’onglet « Dernière opération » (de proche en proche ; seulement entre sommets d’un même objet) : parmi les sommets de la sélection (ceux des arêtes et faces compris), ou tout le maillage sans sélection ; les faces devenues dégénérées disparaissent",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "⊚", court: "Confondus", defaut: false }),
    },
    Op {
        action: Action::Revolution,
        menu: Menu::Geometrie,
        rubrique: "Sommets",
        libelle: "Révolution",
        aide: "faire tourner les sommets et arêtes sélectionnés autour d’un axe (X, Y, Z ou direction de la vue) passant par le curseur 3D, réglé ensuite dans l’onglet « Dernière opération » : angle (360 : tour complet), sens, nombre de segments ; chaque sommet seul décrit un arc d’arêtes libres, chaque arête (sélectionnée, ou dont les deux extrémités le sont) une bande de quadrilatères (triangles contre l’axe, où les sommets ne sont pas recopiés) ; tour complet : Fermer soude la dernière copie à la première ; faces orientées vers l’extérieur de l’axe (ou dans le sens des faces voisines du profil) ; la géométrie créée est sélectionnée",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "↻", court: "Révolution", defaut: false }),
    },
    Op {
        action: Action::SupprimerSommets,
        menu: Menu::Geometrie,
        rubrique: "Sommets",
        libelle: "Supprimer (et leurs faces)",
        aide: "supprimer les sommets sélectionnés et les faces qui les utilisent",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "✕", court: "Sommets", defaut: true }),
    },
    Op {
        action: Action::Lies,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Éléments liés à l’actif",
        aide: "ajouter à la sélection les sommets, arêtes et faces reliés, de proche en proche par les faces, à l’élément actif (le dernier touché) ; les filtres ne changent pas : les éléments d’un type au filtre éteint sont mis de côté et apparaissent quand on le rallume",
        barre: Some(Barre { groupe: Groupe::ElementActif, glyphe: "⊶", court: "Liés", defaut: true }),
    },
    Op {
        action: Action::TousSommets,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Tous les sommets",
        aide: "sélectionner tous les sommets des objets édités (le type sommet devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▣", court: "Tous sommets", defaut: false }),
    },
    Op {
        action: Action::ToutesAretes,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Toutes les arêtes",
        aide: "sélectionner toutes les arêtes des objets édités (le type arête devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▣", court: "Toutes arêtes", defaut: false }),
    },
    Op {
        action: Action::ToutesFaces,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Toutes les faces",
        aide: "sélectionner toutes les faces des objets édités (le type face devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▣", court: "Toutes faces", defaut: false }),
    },
    Op {
        action: Action::AucunSommet,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Aucun sommet",
        aide: "désélectionner les sommets seuls ; les arêtes et faces choisies le restent",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "▢", court: "Aucun sommet", defaut: false }),
    },
    Op {
        action: Action::AucuneArete,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Aucune arête",
        aide: "désélectionner les arêtes seules ; les sommets et faces choisis le restent",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "▢", court: "Aucune arête", defaut: false }),
    },
    Op {
        action: Action::AucuneFace,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Aucune face",
        aide: "désélectionner les faces seules ; les sommets et arêtes choisis le restent",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "▢", court: "Aucune face", defaut: false }),
    },
    Op {
        action: Action::ToutDeselectionner,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Tout désélectionner",
        aide: "désélectionner tous les éléments, de tous les types, ceux mis de côté compris",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▢", court: "Désélect.", defaut: false }),
    },
    Op {
        action: Action::FacesSommets,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Faces → leurs sommets",
        aide: "remplacer les faces sélectionnées par leurs sommets (le type sommet devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "•", court: "F → sommets", defaut: false }),
    },
    Op {
        action: Action::FacesAretes,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Faces → leurs arêtes",
        aide: "remplacer les faces sélectionnées par leurs arêtes (le type arête devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Faces, glyphe: "╱", court: "F → arêtes", defaut: false }),
    },
    Op {
        action: Action::AretesSommets,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Arêtes → leurs sommets",
        aide: "remplacer les arêtes sélectionnées par leurs extrémités (le type sommet devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "•", court: "A → sommets", defaut: false }),
    },
    Op {
        action: Action::AretesFaces,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Arêtes → faces bordées",
        aide: "remplacer les arêtes sélectionnées par les faces qu’elles bordent (le type face devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Aretes, glyphe: "◭", court: "A → faces", defaut: false }),
    },
    Op {
        action: Action::SommetsAretes,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Sommets → arêtes entre eux",
        aide: "remplacer les sommets sélectionnés par les arêtes dont les deux extrémités sont sélectionnées (le type arête devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "╱", court: "S → arêtes", defaut: false }),
    },
    Op {
        action: Action::SommetsFaces,
        menu: Menu::Geometrie,
        rubrique: "Sélection",
        libelle: "Sommets → faces entre eux",
        aide: "remplacer les sommets sélectionnés par les faces dont tous les sommets sont sélectionnés (le type face devient sélectionnable)",
        barre: Some(Barre { groupe: Groupe::Sommets, glyphe: "◭", court: "S → faces", defaut: false }),
    },
    Op {
        action: Action::EditerUv,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Éditer les UV",
        aide: "ouvrir ou fermer la vue UV à côté du modèle : les faces choisies y apparaissent, toutes si aucune ne l’est ; le modèle montre la texture et suit chaque déplacement",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "UV", defaut: true }),
    },
    Op {
        action: Action::ProjeterVue,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Projeter : vue",
        aide: "à plat, tel que l’écran montre le modèle : les faces choisies, toutes si aucune ne l’est",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Vue", defaut: false }),
    },
    Op {
        action: Action::ProjeterCube,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Projeter : cube",
        aide: "six côtés, chaque face allant à celui qu’elle regarde : bâtiments, caisses, formes anguleuses",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Cube", defaut: false }),
    },
    Op {
        action: Action::ProjeterCylindre,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Projeter : cylindre",
        aide: "le corps déroulé autour d’un axe, les faces du dessus et du dessous en disques : roues, troncs, membres",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Cylindre", defaut: false }),
    },
    Op {
        action: Action::ProjeterSphere,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Projeter : sphère",
        aide: "longitude et latitude autour d’un axe : têtes, planètes, formes rondes",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Sphère", defaut: false }),
    },
    Op {
        action: Action::ChoisirImage,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Choisir une image…",
        aide: "donner une image PNG ou JPEG au matériau de l’objet actif ; le matériau par défaut, partagé, n’en porte pas : un nouveau matériau est créé",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Image", defaut: false }),
    },
    Op {
        action: Action::MettreEnPage,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Mettre en page",
        aide: "ranger les îlots de la vue UV dans l’image, sans chevauchement, à la plus grande échelle qui tienne",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Page", defaut: false }),
    },
    Op {
        action: Action::MarquerCoutures,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Marquer les coutures",
        aide: "marquer les arêtes choisies comme coutures : le dépliage coupera là",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Couture", defaut: false }),
    },
    Op {
        action: Action::EffacerCoutures,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Effacer les coutures",
        aide: "retirer les coutures des arêtes choisies",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Sans cout.", defaut: false }),
    },
    Op {
        action: Action::CouturesDepuisUv,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Coutures depuis les UV",
        aide: "marquer une couture là où les UV de deux faces voisines se séparent déjà",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Depuis UV", defaut: false }),
    },
    Op {
        action: Action::Deplier,
        menu: Menu::Geometrie,
        rubrique: "Coordonnées de texture",
        libelle: "Déplier",
        aide: "mettre à plat les faces choisies, ou toutes, coupées à leurs coutures, avec le moins de déformation possible, puis les ranger dans l’image",
        barre: Some(Barre { groupe: Groupe::Geo, glyphe: "▦", court: "Déplier", defaut: false }),
    },
    Op {
        action: Action::DeplacerSet,
        menu: Menu::Objet,
        rubrique: "Actions",
        libelle: "Déplacer vers un set",
        aide: "ranger les objets sélectionnés dans un autre set, ou un nouveau ; un appui long sur un objet de la liste fait de même",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "▦", court: "Set", defaut: false }),
    },
    Op {
        action: Action::AjouterArmature,
        menu: Menu::Scene,
        rubrique: "Armatures",
        libelle: "Armature",
        aide: "ajouter sur le curseur 3D une armature d’un os, squelette destiné à animer des maillages ; c’est un objet de la scène, sans géométrie, matériau ni modifieurs, et son échelle reste uniforme ; si une armature est ouverte en mode Géométrie, la nouvelle s’y ouvre aussitôt",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "🦴", court: "Armature", defaut: true }),
    },
    Op {
        action: Action::AjouterHumanoide,
        menu: Menu::Scene,
        rubrique: "Armatures",
        libelle: "Armature humanoïde",
        aide: "ajouter sur le curseur 3D un squelette humanoïde de 1,75 m en T, de 19 os, origine sous les pieds, regard vers +Z",
        barre: Some(Barre { groupe: Groupe::Ajout, glyphe: "🦴", court: "Humanoïde", defaut: false }),
    },
    Op {
        action: Action::LierArmature,
        menu: Menu::Objet,
        rubrique: "Armature",
        libelle: "Lier à une armature",
        aide: "lier les objets choisis à une armature, à tous ses os, à un groupe d’os ou à un seul ; l’armature emporte alors ses objets ; refaire l’opération change la liaison",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⚭", court: "Lier", defaut: false }),
    },
    Op {
        action: Action::Delier,
        menu: Menu::Objet,
        rubrique: "Armature",
        libelle: "Délier",
        aide: "détacher les objets choisis de leur armature, sans les déplacer",
        barre: Some(Barre { groupe: Groupe::Objet, glyphe: "⚮", court: "Délier", defaut: false }),
    },
    Op {
        action: Action::ReposOs,
        menu: Menu::Os,
        rubrique: "Pose",
        libelle: "Remettre au repos",
        aide: "ramener à leur repos les os choisis, ou tous les os si aucun n’est choisi",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "⟲", court: "Repos", defaut: false }),
    },
    Op {
        action: Action::AjouterOs,
        menu: Menu::Os,
        rubrique: "Os",
        libelle: "Ajouter (sur le curseur 3D)",
        aide: "ajouter un os sans parent, tête sur le curseur 3D, dirigé vers le haut, de la longueur de l’os actif ; il est sélectionné",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "+", court: "Ajouter os", defaut: true }),
    },
    Op {
        action: Action::ExtruderOs,
        menu: Menu::Os,
        rubrique: "Os",
        libelle: "Extruder",
        aide: "prolonger chaque os dont la queue est sélectionnée par un os enfant, soudé, de même direction et de même longueur ; les nouvelles queues sont sélectionnées : glissez pour les placer",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "⤴", court: "Extruder", defaut: true }),
    },
    Op {
        action: Action::SupprimerOs,
        menu: Menu::Os,
        rubrique: "Os",
        libelle: "Supprimer",
        aide: "supprimer les os dont les deux bouts sont sélectionnés ; leurs enfants passent à leur parent, sans bouger ; une armature garde au moins un os",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "✕", court: "Suppr. os", defaut: true }),
    },
    Op {
        action: Action::ParenterOs,
        menu: Menu::Os,
        rubrique: "Hiérarchie",
        libelle: "Parenter à l’actif",
        aide: "rattacher les os choisis à l’os actif, sans les bouger ; refusé pour un os dont l’actif descend (boucle)",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "⇠", court: "Parenter", defaut: true }),
    },
    Op {
        action: Action::DetacherOs,
        menu: Menu::Os,
        rubrique: "Hiérarchie",
        libelle: "Détacher du parent",
        aide: "faire des os choisis des os sans parent, sans les bouger",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "⇥", court: "Détacher", defaut: false }),
    },
    Op {
        action: Action::TousOs,
        menu: Menu::Os,
        rubrique: "Sélection",
        libelle: "Tous les os",
        aide: "sélectionner les deux bouts de tous les os",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "▣", court: "Tous les os", defaut: false }),
    },
    Op {
        action: Action::AucunOs,
        menu: Menu::Os,
        rubrique: "Sélection",
        libelle: "Aucun os",
        aide: "vider la sélection de bouts",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "▢", court: "Aucun os", defaut: false }),
    },
    Op {
        action: Action::DescendantsOs,
        menu: Menu::Os,
        rubrique: "Sélection",
        libelle: "Descendants de l’actif",
        aide: "ajouter à la sélection tous les os qui descendent de l’os actif",
        barre: Some(Barre { groupe: Groupe::Os, glyphe: "⊶", court: "Descendants", defaut: false }),
    },
    Op {
        action: Action::ExporterSage,
        menu: Menu::Exporter,
        rubrique: "Personnage pour S.A.G.E.",
        libelle: "GLB animé",
        aide: "exporter l’armature active (ou celle de l’objet actif) et ses objets liés en un .glb : os au repos, peau aux poids calculés ou peints, animations ; longueurs, limites, ancre et poses nommées dans les extras",
        barre: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn chaque_action_a_une_seule_entree_a_son_rang() {
        for (k, o) in CATALOGUE.iter().enumerate() {
            assert_eq!(o.action as usize, k, "{:?}", o.action);
        }
        assert_eq!(CATALOGUE.len(), Action::ExporterSage as usize + 1);
    }

    #[test]
    fn chaque_entree_est_complete_et_bien_ecrite() {
        for o in CATALOGUE {
            assert!(!o.libelle.is_empty() && !o.rubrique.is_empty(), "{:?}", o.action);
            assert!(!o.aide.is_empty(), "{:?} sans aide", o.action);
            assert!(!o.aide.ends_with('.'), "{:?}", o.action);
            assert!(!o.aide.chars().next().unwrap().is_uppercase() || o.aide.starts_with("OBJ"), "{:?}", o.action);
        }
    }

    #[test]
    fn la_barre_identifie_chaque_operation() {
        let mut vus = HashSet::new();
        for b in CATALOGUE.iter().filter_map(|o| o.barre) {
            assert!(!b.glyphe.is_empty() && !b.court.is_empty());
            assert!(vus.insert((b.groupe, b.court)), "{} en double", b.court);
        }
    }

    #[test]
    fn les_rubriques_d_un_menu_ne_sont_pas_coupees() {
        for m in [Menu::Projet, Menu::Importer, Menu::Exporter, Menu::Vue, Menu::Scene, Menu::Objet, Menu::Geometrie, Menu::Modifieurs, Menu::Os] {
            let mut vues: Vec<&str> = Vec::new();
            for o in du_menu(m) {
                if vues.last() != Some(&o.rubrique) {
                    assert!(!vues.contains(&o.rubrique), "{} en deux morceaux", o.rubrique);
                    vues.push(o.rubrique);
                }
            }
        }
    }

    fn noms(g: &[(Groupe, Vec<&'static Op>)]) -> Vec<&'static str> {
        g.iter().flat_map(|x| &x.1).map(|o| o.barre.unwrap().court).collect()
    }

    #[test]
    fn barre_de_scene_selon_le_choix() {
        let c = Choix::new();
        let vide = Contexte { affiches: 1, ..Default::default() };
        assert_eq!(noms(&barre(&vide, &c)), ["Fichier", "Cube", "Armature"]);
        let un = Contexte { affiches: 1, choisis: 1, editable: true, ..Default::default() };
        assert_eq!(noms(&barre(&un, &c)), ["Dupliquer", "Supprimer", "Géométrie"]);
    }

    #[test]
    fn barre_de_geometrie_commence_par_le_type_actif() {
        let c = Choix::new();
        // Sans sélection, il reste la vue UV
        assert_eq!(noms(&barre(&Contexte { geo: true, ..Default::default() }, &c)), ["UV"]);
        let x = Contexte { geo: true, actif: true, type_actif: Some(Groupe::Sommets), sommets: 3, faces: 1, ..Default::default() };
        let g: Vec<Groupe> = barre(&x, &c).iter().map(|e| e.0).collect();
        assert_eq!(g, [Groupe::ElementActif, Groupe::Sommets, Groupe::Faces, Groupe::Geo]);
        let n = noms(&barre(&x, &c));
        assert_eq!(n[..3], ["Liés", "Face", "Fusionner"]);
        assert!(!n.contains(&"Arête"), "il faut deux sommets exactement");
    }

    #[test]
    fn le_choix_ajoute_et_retire() {
        let mut c = Choix::new();
        c.insert("Ajout:Cube".into(), false);
        c.insert("Ajout:Sphère".into(), true);
        c.insert("Vue:Recadrer".into(), true);
        let vide = Contexte { affiches: 1, ..Default::default() };
        assert_eq!(noms(&barre(&vide, &c)), ["Fichier", "Sphère", "Armature", "Recadrer"]);
        // Une opération du curseur retenue suit la sélection en Géométrie
        c.insert("Objet:Curs. → sél.".into(), true);
        let x = Contexte { geo: true, sommets: 1, ..Default::default() };
        assert!(noms(&barre(&x, &c)).contains(&"Curs. → sél."));
    }

    #[test]
    fn disponibilite_comme_gda() {
        use Action::*;
        let c = Contexte { geo: true, sommets: 2, ..Default::default() };
        assert!(c.possible(CreerArete) && c.possible(Fusionner) && c.possible(Revolution));
        assert!(!c.possible(CreerFace) && !c.possible(Subdiviser) && !c.possible(Inverser) && !c.possible(Dupliquer));
        let c = Contexte { geo: true, sommets: 3, ..Default::default() };
        assert!(c.possible(CreerFace) && !c.possible(CreerArete));
        let c = Contexte { geo: true, aretes: 1, dissolution: Some(Err("bord".into())), ..Default::default() };
        assert!(c.possible(Revolution) && !c.possible(Dissoudre) && !c.possible(Boucle));
        assert_eq!(c.note(Dissoudre).unwrap(), "Dissolution impossible : bord");
        let c = Contexte { affiches: 1, ..Default::default() };
        assert!(c.possible(ToutChoisir) && c.possible(AjouterCube) && !c.possible(Dupliquer) && !c.possible(EditerGeometrie) && !c.possible(Fusionner));
        assert!(!Contexte { geo: true, ..Default::default() }.possible(AjouterCube));
    }
}
