//! Armature : squelette d'os hiérarchisés, comme dans `moteur/armature.js` de GDA.
//! Un os est un repère placé dans celui de son parent, sans échelle ; il part de son
//! origine et s'étend le long de +Y. L'édition le décrit par deux bouts, sa tête et sa
//! queue. La pose donne à chaque os un écart à son repos. Poses nommées, ancre, contraintes
//! et couleurs ne sont pas interprétées ici : elles restent telles quelles dans le fichier.

use crate::edition::{qappliquer, qinv, qmul, qnorm};
use crate::json::J;
use crate::maillage::{defaut, Face, Maillage, P3};
use std::collections::{HashMap, HashSet};

type Q = [f64; 4];

/// En deçà, un os n'a plus de direction.
pub const LONGUEUR_MIN: f64 = 1e-4;
/// Écart entre deux bouts confondus.
pub const SOUDURE: f64 = 1e-5;

#[derive(Clone, Debug, PartialEq)]
pub struct Os {
    pub id: String,
    pub nom: String,
    pub parent: Option<String>,
    pub position: P3,
    pub rotation: Q,
    pub longueur: f64,
    /// Couleur et contraintes, gardées telles quelles.
    pub extra: Vec<(String, J)>,
}

/// Écart d'un os à son repos, dans le repère de son parent, comme dans GDA.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ecart {
    pub r: Q,
    pub t: P3,
}

pub type Pose = HashMap<String, Ecart>;

#[derive(Clone, Debug, PartialEq)]
pub struct Armature {
    pub os: Vec<Os>,
    pub pose: Pose,
    /// Poses nommées, ancre.
    pub reste: Vec<(String, J)>,
}

/// Un os à plat, dans le repère de l'armature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub tete: P3,
    pub queue: P3,
    pub q: Q,
}

pub type Segments = HashMap<String, Segment>;

/// Un bout d'os : sa tête, ou sa queue.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Bout {
    pub os: String,
    pub queue: bool,
}

impl Bout {
    pub fn tete(id: &str) -> Bout {
        Bout { os: id.to_string(), queue: false }
    }

    pub fn queue(id: &str) -> Bout {
        Bout { os: id.to_string(), queue: true }
    }
}

fn scal(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Quaternion qui amène un vecteur unitaire sur un autre, au plus court, comme
/// `setFromUnitVectors` de three.js.
fn entre(a: P3, b: P3) -> Q {
    let mut r = scal(a, b) + 1.0;
    let q = if r < f64::EPSILON {
        r = 0.0;
        if a[0].abs() > a[2].abs() { [-a[1], a[0], 0.0, r] } else { [0.0, -a[2], a[1], r] }
    } else {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0], r]
    };
    qnorm(q)
}

