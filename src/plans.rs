//! Plans de référence (blueprints) : des images posées face à une vue alignée sur un
//! axe, pour modeler par-dessus. Sans interface ni OpenGL : ce module dit où va
//! chaque plan, et comment il s'écrit dans le `.gda` (clé `plans`, que GDA ignore).
//!
//! Un plan est vu en fond dans sa vue (Face, Dessus…) : derrière tout le reste, comme
//! un calque. Hors de sa vue, il se cache, sauf si `partout` le pose dans la scène, dans
//! le plan qui passe par l'origine.
//!
//! Une planche (une image à plusieurs vues) donne plusieurs plans de la même image :
//! chacun n'en prend qu'une `zone`. Ses vues sont à la même échelle : changer la taille
//! réelle de l'un change celle des autres (`echelle_commune`). `detecter` trouve les
//! dessins de la planche, `repartir` leur donne une vue.

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
    /// Partie de l'image montrée : gauche, haut, droite, bas, de 0 à 1 (haut à 0).
    pub zone: [f64; 4],
    /// Taille de l'image en pixels ; seul le rapport compte pour un ancien plan.
    pub pixels: [u32; 2],
    /// Quarts de tour de l'image, dans le sens contraire des aiguilles d'une montre.
    pub quart: u8,
    /// Calé : centré sur l'origine, posé sur le sol (y = 0) sauf vu de dessus ou de
    /// dessous ; le décalage n'est alors plus lu.
    pub cale: bool,
}

/// Toute l'image.
pub const ZONE_ENTIERE: [f64; 4] = [0.0, 0.0, 1.0, 1.0];

impl Plan {
    pub fn neuf(nom: &str, image: &str, rapport: f64, vue: usize, largeur: f64, decalage: [f64; 2]) -> Plan {
        let pixels = [1000, (1000.0 * rapport).round().max(1.0) as u32];
        Plan { nom: nom.to_string(), image: image.to_string(), rapport, vue: vue.min(5), largeur, decalage, opacite: 0.6, visible: true, partout: false, zone: ZONE_ENTIERE, pixels, quart: 0, cale: false }
    }

    /// Largeur et hauteur de la zone montrée, en pixels de l'image, après les quarts de
    /// tour : dans le sens de l'écran.
    pub fn pixels_vus(&self) -> (f64, f64) {
        let z = self.zone;
        let (l, h) = ((z[2] - z[0]).abs() * self.pixels[0] as f64, (z[3] - z[1]).abs() * self.pixels[1] as f64);
        let (l, h) = (l.max(1e-6), h.max(1e-6));
        if self.quart % 2 == 1 { (h, l) } else { (l, h) }
    }

    /// Hauteur sur largeur de ce qui est montré.
    pub fn aspect(&self) -> f64 {
        let (l, h) = self.pixels_vus();
        h / l
    }

    /// Unités du monde par pixel de l'image.
    pub fn echelle(&self) -> f64 {
        self.largeur / self.pixels_vus().0
    }

    /// Décalage effectif : celui du calage, ou celui réglé.
    pub fn decalage_vu(&self) -> [f64; 2] {
        if !self.cale {
            return self.decalage;
        }
        let pose = self.vue < 4;
        [0.0, if pose { self.largeur * self.aspect() / 2.0 } else { 0.0 }]
    }

    /// Coordonnées de texture des coins du carré (bas gauche, bas droit, haut droit, haut
    /// gauche), selon la zone et les quarts de tour.
    pub fn coins_uv(&self) -> [[f32; 2]; 4] {
        let z = self.zone.map(|x| x as f32);
        // Coins de la zone dans l'image : haut gauche, haut droit, bas droit, bas gauche
        let anneau = [[z[0], z[1]], [z[2], z[1]], [z[2], z[3]], [z[0], z[3]]];
        let k = (self.quart % 4) as usize;
        // À l'écran : haut gauche, haut droit, bas droit, bas gauche
        let ecran: [[f32; 2]; 4] = std::array::from_fn(|i| anneau[(i + k) % 4]);
        [ecran[3], ecran[2], ecran[1], ecran[0]]
    }

