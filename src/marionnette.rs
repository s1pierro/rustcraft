//! Saisie directe d'une armature posée, comme `moteur/marionnette.js` de GDA : l'ancre
//! reste fixe dans le monde, et tirer un os plie la chaîne d'articulations qui le relie à
//! l'ancre, en descente cyclique de coordonnées (CCD). À chaque articulation libre du
//! chemin, le côté saisi tourne vers la cible, puis la rotation est ramenée aux limites.
//! Une articulation lue à l'envers depuis l'ancre (`inverse`) fait tourner son côté
//! parent : l'écart local prend la rotation contraire. Tout est dans le repère de
//! l'armature ; rien ici ne touche à l'interface.

use crate::armature::{angles_os, Armature, Ecart, Limite, Pose};
use crate::edition::{qappliquer, qaxe, qinv, qmul, qnorm};
use crate::maillage::P3;
use std::collections::HashMap;

type Q = [f64; 4];
type Reperes = HashMap<String, (P3, Q)>;

const ID: Q = [0.0, 0.0, 0.0, 1.0];

/// Un pas de la chaîne : l'articulation de l'os `j`, et son sens.
#[derive(Clone, Debug, PartialEq)]
pub struct Pas {
    pub j: String,
    pub inverse: bool,
}

/// Une saisie en cours : l'os tenu, le point tenu dans son repère, la chaîne jusqu'à
/// l'ancre, les repères et la pose du début.
#[derive(Clone, Debug)]
pub struct Saisie {
    pub os: String,
    pub local: P3,
    pub chaine: Vec<Pas>,
    pub avant: Reperes,
    pub depart: Pose,
}

