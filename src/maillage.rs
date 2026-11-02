//! Maillage éditable (sommets et faces polygonales) et formats de fichiers 3D :
//! Wavefront OBJ et MTL, STL, glTF 2.0. Portage de `moteur/maillage.js` de GDA,
//! vérifié face à l'original par `tests/differentiel.rs`.
//!
//! Repère : Y vers le haut. Les STL sont supposés Z vers le haut et convertis
//! dans les deux sens. Une face est tournée dans le sens trigonométrique vue de
//! dehors. Là où GDA désigne une face par son objet, on passe ici son indice.

use crate::json::{self, J};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::sync::Arc;

pub type P3 = [f64; 3];
pub type Mat = Arc<str>;
pub type Arete = [usize; 2];
type Cle = (usize, usize);

/// Nom écrit dans l'en-tête des fichiers exportés.
pub const SIGNATURE: &str = "Rustcraft";

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub s: Vec<usize>,
    pub m: Mat,
    /// Coordonnées de texture de chaque coin, comme `s` ; vide si la face n'en a pas.
    /// (0, 0) est le coin haut gauche de l'image, comme en glTF. Une couture du
    /// dépliage vit ainsi sur les coins : le sommet, lui, reste soudé.
    pub uv: Vec<[f32; 2]>,
}

fn mix_uv(a: [f32; 2], b: [f32; 2], t: f64) -> [f32; 2] {
    let t = t as f32;
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Retire les voisins égaux d'un polygone fermé.
fn sans_doublons(s: &[usize]) -> Vec<usize> {
    (0..s.len()).filter(|&k| s[k] != s[(k + 1) % s.len()]).map(|k| s[k]).collect()
}

/// Rend une face à ses sommets `m`, voisins égaux retirés ; les UV suivent leurs coins.
fn recoller(f: &mut Face, m: &[usize]) {
    let n = m.len();
    let garde: Vec<usize> = (0..n).filter(|&k| m[k] != m[(k + 1) % n]).collect();
    f.uv = if f.uv.len() == n { garde.iter().map(|&k| f.uv[k]).collect() } else { Vec::new() };
    f.s = garde.iter().map(|&k| m[k]).collect();
}

/// Matériau lu dans un glTF. `couleur` : rouge, vert, bleu, en linéaire, et opacité.
#[derive(Clone, Debug, PartialEq)]
pub struct MatGltf {
    pub nom: String,
    pub couleur: [f64; 4],
    pub image: Option<(String, Vec<u8>)>,
    /// Il a une texture, mais hors du fichier.
    pub manquante: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Maillage {
    pub sommets: Vec<P3>,
    pub faces: Vec<Face>,
    /// Arêtes libres : sans face, lignes « l » de l'OBJ.
    pub fils: Vec<Arete>,
    /// Arêtes où le dépliage coupe. Une arête disparue reste sans effet : seules
    /// `coutures_valides` comptent.
    pub coutures: Vec<Arete>,
}

/// Où placer le sommet issu d'une fusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cible {
    Centre,
    Premier,
    Dernier,
}

/// Boîte englobante.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boite {
    pub min: P3,
    pub max: P3,
}