    /// Matrice (par colonnes) qui pose le carré unité [-½, ½]² du plan XY, tourné vers
    /// +Z, à sa place : x vers la droite de la vue, y vers son haut, z vers l'œil. Le plan
    /// passe par l'origine, reculé de `recul` à l'opposé de l'œil.
    pub fn matrice(&self, recul: f64) -> [f64; 16] {
        let (_, n, d, h) = VUES[self.vue.min(5)];
        let (sx, sy) = (self.largeur, self.largeur * self.aspect());
        let dc = self.decalage_vu();
        let o = [0, 1, 2].map(|i| d[i] * dc[0] + h[i] * dc[1] - n[i] * recul);
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
            c("zone", J::Liste(self.zone.iter().map(|x| J::Num(*x)).collect())),
            c("pixels", J::Liste(self.pixels.iter().map(|x| J::Num(*x as f64)).collect())),
            c("quart", J::Num(self.quart as f64)),
            c("cale", J::Bool(self.cale)),
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
        if let Some(z) = j.get("zone").and_then(J::nombres::<4>).filter(|z| z.iter().all(|x| (0.0..=1.0).contains(x)) && z[2] > z[0] && z[3] > z[1]) {
            p.zone = z;
        }
        if let Some(x) = j.get("pixels").and_then(J::nombres::<2>).filter(|x| x[0] >= 1.0 && x[1] >= 1.0) {
            p.pixels = [x[0] as u32, x[1] as u32];
        }
        p.quart = (num("quart", 0.0) as i64).rem_euclid(4) as u8;
        p.cale = j.get("cale").and_then(J::bool).unwrap_or(false);
        Some(p)
    }
}

/// Le carré unité du plan XY, tourné vers +Z, qui montre `uv` (coins bas gauche, bas
/// droit, haut droit, haut gauche ; haut gauche de l'image en (0, 0), comme en glTF).
pub fn carre(uv: [[f32; 2]; 4]) -> crate::maillage::Maillage {
    use crate::maillage::{Face, Maillage};
    let s = vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]];
    let f = Face { s: vec![0, 1, 2, 3], m: "plan".into(), uv: uv.to_vec() };
    Maillage::new(s, vec![f], vec![])
}

/// Recul de chaque plan calé : contre la boîte du véhicule que dessine sa planche, pour
/// qu'ils fassent un coin de boîte dans la scène plutôt que de se croiser au milieu. Le
/// profil recule de la demi-largeur (celle de la face ou de l'arrière), la face et
/// l'arrière de la demi-longueur (celle du profil) ; le dessus reste au sol.
pub fn reculs(plans: &[Plan]) -> Vec<f64> {
    plans
        .iter()
        .map(|p| {
            if !p.cale {
                return 0.0;
            }
            let autres = |vues: [usize; 2]| plans.iter().filter(|q| q.image == p.image && vues.contains(&q.vue)).map(|q| q.largeur).fold(0.0, f64::max);
            match p.vue {
                0 | 1 => autres([2, 3]) / 2.0,
                2 | 3 => autres([0, 1]) / 2.0,
                _ => 0.0,
            }
        })
        .collect()
}

/// Changer la taille réelle du plan `k` à `v` change l'échelle de sa planche : les
/// plans de la même image suivent, à la même échelle.
pub fn echelle_commune(plans: &mut [Plan], k: usize, v: f64) {
    let Some(p) = plans.get(k) else { return };
    let (image, s) = (p.image.clone(), v / p.pixels_vus().0);
    for q in plans.iter_mut().filter(|q| q.image == image) {
        q.largeur = s * q.pixels_vus().0;
    }
}

/// Boîte d'un dessin, en pixels : gauche, haut, droite, bas (exclus).
pub type Boite = [usize; 4];

