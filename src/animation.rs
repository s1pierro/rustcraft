//! Animation d'une armature : des clés, chacune une pose à un instant, entre lesquelles
//! chaque os passe d'une clé à la suivante (rotation au plus court, translation en ligne
//! droite). C'est la forme des animations glTF, un canal par os, que S.A.G.E. relira.
//! Gardées dans les données de l'armature, sous `animations`.

use crate::armature::{lire_ecarts, Armature, Ecart, Pose};
use crate::edition::{qnorm, Q};
use crate::json::J;
use crate::maillage::P3;

/// Une pose à un instant, en secondes.
#[derive(Clone, Debug, PartialEq)]
pub struct Cle {
    pub t: f64,
    pub pose: Pose,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub nom: String,
    /// Durée en secondes ; en boucle, la dernière clé revient à la première à cette durée.
    pub duree: f64,
    pub boucle: bool,
    /// Triées par instant, deux clés jamais au même.
    pub cles: Vec<Cle>,
}

/// Deux instants plus proches sont la même clé.
pub const MEME: f64 = 1e-4;
/// Images par seconde : les instants des clés s'y arrondissent.
pub const IPS: f64 = 24.0;
const ID: Q = [0.0, 0.0, 0.0, 1.0];

/// Rotation de `a` vers `b`, au plus court.
pub fn slerp(a: Q, b: Q, u: f64) -> Q {
    let mut d: f64 = (0..4).map(|k| a[k] * b[k]).sum();
    let b = if d < 0.0 {
        d = -d;
        b.map(|x| -x)
    } else {
        b
    };
    if d > 0.9995 {
        return qnorm(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u));
    }
    let th = d.clamp(-1.0, 1.0).acos();
    let (s, sa, sb) = (th.sin(), ((1.0 - u) * th).sin(), (u * th).sin());
    qnorm(std::array::from_fn(|k| (a[k] * sa + b[k] * sb) / s))
}

fn melange(a: Ecart, b: Ecart, u: f64) -> Ecart {
    let t: P3 = std::array::from_fn(|k| a.t[k] + (b.t[k] - a.t[k]) * u);
    Ecart { r: slerp(a.r, b.r, u), t }
}

fn repos() -> Ecart {
    Ecart { r: ID, t: [0.0; 3] }
}

/// Un écart sans effet n'est pas gardé dans la pose.
fn neutre(e: &Ecart) -> bool {
    (e.r[3].abs() - 1.0).abs() < 1e-12 && e.t.iter().all(|v| v.abs() < 1e-12)
}

impl Animation {
    pub fn neuve(nom: &str) -> Animation {
        Animation { nom: nom.into(), duree: 2.0, boucle: true, cles: Vec::new() }
    }

    /// L'instant arrondi à l'image et ramené dans la durée.
    pub fn instant(&self, t: f64) -> f64 {
        ((t * IPS).round() / IPS).clamp(0.0, self.duree)
    }

    /// Rang de la clé à l'instant `t`.
    pub fn cle_a(&self, t: f64) -> Option<usize> {
        self.cles.iter().position(|c| (c.t - t).abs() < MEME)
    }

    /// Pose une clé à `t` : la remplace s'il y en a déjà une.
    pub fn poser_cle(&mut self, t: f64, pose: Pose) {
        let t = self.instant(t);
        let pose: Pose = pose.into_iter().filter(|x| !neutre(&x.1)).collect();
        match self.cle_a(t) {
            Some(k) => self.cles[k].pose = pose,
            None => {
                let k = self.cles.iter().position(|c| c.t > t).unwrap_or(self.cles.len());
                self.cles.insert(k, Cle { t, pose });
            }
        }
    }

    pub fn retirer_cle(&mut self, t: f64) -> bool {
        let Some(k) = self.cle_a(t) else { return false };
        self.cles.remove(k);
        true
    }

    /// Déplace la clé de `de` vers `vers` ; une autre clé déjà là est remplacée.
    pub fn deplacer_cle(&mut self, de: f64, vers: f64) -> bool {
        let Some(k) = self.cle_a(de) else { return false };
        let c = self.cles.remove(k);
        self.poser_cle(vers, c.pose);
        true
    }

    /// Une durée plus courte retire les clés qui la dépassent.
    pub fn regler_duree(&mut self, d: f64) {
        self.duree = ((d.max(1.0 / IPS) * IPS).round() / IPS).min(600.0);
        let d = self.duree;
        self.cles.retain(|c| c.t <= d + MEME);
    }