/// Anneau d'arêtes : voir [`Maillage::anneau`].
#[derive(Clone, Debug, PartialEq)]
pub struct Anneau {
    pub rails: Vec<Arete>,
    pub quads: Vec<usize>,
    pub depart: usize,
    pub ferme: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub s: Vec<usize>,
    pub plane: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupeDissous {
    pub faces: Vec<usize>,
    pub s: Vec<usize>,
    pub plane: bool,
}

#[derive(Clone, Debug)]
pub struct Revolution {
    pub axe: P3,
    pub centre: P3,
    pub angle: f64,
    pub segments: usize,
    pub fermer: bool,
    pub materiau: Mat,
}

/// Faces et arêtes libres créées par une révolution (indices), et copies de
/// chaque sommet : d'origine, puis aux pas 0 à `segments`.
#[derive(Clone, Debug, Default)]
pub struct Revolue {
    pub faces: Vec<usize>,
    pub fils: Vec<Arete>,
    pub copies: Vec<(usize, Vec<usize>)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fusion {
    pub groupes: usize,
    pub retires: usize,
}

/// Matériau Wavefront : les champs absents ne sont pas écrits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatMtl {
    pub couleur: Option<String>,
    pub ambiante: Option<String>,
    pub speculaire: Option<String>,
    pub emission: Option<String>,
    pub brillance: Option<f64>,
    pub opacite: Option<f64>,
    pub refraction: Option<f64>,
    pub illumination: Option<f64>,
    /// Nom du fichier de la texture de couleur, à l'export seulement.
    pub texture: Option<String>,
}

pub fn defaut() -> Mat {
    Arc::from("defaut")
}

fn cle(i: usize, j: usize) -> Cle {
    if i < j { (i, j) } else { (j, i) }
}

/// `Math.hypot` de V8 (somme de Kahan mise à l'échelle), bit pour bit : à longueurs
/// égales, un plus court chemin fait ainsi le même choix que GDA.
fn hypot3(x: f64, y: f64, z: f64) -> f64 {
    hypot_n(&[x, y, z])
}

fn hypot2(x: f64, y: f64) -> f64 {
    hypot_n(&[x, y])
}

fn hypot_n(l: &[f64]) -> f64 {
    let v: Vec<f64> = l.iter().map(|x| x.abs()).collect();
    let max = v.iter().fold(0.0, |a: f64, b| a.max(*b));
    if max == 0.0 {
        return 0.0;
    }
    let (mut somme, mut reste) = (0.0, 0.0);
    for c in v {
        let n = c / max;
        let terme = n * n - reste;
        let t = somme + terme;
        reste = (t - somme) - terme;
        somme = t;
    }
    somme.sqrt() * max
}

fn dist(p: P3, q: P3) -> f64 {
    hypot3(p[0] - q[0], p[1] - q[1], p[2] - q[2])
}

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn mix(a: P3, b: P3, t: f64) -> P3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Math.round de JavaScript : les demis montent, y compris sous zéro.
fn arrondi(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn distincts(s: &[usize]) -> usize {
    s.iter().collect::<HashSet<_>>().len()
}

fn faces_de(listes: Vec<Vec<usize>>) -> Vec<Face> {
    let m = defaut();
    listes.into_iter().map(|s| Face { s, m: m.clone(), uv: Vec::new() }).collect()
}

impl Maillage {
    pub fn new(sommets: Vec<P3>, faces: Vec<Face>, fils: Vec<Arete>) -> Maillage {
        Maillage { sommets, faces, fils, coutures: Vec::new() }
    }

    #[cfg(test)]
    pub fn supprimer_faces_pour_test(&mut self, faces: &[usize]) {
        let mut k = 0;
        self.faces.retain(|_| {
            k += 1;
            !faces.contains(&(k - 1))
        });
    }

    /// Coutures qui sont encore des arêtes de face, chacune une fois.
    pub fn coutures_valides(&self) -> Vec<Arete> {
        let voulues: HashSet<Cle> = self.coutures.iter().map(|a| cle(a[0], a[1])).collect();
        if voulues.is_empty() {
            return Vec::new();
        }
        let mut vues = HashSet::new();
        let mut l = Vec::new();
        for f in &self.faces {
            for k in 0..f.s.len() {
                let c = cle(f.s[k], f.s[(k + 1) % f.s.len()]);
                if voulues.contains(&c) && vues.insert(c) {
                    l.push([c.0, c.1]);
                }
            }
        }
        l
    }

    // ----- Données des projets ---------------------------------------------

    /// Objet JSON d'un projet : coordonnées à 1e-5 près, comme GDA.
    pub fn vers_donnees(&self) -> J {
        let v = self.sommets.iter().map(|p| J::Liste(p.iter().map(|c| J::Num(arrondi(c * 1e5) / 1e5)).collect())).collect();
        let f = self
            .faces
            .iter()
            .map(|f| {
                let mut l = vec![J::Txt(f.m.to_string())];
                l.extend(f.s.iter().map(|&i| J::Num(i as f64)));
                J::Liste(l)
            })
            .collect();
        let mut d = vec![("v".to_string(), J::Liste(v)), ("f".to_string(), J::Liste(f))];
        if !self.fils.is_empty() {
            let l = self.fils.iter().map(|a| J::Liste(vec![J::Num(a[0] as f64), J::Num(a[1] as f64)])).collect();
            d.push(("l".to_string(), J::Liste(l)));
        }
        // Une liste par face, à plat : absente sans UV, GDA ne la lit pas et ne la gêne pas
        if self.faces.iter().any(|f| !f.uv.is_empty()) {
            let l = self.faces.iter().map(|f| J::Liste(f.uv.iter().flatten().map(|c| J::Num(arrondi(*c as f64 * 1e5) / 1e5)).collect())).collect();
            d.push(("uv".to_string(), J::Liste(l)));
        }
        // Comme les UV, à côté de ce que GDA lit
        let c = self.coutures_valides();
        if !c.is_empty() {
            d.push(("c".to_string(), J::Liste(c.iter().map(|a| J::Liste(vec![J::Num(a[0] as f64), J::Num(a[1] as f64)])).collect())));
        }
        J::Objet(d)
    }

    pub fn depuis_donnees(d: &J) -> Option<Maillage> {
        let v = d.get("v")?.liste()?;
        let f = d.get("f")?.liste()?;
        let mut sommets = Vec::with_capacity(v.len());
        for p in v {
            sommets.push(p.nombres::<3>()?);
        }
        let mut noms: HashMap<&str, Mat> = HashMap::new();
        let mut faces = Vec::with_capacity(f.len());
        for x in f {
            let l = x.liste()?;
            let nom = l.first()?.txt().unwrap_or("defaut");
            let m = noms.entry(nom).or_insert_with(|| Arc::from(nom)).clone();
            let mut s = Vec::with_capacity(l.len() - 1);
            for i in &l[1..] {
                let n = i.num()?;
                if n < 0.0 || n as usize >= sommets.len() {
                    return None;
                }
                s.push(n as usize);
            }
            faces.push(Face { s, m, uv: Vec::new() });
        }
        // Des UV illisibles laissent la face sans texture, pas le projet illisible
        if let Some(l) = d.get("uv").and_then(J::liste).filter(|l| l.len() == faces.len()) {
            for (f, x) in faces.iter_mut().zip(l) {
                let c: Vec<f32> = x.liste().unwrap_or(&[]).iter().filter_map(J::num).map(|v| v as f32).collect();
                if c.len() == f.s.len() * 2 {
                    f.uv = c.chunks_exact(2).map(|p| [p[0], p[1]]).collect();
                }
            }
        }
        let mut fils = Vec::new();
        if let Some(l) = d.get("l").and_then(J::liste) {
            for a in l {
                let [i, j] = a.nombres::<2>()?;
                if i as usize >= sommets.len() || j as usize >= sommets.len() {
                    return None;
                }
                fils.push([i as usize, j as usize]);
            }
        }
        // Des coutures illisibles ne coûtent que les coutures
        let n = sommets.len();
        let coutures = d.get("c").and_then(J::liste).map_or(Vec::new(), |l| l.iter().filter_map(|a| a.nombres::<2>()).map(|[i, j]| [i as usize, j as usize]).filter(|a| a[0] < n && a[1] < n).collect());
        Some(Maillage { sommets, faces, fils, coutures })
    }

    /// Arêtes libres mises en ordre : sans doublon, ni boucle sur un sommet, ni
    /// arête déjà bordée par une face.
    pub fn nettoyer_fils(&mut self) -> &mut Self {
        let mut vues = HashSet::new();
        for f in &self.faces {
            for k in 0..f.s.len() {
                vues.insert(cle(f.s[k], f.s[(k + 1) % f.s.len()]));
            }
        }
        self.fils.retain(|a| a[0] != a[1] && vues.insert(cle(a[0], a[1])));
        self
    }

    // ----- Primitives ------------------------------------------------------

    /// Boîte centrée sur l'origine : six quadrilatères.
    pub fn boite(lx: f64, ly: f64, lz: f64) -> Maillage {
        let mut s = Vec::with_capacity(8);
        for x in [-lx / 2.0, lx / 2.0] {
            for y in [-ly / 2.0, ly / 2.0] {
                for z in [-lz / 2.0, lz / 2.0] {
                    s.push([x, y, z]);
                }
            }
        }
        let f = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
        Maillage::new(s, faces_de(f.iter().map(|q| q.to_vec()).collect()), vec![])
    }

    pub fn cube(c: f64) -> Maillage {
        Maillage::boite(c, c, c)
    }

    /// Cône ou tronc de cône d'axe Y, centré : rayon `r1` en bas, `r2` en haut
    /// (0 : pointe). `eventail` : bases en triangles autour d'un sommet central,
    /// sinon une face à `n` côtés.
    pub fn cone(r1: f64, r2: f64, h: f64, n: usize, eventail: bool) -> Maillage {
        let mut s: Vec<P3> = Vec::new();
        let mut f: Vec<Vec<usize>> = Vec::new();
        fn anneau(s: &mut Vec<P3>, n: usize, r: f64, y: f64) -> usize {
            let d = s.len();
            for k in 0..n {
                let a = 2.0 * PI * k as f64 / n as f64;
                s.push([r * a.cos(), y, r * a.sin()]);
            }
            d
        }
        fn fermer(s: &mut Vec<P3>, f: &mut Vec<Vec<usize>>, eventail: bool, tour: Vec<usize>, y: f64) {
            if !eventail {
                f.push(tour);
                return;
            }
            let c = s.len();
            s.push([0.0, y, 0.0]);
            for k in 0..tour.len() {
                f.push(vec![c, tour[k], tour[(k + 1) % tour.len()]]);
            }
        }
        let b = anneau(&mut s, n, r1, -h / 2.0);
        let bk = |k: usize| b + k % n;
        if r2 > 0.0 {
            let t = anneau(&mut s, n, r2, h / 2.0);
            let tk = |k: usize| t + k % n;
            for k in 0..n {
                f.push(vec![bk(k), tk(k), tk(k + 1), bk(k + 1)]);
            }
            fermer(&mut s, &mut f, eventail, (0..n).map(|k| tk(n - 1 - k)).collect(), h / 2.0);
        } else {
            let p = s.len();
            s.push([0.0, h / 2.0, 0.0]);
            for k in 0..n {
                f.push(vec![bk(k), p, bk(k + 1)]);
            }
        }
        fermer(&mut s, &mut f, eventail, (0..n).map(bk).collect(), -h / 2.0);
        Maillage::new(s, faces_de(f), vec![])
    }

    pub fn cylindre(r: f64, h: f64, n: usize, eventail: bool) -> Maillage {
        Maillage::cone(r, r, h, n, eventail)
    }

    /// Sphère UV, pôles sur l'axe Y.
    pub fn sphere_uv(r: f64, segments: usize, anneaux: usize) -> Maillage {
        let mut s = vec![[0.0, r, 0.0]];
        let mut f = Vec::new();
        for j in 1..anneaux {
            let p = PI * j as f64 / anneaux as f64;
            for k in 0..segments {
                let a = 2.0 * PI * k as f64 / segments as f64;
                s.push([r * p.sin() * a.cos(), r * p.cos(), r * p.sin() * a.sin()]);
            }
        }
        let sud = s.len();
        s.push([0.0, -r, 0.0]);
        let i = |j: usize, k: usize| 1 + (j - 1) * segments + k % segments;
        for k in 0..segments {
            f.push(vec![i(1, k), 0, i(1, k + 1)]);
            for j in 1..anneaux - 1 {
                f.push(vec![i(j + 1, k), i(j, k), i(j, k + 1), i(j + 1, k + 1)]);
            }
            f.push(vec![sud, i(anneaux - 1, k), i(anneaux - 1, k + 1)]);
        }
        Maillage::new(s, faces_de(f), vec![])
    }

    /// Icosphère : chaque triangle coupé en quatre à chaque subdivision.
    pub fn icosphere(r: f64, subdivisions: usize) -> Maillage {
        let t = (1.0 + 5f64.sqrt()) / 2.0;
        let mut s: Vec<P3> = Vec::new();
        fn point(s: &mut Vec<P3>, r: f64, p: P3) -> usize {
            let l = hypot3(p[0], p[1], p[2]);
            s.push([p[0] * r / l, p[1] * r / l, p[2] * r / l]);
            s.len() - 1
        }
        for p in [[-1., t, 0.], [1., t, 0.], [-1., -t, 0.], [1., -t, 0.], [0., -1., t], [0., 1., t], [0., -1., -t], [0., 1., -t], [t, 0., -1.], [t, 0., 1.], [-t, 0., -1.], [-t, 0., 1.]] {
            point(&mut s, r, p);
        }
        let mut f: Vec<[usize; 3]> = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        for _ in 1..subdivisions {
            let mut milieux: HashMap<Cle, usize> = HashMap::new();
            let mut g = Vec::with_capacity(f.len() * 4);
            for [a, b, c] in f {
                let mut milieu = |a: usize, b: usize| {
                    *milieux.entry(cle(a, b)).or_insert_with(|| {
                        let (p, q) = (s[a], s[b]);
                        point(&mut s, r, [p[0] + q[0], p[1] + q[1], p[2] + q[2]])
                    })
                };
                let (ab, bc, ca) = (milieu(a, b), milieu(b, c), milieu(c, a));
                g.extend([[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]]);
            }
            f = g;
        }
        Maillage::new(s, faces_de(f.iter().map(|q| q.to_vec()).collect()), vec![])
    }

    // ----- Parcours ----------------------------------------------------------

    /// Triangles en éventail par matériau, dans l'ordre de première apparition.
    pub fn triangles(&self) -> Vec<(Mat, Vec<usize>)> {
        let mut r: Vec<(Mat, Vec<usize>)> = Vec::new();
        for f in &self.faces {
            let k = match r.iter().position(|(m, _)| *m == f.m) {
                Some(k) => k,
                None => {
                    r.push((f.m.clone(), Vec::new()));
                    r.len() - 1
                }
            };
            for t in 1..f.s.len().saturating_sub(1) {
                r[k].1.extend([f.s[0], f.s[t], f.s[t + 1]]);
            }
        }
        r
    }

    /// Partie connexe, par les faces et les arêtes libres, des sommets `depart` :
    /// sommets en ordre croissant, et indices des faces qui les utilisent.
    pub fn connexe(&self, depart: &[usize]) -> (Vec<usize>, Vec<usize>) {
        let mut par_sommet: Vec<Vec<Result<usize, usize>>> = vec![Vec::new(); self.sommets.len()];
        for (n, f) in self.faces.iter().enumerate() {
            for &i in &f.s {
                par_sommet[i].push(Ok(n));
            }
        }
        for (n, a) in self.fils.iter().enumerate() {
            for &i in a {
                par_sommet[i].push(Err(n));
            }
        }
        let mut vus: HashSet<usize> = depart.iter().copied().filter(|&i| i < self.sommets.len()).collect();
        let mut pile: Vec<usize> = vus.iter().copied().collect();
        let mut faces = HashSet::new();
        while let Some(i) = pile.pop() {
            for e in &par_sommet[i] {
                let liste: &[usize] = match e {
                    Ok(n) => {
                        if !faces.insert(*n) {
                            continue;
                        }
                        &self.faces[*n].s
                    }
                    Err(n) => &self.fils[*n],
                };
                for &j in liste {
                    if vus.insert(j) {
                        pile.push(j);
                    }
                }
            }
        }
        let mut s: Vec<usize> = vus.into_iter().collect();
        s.sort_unstable();
        let mut f: Vec<usize> = faces.into_iter().collect();
        f.sort_unstable();
        (s, f)
    }

    /// Arêtes uniques des polygones, puis arêtes libres.
    pub fn aretes(&self) -> Vec<Arete> {
        let mut vues = HashSet::new();
        let mut a = Vec::new();
        let mut ajouter = |i: usize, j: usize| {
            if vues.insert(cle(i, j)) {
                a.push([i, j]);
            }
        };
        for f in &self.faces {
            for k in 0..f.s.len() {
                ajouter(f.s[k], f.s[(k + 1) % f.s.len()]);
            }
        }
        for x in &self.fils {
            ajouter(x[0], x[1]);
        }
        a
    }

    /// Plus court chemin de sommets, le long des arêtes, de `depart` à `arrivee`.
    pub fn chemin_sommets(&self, depart: usize, arrivee: usize) -> Option<Vec<usize>> {
        let adj = self.adjacence();
        dijkstra(self.sommets.len(), |k| adj[k].clone(), &[depart], &HashSet::from([arrivee]))
    }

    /// Les deux arêtes et celles du plus court chemin entre leurs extrémités.
    pub fn chemin_aretes(&self, depart: Arete, arrivee: Arete) -> Option<Vec<Arete>> {
        let adj = self.adjacence();
        let l = dijkstra(self.sommets.len(), |k| adj[k].clone(), &depart, &HashSet::from(arrivee))?;
        let mut r = vec![depart];
        let mut vues = HashSet::from([cle(depart[0], depart[1])]);
        for a in l.windows(2).map(|w| [w[0], w[1]]).chain([arrivee]) {
            if vues.insert(cle(a[0], a[1])) {
                r.push(a);
            }
        }
        Some(r)
    }

    /// Plus court chemin de face en face par les arêtes communes.
    pub fn chemin_faces(&self, depart: usize, arrivee: usize) -> Option<Vec<usize>> {
        let n = self.faces.len();
        if depart >= n || arrivee >= n {
            return None;
        }
        let centres: Vec<P3> = self
            .faces
            .iter()
            .map(|f| {
                let mut c = [0.0; 3];
                for d in 0..3 {
                    c[d] = f.s.iter().fold(0.0, |x, &i| x + self.sommets[i][d]) / f.s.len() as f64;
                }
                c
            })
            .collect();
        let mut par_arete: HashMap<Cle, Vec<usize>> = HashMap::new();
        for (k, f) in self.faces.iter().enumerate() {
            for m in 0..f.s.len() {
                par_arete.entry(cle(f.s[m], f.s[(m + 1) % f.s.len()])).or_default().push(k);
            }
        }
        let voisins = |k: usize| {
            let f = &self.faces[k];
            let mut r = Vec::new();
            for m in 0..f.s.len() {
                for &g in &par_arete[&cle(f.s[m], f.s[(m + 1) % f.s.len()])] {
                    if g != k {
                        r.push((g, dist(centres[k], centres[g])));
                    }
                }
            }
            r
        };
        dijkstra(n, voisins, &[depart], &HashSet::from([arrivee]))
    }

    fn adjacence(&self) -> Vec<Vec<(usize, f64)>> {
        let mut adj = vec![Vec::new(); self.sommets.len()];
        for [i, j] in self.aretes() {
            let w = dist(self.sommets[i], self.sommets[j]);
            adj[i].push((j, w));
            adj[j].push((i, w));
        }
        adj
    }

    /// Normale d'une face (méthode de Newell : tient compte de tout le polygone).
    pub fn normale(&self, f: &[usize]) -> P3 {
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        for k in 0..f.len() {
            let a = self.sommets[f[k]];
            let b = self.sommets[f[(k + 1) % f.len()]];
            x += (a[1] - b[1]) * (a[2] + b[2]);
            y += (a[2] - b[2]) * (a[0] + b[0]);
            z += (a[0] - b[0]) * (a[1] + b[1]);
        }
        let l = hypot3(x, y, z);
        let l = if l == 0.0 { 1.0 } else { l };
        [x / l, y / l, z / l]
    }

    /// Découpe d'une face en triangles, dans son sens : oreilles successives dans
    /// le plan moyen, ce qui convient aux polygones concaves ; parmi les oreilles
    /// possibles, celle dont le plus petit angle est le plus grand.
    pub fn trianguler(&self, s: &[usize]) -> Vec<[usize; 3]> {
        if s.len() < 3 {
            return vec![];
        }
        if s.len() == 3 {
            return vec![[s[0], s[1], s[2]]];
        }
        let n = self.normale(s);
        let ax = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = cross(n, ax);
        let lu = hypot3(u[0], u[1], u[2]);
        let u = [u[0] / lu, u[1] / lu, u[2] / lu];
        let v = cross(n, u);
        let p: HashMap<usize, [f64; 2]> = s
            .iter()
            .map(|&i| {
                let q = self.sommets[i];
                (i, [dot(q, u), dot(q, v)])
            })
            .collect();
        let croix = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        let angle = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
            let (x, y) = ([b[0] - a[0], b[1] - a[1]], [c[0] - a[0], c[1] - a[1]]);
            let d = hypot2(x[0], x[1]) * hypot2(y[0], y[1]);
            let d = if d == 0.0 { 1.0 } else { d };
            ((x[0] * y[0] + x[1] * y[1]) / d).clamp(-1.0, 1.0).acos()
        };
        let mut reste = s.to_vec();
        let mut tris = Vec::new();
        let echelle = p.values().fold(0.0f64, |e, q| e.max(q[0].abs()).max(q[1].abs()));
        let eps = 1e-12 * if echelle == 0.0 { 1.0 } else { echelle * echelle };
        while reste.len() > 3 {
            let mut meilleur = None;
            let mut note = -1.0;
            let l = reste.len();
            for k in 0..l {
                let (ia, ib, ic) = (reste[(k + l - 1) % l], reste[k], reste[(k + 1) % l]);
                let (a, b, c) = (p[&ia], p[&ib], p[&ic]);
                if croix(a, b, c) <= eps {
                    continue;
                }
                let dedans = reste.iter().any(|&j| {
                    j != ia && j != ib && j != ic && croix(a, b, p[&j]) >= -eps && croix(b, c, p[&j]) >= -eps && croix(c, a, p[&j]) >= -eps
                });
                if dedans {
                    continue;
                }
                let m = angle(a, b, c).min(angle(b, c, a)).min(angle(c, a, b));
                if m > note {
                    note = m;
                    meilleur = Some(k);
                }
            }
            let Some(k) = meilleur else { break };
            tris.push([reste[(k + l - 1) % l], reste[k], reste[(k + 1) % l]]);
            reste.remove(k);
        }
        for k in 1..reste.len() - 1 {
            tris.push([reste[0], reste[k], reste[k + 1]]);
        }
        tris
    }

    // ----- Édition -----------------------------------------------------------

    /// Retire des sommets, et les faces et arêtes libres qui les utilisent.
    /// Renvoie la table ancien indice → nouveau (−1 : supprimé).
    pub fn supprimer_sommets(&mut self, indices: &[usize]) -> Vec<isize> {
        let retires: HashSet<usize> = indices.iter().copied().collect();
        let mut n = Vec::with_capacity(self.sommets.len());
        let mut nouveau = Vec::with_capacity(self.sommets.len());
        for (i, p) in self.sommets.iter().enumerate() {
            if retires.contains(&i) {
                n.push(-1);
            } else {
                n.push(nouveau.len() as isize);
                nouveau.push(*p);
            }
        }
        self.sommets = nouveau;
        self.faces.retain(|f| f.s.iter().all(|i| !retires.contains(i)));
        for f in &mut self.faces {
            for i in &mut f.s {
                *i = n[*i] as usize;
            }
        }
        self.fils.retain(|a| !retires.contains(&a[0]) && !retires.contains(&a[1]));
        for a in &mut self.fils {
            *a = [n[a[0]] as usize, n[a[1]] as usize];
        }
        self.coutures.retain(|a| !retires.contains(&a[0]) && !retires.contains(&a[1]));
        for a in &mut self.coutures {
            *a = [n[a[0]] as usize, n[a[1]] as usize];
        }
        n
    }

    /// Fusionne des sommets en un seul, placé selon `cible`.
    pub fn fusionner(&mut self, indices: &[usize], cible: Cible) -> Vec<isize> {
        if indices.len() < 2 {
            return (0..self.sommets.len() as isize).collect();
        }
        let p = match cible {
            Cible::Centre => None,
            Cible::Premier => Some(self.sommets[indices[0]]),
            Cible::Dernier => Some(self.sommets[*indices.last().unwrap()]),
        };
        let n = self.reunir(&[indices.to_vec()]);
        if let Some(p) = p {
            self.sommets[n[indices[0]] as usize] = p;
        }
        n
    }

    /// Chaque groupe de sommets réuni en son premier, à leur position moyenne ;
    /// les faces dégénérées disparaissent. Renvoie la table ancien → nouveau.
    pub fn reunir(&mut self, groupes: &[Vec<usize>]) -> Vec<isize> {
        let mut vers: Vec<usize> = (0..self.sommets.len()).collect();
        let mut autres = Vec::new();
        for g in groupes {
            if g.len() < 2 {
                continue;
            }
            let mut c = [0.0; 3];
            for &i in g {
                for k in 0..3 {
                    c[k] += self.sommets[i][k] / g.len() as f64;
                }
            }
            self.sommets[g[0]] = c;
            for &i in &g[1..] {
                vers[i] = g[0];
                autres.push(i);
            }
        }
        if autres.is_empty() {
            return vers.iter().map(|&i| i as isize).collect();
        }
        for f in &mut self.faces {
            let m: Vec<usize> = f.s.iter().map(|&i| vers[i]).collect();
            recoller(f, &m);
        }
        self.faces.retain(|f| distincts(&f.s) >= 3);
        for a in self.fils.iter_mut().chain(&mut self.coutures) {
            *a = [vers[a[0]], vers[a[1]]];
        }
        self.nettoyer_fils();
        let mut n = self.supprimer_sommets(&autres);
        for &i in &autres {
            n[i] = n[vers[i]];
        }
        n
    }

    /// Sommets distants d'au plus `tolerance`, de proche en proche, réunis ;
    /// `groupe(i)` : seuls les sommets d'un même objet se réunissent.
    pub fn fusionner_proches(&mut self, indices: Option<&[usize]>, tolerance: f64, groupe: &dyn Fn(usize) -> usize) -> (Vec<isize>, Fusion) {
        let tous: Vec<usize> = (0..self.sommets.len()).collect();
        let indices = indices.unwrap_or(&tous);
        let pas = if tolerance > 0.0 { tolerance } else { 1.0 };
        let t2 = tolerance * tolerance;
        let mut grille: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        let mut parent: Vec<usize> = (0..self.sommets.len()).collect();
        fn racine(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for &i in indices {
            let p = self.sommets[i];
            let c = [(p[0] / pas).floor() as i64, (p[1] / pas).floor() as i64, (p[2] / pas).floor() as i64];
            let g = groupe(i);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let Some(l) = grille.get(&[c[0] + dx, c[1] + dy, c[2] + dz]) else { continue };
                        for &j in l {
                            let q = self.sommets[j];
                            let d = sub(p, q);
                            if groupe(j) != g || dot(d, d) > t2 {
                                continue;
                            }
                            let (a, b) = (racine(&mut parent, i), racine(&mut parent, j));
                            if a != b {
                                parent[a.max(b)] = a.min(b);
                            }
                        }
                    }
                }
            }
            grille.entry(c).or_default().push(i);
        }
        let mut tries = indices.to_vec();
        tries.sort_unstable();
        let mut ordre: Vec<usize> = Vec::new();
        let mut par_racine: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in tries {
            let r = racine(&mut parent, i);
            let e = par_racine.entry(r).or_default();
            if e.is_empty() {
                ordre.push(r);
            }
            e.push(i);
        }
        let groupes: Vec<Vec<usize>> = ordre.iter().map(|r| par_racine.remove(r).unwrap()).filter(|g| g.len() > 1).collect();
        let f = Fusion { groupes: groupes.len(), retires: groupes.iter().map(|g| g.len() - 1).sum() };
        (self.reunir(&groupes), f)
    }

    /// Révolution autour d'un axe : chaque sommet est recopié à chaque pas, un
    /// sommet seul donne un arc d'arêtes libres, une arête une bande de faces.
    /// Faces orientées à l'opposé d'une face qui borde le profil, sinon vers
    /// l'extérieur de l'axe.
    pub fn revolution(&mut self, sommets: &[usize], aretes: &[Arete], o: &Revolution) -> Revolue {
        let lu = hypot3(o.axe[0], o.axe[1], o.axe[2]);
        let u = [o.axe[0] / lu, o.axe[1] / lu, o.axe[2] / lu];
        let ferme = o.fermer && (o.angle.abs() - 360.0).abs() < 1e-9;
        let pas = o.angle / o.segments as f64 * PI / 180.0;
        let centre = o.centre;
        let tourne = |p: P3, a: f64| {
            let d = sub(p, centre);
            let (c, s) = (a.cos(), a.sin());
            let x = cross(u, d);
            let k = dot(u, d) * (1.0 - c);
            [
                centre[0] + d[0] * c + x[0] * s + u[0] * k,
                centre[1] + d[1] * c + x[1] * s + u[1] * k,
                centre[2] + d[2] * c + x[2] * s + u[2] * k,
            ]
        };
        let sur_axe = |p: P3| {
            let d = sub(p, centre);
            let t = dot(d, u);
            hypot3(d[0] - u[0] * t, d[1] - u[1] * t, d[2] - u[2] * t) < 1e-9
        };
        let extremites: HashSet<usize> = aretes.iter().flatten().copied().collect();
        let mut copies: Vec<(usize, Vec<usize>)> = Vec::new();
        let mut index: HashMap<usize, usize> = HashMap::new();
        for &i in sommets.iter().chain(aretes.iter().flatten()) {
            if index.contains_key(&i) {
                continue;
            }
            let p = self.sommets[i];
            let fixe = sur_axe(p);
            let mut l = vec![i];
            for k in 1..=o.segments {
                if fixe || (ferme && k == o.segments) {
                    l.push(i);
                } else {
                    self.sommets.push(tourne(p, pas * k as f64));
                    l.push(self.sommets.len() - 1);
                }
            }
            index.insert(i, copies.len());
            copies.push((i, l));
        }
        let mut fils = Vec::new();
        let mut vus = HashSet::new();
        for &i in sommets {
            if !vus.insert(i) || extremites.contains(&i) {
                continue;
            }
            let l = &copies[index[&i]].1;
            for k in 0..o.segments {
                if l[k] != l[k + 1] {
                    fils.push([l[k], l[k + 1]]);
                }
            }
        }
        // Profil orienté de proche en proche, par composante d'arêtes reliées
        let mut sens: Vec<Arete> = Vec::new();
        let mut rang: HashMap<Cle, usize> = HashMap::new();
        let mut voisines: HashMap<usize, Vec<Arete>> = HashMap::new();
        for a in aretes {
            for s in *a {
                voisines.entry(s).or_default().push(*a);
            }
        }
        let mut dirigees = HashSet::new();
        for f in &self.faces {
            for k in 0..f.s.len() {
                dirigees.insert((f.s[k], f.s[(k + 1) % f.s.len()]));
            }
        }
        for depart in aretes {
            if rang.contains_key(&cle(depart[0], depart[1])) {
                continue;
            }
            let mut composante = Vec::new();
            sens.push(*depart);
            rang.insert(cle(depart[0], depart[1]), sens.len() - 1);
            let mut file = vec![sens.len() - 1];
            while let Some(n) = file.pop() {
                composante.push(n);
                let [a, b] = sens[n];
                for (s, apres) in [(b, true), (a, false)] {
                    for &[i, j] in &voisines[&s] {
                        if rang.contains_key(&cle(i, j)) {
                            continue;
                        }
                        let autre = if i == s { j } else { i };
                        sens.push(if apres { [s, autre] } else { [autre, s] });
                        rang.insert(cle(i, j), sens.len() - 1);
                        file.push(sens.len() - 1);
                    }
                }
            }
            let mut score = 0.0;
            for &n in &composante {
                let [a, b] = sens[n];
                if dirigees.contains(&(a, b)) {
                    score -= 1e9;
                } else if dirigees.contains(&(b, a)) {
                    score += 1e9;
                } else {
                    let (pa, pb) = (self.sommets[a], self.sommets[b]);
                    let m = mix(pa, pb, 0.5);
                    let d = sub(tourne(m, pas), m);
                    let r = sub(m, centre);
                    let t = dot(r, u);
                    let radial = [r[0] - u[0] * t, r[1] - u[1] * t, r[2] - u[2] * t];
                    score += dot(cross(sub(pb, pa), d), radial);
                }
            }
            if score < 0.0 {
                for &n in &composante {
                    sens[n].reverse();
                }
            }
        }
        let premiere = self.faces.len();
        for [a, b] in sens {
            let (ca, cb) = (&copies[index[&a]].1, &copies[index[&b]].1);
            for k in 0..o.segments {
                let s = sans_doublons(&[ca[k], cb[k], cb[k + 1], ca[k + 1]]);
                if distincts(&s) >= 3 {
                    self.faces.push(Face { s, m: o.materiau.clone(), uv: Vec::new() });
                }
            }
        }
        self.fils.extend_from_slice(&fils);
        self.nettoyer_fils();
        Revolue { faces: (premiere..self.faces.len()).collect(), fils, copies }
    }

    /// Contour d'un ensemble de faces réunies en une seule. Elles doivent être
    /// connexes par leurs arêtes, de même orientation, et former un seul polygone
    /// sans trou. `plane` : faux si elles ne sont pas dans un même plan.
    pub fn contour_faces(&self, faces: &[usize]) -> Result<Contour, String> {
        if faces.len() < 2 {
            return Err("Sélectionnez au moins 2 faces".into());
        }
        let mut par_arete: HashMap<Cle, Vec<usize>> = HashMap::new();
        let mut ordre_aretes = Vec::new();
        let mut dirigees = HashSet::new();
        let mut ordre = Vec::new();
        for (n, &nf) in faces.iter().enumerate() {
            let s = &self.faces[nf].s;
            for k in 0..s.len() {
                let (i, j) = (s[k], s[(k + 1) % s.len()]);
                if !dirigees.insert((i, j)) {
                    return Err("Faces d’orientations opposées : inversez-en".into());
                }
                ordre.push((i, j));
                let e = par_arete.entry(cle(i, j)).or_default();
                if e.is_empty() {
                    ordre_aretes.push(cle(i, j));
                }
                e.push(n);
            }
        }
        let mut parent: Vec<usize> = (0..faces.len()).collect();
        fn racine(p: &mut [usize], n: usize) -> usize {
            if p[n] != n {
                let r = racine(p, p[n]);
                p[n] = r;
            }
            p[n]
        }
        for c in &ordre_aretes {
            let l = &par_arete[c];
            if l.len() > 2 {
                return Err("Une arête est partagée par plus de 2 faces".into());
            }
            if l.len() == 2 {
                let (a, b) = (racine(&mut parent, l[0]), racine(&mut parent, l[1]));
                parent[a] = b;
            }
        }
        let r0 = racine(&mut parent, 0);
        if (0..faces.len()).any(|n| racine(&mut parent, n) != r0) {
            return Err("Faces non connexes : au moins une est isolée".into());
        }
        let mut suivant: HashMap<usize, usize> = HashMap::new();
        let mut premier = None;
        for &(i, j) in &ordre {
            if dirigees.contains(&(j, i)) {
                continue;
            }
            if suivant.insert(i, j).is_some() {
                return Err("Les faces se touchent par un sommet seulement : contour non simple".into());
            }
            premier.get_or_insert(i);
        }
        let Some(premier) = premier else { return Err("Surface fermée : pas de contour".into()) };
        let depart = self.faces[faces[0]].s.iter().copied().find(|i| suivant.contains_key(i)).unwrap_or(premier);
        let mut s = vec![depart];
        let mut i = suivant[&depart];
        while i != depart {
            s.push(i);
            match suivant.get(&i) {
                Some(&j) if s.len() <= suivant.len() => i = j,
                _ => return Err("Contour ouvert".into()),
            }
        }
        if s.len() < suivant.len() {
            return Err("La réunion entourerait un trou".into());
        }
        let n = self.normale(&s);
        let o = self.sommets[s[0]];
        let (mut ecart, mut taille) = (0.0f64, 0.0f64);
        for &i in &s {
            let d = sub(self.sommets[i], o);
            ecart = ecart.max(dot(d, n).abs());
            taille = taille.max(hypot3(d[0], d[1], d[2]));
        }
        let t = if taille == 0.0 { 1.0 } else { taille };
        Ok(Contour { s, plane: ecart <= 1e-4 * t })
    }

    /// Coupe chaque arête en `coupes` + 1 segments égaux ; les nouveaux sommets
    /// sont insérés dans toutes les faces qui la bordent. Renvoie, par arête
    /// (i < j), les nouveaux sommets de i vers j.
    pub fn subdiviser(&mut self, aretes: &[Arete], coupes: usize) -> HashMap<Cle, Vec<usize>> {
        let mut nouveaux: HashMap<Cle, Vec<usize>> = HashMap::new();
        for &[a, b] in aretes {
            let c = cle(a, b);
            if nouveaux.contains_key(&c) || a == b {
                continue;
            }
            let (p, q) = (self.sommets[c.0], self.sommets[c.1]);
            let mut l = Vec::with_capacity(coupes);
            for k in 1..=coupes {
                l.push(self.sommets.len());
                self.sommets.push(mix(p, q, k as f64 / (coupes + 1) as f64));
            }
            nouveaux.insert(c, l);
        }
        for f in &mut self.faces {
            let mut s = Vec::with_capacity(f.s.len());
            let avec_uv = f.uv.len() == f.s.len();
            let mut uv = Vec::with_capacity(f.uv.len());
            for k in 0..f.s.len() {
                let (a, b) = (f.s[k], f.s[(k + 1) % f.s.len()]);
                s.push(a);
                if avec_uv {
                    uv.push(f.uv[k]);
                }
                if let Some(l) = nouveaux.get(&cle(a, b)) {
                    if a < b {
                        s.extend_from_slice(l);
                    } else {
                        s.extend(l.iter().rev());
                    }
                    if avec_uv {
                        let n = l.len() + 1;
                        uv.extend((1..n).map(|m| mix_uv(f.uv[k], f.uv[(k + 1) % f.s.len()], m as f64 / n as f64)));
                    }
                }
            }
            f.s = s;
            f.uv = uv;
        }
        let mut fils = Vec::with_capacity(self.fils.len());
        for &[a, b] in &self.fils {
            match nouveaux.get(&cle(a, b)) {
                None => fils.push([a, b]),
                Some(l) => {
                    let mut c = vec![a.min(b)];
                    c.extend_from_slice(l);
                    c.push(a.max(b));
                    fils.extend(c.windows(2).map(|w| [w[0], w[1]]));
                }
            }
        }
        self.fils = fils;
        self.coutures = self.coutures.iter().flat_map(|&[a, b]| match nouveaux.get(&cle(a, b)) {
            None => vec![[a, b]],
            Some(l) => {
                let mut c = vec![a.min(b)];
                c.extend_from_slice(l);
                c.push(a.max(b));
                c.windows(2).map(|w| [w[0], w[1]]).collect()
            }
        }).collect();
        nouveaux
    }

    /// Anneau d'arêtes à partir de [i, j] : de quadrilatère en quadrilatère, par
    /// l'arête opposée, dans les deux sens, jusqu'à une face qui n'en est pas un,
    /// un bord, une arête à plus de deux faces, ou le retour au départ.
    pub fn anneau(&self, i: usize, j: usize) -> Option<Anneau> {
        let mut bordees: HashMap<Cle, Vec<usize>> = HashMap::new();
        for (n, f) in self.faces.iter().enumerate() {
            for k in 0..f.s.len() {
                bordees.entry(cle(f.s[k], f.s[(k + 1) % f.s.len()])).or_default().push(n);
            }
        }
        let vide = Vec::new();
        let depart = bordees.get(&cle(i, j)).unwrap_or(&vide);
        if depart.len() > 2 || !depart.iter().any(|&f| self.faces[f].s.len() == 4) {
            return None;
        }
        let mut vus = HashSet::new();
        let mut ferme = false;
        let (r1, q1) = self.marcher(&bordees, [i, j], depart[0], &mut vus, &mut ferme);
        let mut rails = vec![[i, j]];
        rails.extend(r1);
        let mut quads = q1;
        let mut d = 0;
        if !ferme && depart.len() > 1 {
            let (mut r2, mut q2) = self.marcher(&bordees, [i, j], depart[1], &mut vus, &mut ferme);
            d = r2.len();
            r2.reverse();
            q2.reverse();
            r2.extend(rails);
            q2.extend(quads);
            rails = r2;
            quads = q2;
        }
        Some(Anneau { rails, quads, depart: d, ferme })
    }

    /// Marche d'un anneau depuis le rail `depart` à travers la face `f0`.
    fn marcher(&self, bordees: &HashMap<Cle, Vec<usize>>, depart: Arete, f0: usize, vus: &mut HashSet<usize>, ferme: &mut bool) -> (Vec<Arete>, Vec<usize>) {
        let voisin = |f: usize, a: usize, b: usize| {
            let s = &self.faces[f].s;
            let k = s.iter().position(|&x| x == a).unwrap();
            let u = s[(k + 1) % 4];
            if u == b { s[(k + 3) % 4] } else { u }
        };
        let [i, j] = depart;
        let (mut a, mut b) = (i, j);
        let (mut rails, mut quads) = (Vec::new(), Vec::new());
        let mut f = Some(f0);
        while let Some(n) = f {
            if self.faces[n].s.len() != 4 || vus.contains(&n) {
                break;
            }
            vus.insert(n);
            quads.push(n);
            (a, b) = (voisin(n, a, b), voisin(n, b, a));
            if cle(a, b) == cle(i, j) {
                *ferme = true;
                break;
            }
            let l = &bordees[&cle(a, b)];
            let g = if l.len() == 2 { Some(if l[0] == n { l[1] } else { l[0] }) } else { None };
            // Anneau qui revient sur un quadrilatère déjà coupé ailleurs : arrêt avant
            if g.is_some_and(|g| vus.contains(&g)) {
                quads.pop();
                break;
            }
            rails.push([a, b]);
            f = g;
        }
        (rails, quads)
    }

    /// Boucle de coupe le long de l'anneau : `coupes` boucles régulières ;
    /// `glissement` ∈ ]−1, 1[ les resserre vers un côté ; `uniforme` : même
    /// distance sur chaque rail que sur le rail de départ, comptée depuis le côté
    /// b avec `inverser`. Renvoie les arêtes des nouvelles boucles.
    pub fn couper_boucle(&mut self, an: &Anneau, coupes: usize, glissement: f64, uniforme: bool, inverser: bool) -> Vec<Arete> {
        self.couper_boucle_suivi(an, coupes, glissement, uniforme, inverser).0
    }

    /// Rend aussi la nouvelle place de chaque face, `None` pour les quadrilatères coupés.
    pub fn couper_boucle_suivi(&mut self, an: &Anneau, coupes: usize, glissement: f64, uniforme: bool, inverser: bool) -> (Vec<Arete>, Vec<Option<usize>>) {
        let g = glissement.clamp(-0.99, 0.99);
        let ts: Vec<f64> = (0..coupes)
            .map(|k| {
                let t = (k + 1) as f64 / (coupes + 1) as f64;
                if g > 0.0 { t + g * (1.0 - t) } else { t + g * t }
            })
            .collect();
        let [i0, j0] = an.rails[an.depart];
        let l0 = dist(self.sommets[i0], self.sommets[j0]);
        let mut nouveaux: HashMap<Cle, (usize, Vec<usize>, Vec<f64>)> = HashMap::new();
        for &[a, b] in &an.rails {
            let (p, q) = (self.sommets[a], self.sommets[b]);
            let l = dist(p, q);
            let mut ids = Vec::with_capacity(coupes);
            let mut places = Vec::with_capacity(coupes);
            for &t in &ts {
                let mut u = t;
                if uniforme && l > 0.0 {
                    u = (if inverser { 1.0 - (1.0 - t) * l0 / l } else { t * l0 / l }).clamp(0.001, 0.999);
                }
                ids.push(self.sommets.len());
                places.push(u);
                self.sommets.push(mix(p, q, u));
            }
            nouveaux.insert(cle(a, b), (a, ids, places));
        }
        let depuis = |a: usize, b: usize| -> Option<Vec<usize>> {
            nouveaux.get(&cle(a, b)).map(|(d, l, _)| if *d == a { l.clone() } else { l.iter().rev().copied().collect() })
        };
        // Place des nouveaux sommets de a vers b, dans l'ordre de `depuis`
        let places = |a: usize, b: usize| -> Vec<f64> {
            nouveaux.get(&cle(a, b)).map_or(Vec::new(), |(d, _, p)| if *d == a { p.clone() } else { p.iter().rev().map(|u| 1.0 - u).collect() })
        };
        let quads: HashSet<usize> = an.quads.iter().copied().collect();
        let (mut aretes, mut faces) = (Vec::new(), Vec::with_capacity(self.faces.len()));
        let mut table = Vec::with_capacity(self.faces.len());
        for (n, f) in std::mem::take(&mut self.faces).into_iter().enumerate() {
            let s = &f.s;
            let k = if quads.contains(&n) { [0, 1].into_iter().find(|&k| depuis(s[k], s[k + 1]).is_some() && depuis(s[(k + 3) % 4], s[(k + 2) % 4]).is_some()) } else { None };
            if let Some(k) = k {
                let [p0, p1, p2, p3] = [s[k], s[(k + 1) % 4], s[(k + 2) % 4], s[(k + 3) % 4]];
                let bande = |x: usize, y: usize| {
                    let mut c = vec![x];
                    c.extend(depuis(x, y).unwrap());
                    c.push(y);
                    c
                };
                let (c0, c3) = (bande(p0, p1), bande(p3, p2));
                let avec_uv = f.uv.len() == 4;
                let long = |x: usize, y: usize, a: usize, b: usize| -> Vec<[f32; 2]> {
                    let mut l = vec![f.uv[a]];
                    l.extend(places(x, y).iter().map(|u| mix_uv(f.uv[a], f.uv[b], *u)));
                    l.push(f.uv[b]);
                    l
                };
                let (u0, u3) = if avec_uv { (long(p0, p1, k, (k + 1) % 4), long(p3, p2, (k + 3) % 4, (k + 2) % 4)) } else { (Vec::new(), Vec::new()) };
                for m in 0..c0.len() - 1 {
                    let uv = if avec_uv { vec![u0[m], u0[m + 1], u3[m + 1], u3[m]] } else { Vec::new() };
                    faces.push(Face { s: vec![c0[m], c0[m + 1], c3[m + 1], c3[m]], m: f.m.clone(), uv });
                }
                for m in 1..c0.len() - 1 {
                    aretes.push([c0[m], c3[m]]);
                }
                table.push(None);
                continue;
            }
            table.push(Some(faces.len()));
            let mut t = Vec::with_capacity(s.len());
            let avec_uv = f.uv.len() == s.len();
            let mut uv = Vec::with_capacity(f.uv.len());
            for k in 0..s.len() {
                let suivant = (k + 1) % s.len();
                t.push(s[k]);
                if avec_uv {
                    uv.push(f.uv[k]);
                }
                if let Some(l) = depuis(s[k], s[suivant]) {
                    t.extend(l);
                    if avec_uv {
                        uv.extend(places(s[k], s[suivant]).iter().map(|u| mix_uv(f.uv[k], f.uv[suivant], *u)));
                    }
                }
            }
            faces.push(Face { s: t, m: f.m, uv });
        }
        self.faces = faces;
        let coutures = self.coutures.iter().flat_map(|&[a, b]| match depuis(a, b) {
            None => vec![[a, b]],
            Some(l) => {
                let mut c = vec![a];
                c.extend(l);
                c.push(b);
                c.windows(2).map(|w| [w[0], w[1]]).collect()
            }
        }).collect();
        self.coutures = coutures;
        (aretes, table)
    }

    /// Dissolution d'arêtes : chacune doit border exactement deux faces ; les
    /// faces reliées forment des groupes, réunis chacun en une face.
    pub fn dissolution(&self, aretes: &[Arete]) -> Result<Vec<GroupeDissous>, String> {
        if aretes.is_empty() {
            return Err("Sélectionnez au moins une arête".into());
        }
        let mut bordees: HashMap<Cle, Vec<usize>> = HashMap::new();
        for (n, f) in self.faces.iter().enumerate() {
            for k in 0..f.s.len() {
                bordees.entry(cle(f.s[k], f.s[(k + 1) % f.s.len()])).or_default().push(n);
            }
        }
        let mut parent: HashMap<usize, usize> = HashMap::new();
        fn racine(p: &HashMap<usize, usize>, f: usize) -> usize {
            let mut f = f;
            while p[&f] != f {
                f = p[&f];
            }
            f
        }
        let vide = Vec::new();
        for &[i, j] in aretes {
            let l = bordees.get(&cle(i, j)).unwrap_or(&vide);
            if l.len() < 2 {
                return Err("Arête de bord : une seule face, rien à dissoudre".into());
            }
            if l.len() > 2 {
                return Err("Une arête est partagée par plus de 2 faces".into());
            }
            if l[0] == l[1] {
                return Err("Arête bordée deux fois par la même face".into());
            }
            for &f in l {
                parent.entry(f).or_insert(f);
            }
            let (a, b) = (racine(&parent, l[0]), racine(&parent, l[1]));
            parent.insert(a, b);
        }
        let mut ordre = Vec::new();
        let mut groupes: HashMap<usize, Vec<usize>> = HashMap::new();
        for n in 0..self.faces.len() {
            if !parent.contains_key(&n) {
                continue;
            }
            let r = racine(&parent, n);
            let e = groupes.entry(r).or_default();
            if e.is_empty() {
                ordre.push(r);
            }
            e.push(n);
        }
        let mut r = Vec::with_capacity(ordre.len());
        for g in ordre {
            let faces = groupes.remove(&g).unwrap();
            let c = self.contour_faces(&faces)?;
            r.push(GroupeDissous { faces, s: c.s, plane: c.plane });
        }
        Ok(r)
    }

    /// Sommets à la même position (à `tolerance` près) réunis : les fichiers STL
    /// et glTF les répètent.
    pub fn souder(&mut self, tolerance: f64) -> &mut Self {
        let mut deja: HashMap<[i64; 3], usize> = HashMap::new();
        let mut sommets = Vec::new();
        let n: Vec<usize> = self
            .sommets
            .iter()
            .map(|p| {
                let q = |c: f64| arrondi(c / tolerance) as i64;
                *deja.entry([q(p[0]), q(p[1]), q(p[2])]).or_insert_with(|| {
                    sommets.push(*p);
                    sommets.len() - 1
                })
            })
            .collect();
        self.sommets = sommets;
        for f in &mut self.faces {
            let m: Vec<usize> = f.s.iter().map(|&i| n[i]).collect();
            recoller(f, &m);
        }
        self.faces.retain(|f| distincts(&f.s) >= 3);
        for a in self.fils.iter_mut().chain(&mut self.coutures) {
            *a = [n[a[0]], n[a[1]]];
        }
        self.nettoyer_fils()
    }

    /// Morceaux d'un seul tenant : indices de leurs sommets.
    pub fn ilots(&self) -> Vec<Vec<usize>> {
        let mut p: Vec<usize> = (0..self.sommets.len()).collect();
        fn chef(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for f in &self.faces {
            for &i in &f.s {
                let (a, b) = (chef(&mut p, i), chef(&mut p, f.s[0]));
                p[a] = b;
            }
        }
        for &[i, j] in &self.fils {
            let (a, b) = (chef(&mut p, i), chef(&mut p, j));
            p[a] = b;
        }
        let mut ordre = Vec::new();
        let mut r: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..self.sommets.len() {
            let c = chef(&mut p, i);
            let e = r.entry(c).or_default();
            if e.is_empty() {
                ordre.push(c);
            }
            e.push(i);
        }
        ordre.iter().map(|c| r.remove(c).unwrap()).collect()
    }

    pub fn boite_englobante(&self) -> Option<Boite> {
        let mut b = Boite { min: *self.sommets.first()?, max: self.sommets[0] };
        for p in &self.sommets {
            for k in 0..3 {
                b.min[k] = b.min[k].min(p[k]);
                b.max[k] = b.max[k].max(p[k]);
            }
        }
        Some(b)
    }

    /// Un maillage par matériau, dans l'ordre de première apparition ; les
    /// sommets isolés et les arêtes libres vont avec le premier.
    pub fn par_materiau(&self) -> Vec<(Mat, Maillage)> {
        let mut groupes: Vec<(Mat, Vec<&Face>)> = Vec::new();
        for f in &self.faces {
            match groupes.iter_mut().find(|(m, _)| *m == f.m) {
                Some((_, l)) => l.push(f),
                None => groupes.push((f.m.clone(), vec![f])),
            }
        }
        if groupes.len() < 2 {
            return vec![(self.faces.first().map_or_else(defaut, |f| f.m.clone()), self.clone())];
        }
        let utilises: HashSet<usize> = self.faces.iter().flat_map(|f| f.s.iter().copied()).collect();
        let mut sortie = Vec::with_capacity(groupes.len());
        for (n, (m, faces)) in groupes.into_iter().enumerate() {
            let mut r = Maillage::default();
            let mut nouveau: HashMap<usize, usize> = HashMap::new();
            let sommets = &self.sommets;
            let mut prendre = |i: usize, r: &mut Maillage| {
                *nouveau.entry(i).or_insert_with(|| {
                    r.sommets.push(sommets[i]);
                    r.sommets.len() - 1
                })
            };
            if n == 0 {
                for i in 0..sommets.len() {
                    if !utilises.contains(&i) {
                        prendre(i, &mut r);
                    }
                }
            }
            for f in faces {
                let s = f.s.iter().map(|&i| prendre(i, &mut r)).collect();
                r.faces.push(Face { s, m: m.clone(), uv: f.uv.clone() });
            }
            if n == 0 {
                for a in &self.fils {
                    let x = [prendre(a[0], &mut r), prendre(a[1], &mut r)];
                    r.fils.push(x);
                }
            }
            sortie.push((m, r));
        }
        sortie
    }

    /// Un seul maillage fait de plusieurs, indices décalés.
    pub fn assembler<'a>(liste: impl IntoIterator<Item = &'a Maillage>) -> Maillage {
        let mut r = Maillage::default();
        for m in liste {
            let base = r.sommets.len();
            r.sommets.extend_from_slice(&m.sommets);
            for f in &m.faces {
                r.faces.push(Face { s: f.s.iter().map(|i| i + base).collect(), m: f.m.clone(), uv: f.uv.clone() });
            }
            for a in &m.fils {
                r.fils.push([a[0] + base, a[1] + base]);
            }
            for a in &m.coutures {
                r.coutures.push([a[0] + base, a[1] + base]);
            }
        }
        r
    }

    // ----- Lecture -----------------------------------------------------------

    /// D'après l'extension du nom de fichier.
    pub fn depuis_fichier(nom: &str, contenu: &[u8]) -> Result<Maillage, String> {
        let ext = extension(nom);
        match ext.as_str() {
            "obj" => Ok(Maillage::depuis_obj(&String::from_utf8_lossy(contenu), "defaut")),
            "stl" => Ok(Maillage::depuis_stl(contenu, "defaut", true)),
            "gltf" | "glb" => Maillage::depuis_gltf(contenu),
            e => Err(format!("format non pris en charge : {} (OBJ, STL, glTF, GLB)", if e.is_empty() { nom } else { e })),
        }
    }

    /// Lignes lues : v, f, l et usemtl.
    pub fn depuis_obj(texte: &str, materiau: &str) -> Maillage {
        let mut r = Maillage::default();
        let par_defaut: Mat = Arc::from(materiau);
        let mut m = par_defaut.clone();
        for brute in texte.lines() {
            let ligne = brute.trim();
            if ligne.is_empty() || ligne.starts_with('#') {
                continue;
            }
            let t: Vec<&str> = ligne.split_whitespace().collect();
            match t[0] {
                "v" => r.sommets.push(sommet_obj(&t)),
                "usemtl" => m = t.get(1).map_or(par_defaut.clone(), |n| Arc::from(*n)),
                "f" => {
                    let s = indices_obj(&t, r.sommets.len());
                    if s.len() >= 3 {
                        r.faces.push(Face { s: s.into_iter().map(|i| i as usize).collect(), m: m.clone(), uv: Vec::new() });
                    }
                }
                "l" => {
                    let l = fils_obj(&indices_obj(&t, r.sommets.len()));
                    r.fils.extend(l.into_iter().map(|[a, b]| [a as usize, b as usize]));
                }
                _ => {}
            }
        }
        let n = r.sommets.len();
        r.fils.retain(|a| a[0] < n && a[1] < n);
        r.nettoyer_fils();
        r
    }

    /// OBJ à plusieurs objets : un par nom de « o » (à défaut, de « g »), avec les
    /// sommets qu'utilisent ses faces et les sommets isolés de son bloc.
    pub fn objets_depuis_obj(texte: &str, materiau: &str) -> Vec<(String, Maillage)> {
        #[derive(Default)]
        struct Obj {
            nom: String,
            faces: Vec<(Vec<isize>, Mat, Vec<isize>)>,
            fils: Vec<[isize; 2]>,
            declares: Vec<usize>,
        }
        let lignes: Vec<&str> = texte.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        let a_des_o = lignes.iter().any(|l| *l == "o" || l.starts_with("o ") || l.starts_with("o\t"));
        let marque = if a_des_o { "o" } else { "g" };
        let mut sommets: Vec<P3> = Vec::new();
        let mut vts: Vec<[f32; 2]> = Vec::new();
        let mut objets: Vec<Obj> = Vec::new();
        let mut utilises = HashSet::new();
        let par_defaut: Mat = Arc::from(materiau);
        let mut m = par_defaut.clone();
        let mut courant: Option<usize> = None;
        fn objet(l: &mut Vec<Obj>, nom: &str) -> usize {
            if let Some(k) = l.iter().position(|o| o.nom == nom) {
                return k;
            }
            l.push(Obj { nom: nom.to_string(), ..Obj::default() });
            l.len() - 1
        }
        for ligne in lignes {
            let t: Vec<&str> = ligne.split_whitespace().collect();
            if t[0] == marque {
                let nom = t[1..].join(" ");
                courant = Some(objet(&mut objets, if nom.is_empty() { "objet" } else { &nom }));
            } else if t[0] == "v" {
                sommets.push(sommet_obj(&t));
                if let Some(c) = courant {
                    objets[c].declares.push(sommets.len() - 1);
                }
            } else if t[0] == "vt" {
                // L'OBJ compte v depuis le bas de l'image
                let c = |k: usize| t.get(k).and_then(|x| x.parse::<f32>().ok()).unwrap_or(0.0);
                vts.push([c(1), 1.0 - c(2)]);
            } else if t[0] == "usemtl" {
                m = t.get(1).map_or(par_defaut.clone(), |n| Arc::from(*n));
            } else if t[0] == "f" {
                let s = indices_obj(&t, sommets.len());
                if s.len() < 3 {
                    continue;
                }
                let c = *courant.get_or_insert_with(|| objet(&mut objets, "objet"));
                utilises.extend(s.iter().copied());
                let uv = uvs_obj(&t, vts.len());
                objets[c].faces.push((s, m.clone(), uv));
            } else if t[0] == "l" {
                let l = fils_obj(&indices_obj(&t, sommets.len()));
                let c = *courant.get_or_insert_with(|| objet(&mut objets, "objet"));
                utilises.extend(l.iter().flatten().copied());
                objets[c].fils.extend(l);
            }
        }
        let mut liste = Vec::new();
        let n = sommets.len() as isize;
        let existe = |i: isize| i >= 0 && i < n;
        for o in &objets {
            let mut mm = Maillage::default();
            let mut local: HashMap<usize, usize> = HashMap::new();
            let mut prendre = |i: usize, mm: &mut Maillage| {
                *local.entry(i).or_insert_with(|| {
                    mm.sommets.push(sommets[i]);
                    mm.sommets.len() - 1
                })
            };
            for (s, m, uv) in &o.faces {
                let retenus: Vec<usize> = (0..s.len()).filter(|&k| existe(s[k])).collect();
                let complet = uv.len() == s.len() && uv.iter().all(|&i| i >= 0 && (i as usize) < vts.len());
                let uv = if complet { retenus.iter().map(|&k| vts[uv[k] as usize]).collect() } else { Vec::new() };
                let s = retenus.iter().map(|&k| prendre(s[k] as usize, &mut mm)).collect();
                mm.faces.push(Face { s, m: m.clone(), uv });
            }
            for a in &o.fils {
                if existe(a[0]) && existe(a[1]) {
                    let x = [prendre(a[0] as usize, &mut mm), prendre(a[1] as usize, &mut mm)];
                    mm.fils.push(x);
                }
            }
            for &i in &o.declares {
                if !utilises.contains(&(i as isize)) {
                    prendre(i, &mut mm);
                }
            }
            mm.faces.retain(|f| f.s.len() >= 3);
            mm.nettoyer_fils();
            if !mm.sommets.is_empty() {
                liste.push((o.nom.clone(), mm));
            }
        }
        if objets.is_empty() && !sommets.is_empty() {
            liste.push(("objet".to_string(), Maillage::new(sommets, vec![], vec![])));
        }
        liste
    }

    /// STL binaire ou texte, sommets soudés. `z_en_haut` : (x, y, z) du fichier
    /// devient (x, z, −y).
    pub fn depuis_stl(d: &[u8], materiau: &str, z_en_haut: bool) -> Maillage {
        let mut t: Vec<f64> = Vec::new();
        let n = if d.len() >= 84 { u32_le(d, 80) as usize } else { usize::MAX };
        if n != usize::MAX && 84 + n * 50 == d.len() {
            for k in 0..n {
                let o = 84 + k * 50 + 12;
                for c in 0..9 {
                    t.push(f32_le(d, o + c * 4) as f64);
                }
            }
        } else {
            let texte = String::from_utf8_lossy(d);
            let mut mots = texte.split_whitespace();
            while let Some(m) = mots.next() {
                if m == "vertex" {
                    for _ in 0..3 {
                        t.push(mots.next().and_then(|x| x.parse().ok()).unwrap_or(f64::NAN));
                    }
                }
            }
        }
        let mut r = Maillage::default();
        let m: Mat = Arc::from(materiau);
        for c in t.chunks_exact(3) {
            r.sommets.push(if z_en_haut { [c[0], c[2], -c[1]] } else { [c[0], c[1], c[2]] });
        }
        for k in (0..r.sommets.len().saturating_sub(2)).step_by(3) {
            r.faces.push(Face { s: vec![k, k + 1, k + 2], m: m.clone(), uv: Vec::new() });
        }
        r.souder(1e-4);
        r
    }

    /// glTF 2.0 : .glb, ou .gltf à tampons intégrés ; toutes les primitives
    /// triangulaires de la scène en un seul maillage.
    pub fn depuis_gltf(d: &[u8]) -> Result<Maillage, String> {
        let g = Gltf::lire(d)?;
        let mut m = Maillage::default();
        for n in &g.noeuds {
            if n.mesh.is_some() {
                g.ajouter(n, &mut m)?;
            }
        }
        m.souder(1e-5);
        Ok(m)
    }

    /// Matériaux d'un glTF, sous les noms que portent ses faces.
    pub fn materiaux_gltf(d: &[u8]) -> Result<Vec<MatGltf>, String> {
        let g = Gltf::lire(d)?;
        let mut r = Vec::new();
        for (k, m) in g.json.get("materials").and_then(J::liste).unwrap_or(&[]).iter().enumerate() {
            let pbr = m.get("pbrMetallicRoughness");
            let base = pbr.and_then(|p| p.get("baseColorFactor")).and_then(J::nombres::<4>).unwrap_or([1.0; 4]);
            let image = pbr
                .and_then(|p| p.get("baseColorTexture"))
                .and_then(|t| t.get("index"))
                .and_then(J::num)
                .and_then(|t| g.json.get("textures")?.liste()?.get(t as usize)?.get("source")?.num())
                .and_then(|s| g.image(s as usize));
            let attendue = pbr.is_some_and(|p| p.get("baseColorTexture").is_some());
            let nom = m.get("name").and_then(J::txt).map_or_else(|| format!("materiau{k}"), String::from);
            r.push(MatGltf { manquante: attendue && image.is_none(), nom, couleur: base, image });
        }
        Ok(r)
    }

    /// Un maillage par nœud : un par objet de Blender.
    pub fn objets_depuis_gltf(d: &[u8]) -> Result<Vec<(String, Maillage)>, String> {
        let g = Gltf::lire(d)?;
        let mut r = Vec::new();
        for n in &g.noeuds {
            let Some(k) = n.mesh else { continue };
            let mut m = Maillage::default();
            g.ajouter(n, &mut m)?;
            m.souder(1e-5);
            let nom = n.nom.clone().or_else(|| g.json.get("meshes")?.liste()?.get(k)?.get("name")?.txt().map(String::from));
            r.push((nom.filter(|s| !s.is_empty()).unwrap_or_else(|| "maillage".into()), m));
        }
        Ok(r)
    }

    // ----- Écriture ----------------------------------------------------------

    pub fn vers_obj(&self, nom: &str, mtllib: Option<&str>) -> String {
        self.vers_obj_signe(nom, mtllib, SIGNATURE)
    }

    pub fn vers_obj_signe(&self, nom: &str, mtllib: Option<&str>, signature: &str) -> String {
        let mut l = vec![format!("# {nom} — exporté par {signature}")];
        if let Some(b) = mtllib {
            l.push(format!("mtllib {b}"));
        }
        l.push(format!("o {}", nom.split_whitespace().collect::<Vec<_>>().join("_")));
        for p in &self.sommets {
            l.push(format!("v {} {} {}", fixe(p[0], 6), fixe(p[1], 6), fixe(p[2], 6)));
        }
        // Chaque coordonnée une fois ; l'OBJ compte v depuis le bas de l'image
        let mut vts: HashMap<[u32; 2], usize> = HashMap::new();
        let mut m: Option<&Mat> = None;
        let mut lignes_f = Vec::with_capacity(self.faces.len());
        for f in &self.faces {
            if m != Some(&f.m) {
                m = Some(&f.m);
                lignes_f.push(format!("usemtl {}", f.m));
            }
            let avec_uv = f.uv.len() == f.s.len();
            let s: Vec<String> = f
                .s
                .iter()
                .enumerate()
                .map(|(k, i)| {
                    if !avec_uv {
                        return (i + 1).to_string();
                    }
                    let c = f.uv[k];
                    let n = vts.len() + 1;
                    let vt = *vts.entry(c.map(f32::to_bits)).or_insert_with(|| {
                        l.push(format!("vt {} {}", fixe(c[0] as f64, 6), fixe(1.0 - c[1] as f64, 6)));
                        n
                    });
                    format!("{}/{vt}", i + 1)
                })
                .collect();
            lignes_f.push(format!("f {}", s.join(" ")));
        }
        l.append(&mut lignes_f);
        for a in &self.fils {
            l.push(format!("l {} {}", a[0] + 1, a[1] + 1));
        }
        l.join("\n") + "\n"
    }

    /// OBJ de plusieurs objets : un « o » chacun, numérotation commune.
    pub fn objets_vers_obj(liste: &[(String, Maillage)], titre: &str, mtllib: Option<&str>) -> String {
        Maillage::objets_vers_obj_signe(liste, titre, mtllib, SIGNATURE)
    }

    pub fn objets_vers_obj_signe(liste: &[(String, Maillage)], titre: &str, mtllib: Option<&str>, signature: &str) -> String {
        let mut l = vec![format!("# {titre} — exporté par {signature}")];
        if let Some(b) = mtllib {
            l.push(format!("mtllib {b}"));
        }
        let (mut base, mut base_vt) = (0, 0);
        for (nom, m) in liste {
            let mut nb_vt = 0;
            for ligne in m.vers_obj_signe(nom, None, signature).split('\n').skip(1).filter(|x| !x.is_empty()) {
                if ligne.starts_with("f ") || ligne.starts_with("l ") {
                    let decale = |c: &str| match c.split_once('/') {
                        Some((v, t)) => format!("{}/{}", v.parse::<usize>().unwrap() + base, t.parse::<usize>().unwrap() + base_vt),
                        None => (c.parse::<usize>().unwrap() + base).to_string(),
                    };
                    let s: Vec<String> = ligne[2..].split(' ').map(decale).collect();
                    l.push(format!("{} {}", &ligne[..1], s.join(" ")));
                } else {
                    nb_vt += ligne.starts_with("vt ") as usize;
                    l.push(ligne.to_string());
                }
            }
            base += m.sommets.len();
            base_vt += nb_vt;
        }
        l.join("\n") + "\n"
    }

    /// STL binaire, Z vers le haut.
    pub fn vers_stl(&self, nom: &str) -> Vec<u8> {
        self.vers_stl_signe(nom, SIGNATURE)
    }

    pub fn vers_stl_signe(&self, nom: &str, signature: &str) -> Vec<u8> {
        let tri: Vec<usize> = self.triangles().into_iter().flat_map(|(_, l)| l).collect();
        let n = tri.len() / 3;
        let mut d = vec![0u8; 84 + n * 50];
        let tete = format!("{nom} — {signature}");
        let mut lg = tete.len().min(79);
        while !tete.is_char_boundary(lg) {
            lg -= 1;
        }
        d[..lg].copy_from_slice(&tete.as_bytes()[..lg]);
        d[80..84].copy_from_slice(&(n as u32).to_le_bytes());
        let z = |p: P3| [p[0], -p[2], p[1]];
        for t in 0..n {
            let (a, b, c) = (z(self.sommets[tri[t * 3]]), z(self.sommets[tri[t * 3 + 1]]), z(self.sommets[tri[t * 3 + 2]]));
            let nn = cross(sub(b, a), sub(c, a));
            let l = hypot3(nn[0], nn[1], nn[2]);
            let l = if l == 0.0 { 1.0 } else { l };
            let v = [nn[0] / l, nn[1] / l, nn[2] / l, a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]];
            for (k, x) in v.iter().enumerate() {
                d[84 + t * 50 + k * 4..][..4].copy_from_slice(&(*x as f32).to_le_bytes());
            }
        }
        d
    }

    /// Bibliothèque de matériaux (.mtl).
    pub fn vers_mtl(materiaux: &[(String, MatMtl)], titre: &str) -> String {
        Maillage::vers_mtl_signe(materiaux, titre, SIGNATURE)
    }

    pub fn vers_mtl_signe(materiaux: &[(String, MatMtl)], titre: &str, signature: &str) -> String {
        let mut l = vec![format!("# {titre} — exporté par {signature}")];
        for (nom, m) in materiaux {
            l.push(String::new());
            l.push(format!("newmtl {nom}"));
            for (mot, v) in [("Kd", &m.couleur), ("Ka", &m.ambiante), ("Ks", &m.speculaire), ("Ke", &m.emission)] {
                if let Some(v) = v {
                    let c = rvb(v);
                    l.push(format!("{mot} {} {} {}", fixe(c[0], 4), fixe(c[1], 4), fixe(c[2], 4)));
                }
            }
            for (mot, v) in [("Ns", m.brillance), ("d", m.opacite), ("Ni", m.refraction)] {
                if let Some(v) = v {
                    l.push(format!("{mot} {}", fixe(v, 4)));
                }
            }
            if let Some(v) = m.illumination {
                l.push(format!("illum {}", json::nombre(arrondi(v))));
            }
            if let Some(t) = &m.texture {
                l.push(format!("map_Kd {t}"));
            }
        }
        l.join("\n") + "\n"
    }

    /// Textures de couleur d'un .mtl : (matériau, nom du fichier, sans son chemin).
    pub fn textures_mtl(texte: &str) -> Vec<(String, String)> {
        let (mut r, mut m) = (Vec::new(), None);
        for l in texte.lines() {
            let t: Vec<&str> = l.split_whitespace().collect();
            match t.first() {
                Some(&"newmtl") => m = t.get(1).map(|x| x.to_string()),
                Some(x) if x.eq_ignore_ascii_case("map_Kd") => {
                    // Le nom du fichier vient après d'éventuelles options
                    if let (Some(n), Some(f)) = (&m, t.last().filter(|_| t.len() > 1)) {
                        r.push((n.clone(), f.rsplit(['/', '\\']).next().unwrap_or(f).to_string()));
                    }
                }
                _ => {}
            }
        }
        r
    }

    /// Lecture d'un .mtl : matériaux, et nombre de lignes de texture ignorées.
    /// « Tr » (transparence) devient l'opacité 1 − Tr si « d » manque.
    pub fn depuis_mtl(texte: &str) -> (Vec<(String, MatMtl)>, usize) {
        let mut mats: Vec<(String, MatMtl, Option<f64>)> = Vec::new();
        let mut textures = 0;
        let hex = |c: [f64; 3]| {
            let v = |x: f64| arrondi(x.clamp(0.0, 1.0) * 255.0) as u8;
            format!("#{:02x}{:02x}{:02x}", v(c[0]), v(c[1]), v(c[2]))
        };
        for brute in texte.lines() {
            let t: Vec<&str> = brute.split_whitespace().collect();
            let Some(&mot) = t.first() else { continue };
            if mot == "newmtl" {
                let n = brute.trim()[6..].trim();
                let n = if n.is_empty() { "materiau" } else { n }.to_string();
                mats.retain(|x| x.0 != n);
                mats.push((n, MatMtl::default(), None));
                continue;
            }
            let Some((_, m, tr)) = mats.last_mut() else { continue };
            if mot.starts_with('#') {
                continue;
            }
            let bas = mot.to_lowercase();
            if ["map_", "bump", "disp", "decal", "refl", "norm"].iter().any(|p| bas.starts_with(p)) {
                textures += 1;
                continue;
            }
            let nb = |i: usize| t.get(i).and_then(|x| nombre_debut(x)).filter(|v| v.is_finite());
            if mot == "Tr" && m.opacite.is_none() {
                if let Some(v) = nb(1) {
                    *tr = Some(1.0 - v);
                }
                continue;
            }
            let champ = match mot {
                "Kd" => Some(&mut m.couleur),
                "Ka" => Some(&mut m.ambiante),
                "Ks" => Some(&mut m.speculaire),
                "Ke" => Some(&mut m.emission),
                _ => None,
            };
            if let Some(champ) = champ {
                let c: Vec<Option<f64>> = t[1..t.len().min(4)].iter().map(|x| nombre_debut(x).filter(|v| v.is_finite())).collect();
                if !c.is_empty() && c.iter().all(Option::is_some) {
                    let a = c[0].unwrap();
                    *champ = Some(hex(if c.len() == 3 { [a, c[1].unwrap(), c[2].unwrap()] } else { [a, a, a] }));
                }
                continue;
            }
            match mot {
                "Ns" => m.brillance = nb(1).or(m.brillance),
                "d" => m.opacite = nb(1).or(m.opacite),
                "Ni" => m.refraction = nb(1).or(m.refraction),
                "illum" => m.illumination = nb(1).map(arrondi).or(m.illumination),
                _ => {}
            }
        }
        (mats.into_iter().map(|(n, mut m, tr)| {
            m.opacite = m.opacite.or(tr);
            (n, m)
        }).collect(), textures)
    }

    /// glTF binaire (.glb) : un nœud, une primitive par matériau, sans normales.
    /// Couleurs converties de sRGB vers le linéaire, comme le veut glTF.
    pub fn vers_glb(&self, nom: &str, materiaux: &[(String, MatMtl)]) -> Vec<u8> {
        self.vers_glb_signe(nom, materiaux, SIGNATURE)
    }

    pub fn vers_glb_signe(&self, nom: &str, materiaux: &[(String, MatMtl)], signature: &str) -> Vec<u8> {
        self.ecrire_glb(nom, materiaux, signature, &[], &[])
    }

    /// GLB aux textures intégrées : `images` donne (nom de fichier, octets), comme les
    /// désigne `MatMtl::texture`.
    pub fn vers_glb_images(&self, nom: &str, materiaux: &[(String, MatMtl)], images: &[(String, Arc<Vec<u8>>)]) -> Vec<u8> {
        if self.faces.iter().all(|f| f.uv.is_empty()) {
            return self.ecrire_glb(nom, materiaux, SIGNATURE, &[], &[]);
        }
        let (m, uv) = self.deplie();
        m.ecrire_glb(nom, materiaux, SIGNATURE, &uv, images)
    }

    /// Un sommet par couple (position, UV), comme le veut glTF : une couture y dédouble
    /// ses sommets. Rend aussi l'UV de chaque sommet.
    fn deplie(&self) -> (Maillage, Vec<[f32; 2]>) {
        let mut r = self.clone();
        let mut uv = vec![[0.0; 2]; r.sommets.len()];
        let mut pris = vec![false; r.sommets.len()];
        let mut copies: HashMap<(usize, [u32; 2]), usize> = HashMap::new();
        for f in &mut r.faces {
            if f.uv.len() != f.s.len() {
                continue;
            }
            for (i, c) in f.s.iter_mut().zip(&f.uv) {
                let cle = (*i, c.map(f32::to_bits));
                *i = *copies.entry(cle).or_insert_with(|| {
                    if !pris[*i] {
                        pris[*i] = true;
                        uv[*i] = *c;
                        return *i;
                    }
                    r.sommets.push(r.sommets[*i]);
                    uv.push(*c);
                    r.sommets.len() - 1
                });
            }
        }
        (r, uv)
    }

    /// `uv` : un par sommet, ou vide ; alors ni UV ni image.
    fn ecrire_glb(&self, nom: &str, materiaux: &[(String, MatMtl)], signature: &str, uv: &[[f32; 2]], images: &[(String, Arc<Vec<u8>>)]) -> Vec<u8> {
        let groupes: Vec<(Mat, Vec<usize>)> = self.triangles().into_iter().filter(|(_, l)| !l.is_empty()).collect();
        let nv = self.sommets.len();
        let lg_pos = nv * 12;
        let lg_idx: usize = groupes.iter().map(|(_, l)| l.len() * 4).sum();
        let mut bin = Vec::with_capacity(lg_pos + lg_idx);
        for p in &self.sommets {
            for c in p {
                bin.extend_from_slice(&(*c as f32).to_le_bytes());
            }
        }
        let b = self.boite_englobante().unwrap_or(Boite { min: [0.0; 3], max: [0.0; 3] });
        let nb = json::nombre;
        let simple = |v: P3| format!("[{},{},{}]", nb(v[0] as f32 as f64), nb(v[1] as f32 as f64), nb(v[2] as f32 as f64));
        let mut nom_j = String::new();
        json::texte(nom, &mut nom_j);
        let mut sign_j = String::new();
        json::texte(signature, &mut sign_j);
        let (mut prims, mut mats, mut vues, mut acces) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        vues.push(format!("{{\"buffer\":0,\"byteOffset\":0,\"byteLength\":{lg_pos},\"target\":34962}}"));
        acces.push(format!("{{\"bufferView\":0,\"componentType\":5126,\"count\":{nv},\"type\":\"VEC3\",\"min\":{},\"max\":{}}}", simple(b.min), simple(b.max)));
        let lin = |v: f64| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        let lineaire = |h: &str| {
            let c = rvb(h);
            [fixe_val(lin(c[0]), 5), fixe_val(lin(c[1]), 5), fixe_val(lin(c[2]), 5)]
        };
        let attr_uv = if uv.is_empty() { String::new() } else { format!(",\"TEXCOORD_0\":{}", groupes.len() + 1) };
        let mut o = 0;
        for (k, (m, l)) in groupes.iter().enumerate() {
            for &i in l {
                bin.extend_from_slice(&(i as u32).to_le_bytes());
            }
            vues.push(format!("{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{},\"target\":34963}}", lg_pos + o, l.len() * 4));
            acces.push(format!("{{\"bufferView\":{},\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}}", k + 1, l.len()));
            let d = materiaux.iter().find(|(n, _)| **n == **m).map(|(_, d)| d.clone()).unwrap_or_default();
            let opacite = d.opacite.filter(|v| v.is_finite()).map_or(1.0, |v| v.clamp(0.0, 1.0));
            let c = d.couleur.as_deref().map_or([0.8, 0.8, 0.8], lineaire);
            let mut nom_m = String::new();
            json::texte(m, &mut nom_m);
            // Coupure nette à mi-opacité, comme le rendu de l'application
            let image = d.texture.as_ref().and_then(|t| images.iter().position(|(n, _)| n == t)).filter(|_| !uv.is_empty());
            let texture_j = image.map_or(String::new(), |k| format!("\"baseColorTexture\":{{\"index\":{k}}},"));
            let mut t = format!(
                "{{\"name\":{nom_m},\"pbrMetallicRoughness\":{{{texture_j}\"baseColorFactor\":[{},{},{},{}],\"metallicFactor\":0,\"roughnessFactor\":0.9}}",
                nb(c[0]), nb(c[1]), nb(c[2]), nb(opacite)
            );
            if opacite < 1.0 {
                t.push_str(",\"alphaMode\":\"BLEND\"");
            } else if image.is_some() {
                t.push_str(",\"alphaMode\":\"MASK\"");
            }
            if let Some(e) = d.emission.as_deref().filter(|e| *e != "#000000") {
                let e = lineaire(e);
                t.push_str(&format!(",\"emissiveFactor\":[{},{},{}]", nb(e[0]), nb(e[1]), nb(e[2])));
            }
            t.push('}');
            mats.push(t);
            prims.push(format!("{{\"attributes\":{{\"POSITION\":0{attr_uv}}},\"indices\":{},\"material\":{k},\"mode\":4}}", k + 1));
            o += l.len() * 4;
        }
        // Après les indices : les vues des sommets et des triangles restent les mêmes
        let mut suite = String::new();
        if !uv.is_empty() {
            vues.push(format!("{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{},\"target\":34962}}", bin.len(), uv.len() * 8));
            acces.push(format!("{{\"bufferView\":{},\"componentType\":5126,\"count\":{},\"type\":\"VEC2\"}}", groupes.len() + 1, uv.len()));
            bin.extend(uv.iter().flatten().flat_map(|c| c.to_le_bytes()));
            let (mut l_images, mut l_textures) = (Vec::new(), Vec::new());
            for (k, (n, octets)) in images.iter().enumerate() {
                let debut = bin.len();
                bin.extend_from_slice(octets);
                let mut nom_i = String::new();
                json::texte(n, &mut nom_i);
                l_images.push(format!("{{\"name\":{nom_i},\"bufferView\":{},\"mimeType\":\"{}\"}}", vues.len(), crate::texture::mime(octets)));
                l_textures.push(format!("{{\"source\":{k}}}"));
                vues.push(format!("{{\"buffer\":0,\"byteOffset\":{debut},\"byteLength\":{}}}", octets.len()));
                bin.resize((bin.len() + 3) & !3, 0);
            }
            if !images.is_empty() {
                suite = format!(",\"images\":[{}],\"textures\":[{}]", l_images.join(","), l_textures.join(","));
            }
        }
        let js = format!(
            "{{\"asset\":{{\"version\":\"2.0\",\"generator\":{sign_j}}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"mesh\":0,\"name\":{nom_j}}}],\"meshes\":[{{\"name\":{nom_j},\"primitives\":[{}]}}],\"materials\":[{}],\"buffers\":[{{\"byteLength\":{}}}],\"bufferViews\":[{}],\"accessors\":[{}]{suite}}}",
            prims.join(","), mats.join(","), bin.len(), vues.join(","), acces.join(",")
        );
        let pad4 = |n: usize| (n + 3) & !3;
        let (lj, lb) = (pad4(js.len()), pad4(bin.len()));
        let total = 12 + 8 + lj + 8 + lb;
        let mut d = Vec::with_capacity(total);
        d.extend_from_slice(&0x46546C67u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&(total as u32).to_le_bytes());
        d.extend_from_slice(&(lj as u32).to_le_bytes());
        d.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
        d.extend_from_slice(js.as_bytes());
        d.resize(20 + lj, 0x20);
        d.extend_from_slice(&(lb as u32).to_le_bytes());
        d.extend_from_slice(&0x004E4942u32.to_le_bytes());
        d.extend_from_slice(&bin);
        d.resize(total, 0);
        d
    }
}