/// Dessins d'une planche : les taches d'encre, réunies quand leurs boîtes se
/// chevauchent, de la plus grande à la plus petite ; celles de moins de 2 % de l'image
/// (légendes, cotes) sont laissées.
pub fn detecter(rgba: &[u8], l: usize, h: usize) -> Vec<Boite> {
    const CASE: usize = 4;
    let encre = |x: usize, y: usize| {
        let i = 4 * (y * l + x);
        let p = &rgba[i..i + 4];
        p[3] > 128 && (p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32) < 1100
    };
    let (gl, gh) = (l.div_ceil(CASE), h.div_ceil(CASE));
    let mut grille = vec![false; gl * gh];
    for y in 0..h {
        for x in 0..l {
            if encre(x, y) {
                grille[(y / CASE) * gl + x / CASE] = true;
            }
        }
    }
    // Taches : cases d'encre voisines, en diagonale aussi
    let mut vu = vec![false; gl * gh];
    let mut boites: Vec<Boite> = Vec::new();
    for depart in 0..gl * gh {
        if !grille[depart] || vu[depart] {
            continue;
        }
        let mut b = [usize::MAX, usize::MAX, 0, 0];
        let mut pile = vec![depart];
        vu[depart] = true;
        while let Some(c) = pile.pop() {
            let (cx, cy) = (c % gl, c / gl);
            b = [b[0].min(cx), b[1].min(cy), b[2].max(cx + 1), b[3].max(cy + 1)];
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (cx as i64 + dx, cy as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= gl as i64 || ny >= gh as i64 {
                        continue;
                    }
                    let n = ny as usize * gl + nx as usize;
                    if grille[n] && !vu[n] {
                        vu[n] = true;
                        pile.push(n);
                    }
                }
            }
        }
        boites.push([b[0] * CASE, b[1] * CASE, (b[2] * CASE).min(l), (b[3] * CASE).min(h)]);
    }
    // Les morceaux d'un même dessin : boîtes qui se chevauchent
    let chevauche = |a: &Boite, b: &Boite| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
    loop {
        let mut fusion = None;
        'cherche: for i in 0..boites.len() {
            for j in i + 1..boites.len() {
                if chevauche(&boites[i], &boites[j]) {
                    fusion = Some((i, j));
                    break 'cherche;
                }
            }
        }
        let Some((i, j)) = fusion else { break };
        let b = boites.swap_remove(j);
        let a = &mut boites[i];
        *a = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
    }
    let aire = |b: &Boite| (b[2] - b[0]) * (b[3] - b[1]);
    boites.retain(|b| aire(b) * 50 >= l * h);
    // Au pixel près : l'encre réelle dans la boîte
    for b in boites.iter_mut() {
        let mut r = [usize::MAX, usize::MAX, 0, 0];
        for y in b[1]..b[3] {
            for x in b[0]..b[2] {
                if encre(x, y) {
                    r = [r[0].min(x), r[1].min(y), r[2].max(x + 1), r[3].max(y + 1)];
                }
            }
        }
        if r[2] > r[0] {
            *b = r;
        }
    }
    // Deux dessins reliés par un trait fin (une antenne, une cote) : coupés là
    let mut coupees = Vec::new();
    while let Some(b) = boites.pop() {
        match couper(&b, &encre) {
            Some((x, y)) => boites.extend([x, y]),
            None => coupees.push(b),
        }
    }
    let mut boites = coupees;
    boites.sort_by_key(|b| std::cmp::Reverse(aire(b)));
    boites
}

/// Coupe une boîte en deux, de part et d'autre de la plus longue bande où l'encre tient dans un
/// filet étroit (moins de 4 % de la boîte en travers), si les deux parts gardent chacune
/// au moins un cinquième de la boîte. Essaie les lignes, puis les colonnes.
fn couper(b: &Boite, encre: &impl Fn(usize, usize) -> bool) -> Option<(Boite, Boite)> {
    for horizontal in [true, false] {
        // Le long de l'axe coupé : position ; en travers : étendue de l'encre
        let (debut, fin, t0, t1) = if horizontal { (b[1], b[3], b[0], b[2]) } else { (b[0], b[2], b[1], b[3]) };
        let etroit = |k: usize| {
            let (mut lo, mut hi) = (usize::MAX, 0);
            for t in t0..t1 {
                let e = if horizontal { encre(t, k) } else { encre(k, t) };
                if e {
                    lo = lo.min(t);
                    hi = hi.max(t);
                }
            }
            lo == usize::MAX || (hi - lo) * 25 < t1 - t0
        };
        let (mut meilleure, mut cours) = ((0, 0), None);
        for k in debut..=fin {
            let ok = k < fin && etroit(k);
            match (ok, cours) {
                (true, None) => cours = Some(k),
                (false, Some(a)) => {
                    if k - a > meilleure.1 - meilleure.0 {
                        meilleure = (a, k);
                    }
                    cours = None;
                }
                _ => {}
            }
        }
        let (a, z) = meilleure;
        let long = fin - debut;
        if z - a >= 6 && a > debut && z < fin && (a - debut) * 5 >= long && (fin - z) * 5 >= long {
            // Le filet lui-même n'appartient à aucun des deux
            let (x, y) = if horizontal { ([b[0], b[1], b[2], a], [b[0], z, b[2], b[3]]) } else { ([b[0], b[1], a, b[3]], [z, b[1], b[2], b[3]]) };
            let serrer = |c: Boite| {
                let mut r = [usize::MAX, usize::MAX, 0, 0];
                for yy in c[1]..c[3] {
                    for xx in c[0]..c[2] {
                        if encre(xx, yy) {
                            r = [r[0].min(xx), r[1].min(yy), r[2].max(xx + 1), r[3].max(yy + 1)];
                        }
                    }
                }
                if r[2] > r[0] { r } else { c }
            };
            return Some((serrer(x), serrer(y)));
        }
    }
    None
}