    /// Clé précédente ou suivante de `t`.
    pub fn voisine(&self, t: f64, apres: bool) -> Option<f64> {
        if apres {
            self.cles.iter().map(|c| c.t).find(|x| *x > t + MEME)
        } else {
            self.cles.iter().rev().map(|c| c.t).find(|x| *x < t - MEME)
        }
    }

    /// La pose à l'instant `t`, os par os : chacun passe de la clé d'avant à celle
    /// d'après ; un os absent d'une clé y est au repos.
    pub fn pose_a(&self, t: f64) -> Pose {
        let n = self.cles.len();
        if n == 0 {
            return Pose::new();
        }
        let ecart = |k: usize, id: &str| self.cles[k].pose.get(id).copied().unwrap_or_else(repos);
        // Les deux clés qui encadrent t, et la part de la seconde
        let (a, b, u) = if n == 1 {
            (0, 0, 0.0)
        } else if let Some(j) = self.cles.iter().position(|c| c.t > t) {
            if j == 0 {
                if self.boucle {
                    // Avant la première : on vient de la dernière, la boucle passée
                    let (t0, t1) = (self.cles[n - 1].t - self.duree, self.cles[0].t);
                    (n - 1, 0, if t1 - t0 > MEME { (t - t0) / (t1 - t0) } else { 1.0 })
                } else {
                    (0, 0, 0.0)
                }
            } else {
                let (t0, t1) = (self.cles[j - 1].t, self.cles[j].t);
                (j - 1, j, (t - t0) / (t1 - t0))
            }
        } else if self.boucle {
            let (t0, t1) = (self.cles[n - 1].t, self.cles[0].t + self.duree);
            (n - 1, 0, if t1 - t0 > MEME { (t - t0) / (t1 - t0) } else { 0.0 })
        } else {
            (n - 1, n - 1, 0.0)
        };
        let mut ids: Vec<&String> = self.cles[a].pose.keys().chain(self.cles[b].pose.keys()).collect();
        ids.sort();
        ids.dedup();
        ids.into_iter()
            .map(|id| (id.clone(), melange(ecart(a, id), ecart(b, id), u.clamp(0.0, 1.0))))
            .filter(|x| !neutre(&x.1))
            .collect()
    }

    fn lire(j: &J, garder: &impl Fn(&String) -> bool) -> Option<Animation> {
        let nom = j.get("nom").and_then(J::txt).unwrap_or("animation").to_string();
        let duree = j.get("duree").and_then(J::num).filter(|d| *d > 0.0).unwrap_or(2.0);
        let boucle = !matches!(j.get("boucle"), Some(J::Bool(false)));
        let mut a = Animation { nom, duree, boucle, cles: Vec::new() };
        for c in j.get("cles").and_then(J::liste).unwrap_or(&[]) {
            let t = c.get("t").and_then(J::num)?;
            let pose = c.get("os").map(|o| lire_ecarts(o, garder)).unwrap_or_default();
            a.poser_cle(t, pose);
        }
        Some(a)
    }

    fn donnees(&self, ordre: &[String]) -> J {
        let nums = |v: &[f64]| J::Liste(v.iter().map(|x| J::Num(*x)).collect());
        let cles = self.cles.iter().map(|c| {
            let os = ordre.iter().filter_map(|id| {
                let e = c.pose.get(id)?;
                Some((id.clone(), J::Objet(vec![("r".into(), nums(&e.r)), ("t".into(), nums(&e.t))])))
            });
            J::Objet(vec![("t".into(), J::Num(c.t)), ("os".into(), J::Objet(os.collect()))])
        });
        J::Objet(vec![
            ("nom".into(), J::Txt(self.nom.clone())),
            ("duree".into(), J::Num(self.duree)),
            ("boucle".into(), J::Bool(self.boucle)),
            ("cles".into(), J::Liste(cles.collect())),
        ])
    }
}

impl Armature {
    /// Les animations de l'armature ; un os disparu en sort.
    pub fn animations(&self) -> Vec<Animation> {
        let Some((_, J::Liste(l))) = self.reste.iter().find(|x| x.0 == "animations") else { return Vec::new() };
        let garder = |i: &String| self.trouver(i).is_some();
        l.iter().filter_map(|a| Animation::lire(a, &garder)).collect()
    }