pub fn extension(nom: &str) -> String {
    let n = nom.split(['?', '#']).next().unwrap_or(nom);
    match n.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()) => e.to_lowercase(),
        _ => String::new(),
    }
}

/// `+x.toFixed(n)` de JavaScript, écrit comme un nombre.
fn fixe(x: f64, n: usize) -> String {
    json::nombre(fixe_val(x, n))
}

fn fixe_val(x: f64, n: usize) -> f64 {
    format!("{x:.n$}").parse().unwrap_or(x)
}

/// '#rrggbb' vers r, v, b entre 0 et 1 (gris clair si illisible).
pub fn rvb(hex: &str) -> [f64; 3] {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    match (h.len() == 6).then(|| u32::from_str_radix(h, 16).ok()).flatten() {
        Some(n) => [((n >> 16) & 255) as f64 / 255.0, ((n >> 8) & 255) as f64 / 255.0, (n & 255) as f64 / 255.0],
        None => [0.8, 0.8, 0.8],
    }
}

/// `parseFloat` : le plus long début qui soit un nombre.
fn nombre_debut(s: &str) -> Option<f64> {
    (1..=s.len()).rev().filter(|&k| s.is_char_boundary(k)).find_map(|k| s[..k].parse().ok())
}

fn sommet_obj(t: &[&str]) -> P3 {
    let c = |k: usize| t.get(k).and_then(|x| x.parse().ok()).unwrap_or(f64::NAN);
    [c(1), c(2), c(3)]
}