/// Vue et quarts de tour des dessins d'une planche (les quatre plus grands) : le plus
/// allongé est le profil (vue Droite : l'avant à gauche, comme sur la plupart des
/// planches), le suivant, s'il est allongé, le dessus (tourné d'un quart : l'avant vers
/// le bas, vers +Z) ; des deux autres, le plus haut, ou le plus à gauche, est la face.
pub fn repartir(boites: &[Boite]) -> Vec<(usize, Boite, u8)> {
    let mut reste: Vec<Boite> = boites.iter().take(4).copied().collect();
    let allonge = |b: &Boite| (b[2] - b[0]) as f64 / (b[3] - b[1]).max(1) as f64;
    let prendre = |reste: &mut Vec<Boite>| -> Option<Boite> {
        let k = (0..reste.len()).max_by(|a, b| allonge(&reste[*a]).total_cmp(&allonge(&reste[*b])))?;
        Some(reste.remove(k))
    };
    let mut r = Vec::new();
    if let Some(b) = prendre(&mut reste) {
        r.push((2, b, 0));
    }
    if reste.len() >= 3 || reste.iter().any(|b| allonge(b) > 1.6) {
        if let Some(b) = prendre(&mut reste) {
            r.push((4, b, if allonge(&b) >= 1.0 { 1 } else { 2 }));
        }
    }
    reste.sort_by_key(|b| (b[1] / 64, b[0]));
    for (k, b) in reste.into_iter().enumerate() {
        r.push((k.min(1), b, 0));
    }
    r
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
        p.pixels = [400, 200];
        let m = p.matrice(0.0);
        // Vue de droite : le coin haut droit de l'image est vers -Z et +Y
        assert_eq!(appliquer(&m, [0.5, 0.5, 0.0]), [0.0, 1.5, -3.0]);
        assert_eq!(appliquer(&m, [-0.5, -0.5, 0.0]), [0.0, -0.5, 1.0]);
        p.opacite = 0.3;
        p.partout = true;
        p.zone = [0.25, 0.5, 0.75, 1.0];
        p.quart = 1;
        p.cale = true;
        let q = Plan::lire(&p.vers_json()).unwrap();
        assert_eq!(q, p);
    }

    #[test]
    fn zone_quart_de_tour_et_calage() {
        // Planche de 400 × 200 ; la zone, la moitié droite du bas : 200 × 100 pixels
        let mut p = Plan::neuf("p", "i", 0.5, 4, 2.0, [5.0, 5.0]);
        p.pixels = [400, 200];
        p.zone = [0.5, 0.5, 1.0, 1.0];
        assert_eq!(p.pixels_vus(), (200.0, 100.0));
        assert_eq!(p.aspect(), 0.5);
        // Un quart de tour : le bord gauche de la zone passe en bas
        p.quart = 1;
        assert_eq!(p.pixels_vus(), (100.0, 200.0));
        let uv = p.coins_uv();
        assert_eq!(uv[0], [0.5, 0.5], "bas gauche à l'écran = haut gauche de la zone");
        assert_eq!(uv[1], [0.5, 1.0], "bas droit = bas gauche de la zone");
        // Calé : vu de dessus, centré ; de profil, posé sur le sol
        p.cale = true;
        assert_eq!(p.decalage_vu(), [0.0, 0.0]);
        p.vue = 2;
        assert_eq!(p.decalage_vu(), [0.0, 2.0], "hauteur 4, posé sur y = 0");
        let m = p.matrice(0.0);
        assert!((appliquer(&m, [0.0, -0.5, 0.0])[1]).abs() < 1e-12, "le bas touche le sol");
    }

    #[test]
    fn une_planche_garde_une_seule_echelle() {
        let mut a = Plan::neuf("s", "i", 0.5, 2, 1.0, [0.0; 2]);
        a.pixels = [1000, 500];
        a.zone = [0.0, 0.0, 0.6, 0.4];
        let mut b = a.clone();
        b.vue = 0;
        b.zone = [0.65, 0.0, 0.95, 0.4];
        let mut autre = Plan::neuf("x", "j", 1.0, 0, 7.0, [0.0; 2]);
        autre.pixels = [10, 10];
        let mut l = vec![a, b, autre];
        echelle_commune(&mut l, 0, 4.2);
        assert!((l[0].largeur - 4.2).abs() < 1e-12);
        assert!((l[1].largeur - 4.2 * 300.0 / 600.0).abs() < 1e-12);
        assert_eq!(l[2].largeur, 7.0, "une autre image ne bouge pas");
    }

    /// Planche à la main : profil allongé en haut à gauche, face et arrière à droite,
    /// dessus en bas à gauche, une légende en bas.
    fn planche() -> (Vec<u8>, usize, usize) {
        let (l, h) = (800, 500);
        let mut px = vec![255u8; l * h * 4];
        let mut cadre = |x0: usize, y0: usize, x1: usize, y1: usize| {
            for y in y0..y1 {
                for x in x0..x1 {
                    // Contour, et un trait coupé au milieu : deux morceaux d'un dessin
                    let bord = x < x0 + 2 || x >= x1 - 2 || y < y0 + 2 || y >= y1 - 2;
                    let trait_ = y == (y0 + y1) / 2 && (x - x0) % 40 < 25;
                    if bord || trait_ {
                        let i = 4 * (y * l + x);
                        px[i..i + 3].copy_from_slice(&[10, 10, 10]);
                    }
                }
            }
        };
        cadre(20, 20, 520, 190);
        cadre(560, 20, 760, 190);
        cadre(20, 240, 520, 440);
        cadre(560, 250, 760, 420);
        cadre(30, 470, 200, 480);
        (px, l, h)
    }

    #[test]
    fn la_planche_se_decoupe_en_quatre_vues() {
        let (px, l, h) = planche();
        let b = detecter(&px, l, h);
        assert_eq!(b.len(), 4, "{b:?}");
        let r = repartir(&b);
        let vue = |v: usize| r.iter().find(|x| x.0 == v).map(|x| (x.1, x.2));
        assert_eq!(vue(2), Some(([20, 20, 520, 190], 0)), "profil");
        assert_eq!(vue(4), Some(([20, 240, 520, 440], 1)), "dessus, tourné");
        assert_eq!(vue(0), Some(([560, 20, 760, 190], 0)), "face");
        assert_eq!(vue(1), Some(([560, 250, 760, 420], 0)), "arrière");
    }

    /// Bilan de la détection sur un dossier de planches : `cargo test -- --ignored bilan`.
    #[test]
    #[ignore]
    fn bilan_des_planches() {
        let racine = std::env::var("PLANCHES").unwrap_or("/sdcard/bang/blueprints-psa".into());
        let mut pile = vec![std::path::PathBuf::from(racine)];
        let mut compte = std::collections::BTreeMap::new();
        while let Some(d) = pile.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    pile.push(p);
                    continue;
                }
                let Ok(o) = std::fs::read(&p) else { continue };
                let Ok(i) = crate::texture::decoder(&o) else { continue };
                let n = repartir(&detecter(&i.rgba, i.l as usize, i.h as usize)).len();
                *compte.entry(n).or_insert(0) += 1;
                if n < 4 {
                    eprintln!("{n} {} ({}×{})", p.display(), i.l, i.h);
                }
            }
        }
        eprintln!("vues trouvées → planches : {compte:?}");
    }

    #[test]
    fn la_planche_de_la_306_si_elle_est_la() {
        let f = "/sdcard/bang/blueprints-psa/peugeot/306/1993-peugeot-306-5-door-tag.png";
        let Ok(d) = std::fs::read(f) else { return };
        let i = crate::texture::decoder(&d).unwrap();
        let b = detecter(&i.rgba, i.l as usize, i.h as usize);
        let r = repartir(&b);
        eprintln!("{b:?}\n{r:?}");
        let vue = |v: usize| r.iter().find(|x| x.0 == v).map(|x| x.1).unwrap();
        // Profil en haut à gauche, dessus dessous, face en haut à droite, arrière dessous
        assert!(vue(2)[0] < 50 && vue(2)[2] > 1100 && vue(2)[3] < 430, "{:?}", vue(2));
        assert!(vue(4)[1] > 430 && vue(4)[2] < 1200, "{:?}", vue(4));
        assert!(vue(0)[0] > 1150 && vue(0)[3] < 430, "{:?}", vue(0));
        assert!(vue(1)[0] > 1150 && vue(1)[1] > 430, "{:?}", vue(1));
    }
}