fn sous(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scal(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vect(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norme2(a: P3) -> f64 {
    scal(a, a)
}

fn unitaire(a: P3) -> P3 {
    let l = norme2(a).sqrt();
    if l > 0.0 { a.map(|x| x / l) } else { a }
}

/// Rotation la plus courte de `u` vers `v`, unitaires, comme `setFromUnitVectors` de three.js.
fn entre(u: P3, v: P3) -> Q {
    let r = scal(u, v) + 1.0;
    let q = if r < f64::EPSILON {
        if u[0].abs() > u[2].abs() { [-u[1], u[0], 0.0, 0.0] } else { [0.0, -u[2], u[1], 0.0] }
    } else {
        let c = vect(u, v);
        [c[0], c[1], c[2], r]
    };
    qnorm(q)
}

/// Repère `m` appliqué à un point.
fn appliquer(m: (P3, Q), p: P3) -> P3 {
    let v = qappliquer(m.1, p);
    [m.0[0] + v[0], m.0[1] + v[1], m.0[2] + v[2]]
}

fn inverse(m: (P3, Q)) -> (P3, Q) {
    let qi = qinv(m.1);
    (qappliquer(qi, m.0).map(|x| -x), qi)
}

fn composer(a: (P3, Q), b: (P3, Q)) -> (P3, Q) {
    (appliquer(a, b.0), qmul(a.1, b.1))
}

impl Armature {
    /// L'os en contact avec le monde, tel qu'enregistré (`ancre` de GDA), s'il existe.
    pub fn ancre_gardee(&self) -> Option<String> {
        self.reste.iter().find(|(k, _)| k == "ancre").and_then(|(_, v)| v.txt()).filter(|i| self.trouver(i).is_some()).map(str::to_string)
    }

    /// L'ancre : celle enregistrée, sinon la racine du premier os.
    pub fn ancre(&self) -> Option<String> {
        self.ancre_gardee().or_else(|| self.os.first().map(|b| self.racine(&b.id)))
    }

    pub fn regler_ancre(&mut self, id: Option<&str>) {
        self.reste.retain(|(k, _)| k != "ancre");
        if let Some(i) = id.filter(|i| self.trouver(i).is_some()) {
            self.reste.push(("ancre".into(), crate::json::J::Txt(i.into())));
        }
    }

    pub fn racine(&self, id: &str) -> String {
        let mut i = id.to_string();
        while let Some(p) = self.trouver(&i).and_then(|b| b.parent.clone()) {
            i = p;
        }
        i
    }

    /// Articulation immobile : les trois rotations bloquées, l'os fait bloc avec son
    /// parent. Une racine n'en a pas.
    pub fn rigide(&self, id: &str) -> bool {
        self.trouver(id).is_some_and(|b| b.parent.is_some() && b.limites() == [Limite::Bloquee; 3])
    }

    /// Pose déduite des repères de chaque os, dans le repère de l'armature.
    pub fn pose_depuis_reperes(&mut self, m: &Reperes) {
        let mut pose = Pose::new();
        for b in &self.os {
            let Some(&mb) = m.get(&b.id) else { continue };
            let l = match b.parent.as_ref().and_then(|p| m.get(p)) {
                Some(&mp) => composer(inverse(mp), mb),
                None => mb,
            };
            let r = qmul(qinv(b.rotation), qnorm(l.1));
            let e = sous(l.0, b.position);
            if (r[3].abs() - 1.0).abs() > 1e-10 || norme2(e) > 1e-16 {
                pose.insert(b.id.clone(), Ecart { r, t: e });
            }
        }
        self.pose = pose;
    }

    /// Recale l'arbre de l'ancre : l'os ancré revient où il était dans `avant`, le reste
    /// de son arbre suit d'un bloc.
    pub fn recaler_ancre(&mut self, avant: &Reperes) {
        let Some(a) = self.ancre() else { return };
        let mut m = self.matrices_pose();
        let (Some(&va), Some(&ma)) = (avant.get(&a), m.get(&a)) else { return };
        let mx = composer(va, inverse(ma));
        let r = self.racine(&a);
        for i in std::iter::once(r.clone()).chain(self.descendants(&r)) {
            if let Some(v) = m.get_mut(&i) {
                *v = composer(mx, *v);
            }
        }
        self.pose_depuis_reperes(&m);
    }

    /// Repère de l'articulation de `id`, celui de ses limites : sans son écart de rotation.
    fn repere_articulation(&self, id: &str, m: &Reperes) -> (P3, Q) {
        let b = self.trouver(id).expect("os");
        let t = self.pose.get(id).map_or(b.position, |e| [0, 1, 2].map(|k| b.position[k] + e.t[k]));
        match b.parent.as_ref().and_then(|p| m.get(p)) {
            Some(&mp) => composer(mp, (t, b.rotation)),
            None => (t, b.rotation),
        }
    }

    /// Articulations entre l'os `id` et l'ancre, du côté saisi vers l'ancre. Hors de
    /// l'arbre de l'ancre, jusqu'à sa racine.
    pub fn chaine(&self, id: &str) -> Vec<Pas> {
        let montee = |i: &str| {
            let mut r = vec![i.to_string()];
            while let Some(p) = self.trouver(r.last().unwrap()).and_then(|b| b.parent.clone()) {
                r.push(p);
            }
            r
        };
        let haut = montee(id);
        let bas = self.ancre().map(|a| montee(&a)).unwrap_or_default();
        let r: Vec<Pas> = match haut.iter().position(|i| bas.contains(i)) {
            None => haut[..haut.len() - 1].iter().map(|j| Pas { j: j.clone(), inverse: false }).collect(),
            Some(h) => {
                let b = bas.iter().position(|i| *i == haut[h]).unwrap();
                haut[..h].iter().map(|j| Pas { j: j.clone(), inverse: false }).chain(bas[..b].iter().rev().map(|j| Pas { j: j.clone(), inverse: true })).collect()
            }
        };
        r.into_iter().filter(|p| !self.rigide(&p.j)).collect()
    }

    /// Saisie de l'os `id` au point `point`.
    pub fn saisir(&self, id: &str, point: P3) -> Saisie {
        let m = self.matrices_pose();
        let local = m.get(id).map_or(point, |&mb| appliquer(inverse(mb), point));
        Saisie { os: id.to_string(), local, chaine: self.chaine(id), avant: m, depart: self.pose.clone() }
    }

    /// Point tenu, dans la pose courante.
    pub fn point_saisi(&self, s: &Saisie) -> P3 {
        self.matrices_pose().get(&s.os).map_or(s.local, |&m| appliquer(m, s.local))
    }

    /// Amène le point saisi vers `cible` ; la pose du début de la saisie garde les axes bloqués.
    pub fn tirer(&mut self, s: &Saisie, cible: P3, iterations: usize) {
        for _ in 0..iterations {
            if norme2(sous(self.point_saisi(s), cible)) <= 1e-10 {
                break;
            }
            for p in &s.chaine {
                self.pas_ccd(s, p, cible);
            }
        }
    }

    /// Une étape : l'articulation rapproche le point de la cible, axe par axe si l'un
    /// d'eux est bloqué.
    fn pas_ccd(&mut self, s: &Saisie, p: &Pas, cible: P3) {
        let Some(l) = self.trouver(&p.j).map(|b| b.limites()) else { return };
        let axes: Vec<usize> = (0..3).filter(|k| l[*k] != Limite::Bloquee).collect();
        if axes.len() == 3 {
            let (pivot, repere) = self.repere_articulation(&p.j, &self.matrices_pose());
            let (u, v) = (sous(self.point_saisi(s), pivot), sous(cible, pivot));
            if norme2(u) > 1e-12 && norme2(v) > 1e-12 {
                self.tourner_articulation(s, p, repere, entre(unitaire(u), unitaire(v)));
            }
            return;
        }
        for k in axes {
            let (pivot, repere) = self.repere_articulation(&p.j, &self.matrices_pose());
            let w = self.axe_articulation(&p.j, k, repere);
            let plan = |x: P3| {
                let d = scal(w, x);
                [x[0] - w[0] * d, x[1] - w[1] * d, x[2] - w[2] * d]
            };
            let (u, v) = (plan(sous(self.point_saisi(s), pivot)), plan(sous(cible, pivot)));
            if norme2(u).sqrt() * norme2(v).sqrt() < 1e-12 {
                continue;
            }
            let angle = scal(vect(u, v), w).atan2(scal(u, v));
            self.tourner_articulation(s, p, repere, qaxe(w, angle));
        }
    }

    /// Axe `k` de l'articulation de `j`, dans le repère de l'armature : dans l'ordre des
    /// angles des os (X, puis Z, puis Y), chacun porté par ceux qui le précèdent.
    fn axe_articulation(&self, j: &str, k: usize, repere: Q) -> P3 {
        let g = angles_os(self.pose.get(j).map_or(ID, |e| e.r)).map(f64::to_radians);
        let mut w = [0.0; 3];
        w[k] = 1.0;
        if k == 1 {
            w = qappliquer(qaxe([0.0, 0.0, 1.0], g[2]), w);
        }
        if k != 0 {
            w = qappliquer(qaxe([1.0, 0.0, 0.0], g[0]), w);
        }
        unitaire(qappliquer(repere, w))
    }

    /// Le côté saisi de l'articulation tourne de `q`, limites appliquées, l'ancre restant
    /// en place.
    fn tourner_articulation(&mut self, s: &Saisie, p: &Pas, repere: Q, q: Q) {
        let e = self.pose.get(&p.j).copied().unwrap_or(Ecart { r: ID, t: [0.0; 3] });
        let ecart = qmul(qmul(qinv(repere), if p.inverse { qinv(q) } else { q }), repere);
        let r = qnorm(qmul(ecart, e.r));
        let d = s.depart.get(&p.j).map_or(ID, |x| x.r);
        let r = self.trouver(&p.j).map_or(r, |b| b.borner(r, d));
        self.pose.insert(p.j.clone(), Ecart { r, t: e.t });
        self.recaler_ancre(&s.avant);
    }
}