fn plus(a: P3, b: P3) -> P3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn moins(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Quaternion lu : une rotation déjà unitaire n'est pas recalculée, le fichier reste exact.
fn unitaire(j: Option<&J>) -> Q {
    j.and_then(J::nombres::<4>).filter(|q| q.iter().map(|x| x * x).sum::<f64>() > 0.0).map_or([0.0, 0.0, 0.0, 1.0], |q| {
        if (q.iter().map(|x| x * x).sum::<f64>() - 1.0).abs() < 1e-12 { q } else { qnorm(q) }
    })
}

impl Armature {
    /// Une armature d'un os.
    pub fn nouvelle() -> Armature {
        let mut a = Armature { os: Vec::new(), pose: Pose::new(), reste: Vec::new() };
        a.ajouter("os", None, [0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0);
        a
    }

    pub fn trouver(&self, id: &str) -> Option<&Os> {
        self.os.iter().find(|b| b.id == id)
    }

    fn trouver_mut(&mut self, id: &str) -> Option<&mut Os> {
        self.os.iter_mut().find(|b| b.id == id)
    }

    pub fn enfants<'a>(&'a self, id: Option<&'a str>) -> impl Iterator<Item = &'a Os> + 'a {
        self.os.iter().filter(move |b| b.parent.as_deref() == id)
    }

    pub fn descendants(&self, id: &str) -> Vec<String> {
        let mut r = Vec::new();
        for b in self.enfants(Some(id)) {
            r.push(b.id.clone());
            r.extend(self.descendants(&b.id));
        }
        r
    }

    /// Os dans l'ordre de la hiérarchie : chaque parent avant ses enfants.
    pub fn ordre(&self) -> Vec<&Os> {
        fn voir<'a>(a: &'a Armature, p: Option<&'a str>, r: &mut Vec<&'a Os>) {
            for b in a.enfants(p) {
                r.push(b);
                voir(a, Some(&b.id), r);
            }
        }
        let mut r = Vec::with_capacity(self.os.len());
        voir(self, None, &mut r);
        r
    }

    /// Profondeur d'un os dans la hiérarchie.
    pub fn profondeur(&self, id: &str) -> usize {
        let mut n = 0;
        let mut b = self.trouver(id);
        while let Some(p) = b.and_then(|x| x.parent.as_deref()) {
            n += 1;
            b = self.trouver(p);
        }
        n
    }

    fn id_libre(&self) -> String {
        let pris: HashSet<&str> = self.os.iter().map(|b| b.id.as_str()).collect();
        let mut n = self.os.len() + 1;
        while pris.contains(format!("o{n}").as_str()) {
            n += 1;
        }
        format!("o{n}")
    }

    pub fn nom_libre(&self, base: &str) -> String {
        let pris: HashSet<&str> = self.os.iter().map(|b| b.nom.as_str()).collect();
        if !pris.contains(base) {
            return base.to_string();
        }
        let mut n = 2;
        while pris.contains(format!("{base} {n}").as_str()) {
            n += 1;
        }
        format!("{base} {n}")
    }

    pub fn ajouter(&mut self, nom: &str, parent: Option<String>, position: P3, rotation: Q, longueur: f64) -> String {
        let id = self.id_libre();
        self.os.push(Os { id: id.clone(), nom: self.nom_libre(nom), parent, position, rotation, longueur, extra: Vec::new() });
        id
    }

    /// Os allant de `tete` à `queue`, deux points dans le repère de l'armature.
    pub fn ajouter_segment(&mut self, nom: &str, parent: Option<&str>, tete: P3, queue: P3) -> Option<String> {
        let d = moins(queue, tete);
        let longueur = scal(d, d).sqrt();
        if longueur <= 0.0 {
            return None;
        }
        let mut q = entre([0.0, 1.0, 0.0], d.map(|x| x / longueur));
        let mut t = tete;
        if let Some((pp, qp)) = parent.and_then(|p| self.repos().get(p).copied()) {
            let i = qinv(qp);
            t = qappliquer(i, moins(t, pp));
            q = qmul(i, q);
        }
        Some(self.ajouter(nom, parent.map(str::to_string), t, q, longueur))
    }

    /// Queue de l'os prolongée par un enfant de même direction et de même longueur.
    pub fn extruder(&mut self, id: &str) -> Option<String> {
        let b = self.trouver(id)?;
        let (nom, l) = (b.nom.clone(), b.longueur);
        Some(self.ajouter(&nom, Some(id.to_string()), [0.0, l, 0.0], [0.0, 0.0, 0.0, 1.0], l))
    }

    /// Retire un os ; ses enfants passent à son parent, sans bouger.
    pub fn retirer(&mut self, id: &str) {
        let Some(k) = self.os.iter().position(|b| b.id == id) else { return };
        let s = self.segments();
        let parent = self.os[k].parent.clone();
        for e in self.os.iter_mut().filter(|e| e.parent.as_deref() == Some(id)) {
            e.parent = parent.clone();
        }
        self.os.remove(k);
        self.pose.remove(id);
        self.appliquer_segments(&s);
    }

    /// Rattache un os sans le bouger ; faux s'il formerait une boucle.
    pub fn changer_parent(&mut self, id: &str, parent: Option<&str>) -> bool {
        if parent == Some(id) || parent.is_some_and(|p| self.descendants(id).iter().any(|d| d == p)) {
            return false;
        }
        let s = self.segments();
        if let Some(b) = self.trouver_mut(id) {
            b.parent = parent.map(str::to_string);
        }
        self.appliquer_segments(&s);
        true
    }

    /// Pose de repos : position et rotation de chaque os dans le repère de l'armature.
    pub fn repos(&self) -> HashMap<String, (P3, Q)> {
        self.reperes(&Pose::new())
    }

    /// Pose courante : position et rotation de chaque os dans le repère de l'armature.
    pub fn matrices_pose(&self) -> HashMap<String, (P3, Q)> {
        self.reperes(&self.pose)
    }

    fn reperes(&self, pose: &Pose) -> HashMap<String, (P3, Q)> {
        let mut r: HashMap<String, (P3, Q)> = HashMap::with_capacity(self.os.len());
        for b in self.ordre() {
            let (t, q) = pose.get(&b.id).map_or((b.position, b.rotation), |e| (plus(b.position, e.t), qmul(b.rotation, e.r)));
            let m = match b.parent.as_ref().and_then(|p| r.get(p)) {
                Some((pp, qp)) => (plus(*pp, qappliquer(*qp, t)), qmul(*qp, q)),
                None => (t, q),
            };
            r.insert(b.id.clone(), m);
        }
        r
    }

    /// Tourne des os autour de leur tête, de `q` dans le repère de l'armature. Aucun
    /// d'eux ne doit descendre d'un autre : leurs enfants suivent d'eux-mêmes.
    pub fn tourner(&mut self, ids: &[String], q: Q) {
        let m = self.matrices_pose();
        for id in ids {
            let (Some(b), Some((_, qb))) = (self.trouver(id), m.get(id)) else { continue };
            let qp = b.parent.as_ref().and_then(|p| m.get(p)).map_or([0.0, 0.0, 0.0, 1.0], |x| x.1);
            let local = qmul(qinv(qp), qmul(q, *qb));
            let r = qnorm(qmul(qinv(b.rotation), local));
            let e = self.pose.entry(id.clone()).or_insert(Ecart { r, t: [0.0; 3] });
            e.r = r;
        }
    }

    /// Ces os reviennent à leur repos.
    pub fn remettre(&mut self, ids: &[String]) {
        for id in ids {
            self.pose.remove(id);
        }
    }

    /// Transformation rigide d'un os, de son repos à la pose, dans le repère de
    /// l'armature : tête au repos, tête posée, rotation. Rien si l'os ne bouge pas.
    pub fn ecart_os(&self, id: &str) -> Option<(P3, P3, Q)> {
        let ((pr, qr), (pp, qp)) = (*self.repos().get(id)?, *self.matrices_pose().get(id)?);
        (pr != pp || qr != qp).then(|| (pr, pp, qnorm(qmul(qp, qinv(qr)))))
    }

    /// Pose de repos à plat.
    pub fn segments(&self) -> Segments {
        self.segments_de(&self.repos())
    }

    /// Pose courante à plat.
    pub fn segments_pose(&self) -> Segments {
        self.segments_de(&self.matrices_pose())
    }

    fn segments_de(&self, m: &HashMap<String, (P3, Q)>) -> Segments {
        self.os.iter().filter_map(|b| {
            let (p, q) = *m.get(&b.id)?;
            Some((b.id.clone(), Segment { tete: p, queue: plus(p, qappliquer(q, [0.0, b.longueur, 0.0])), q }))
        }).collect()
    }

    /// Replace tous les os d'après une table : chaque repère local est recalculé depuis
    /// son parent, et `q` tourné au plus court vers la direction de l'os, ce qui garde
    /// son roulis.
    pub fn appliquer_segments(&mut self, table: &Segments) {
        let ordre: Vec<String> = self.ordre().iter().map(|b| b.id.clone()).collect();
        let mut monde: HashMap<String, (P3, Q)> = HashMap::with_capacity(ordre.len());
        for id in ordre {
            let Some(s) = table.get(&id) else { continue };
            let d = moins(s.queue, s.tete);
            let l = scal(d, d).sqrt();
            let mut q = s.q;
            if l > LONGUEUR_MIN {
                q = qnorm(qmul(entre(qappliquer(q, [0.0, 1.0, 0.0]), d.map(|x| x / l)), q));
            }
            let parent = self.trouver(&id).and_then(|b| b.parent.clone()).and_then(|p| monde.get(&p).copied());
            if let Some(b) = self.trouver_mut(&id) {
                b.longueur = l.max(LONGUEUR_MIN);
                match parent {
                    Some((pp, qp)) => {
                        let i = qinv(qp);
                        b.position = qappliquer(i, moins(s.tete, pp));
                        b.rotation = qnorm(qmul(i, q));
                    }
                    None => {
                        b.position = s.tete;
                        b.rotation = q;
                    }
                }
            }
            monde.insert(id, (s.tete, q));
        }
    }

    /// Bouts soudés : pour chaque bout, ceux qui lui sont confondus, lui compris. La
    /// tête d'un enfant posée sur la queue de son parent y est soudée.
    pub fn soudures(&self) -> HashMap<Bout, Vec<Bout>> {
        let s = self.segments();
        let mut r: HashMap<Bout, Vec<Bout>> = HashMap::with_capacity(self.os.len() * 2);
        for b in &self.os {
            for x in [Bout::tete(&b.id), Bout::queue(&b.id)] {
                r.insert(x.clone(), vec![x]);
            }
        }
        for b in self.ordre() {
            let Some(p) = &b.parent else { continue };
            let (Some(q), Some(t)) = (s.get(p), s.get(&b.id)) else { continue };
            if (0..3).any(|k| (t.tete[k] - q.queue[k]).abs() > SOUDURE) {
                continue;
            }
            let qp = Bout::queue(p);
            let mut g = r[&qp].clone();
            g.push(Bout::tete(&b.id));
            for x in &g {
                r.insert(x.clone(), g.clone());
            }
        }
        r
    }

    /// Dessin des os : un octaèdre allongé par os, de la tête à la queue.
    pub fn enveloppe(&self) -> Maillage {
        self.enveloppe_de(&self.repos())
    }

    /// Dessin des os dans la pose courante.
    pub fn enveloppe_pose(&self) -> Maillage {
        self.enveloppe_de(&self.matrices_pose())
    }

    fn enveloppe_de(&self, m: &HashMap<String, (P3, Q)>) -> Maillage {
        let (mut sommets, mut faces) = (Vec::with_capacity(self.os.len() * 6), Vec::with_capacity(self.os.len() * 8));
        let face = |s: Vec<usize>| Face { s, m: defaut(), uv: Vec::new() };
        for b in self.ordre() {
            let (l, y, r, base) = (b.longueur, b.longueur * 0.1, b.longueur * 0.1, sommets.len());
            let (p, q) = m[&b.id];
            for c in [[0.0, 0.0, 0.0], [0.0, l, 0.0], [r, y, 0.0], [0.0, y, -r], [-r, y, 0.0], [0.0, y, r]] {
                sommets.push(plus(p, qappliquer(q, c)));
            }
            for k in 0..4 {
                let (a, c) = (base + 2 + k, base + 2 + (k + 1) % 4);
                faces.push(face(vec![base + 1, a, c]));
                faces.push(face(vec![base, c, a]));
            }
        }
        Maillage::new(sommets, faces, Vec::new())
    }

    /// Squelette humanoïde de 1,75 m en T : origine sous les pieds, Y vers le haut, regard
    /// vers +Z, côté gauche du personnage en +X.
    pub fn humanoide() -> Armature {
        let mut a = Armature { os: Vec::new(), pose: Pose::new(), reste: Vec::new() };
        let mut os = |nom: &str, p: Option<&str>, t: P3, q: P3| a.ajouter_segment(nom, p, t, q).unwrap();
        let bassin = os("bassin", None, [0.0, 0.95, 0.0], [0.0, 1.05, 0.0]);
        let colonne = os("colonne", Some(&bassin), [0.0, 1.05, 0.0], [0.0, 1.25, 0.0]);
        let thorax = os("thorax", Some(&colonne), [0.0, 1.25, 0.0], [0.0, 1.45, 0.0]);
        let cou = os("cou", Some(&thorax), [0.0, 1.45, 0.0], [0.0, 1.53, 0.0]);
        os("tete", Some(&cou), [0.0, 1.53, 0.0], [0.0, 1.75, 0.0]);
        for (s, c) in [(1.0, "G"), (-1.0, "D")] {
            let clavicule = os(&format!("clavicule.{c}"), Some(&thorax), [s * 0.04, 1.42, 0.0], [s * 0.18, 1.42, 0.0]);
            let bras = os(&format!("bras.{c}"), Some(&clavicule), [s * 0.18, 1.42, 0.0], [s * 0.46, 1.42, 0.0]);
            let avant = os(&format!("avant-bras.{c}"), Some(&bras), [s * 0.46, 1.42, 0.0], [s * 0.72, 1.42, 0.0]);
            os(&format!("main.{c}"), Some(&avant), [s * 0.72, 1.42, 0.0], [s * 0.82, 1.42, 0.0]);
            let cuisse = os(&format!("cuisse.{c}"), Some(&bassin), [s * 0.1, 0.95, 0.0], [s * 0.1, 0.52, 0.01]);
            let jambe = os(&format!("jambe.{c}"), Some(&cuisse), [s * 0.1, 0.52, 0.01], [s * 0.1, 0.1, 0.0]);
            os(&format!("pied.{c}"), Some(&jambe), [s * 0.1, 0.1, 0.0], [s * 0.1, 0.02, 0.16]);
        }
        a
    }

    // ----- Données ---------------------------------------------------------

    /// Données vérifiées, comme dans GDA : parent inconnu ou boucle → os racine ; aucune
    /// donnée valable → un os seul.
    /// Montrée au premier plan, à travers les autres objets, en mode Scène.
    pub fn devant(&self) -> bool {
        self.reste.iter().any(|(k, v)| k == "devant" && *v == J::Bool(true))
    }

    pub fn regler_devant(&mut self, oui: bool) {
        self.reste.retain(|(k, _)| k != "devant");
        if oui {
            self.reste.push(("devant".into(), J::Bool(true)));
        }
    }

    pub fn depuis_donnees(d: &J) -> Armature {
        let mut os: Vec<Os> = Vec::new();
        let mut ids = HashSet::new();
        for b in d.get("os").and_then(J::liste).unwrap_or(&[]) {
            let Some(id) = b.get("id").and_then(J::txt).filter(|i| !i.is_empty()) else { continue };
            if !ids.insert(id.to_string()) {
                continue;
            }
            let rotation = unitaire(b.get("rotation"));
            let extra = match b {
                J::Objet(l) => l.iter().filter(|x| !["id", "nom", "parent", "position", "rotation", "longueur"].contains(&x.0.as_str())).cloned().collect(),
                _ => Vec::new(),
            };
            os.push(Os {
                id: id.to_string(),
                nom: b.get("nom").and_then(J::txt).filter(|n| !n.is_empty()).unwrap_or("os").to_string(),
                parent: b.get("parent").and_then(J::txt).map(str::to_string),
                position: b.get("position").and_then(J::nombres::<3>).unwrap_or([0.0; 3]),
                rotation,
                longueur: b.get("longueur").and_then(J::num).filter(|l| *l > 0.0 && l.is_finite()).unwrap_or(1.0),
                extra,
            });
        }
        for b in &mut os {
            if b.parent.as_ref().is_some_and(|p| !ids.contains(p)) {
                b.parent = None;
            }
        }
        let reste = match d {
            J::Objet(l) => l.iter().filter(|x| x.0 != "os" && x.0 != "pose").cloned().collect(),
            _ => Vec::new(),
        };
        let mut pose = Pose::new();
        if let Some(J::Objet(l)) = d.get("pose") {
            for (id, e) in l.iter().filter(|x| ids.contains(&x.0)) {
                pose.insert(id.clone(), Ecart { r: unitaire(e.get("r")), t: e.get("t").and_then(J::nombres::<3>).unwrap_or([0.0; 3]) });
            }
        }
        let mut a = Armature { os, pose, reste };
        let atteints: HashSet<String> = a.ordre().iter().map(|b| b.id.clone()).collect();
        for b in &mut a.os {
            if !atteints.contains(&b.id) {
                b.parent = None;
            }
        }
        if a.os.is_empty() {
            a.ajouter("os", None, [0.0; 3], [0.0, 0.0, 0.0, 1.0], 1.0);
        }
        a
    }

    /// Données du fichier. La pose des os retirés disparaît avec eux.
    pub fn vers_donnees(&self) -> J {
        let nums = |v: &[f64]| J::Liste(v.iter().map(|x| J::Num(*x)).collect());
        let c = |k: &str, v: J| (k.to_string(), v);
        let os = self.os.iter().map(|b| {
            let mut l = vec![
                c("id", J::Txt(b.id.clone())),
                c("nom", J::Txt(b.nom.clone())),
                c("parent", b.parent.clone().map_or(J::Nul, J::Txt)),
                c("position", nums(&b.position)),
                c("rotation", nums(&b.rotation)),
                c("longueur", J::Num(b.longueur)),
            ];
            l.extend(b.extra.iter().cloned());
            J::Objet(l)
        }).collect();
        let ids: HashSet<&str> = self.os.iter().map(|b| b.id.as_str()).collect();
        let garder = |p: &J| match p {
            J::Objet(l) => J::Objet(l.iter().filter(|x| ids.contains(x.0.as_str())).cloned().collect()),
            x => x.clone(),
        };
        let pose = self.os.iter().filter_map(|b| {
            let e = self.pose.get(&b.id)?;
            Some((b.id.clone(), J::Objet(vec![c("r", nums(&e.r)), c("t", nums(&e.t))])))
        }).collect();
        let mut d = vec![c("os", J::Liste(os)), c("pose", J::Objet(pose))];
        for (k, v) in &self.reste {
            d.push((k.clone(), match k.as_str() {
                "ancre" => if v.txt().is_some_and(|i| ids.contains(i)) { v.clone() } else { J::Nul },
                "poses" => match v {
                    J::Liste(l) => J::Liste(l.iter().map(|p| match p {
                        J::Objet(x) => J::Objet(x.iter().map(|(n, w)| (n.clone(), if n == "os" { garder(w) } else { w.clone() })).collect()),
                        y => y.clone(),
                    }).collect()),
                    x => x.clone(),
                },
                _ => v.clone(),
            }));
        }
        J::Objet(d)
    }
}