/// Indices d'une ligne « f » ou « l » (« 12/5/7 » : 11 ; négatifs : depuis la fin).
fn indices_obj(t: &[&str], n: usize) -> Vec<isize> {
    let mut s = Vec::new();
    for x in &t[1..] {
        let f = x.len() - x.trim_start_matches(['-', '+']).len();
        let lg = f + x[f..].bytes().take_while(u8::is_ascii_digit).count();
        if let Ok(v) = x[..lg].parse::<isize>() {
            s.push(if v < 0 { n as isize + v } else { v - 1 });
        }
    }
    s
}

/// Indices « vt » des coins d'une ligne f, numérotés depuis 0. Vide si un coin n'en a pas.
fn uvs_obj(t: &[&str], n: usize) -> Vec<isize> {
    let mut r = Vec::with_capacity(t.len() - 1);
    for x in &t[1..] {
        let Some(v) = x.split('/').nth(1).and_then(|v| v.parse::<isize>().ok()) else { return Vec::new() };
        r.push(if v < 0 { n as isize + v } else { v - 1 });
    }
    r
}

fn fils_obj(s: &[isize]) -> Vec<[isize; 2]> {
    s.windows(2).filter(|w| w[0] != w[1]).map(|w| [w[0], w[1]]).collect()
}

