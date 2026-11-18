//! Plans de référence (blueprints) : des images posées face à une vue alignée sur un
//! axe, pour modeler par-dessus. Sans interface ni OpenGL : ce module dit où va
//! chaque plan, et comment il s'écrit dans le `.gda` (clé `plans`, que GDA ignore).
//!
//! Un plan est vu en fond dans sa vue (Face, Dessus…) : derrière tout le reste, comme
//! un calque. Hors de sa vue, il se cache, sauf si `partout` le pose dans la scène, dans
//! le plan qui passe par l'origine.

use crate::json::J;

pub type P3 = [f64; 3];

/// Les six vues alignées sur les axes : nom, direction de l'œil (vers lui depuis la
/// cible), droite et haut de l'écran.
pub const VUES: [(&str, P3, P3, P3); 6] = [
    ("Face", [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ("Arrière", [0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ("Droite", [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
    ("Gauche", [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ("Dessus", [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
    ("Dessous", [0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
];

/// Lacet et hauteur de la caméra pour la vue `k` de `VUES`. Les pôles sont évités d'un
/// rien : la caméra garde le haut du monde pour se repérer.
pub fn angles(k: usize) -> (f64, f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    let pole = FRAC_PI_2 - POLE;
    match k {
        0 => (0.0, 0.0),
        1 => (PI, 0.0),
        2 => (FRAC_PI_2, 0.0),
        3 => (-FRAC_PI_2, 0.0),
        4 => (0.0, pole),
        _ => (0.0, -pole),
    }
}

/// Écart aux pôles des vues Dessus et Dessous, en radians.
pub const POLE: f64 = 1e-3;

/// La vue isométrique : œil sur la diagonale +X +Y +Z.
pub fn angles_iso() -> (f64, f64) {
    (std::f64::consts::FRAC_PI_4, (1.0 / 2f64.sqrt()).atan())
}

/// La vue alignée que regarde la direction `dos` (de la cible vers l'œil), s'il y en a une.
pub fn vue_de(dos: P3) -> Option<usize> {
    VUES.iter().position(|v| (0..3).map(|i| v.1[i] * dos[i]).sum::<f64>() > 0.9995)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Nom du fichier de l'image, sans dossier.
    pub nom: String,
    /// Identifiant de l'image dans `Projet::textures`.
    pub image: String,
    /// Hauteur de l'image sur sa largeur.
    pub rapport: f64,
    /// Rang dans `VUES`.
    pub vue: usize,
    /// Largeur dans le monde.
    pub largeur: f64,
    /// Centre de l'image sur le plan : vers la droite, vers le haut de sa vue.
    pub decalage: [f64; 2],
    pub opacite: f32,
    pub visible: bool,
    /// Aussi hors de sa vue, dans la scène.
    pub partout: bool,
}

impl Plan {
    pub fn neuf(nom: &str, image: &str, rapport: f64, vue: usize, largeur: f64, decalage: [f64; 2]) -> Plan {
        Plan { nom: nom.to_string(), image: image.to_string(), rapport, vue: vue.min(5), largeur, decalage, opacite: 0.6, visible: true, partout: false }
    }

    /// Matrice (par colonnes) qui pose le carré unité [-½, ½]² du plan XY, tourné vers
    /// +Z, à sa place : x vers la droite de la vue, y vers son haut, z vers l'œil.
    pub fn matrice(&self) -> [f64; 16] {
        let (_, n, d, h) = VUES[self.vue.min(5)];
        let (sx, sy) = (self.largeur, self.largeur * self.rapport);
        let o = [0, 1, 2].map(|i| d[i] * self.decalage[0] + h[i] * self.decalage[1]);
        [d[0] * sx, d[1] * sx, d[2] * sx, 0.0, h[0] * sy, h[1] * sy, h[2] * sy, 0.0, n[0], n[1], n[2], 0.0, o[0], o[1], o[2], 1.0]
    }

    pub fn vers_json(&self) -> J {
        let c = |k: &str, v: J| (k.to_string(), v);
        J::Objet(vec![
            c("nom", J::Txt(self.nom.clone())),
            c("image", J::Txt(self.image.clone())),
            c("rapport", J::Num(self.rapport)),
            c("vue", J::Txt(VUES[self.vue.min(5)].0.to_string())),
            c("largeur", J::Num(self.largeur)),
            c("decalage", J::Liste(self.decalage.iter().map(|x| J::Num(*x)).collect())),
            c("opacite", J::Num(self.opacite as f64)),
            c("visible", J::Bool(self.visible)),
            c("partout", J::Bool(self.partout)),
        ])
    }

    pub fn lire(j: &J) -> Option<Plan> {
        let image = j.get("image")?.txt()?.to_string();
        let vue = j.get("vue").and_then(J::txt).and_then(|v| VUES.iter().position(|x| x.0 == v)).unwrap_or(0);
        let num = |k: &str, d: f64| j.get(k).and_then(J::num).filter(|v| v.is_finite()).unwrap_or(d);
        let mut p = Plan::neuf(j.get("nom").and_then(J::txt).unwrap_or(""), &image, num("rapport", 1.0).max(1e-3), vue, num("largeur", 1.0).max(1e-6), j.get("decalage").and_then(J::nombres::<2>).unwrap_or([0.0; 2]));
        p.opacite = num("opacite", 0.6).clamp(0.05, 1.0) as f32;
        p.visible = j.get("visible").and_then(J::bool).unwrap_or(true);
        p.partout = j.get("partout").and_then(J::bool).unwrap_or(false);
        Some(p)
    }
}

/// Le carré unité du plan XY, tourné vers +Z, l'image à l'endroit (haut gauche en
/// (0, 0), comme en glTF).
pub fn carre() -> crate::maillage::Maillage {
    use crate::maillage::{Face, Maillage};
    let s = vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]];
    let f = Face { s: vec![0, 1, 2, 3], m: "plan".into(), uv: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]] };
    Maillage::new(s, vec![f], vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appliquer(m: &[f64; 16], p: P3) -> P3 {
        [0, 1, 2].map(|i| m[i] * p[0] + m[4 + i] * p[1] + m[8 + i] * p[2] + m[12 + i])
    }

    #[test]
    fn chaque_vue_regarde_son_axe_et_garde_un_repere_direct() {
        for (k, (_, n, d, h)) in VUES.iter().enumerate() {
            // droite × haut = vers l'œil
            let x = [d[1] * h[2] - d[2] * h[1], d[2] * h[0] - d[0] * h[2], d[0] * h[1] - d[1] * h[0]];
            assert_eq!(x, *n, "vue {k}");
            // Les angles de caméra donnent bien cette direction
            let (l, ht) = angles(k);
            let dos = [l.sin() * ht.cos(), ht.sin(), l.cos() * ht.cos()];
            assert_eq!(vue_de(dos), Some(k));
        }
        let (l, h) = angles_iso();
        assert_eq!(vue_de([l.sin() * h.cos(), h.sin(), l.cos() * h.cos()]), None);
        // Isométrique : les trois axes font le même angle avec la vue
        let dos = [l.sin() * h.cos(), h.sin(), l.cos() * h.cos()];
        assert!(dos.iter().all(|x| (x - 1.0 / 3f64.sqrt()).abs() < 1e-12));
    }

    #[test]
    fn le_plan_se_pose_a_l_endroit_dans_sa_vue() {
        let mut p = Plan::neuf("profil.png", "abc", 0.5, 2, 4.0, [1.0, 0.5]);
        let m = p.matrice();
        // Vue de droite : le coin haut droit de l'image est vers -Z et +Y
        assert_eq!(appliquer(&m, [0.5, 0.5, 0.0]), [0.0, 1.5, -3.0]);
        assert_eq!(appliquer(&m, [-0.5, -0.5, 0.0]), [0.0, -0.5, 1.0]);
        p.opacite = 0.3;
        p.partout = true;
        let q = Plan::lire(&p.vers_json()).unwrap();
        assert_eq!(q, p);
    }
}