/// Sélection du mode Armature : des bouts d'os, dans l'ordre, et l'os touché en dernier.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditionOs {
    /// Rang de l'armature dans la scène ; aucun tant qu'il n'y en a pas de choisie.
    pub objet: Option<usize>,
    pub bouts: Vec<Bout>,
    pub actif: Option<String>,
    /// Change à chaque modification : le dessin s'y accroche.
    pub version: u64,
    /// En pose : les os tournent, sans changer leur repos.
    pub pose: bool,
}

/// Ce qu'un doigt touche : un os, et son bout s'il y en a un (sinon son corps).
#[derive(Clone, Debug, PartialEq)]
pub struct Touche {
    pub os: String,
    pub queue: Option<bool>,
}

impl EditionOs {
    /// Os dont les deux bouts sont sélectionnés.
    pub fn os_choisis(&self, a: &Armature) -> Vec<String> {
        a.os.iter().filter(|b| self.bouts.contains(&Bout::tete(&b.id)) && self.bouts.contains(&Bout::queue(&b.id))).map(|b| b.id.clone()).collect()
    }

    /// Os choisis sans ancêtre choisi : les autres suivent d'eux-mêmes.
    pub fn racines(&self, a: &Armature) -> Vec<String> {
        let l = self.os_choisis(a);
        l.iter().filter(|i| {
            let mut b = a.trouver(i);
            while let Some(p) = b.and_then(|x| x.parent.as_ref()) {
                if l.contains(p) {
                    return false;
                }
                b = a.trouver(p);
            }
            true
        }).cloned().collect()
    }