fn u32_le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn f32_le(d: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_le(d, o))
}

/// Dijkstra sur `n` nœuds, port exact de GDA (mêmes choix à égalité) : départ
/// des `sources`, arrêt au premier nœud de `cibles` atteint.
fn dijkstra(n: usize, voisins: impl Fn(usize) -> Vec<(usize, f64)>, sources: &[usize], cibles: &HashSet<usize>) -> Option<Vec<usize>> {
    let mut dist = vec![f64::INFINITY; n];
    let mut prec = vec![usize::MAX; n];
    let mut tas: Vec<(f64, usize)> = Vec::new();
    fn monter(t: &mut [(f64, usize)], mut k: usize) {
        while k > 0 {
            let p = (k - 1) >> 1;
            if t[p].0 <= t[k].0 {
                break;
            }
            t.swap(p, k);
            k = p;
        }
    }
    fn descendre(t: &mut [(f64, usize)], mut k: usize) {
        loop {
            let (g, d) = (2 * k + 1, 2 * k + 2);
            let mut m = k;
            if g < t.len() && t[g].0 < t[m].0 {
                m = g;
            }
            if d < t.len() && t[d].0 < t[m].0 {
                m = d;
            }
            if m == k {
                return;
            }
            t.swap(m, k);
            k = m;
        }
    }
    for &s in sources {
        dist[s] = 0.0;
        tas.push((0.0, s));
        let k = tas.len() - 1;
        monter(&mut tas, k);
    }
    while !tas.is_empty() {
        let (du, u) = tas[0];
        let fin = tas.pop().unwrap();
        if !tas.is_empty() {
            tas[0] = fin;
            descendre(&mut tas, 0);
        }
        if du > dist[u] {
            continue;
        }
        if cibles.contains(&u) {
            let mut l = vec![u];
            let mut k = u;
            while prec[k] != usize::MAX {
                k = prec[k];
                l.push(k);
            }
            l.reverse();
            return Some(l);
        }
        for (v, w) in voisins(u) {
            if du + w < dist[v] {
                dist[v] = du + w;
                prec[v] = u;
                tas.push((dist[v], v));
                let k = tas.len() - 1;
                monter(&mut tas, k);
            }
        }
    }
    None
}