    /// Remplace les animations ; sans elles, la clé disparaît.
    pub fn regler_animations(&mut self, l: &[Animation]) {
        let ordre: Vec<String> = self.os.iter().map(|b| b.id.clone()).collect();
        self.reste.retain(|x| x.0 != "animations");
        if !l.is_empty() {
            self.reste.push(("animations".into(), J::Liste(l.iter().map(|a| a.donnees(&ordre)).collect())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edition::qaxe;

    fn e(angle: f64, ty: f64) -> Ecart {
        Ecart { r: qaxe([0.0, 0.0, 1.0], angle), t: [0.0, ty, 0.0] }
    }

    fn proche(a: Ecart, b: Ecart) -> bool {
        let d: f64 = (0..4).map(|k| a.r[k] * b.r[k]).sum();
        (d.abs() - 1.0).abs() < 1e-9 && (0..3).all(|k| (a.t[k] - b.t[k]).abs() < 1e-9)
    }

    #[test]
    fn entre_deux_cles_chaque_os_passe_de_l_une_a_l_autre() {
        let mut a = Animation::neuve("marche");
        a.boucle = false;
        a.poser_cle(0.0, Pose::from([("x".into(), e(0.0, 0.0))]));
        a.poser_cle(1.0, Pose::from([("x".into(), e(1.0, 0.4)), ("y".into(), e(0.5, 0.0))]));
        let p = a.pose_a(0.5);
        assert!(proche(p["x"], e(0.5, 0.2)));
        assert!(proche(p["y"], e(0.25, 0.0)), "absent de la première clé : au repos");
        // Avant la première clé et après la dernière, sans boucle : elles tiennent
        assert!(a.pose_a(-1.0).is_empty());
        assert!(proche(a.pose_a(1.8)["x"], e(1.0, 0.4)));
    }

    #[test]
    fn en_boucle_la_derniere_cle_revient_a_la_premiere() {
        let mut a = Animation::neuve("tour");
        a.duree = 2.0;
        a.poser_cle(0.0, Pose::from([("x".into(), e(0.0, 0.0))]));
        a.poser_cle(1.0, Pose::from([("x".into(), e(1.0, 0.0))]));
        assert!(proche(a.pose_a(1.5)["x"], e(0.5, 0.0)));
        assert!(a.pose_a(2.0).is_empty() || proche(a.pose_a(2.0)["x"], e(0.0, 0.0)));
        // Une clé tardive : avant la première, on vient d'elle
        let mut b = Animation::neuve("b");
        b.poser_cle(0.5, Pose::from([("x".into(), e(0.0, 0.0))]));
        b.poser_cle(1.5, Pose::from([("x".into(), e(1.0, 0.0))]));
        assert!(proche(b.pose_a(0.0)["x"], e(0.5, 0.0)));
    }

    #[test]
    fn les_cles_se_rangent_se_remplacent_et_s_arrondissent_a_l_image() {
        let mut a = Animation::neuve("a");
        a.poser_cle(1.0, Pose::new());
        a.poser_cle(0.51, Pose::from([("x".into(), e(1.0, 0.0))]));
        a.poser_cle(0.5, Pose::from([("x".into(), e(0.3, 0.0))]));
        assert_eq!(a.cles.iter().map(|c| c.t).collect::<Vec<_>>(), [0.5, 1.0], "0,51 s tombe sur l'image de 0,5 s");
        assert!(proche(a.cles[0].pose["x"], e(0.3, 0.0)));
        assert_eq!(a.voisine(0.5, true), Some(1.0));
        assert_eq!(a.voisine(0.7, false), Some(0.5));
        assert!(a.deplacer_cle(1.0, 1.5));
        a.regler_duree(1.2);
        assert_eq!(a.cles.len(), 1);
        assert!(a.retirer_cle(0.5) && a.cles.is_empty());
    }

    #[test]
    fn les_animations_reviennent_des_donnees_de_l_armature() {
        let mut x = Armature::humanoide();
        let (i, j) = (x.os[0].id.clone(), x.os[1].id.clone());
        let mut a = Animation::neuve("salut");
        a.poser_cle(0.0, Pose::from([(i.clone(), e(0.2, 0.1))]));
        a.poser_cle(1.0, Pose::from([(j.clone(), e(0.4, 0.0)), ("disparu".into(), e(0.1, 0.0))]));
        x.regler_animations(&[a.clone()]);
        let r = x.animations();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].nom.as_str(), r[0].duree, r[0].boucle), ("salut", 2.0, true));
        assert!(proche(r[0].cles[0].pose[&i], e(0.2, 0.1)));
        assert!(!r[0].cles[1].pose.contains_key("disparu"));
        x.regler_animations(&[]);
        assert!(x.reste.iter().all(|k| k.0 != "animations"));
    }
}