    /// Os dont la queue est sélectionnée.
    pub fn queues_choisies(&self, a: &Armature) -> Vec<String> {
        self.bouts.iter().filter(|b| b.queue && a.trouver(&b.os).is_some()).map(|b| b.os.clone()).collect()
    }

    /// Bouts visés : le bout touché, ou les deux, avec ce qui leur est soudé.
    pub fn cles_de(a: &Armature, t: &Touche) -> Vec<Bout> {
        let s = a.soudures();
        let mut r: Vec<Bout> = Vec::new();
        let bouts = match t.queue {
            Some(q) => vec![Bout { os: t.os.clone(), queue: q }],
            None => vec![Bout::tete(&t.os), Bout::queue(&t.os)],
        };
        for x in bouts.iter().flat_map(|b| s.get(b).into_iter().flatten()) {
            if !r.contains(x) {
                r.push(x.clone());
            }
        }
        r
    }

    /// Bascule ce que vise un toucher.
    pub fn basculer(&mut self, a: &Armature, t: &Touche) {
        let cles = Self::cles_de(a, t);
        if cles.iter().all(|c| self.bouts.contains(c)) {
            self.bouts.retain(|b| !cles.contains(b));
        } else {
            for c in cles {
                if !self.bouts.contains(&c) {
                    self.bouts.push(c);
                }
            }
        }
        self.actif = Some(t.os.clone());
        self.version += 1;
    }