// ----- glTF ---------------------------------------------------------------------

type M4 = [f64; 16];

fn m4_mul(a: &M4, b: &M4) -> M4 {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            m[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    m
}

const M4_ID: M4 = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

fn m4_appliquer(m: &M4, p: [f64; 3]) -> P3 {
    let w = 1.0 / (m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15]);
    [
        (m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12]) * w,
        (m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13]) * w,
        (m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14]) * w,
    ]
}

/// Matrice translation × rotation (quaternion x, y, z, w) × échelle.
pub fn composer(t: P3, q: [f64; 4], e: P3) -> [f64; 16] {
    let [x, y, z, w] = q;
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    [
        (1.0 - (yy + zz)) * e[0], (xy + wz) * e[0], (xz - wy) * e[0], 0.0,
        (xy - wz) * e[1], (1.0 - (xx + zz)) * e[1], (yz + wx) * e[1], 0.0,
        (xz + wy) * e[2], (yz - wx) * e[2], (1.0 - (xx + yy)) * e[2], 0.0,
        t[0], t[1], t[2], 1.0,
    ]
}

struct Noeud {
    mesh: Option<usize>,
    nom: Option<String>,
    skin: bool,
    poids: Option<Vec<f64>>,
    monde: M4,
}

struct Gltf {
    json: J,
    tampons: Vec<Vec<u8>>,
    noeuds: Vec<Noeud>,
}

pub fn base64(s: &str) -> Vec<u8> {
    let mut o = Vec::with_capacity(s.len() * 3 / 4);
    let (mut v, mut n) = (0u32, 0);
    for c in s.bytes() {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        v = (v << 6) | d as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            o.push((v >> n) as u8);
        }
    }
    o
}

impl Gltf {
    fn lire(d: &[u8]) -> Result<Gltf, String> {
        let mut json_txt: Option<&[u8]> = None;
        let mut bin: Option<Vec<u8>> = None;
        if d.len() >= 12 && u32_le(d, 0) == 0x46546C67 {
            let mut o = 12;
            while o + 8 <= d.len() {
                let (lg, t) = (u32_le(d, o) as usize, u32_le(d, o + 4));
                let fin = (o + 8 + lg).min(d.len());
                if t == 0x4E4F534A {
                    json_txt = Some(&d[o + 8..fin]);
                } else if t == 0x004E4942 {
                    bin = Some(d[o + 8..fin].to_vec());
                }
                o += 8 + lg;
            }
        } else {
            json_txt = Some(d);
        }
        let json = json_txt.and_then(|t| json::lire(&String::from_utf8_lossy(t))).ok_or("glTF illisible")?;
        let mut tampons = Vec::new();
        if let Some(l) = json.get("buffers").and_then(J::liste) {
            for (i, b) in l.iter().enumerate() {
                let uri = b.get("uri").and_then(J::txt);
                if let Some(u) = uri.filter(|u| u.starts_with("data:")) {
                    tampons.push(base64(&u[u.find(',').map_or(0, |k| k + 1)..]));
                } else if let (0, Some(x)) = (i, bin.take()) {
                    tampons.push(x);
                } else {
                    return Err(format!("glTF : tampon externe « {} » non pris en charge (utiliser un .glb)", uri.unwrap_or("?")));
                }
            }
        }
        let mut g = Gltf { json, tampons, noeuds: Vec::new() };
        let nb_mesh = g.json.get("meshes").and_then(J::liste).map_or(0, <[J]>::len);
        let scene = g.json.get("scene").and_then(J::num).unwrap_or(0.0) as usize;
        let racines: Option<Vec<usize>> = g
            .json
            .get("scenes")
            .and_then(J::liste)
            .and_then(|l| l.get(scene))
            .and_then(|s| s.get("nodes"))
            .and_then(J::liste)
            .map(|l| l.iter().filter_map(|x| x.num().map(|v| v as usize)).collect());
        let nb_noeuds = g.json.get("nodes").and_then(J::liste).map(<[J]>::len);
        match nb_noeuds {
            None => {
                for k in 0..nb_mesh {
                    g.noeuds.push(Noeud { mesh: Some(k), nom: None, skin: false, poids: None, monde: M4_ID });
                }
            }
            Some(n) => {
                for r in racines.unwrap_or_else(|| (0..n).collect()) {
                    g.noeud(r, &M4_ID, 0)?;
                }
            }
        }
        Ok(g)
    }

    fn noeud(&mut self, i: usize, parent: &M4, prof: usize) -> Result<(), String> {
        if prof > 256 {
            return Err("glTF : hiérarchie trop profonde".into());
        }
        let nd = self.json.get("nodes").and_then(J::liste).and_then(|l| l.get(i)).ok_or("glTF : nœud absent")?.clone();
        let local: M4 = if let Some(m) = nd.get("matrix").and_then(J::nombres::<16>) {
            m
        } else {
            let t = nd.get("translation").and_then(J::nombres::<3>).unwrap_or([0.0; 3]);
            let q = nd.get("rotation").and_then(J::nombres::<4>).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let e = nd.get("scale").and_then(J::nombres::<3>).unwrap_or([1.0; 3]);
            composer(t, q, e)
        };
        let monde = m4_mul(parent, &local);
        self.noeuds.push(Noeud {
            mesh: nd.get("mesh").and_then(J::num).map(|v| v as usize),
            nom: nd.get("name").and_then(J::txt).map(String::from),
            skin: nd.get("skin").is_some(),
            poids: nd.get("weights").and_then(J::liste).map(|l| l.iter().filter_map(J::num).collect()),
            monde,
        });
        if let Some(l) = nd.get("children").and_then(J::liste) {
            for e in l {
                self.noeud(e.num().unwrap_or(0.0) as usize, &monde, prof + 1)?;
            }
        }
        Ok(())
    }

    fn tranche(&self, vue: usize, decalage: usize, type_c: u32, nc: usize, n: usize, r: &mut Vec<f64>) -> Result<(), String> {
        let bv = self.json.get("bufferViews").and_then(J::liste).and_then(|l| l.get(vue)).ok_or("glTF : vue absente")?;
        let ent = |k: &str| bv.get(k).and_then(J::num).unwrap_or(0.0) as usize;
        let t = self.tampons.get(ent("buffer")).ok_or("glTF : tampon absent")?;
        let base = ent("byteOffset") + decalage;
        let lg = match type_c {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return Err("glTF : type de composante inconnu".into()),
        };
        let pas = if ent("byteStride") > 0 { ent("byteStride") } else { nc * lg };
        for i in 0..n {
            for c in 0..nc {
                let o = base + i * pas + c * lg;
                let x = t.get(o..o + lg).ok_or("glTF : données tronquées")?;
                r.push(match type_c {
                    5120 => x[0] as i8 as f64,
                    5121 => x[0] as f64,
                    5122 => i16::from_le_bytes([x[0], x[1]]) as f64,
                    5123 => u16::from_le_bytes([x[0], x[1]]) as f64,
                    5125 => u32_le(x, 0) as f64,
                    _ => f32_le(x, 0) as f64,
                });
            }
        }
        Ok(())
    }