    /// Les bouts de ces os deviennent la sélection, ou s'y ajoutent.
    pub fn selectionner(&mut self, a: &Armature, ids: &[String], ajout: bool) {
        if !ajout {
            self.bouts.clear();
        }
        let s = a.soudures();
        for id in ids {
            for x in [Bout::tete(id), Bout::queue(id)].iter().flat_map(|b| s.get(b).into_iter().flatten()) {
                if !self.bouts.contains(x) {
                    self.bouts.push(x.clone());
                }
            }
        }
        self.version += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pres(a: P3, b: P3) -> bool {
        (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9)
    }

    #[test]
    fn un_os_nouveau_va_de_l_origine_le_long_de_y() {
        let a = Armature::nouvelle();
        let s = &a.segments()["o1"];
        assert!(pres(s.tete, [0.0; 3]) && pres(s.queue, [0.0, 1.0, 0.0]));
    }

    #[test]
    fn un_segment_enfant_a_ses_bouts_au_bon_endroit() {
        let mut a = Armature { os: vec![], pose: Pose::new(), reste: vec![] };
        let b = a.ajouter_segment("bras", None, [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]).unwrap();
        let m = a.ajouter_segment("main", Some(&b), [1.0, 1.0, 0.0], [1.0, 2.0, 0.0]).unwrap();
        let s = a.segments();
        assert!(pres(s[&m].tete, [1.0, 1.0, 0.0]) && pres(s[&m].queue, [1.0, 2.0, 0.0]));
        assert_eq!(a.soudures()[&Bout::tete(&m)].len(), 2);
        assert!(a.ajouter_segment("nul", None, [1.0; 3], [1.0; 3]).is_none());
    }

    #[test]
    fn extruder_prolonge_et_soude() {
        let mut a = Armature::nouvelle();
        let e = a.extruder("o1").unwrap();
        let s = a.segments();
        assert!(pres(s[&e].queue, [0.0, 2.0, 0.0]));
        assert_eq!(a.trouver(&e).unwrap().nom, "os 2");
        assert!(a.soudures()[&Bout::queue("o1")].contains(&Bout::tete(&e)));
    }

    #[test]
    fn retirer_et_rattacher_ne_bougent_pas_les_autres() {
        let mut a = Armature::humanoide();
        let avant = a.segments();
        let id = a.os.iter().find(|b| b.nom == "colonne").unwrap().id.clone();
        a.retirer(&id);
        let apres = a.segments();
        assert_eq!(apres.len(), avant.len() - 1);
        for (k, s) in &apres {
            assert!(pres(s.tete, avant[k].tete) && pres(s.queue, avant[k].queue), "{k}");
        }
        let thorax = a.os.iter().find(|b| b.nom == "thorax").unwrap().id.clone();
        assert!(!a.changer_parent(&a.os[0].id.clone(), Some(&thorax)), "boucle");
        assert!(a.changer_parent(&thorax, None));
        assert!(pres(a.segments()[&thorax].tete, apres[&thorax].tete));
    }

    #[test]
    fn deplacer_une_queue_garde_les_enfants_en_place() {
        let mut a = Armature::nouvelle();
        let e = a.extruder("o1").unwrap();
        let mut s = a.segments();
        let (t, q) = (s[&e].tete, s[&e].queue);
        s.get_mut("o1").unwrap().queue = [1.0, 1.0, 0.0];
        a.appliquer_segments(&s);
        let n = a.segments();
        assert!(pres(n[&e].tete, t) && pres(n[&e].queue, q));
        assert!((a.trouver("o1").unwrap().longueur - 2f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn un_os_ecrase_garde_une_longueur_minimale() {
        let mut a = Armature::nouvelle();
        let mut s = a.segments();
        s.get_mut("o1").unwrap().queue = [0.0; 3];
        a.appliquer_segments(&s);
        assert_eq!(a.os[0].longueur, LONGUEUR_MIN);
    }

    #[test]
    fn les_donnees_abimees_donnent_une_armature_valide() {
        let d = crate::json::lire(r#"{"os":[{"id":"a","parent":"b"},{"id":"b","parent":"a"},{"id":"a"},{"id":"c","parent":"inconnu","longueur":-1,"rotation":[0,0,0,0]}]}"#).unwrap();
        let a = Armature::depuis_donnees(&d);
        assert_eq!(a.os.len(), 3);
        assert_eq!(a.ordre().len(), 3);
        assert_eq!(a.trouver("c").unwrap().parent, None);
        assert_eq!(a.trouver("c").unwrap().longueur, 1.0);
        assert_eq!(Armature::depuis_donnees(&J::Objet(vec![])).os.len(), 1);
    }

    #[test]
    fn ce_qui_n_est_pas_interprete_survit_et_suit_les_os_retires() {
        let t = r##"{"os":[{"id":"a","nom":"a","parent":null,"position":[0,0,0],"rotation":[0,0,0,1],"longueur":1,"couleur":"#ff0000"},{"id":"b","nom":"b","parent":"a","position":[0,1,0],"rotation":[0,0,0,1],"longueur":1,"contraintes":{"r":[{"etat":"bloque"}]}}],"pose":{"b":{"r":[0,0,0,1],"t":[0,0,0]}},"poses":[{"nom":"p","os":{"b":{"r":[0,0,0,1],"t":[0,0,0]}}}],"ancre":"b"}"##;
        let d = crate::json::lire(t).unwrap();
        let mut a = Armature::depuis_donnees(&d);
        assert_eq!(a.vers_donnees(), d);
        a.retirer("b");
        let v = a.vers_donnees();
        assert_eq!(v.get("pose"), Some(&J::Objet(vec![])));
        assert_eq!(v.get("ancre"), Some(&J::Nul));
        assert_eq!(v.get("os").unwrap().liste().unwrap()[0].get("couleur").and_then(J::txt), Some("#ff0000"));
    }

    #[test]
    fn toucher_un_bout_prend_ce_qui_y_est_soude_et_le_corps_prend_tout() {
        let mut a = Armature::nouvelle();
        let f = a.extruder("o1").unwrap();
        let mut e = EditionOs::default();
        e.basculer(&a, &Touche { os: "o1".into(), queue: Some(true) });
        assert_eq!(e.bouts, [Bout::queue("o1"), Bout::tete(&f)]);
        assert!(e.os_choisis(&a).is_empty());
        assert_eq!(e.queues_choisies(&a), ["o1"]);
        e.basculer(&a, &Touche { os: "o1".into(), queue: None });
        assert_eq!(e.os_choisis(&a), ["o1"]);
        e.basculer(&a, &Touche { os: "o1".into(), queue: None });
        assert!(e.bouts.is_empty());
        e.selectionner(&a, &[f.clone()], false);
        assert_eq!(e.os_choisis(&a), [f]);
    }

    fn humain() -> (Armature, String, String) {
        let a = Armature::humanoide();
        let i = |n: &str| a.os.iter().find(|b| b.nom == n).unwrap().id.clone();
        let (x, y) = (i("bras.G"), i("avant-bras.G"));
        (a, x, y)
    }

    fn proche(a: P3, b: P3) {
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9), "{a:?} / {b:?}");
    }

    #[test]
    fn tourner_garde_la_tete_et_emporte_les_enfants() {
        let (mut a, bras, avant) = humain();
        let s = a.segments();
        let q = crate::edition::qaxe([0.0, 0.0, 1.0], 0.7);
        a.tourner(std::slice::from_ref(&bras), q);
        let p = a.segments_pose();
        proche(p[&bras].tete, s[&bras].tete);
        let rel = |v: P3| [0, 1, 2].map(|k| v[k] - s[&bras].tete[k]);
        proche(p[&avant].queue, plus(s[&bras].tete, qappliquer(q, rel(s[&avant].queue))));
        // Le repos ne change pas, et une seconde rotation s'ajoute
        assert_eq!(a.segments(), s);
        a.tourner(std::slice::from_ref(&bras), q);
        let q2 = qmul(q, q);
        proche(a.segments_pose()[&bras].queue, plus(s[&bras].tete, qappliquer(q2, rel(s[&bras].queue))));
    }

    #[test]
    fn tourner_un_enfant_part_de_la_pose_de_son_parent() {
        let (mut a, bras, avant) = humain();
        let q = crate::edition::qaxe([0.0, 0.0, 1.0], 0.5);
        let r = crate::edition::qaxe([1.0, 0.0, 0.0], 0.3);
        a.tourner(std::slice::from_ref(&bras), q);
        let (t, d) = { let s = a.segments_pose(); (s[&avant].tete, moins(s[&avant].queue, s[&avant].tete)) };
        a.tourner(std::slice::from_ref(&avant), r);
        let s = a.segments_pose();
        proche(s[&avant].tete, t);
        proche(s[&avant].queue, plus(t, qappliquer(r, d)));
    }

    #[test]
    fn un_os_retire_ou_remis_quitte_la_pose() {
        let (mut a, bras, avant) = humain();
        let q = crate::edition::qaxe([0.0, 0.0, 1.0], 0.5);
        a.tourner(&[bras.clone(), avant.clone()], q);
        a.remettre(std::slice::from_ref(&avant));
        assert_eq!(a.pose.len(), 1);
        assert!(a.ecart_os(&a.os[0].id.clone()).is_none());
        a.retirer(&bras);
        assert!(a.pose.is_empty());
        assert_eq!(a.segments_pose(), a.segments());
    }

    #[test]
    fn seules_les_racines_du_choix_tournent() {
        let (a, bras, avant) = humain();
        let mut e = EditionOs::default();
        e.selectionner(&a, &[avant.clone(), bras.clone()], false);
        assert_eq!(e.racines(&a), [bras]);
    }

    #[test]
    fn une_pose_lue_ne_garde_que_les_os_connus_et_les_quaternions_normes() {
        let t = r#"{"os":[{"id":"a","nom":"a","parent":null,"position":[0,0,0],"rotation":[0,0,0,1],"longueur":1}],"pose":{"a":{"r":[0,0,0,2]},"z":{"r":[0,0,0,1],"t":[0,0,0]}}}"#;
        let a = Armature::depuis_donnees(&crate::json::lire(t).unwrap());
        assert_eq!(a.pose.len(), 1);
        assert_eq!(a.pose["a"], Ecart { r: [0.0, 0.0, 0.0, 1.0], t: [0.0; 3] });
    }
}