    /// Valeurs d'un accesseur, creux compris (ainsi Blender écrit ses clés de forme).
    fn valeurs(&self, n: usize) -> Result<Vec<f64>, String> {
        let a = self.json.get("accessors").and_then(J::liste).and_then(|l| l.get(n)).ok_or("glTF : accesseur absent")?;
        let nc = match a.get("type").and_then(J::txt) {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            _ => return Err("glTF : type d'accesseur inconnu".into()),
        };
        let ent = |j: &J, k: &str| j.get(k).and_then(J::num);
        let count = ent(a, "count").unwrap_or(0.0) as usize;
        let tc = ent(a, "componentType").unwrap_or(0.0) as u32;
        let mut r = Vec::with_capacity(count * nc);
        match ent(a, "bufferView") {
            Some(v) => self.tranche(v as usize, ent(a, "byteOffset").unwrap_or(0.0) as usize, tc, nc, count, &mut r)?,
            None => r.resize(count * nc, 0.0),
        }
        if let Some(s) = a.get("sparse") {
            let c = ent(s, "count").unwrap_or(0.0) as usize;
            let (ix, vx) = (s.get("indices").ok_or("glTF creux")?, s.get("values").ok_or("glTF creux")?);
            let (mut quels, mut quoi) = (Vec::new(), Vec::new());
            let o = |j: &J| ent(j, "byteOffset").unwrap_or(0.0) as usize;
            self.tranche(ent(ix, "bufferView").unwrap_or(0.0) as usize, o(ix), ent(ix, "componentType").unwrap_or(0.0) as u32, 1, c, &mut quels)?;
            self.tranche(ent(vx, "bufferView").unwrap_or(0.0) as usize, o(vx), tc, nc, c, &mut quoi)?;
            for (k, q) in quels.iter().enumerate() {
                for d in 0..nc {
                    if let Some(x) = r.get_mut(*q as usize * nc + d) {
                        *x = quoi[k * nc + d];
                    }
                }
            }
        }
        Ok(r)
    }

    /// UV d'un accesseur : les entiers y sont normalisés.
    fn uvs(&self, n: usize) -> Result<Vec<f64>, String> {
        let a = self.json.get("accessors").and_then(J::liste).and_then(|l| l.get(n));
        let div = match a.and_then(|a| a.get("componentType")).and_then(J::num).unwrap_or(0.0) as u32 {
            5121 => 255.0,
            5123 => 65535.0,
            _ => 1.0,
        };
        Ok(self.valeurs(n)?.into_iter().map(|v| v / div).collect())
    }

    /// Octets de l'image `n` ; rien si elle est dans un fichier à part.
    fn image(&self, n: usize) -> Option<(String, Vec<u8>)> {
        let i = self.json.get("images")?.liste()?.get(n)?;
        let nom = i.get("name").and_then(J::txt).unwrap_or("image").to_string();
        if let Some(u) = i.get("uri").and_then(J::txt) {
            return u.starts_with("data:").then(|| (nom, base64(&u[u.find(',').map_or(0, |k| k + 1)..])));
        }
        let v = self.json.get("bufferViews")?.liste()?.get(i.get("bufferView")?.num()? as usize)?;
        let ent = |k: &str| v.get(k).and_then(J::num).unwrap_or(0.0) as usize;
        let t = self.tampons.get(ent("buffer"))?;
        Some((nom, t.get(ent("byteOffset")..ent("byteOffset") + ent("byteLength"))?.to_vec()))
    }

    /// Primitives du nœud ajoutées à `m` ; clés de forme appliquées à leurs poids.
    /// Un maillage lié à une armature reste dans son repère.
    fn ajouter(&self, n: &Noeud, m: &mut Maillage) -> Result<(), String> {
        let meshes = self.json.get("meshes").and_then(J::liste).ok_or("glTF sans maillages")?;
        let mesh = meshes.get(n.mesh.unwrap_or(0)).ok_or("glTF : maillage absent")?;
        let repere = if n.skin { M4_ID } else { n.monde };
        let poids_mesh: Vec<f64> = mesh.get("weights").and_then(J::liste).map(|l| l.iter().filter_map(J::num).collect()).unwrap_or_default();
        let fortes = n.poids.as_ref().unwrap_or(&poids_mesh);
        for p in mesh.get("primitives").and_then(J::liste).unwrap_or(&[]) {
            let mode = p.get("mode").and_then(J::num).unwrap_or(4.0) as u32;
            let attrs = p.get("attributes");
            let Some(pos_a) = attrs.and_then(|a| a.get("POSITION")).and_then(J::num) else { continue };
            if mode < 4 {
                continue;
            }
            let mut pos = self.valeurs(pos_a as usize)?;
            let base = m.sommets.len();
            if let Some(ts) = p.get("targets").and_then(J::liste) {
                for (k, t) in ts.iter().enumerate() {
                    let f = fortes.get(k).copied().unwrap_or(0.0);
                    let Some(a) = t.get("POSITION").and_then(J::num).filter(|_| f != 0.0) else { continue };
                    let d = self.valeurs(a as usize)?;
                    for (x, y) in pos.iter_mut().zip(&d) {
                        *x += f * y;
                    }
                }
            }
            for c in pos.chunks_exact(3) {
                m.sommets.push(m4_appliquer(&repere, [c[0], c[1], c[2]]));
            }
            let idx: Vec<usize> = match p.get("indices").and_then(J::num) {
                Some(a) => self.valeurs(a as usize)?.into_iter().map(|v| v as usize).collect(),
                None => (0..pos.len() / 3).collect(),
            };
            if idx.iter().any(|i| i * 3 >= pos.len()) {
                return Err("glTF : indice hors du maillage".into());
            }
            let mt: Mat = match p.get("material").and_then(J::num) {
                Some(k) => {
                    let nom = self.json.get("materials").and_then(J::liste).and_then(|l| l.get(k as usize)).and_then(|x| x.get("name")).and_then(J::txt).filter(|s| !s.is_empty());
                    nom.map_or_else(|| Arc::from(format!("materiau{}", json::nombre(k))), Arc::from)
                }
                None => defaut(),
            };
            let uv = match attrs.and_then(|a| a.get("TEXCOORD_0")).and_then(J::num) {
                Some(a) => Some(self.uvs(a as usize)?),
                None => None,
            };
            let coin = |i: usize| uv.as_ref().and_then(|u| Some([*u.get(i * 2)? as f32, *u.get(i * 2 + 1)? as f32]));
            let mut tri = |a: usize, b: usize, c: usize| {
                let uv = [coin(a), coin(b), coin(c)].into_iter().collect::<Option<Vec<_>>>().unwrap_or_default();
                m.faces.push(Face { s: vec![base + a, base + b, base + c], m: mt.clone(), uv });
            };
            match mode {
                4 => idx.chunks_exact(3).for_each(|c| tri(c[0], c[1], c[2])),
                5 => {
                    for k in 0..idx.len().saturating_sub(2) {
                        if k % 2 == 1 {
                            tri(idx[k + 1], idx[k], idx[k + 2]);
                        } else {
                            tri(idx[k], idx[k + 1], idx[k + 2]);
                        }
                    }
                }
                _ => {
                    for k in 1..idx.len().saturating_sub(1) {
                        tri(idx[0], idx[k], idx[k + 1]);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests_uv {
    use super::*;

    const OBJ_UV: &str = "o carre\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\n";

    fn obj(t: &str) -> Maillage {
        Maillage::objets_depuis_obj(t, "defaut").remove(0).1
    }

    #[test]
    fn obj_les_uv_se_lisent_v_retourne() {
        assert_eq!(obj(OBJ_UV).faces[0].uv, vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    }

    #[test]
    fn obj_des_indices_vt_negatifs_ou_absents() {
        assert_eq!(obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt .5 .5\nf 1/-1 2/-1 3/-1\n").faces[0].uv, vec![[0.5, 0.5]; 3]);
        assert!(obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").faces[0].uv.is_empty());
        // Un indice hors liste : la face reste, sans UV
        assert!(obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1/1 2/1 3/9\n").faces[0].uv.is_empty());
    }

    #[test]
    fn les_coutures_suivent_les_sommets_renumerotes() {
        let mut m = Maillage::boite(1.0, 1.0, 1.0);
        let a = m.aretes();
        m.coutures = a.clone();
        let x = m.sommets.len() - 1;
        m.supprimer_sommets(&[0]);
        assert!(m.coutures.iter().flatten().all(|i| *i < x));
        let v = m.coutures_valides();
        assert!(!v.is_empty() && v.len() < a.len());
        let mut d = Maillage::depuis_donnees(&m.vers_donnees()).unwrap();
        assert_eq!(d.coutures_valides().len(), v.len());
        // Une arête disparue ne compte plus, et un fichier abîmé ne coûte que ses coutures
        d.faces.clear();
        assert!(d.coutures_valides().is_empty());
    }

    #[test]
    fn souder_garde_la_couture_sur_les_coins() {
        // Deux triangles, bord commun en double, UV différents de part et d'autre
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        m.faces.push(Face { s: vec![0, 1, 2], m: defaut(), uv: vec![[0.0, 0.0], [0.5, 0.0], [0.0, 0.5]] });
        m.faces.push(Face { s: vec![3, 5, 4], m: defaut(), uv: vec![[0.6, 0.0], [1.0, 1.0], [0.6, 0.5]] });
        m.souder(1e-5);
        assert_eq!(m.sommets.len(), 4);
        assert_eq!(m.faces[1].s[0], m.faces[0].s[1]);
        assert_eq!(m.faces[1].uv, vec![[0.6, 0.0], [1.0, 1.0], [0.6, 0.5]]);
    }

    #[test]
    fn une_face_qui_perd_un_coin_perd_l_uv_de_ce_coin() {
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        m.faces.push(Face { s: vec![0, 1, 2, 3], m: defaut(), uv: vec![[0.0, 0.0], [0.1, 0.0], [0.2, 0.0], [0.3, 0.0]] });
        m.souder(1e-5);
        assert_eq!(m.faces[0].s.len(), 3);
        assert_eq!(m.faces[0].uv, vec![[0.0, 0.0], [0.2, 0.0], [0.3, 0.0]]);
    }

    #[test]
    fn donnees_les_uv_font_l_aller_retour_et_ne_s_ecrivent_que_s_il_y_en_a() {
        let m = obj(OBJ_UV);
        let d = m.vers_donnees();
        assert!(d.get("uv").is_some());
        assert_eq!(Maillage::depuis_donnees(&d).unwrap().faces[0].uv, m.faces[0].uv);
        assert!(Maillage::cube(1.0).vers_donnees().get("uv").is_none());
    }

    #[test]
    fn subdiviser_ne_touche_qu_aux_uv_des_faces_coupees() {
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]];
        m.faces.push(Face { s: vec![0, 1, 2], m: defaut(), uv: vec![[0.1; 2]; 3] });
        m.faces.push(Face { s: vec![3, 4, 5], m: defaut(), uv: vec![[0.2; 2]; 3] });
        m.subdiviser(&[[0, 1]], 1);
        assert_eq!(m.faces[0].uv.len(), 4);
        assert_eq!(m.faces[1].uv, vec![[0.2; 2]; 3]);
    }

    #[test]
    fn obj_ecrit_ses_vt_et_les_relit() {
        let m = obj(OBJ_UV);
        let t = m.vers_obj("carre", None);
        assert!(t.contains("f 1/1 2/2 3/3 4/4"), "{t}");
        assert_eq!(obj(&t).faces[0].uv, m.faces[0].uv);
    }

    #[test]
    fn obj_a_plusieurs_objets_decale_aussi_les_vt() {
        let a = obj(OBJ_UV);
        let b = obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nvt .5 .5\nf 1/1 2/1 3/1\n");
        let c = Maillage::cube(1.0);
        let t = Maillage::objets_vers_obj(&[("a".into(), a), ("c".into(), c), ("b".into(), b.clone())], "x", None);
        assert!(t.contains("f 13/5 14/5 15/5"), "{t}");
        let l = Maillage::objets_depuis_obj(&t, "defaut");
        assert_eq!(l[2].1.faces[0].uv, b.faces[0].uv);
    }

    #[test]
    fn glb_une_couture_dedouble_ses_sommets_et_les_uv_reviennent() {
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        m.faces.push(Face { s: vec![0, 1, 2], m: defaut(), uv: vec![[0.0, 0.0], [0.5, 0.0], [0.0, 0.5]] });
        m.faces.push(Face { s: vec![1, 3, 2], m: defaut(), uv: vec![[0.6, 0.0], [1.0, 1.0], [0.0, 0.5]] });
        let (d, uv) = m.deplie();
        assert_eq!(d.sommets.len(), 5, "le sommet 1 change d'UV, pas le sommet 2");
        assert_eq!(uv.len(), 5);
        let r = Maillage::objets_depuis_gltf(&m.vers_glb_images("x", &[], &[])).unwrap().remove(0).1;
        assert_eq!(r.sommets.len(), 4, "soudé à la relecture");
        let cles = |m: &Maillage| {
            let mut l: Vec<_> = m.faces.iter().flat_map(|f| f.s.iter().zip(&f.uv).map(|(i, c)| (m.sommets[*i].map(|x| x as f32).map(f32::to_bits), c.map(f32::to_bits)))).collect();
            l.sort();
            l
        };
        assert_eq!(cles(&r), cles(&m));
    }

    #[test]
    fn par_materiau_garde_les_uv() {
        let mut m = Maillage::default();
        m.sommets = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let (a, b): (Mat, Mat) = ("a".into(), "b".into());
        m.faces.push(Face { s: vec![0, 1, 2], m: a, uv: vec![[0.5; 2]; 3] });
        m.faces.push(Face { s: vec![0, 2, 1], m: b, uv: vec![[0.25; 2]; 3] });
        assert_eq!(m.par_materiau()[1].1.faces[0].uv, vec![[0.25; 2]; 3]);
    }

    #[test]
    fn mtl_les_textures_se_lisent_sans_chemin_ni_options() {
        let t = "newmtl a\nKd 1 1 1\nmap_Kd -s 1 1 1 C:\\img\\a.png\nnewmtl b\nmap_kd ../tex/b.jpg\nnewmtl c\nKd 0 0 0\n";
        assert_eq!(Maillage::textures_mtl(t), vec![("a".to_string(), "a.png".to_string()), ("b".to_string(), "b.jpg".to_string())]);
    }
}
