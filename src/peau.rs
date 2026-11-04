//! Peau : les sommets d'un objet répartis entre un groupe d'os, comme dans `moteur/peau.js`
//! de GDA, limité aux os du groupe. Poids par diffusion de chaleur sur la surface (Baran et
//! Popović, « bone heat ») : chaque os chauffe les sommets qui lui reviennent, et sa chaleur
//! se répand le long de la peau, sans sauter d'une jambe à l'autre. Les poids se déduisent
//! du maillage et des os : ils ne sont pas enregistrés. Une fois peints à la main
//! ([`Main`]), ils le sont, avec la liaison, et remplacent ce calcul.

use crate::armature::Armature;
use crate::edition::{qappliquer, qinv, qmul, qnorm};
use crate::geometrie::vers_local;
use crate::json::J;
use crate::maillage::{Maillage, P3};
use crate::projet::Objet;
use crate::vue;
use std::sync::Arc;

/// Os par sommet.
pub const INFLUENCES: usize = 4;
/// Quantile des distances qui donne l'épaisseur de chair.
const QUANTILE: f64 = 0.1;
/// Poids en deçà duquel un os est ignoré.
const MINIMUM: f64 = 0.02;
/// Précision du calcul de chaleur.
const TOLERANCE: f64 = 1e-4;
const ITERATIONS: usize = 400;

struct Seg {
    id: String,
    t: P3,
    d: P3,
    l2: f64,
}

fn scal(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sous(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Os du groupe à plat, parents d'abord.
fn segments(a: &Armature, groupe: &[String]) -> Vec<Seg> {
    let s = a.segments();
    a.ordre().iter().filter(|b| groupe.contains(&b.id)).filter_map(|b| {
        let x = s.get(&b.id)?;
        let d = sous(x.queue, x.tete);
        Some(Seg { id: b.id.clone(), t: x.tete, d, l2: scal(d, d) })
    }).collect()
}

fn distance(p: P3, s: &Seg) -> f64 {
    let v = sous(p, s.t);
    let k = if s.l2 > 0.0 { (scal(v, s.d) / s.l2).clamp(0.0, 1.0) } else { 0.0 };
    let e = [v[0] - k * s.d[0], v[1] - k * s.d[1], v[2] - k * s.d[2]];
    scal(e, e).sqrt()
}

/// Épaisseur de chair autour de chaque os : petit quantile des distances des sommets qui
/// se projettent sur l'os même, les plus proches étant ceux de sa section.
fn epaisseurs(sommets: &[P3], segs: &[Seg], d: &[f32]) -> Vec<f64> {
    let b = segs.len();
    let r: Vec<f64> = segs.iter().enumerate().map(|(j, s)| {
        let mut l: Vec<f32> = Vec::new();
        for (i, p) in sommets.iter().enumerate() {
            let k = scal(sous(*p, s.t), s.d) / if s.l2 != 0.0 { s.l2 } else { 1.0 };
            if k > 0.0 && k < 1.0 {
                l.push(d[i * b + j]);
            }
        }
        if l.is_empty() {
            return 0.0;
        }
        l.sort_by(f32::total_cmp);
        l[(l.len() as f64 * QUANTILE).floor() as usize] as f64
    }).collect();
    let n = r.iter().filter(|x| **x > 0.0).count().max(1);
    let moyenne = r.iter().sum::<f64>() / n as f64;
    r.iter().map(|x| { let v = x.max(moyenne * 0.25); if v != 0.0 { v } else { 1.0 } }).collect()
}

/// Chaque sommet à un os : le plus proche, distance rapportée à la racine de l'épaisseur.
fn attribuer(sommets: &[P3], segs: &[Seg], d: &[f32]) -> Vec<usize> {
    let b = segs.len();
    let echelle: Vec<f64> = epaisseurs(sommets, segs, d).into_iter().map(f64::sqrt).collect();
    (0..sommets.len()).map(|i| {
        let (mut min, mut os) = (f64::INFINITY, 0);
        for (j, e) in echelle.iter().enumerate() {
            let r = d[i * b + j] as f64 / e;
            if r < min {
                (min, os) = (r, j);
            }
        }
        os
    }).collect()
}

/// Matrice creuse par lignes, sans la diagonale.
struct Creuse {
    debut: Vec<usize>,
    col: Vec<u32>,
    val: Vec<f64>,
}

/// Laplacien cotangent (cotangentes négatives écartées), et aire par sommet.
fn laplacien(sommets: &[P3], m: &Maillage) -> (Creuse, Vec<f64>) {
    let n = sommets.len();
    let mut aires = vec![0.0; n];
    let mut t: Vec<(u32, u32, f64)> = Vec::with_capacity(m.faces.len() * 12);
    let mut ajouter = |i: usize, j: usize, c: f64| {
        t.push((i as u32, j as u32, c));
        t.push((j as u32, i as u32, c));
    };
    for f in &m.faces {
        for k in 1..f.s.len().saturating_sub(1) {
            let tr = [f.s[0], f.s[k], f.s[k + 1]];
            let [a, b, c] = tr.map(|i| sommets[i]);
            let (u, v, w) = (sous(b, a), sous(c, a), sous(c, b));
            let x = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let dbl = scal(x, x).sqrt();
            if dbl < 1e-14 {
                continue;
            }
            let cot = [scal(u, v) / dbl, -scal(u, w) / dbl, scal(v, w) / dbl];
            ajouter(tr[1], tr[2], cot[0].max(0.0) / 2.0);
            ajouter(tr[0], tr[2], cot[1].max(0.0) / 2.0);
            ajouter(tr[0], tr[1], cot[2].max(0.0) / 2.0);
            for i in tr {
                aires[i] += dbl / 6.0;
            }
        }
    }
    t.sort_unstable_by_key(|x| (x.0, x.1));
    let (mut debut, mut col, mut val) = (Vec::with_capacity(n + 1), Vec::with_capacity(t.len()), Vec::with_capacity(t.len()));
    let mut k = 0;
    debut.push(0);
    for i in 0..n as u32 {
        while k < t.len() && t[k].0 == i {
            let j = t[k].1;
            let mut c = 0.0;
            while k < t.len() && t[k].0 == i && t[k].1 == j {
                c += t[k].2;
                k += 1;
            }
            col.push(j);
            val.push(c);
        }
        debut.push(col.len());
    }
    (Creuse { debut, col, val }, aires)
}

/// Résout (D - L) x = b par gradient conjugué préconditionné par la diagonale ; `x` :
/// valeur de départ, remplacée.
fn resoudre(l: &Creuse, diag: &[f64], b: &[f64], x: &mut [f64]) {
    let n = diag.len();
    let produit = |u: &[f64], s: &mut [f64]| {
        for i in 0..n {
            let mut t = diag[i] * u[i];
            for k in l.debut[i]..l.debut[i + 1] {
                t -= l.val[k] * u[l.col[k] as usize];
            }
            s[i] = t;
        }
    };
    let (mut r, mut z, mut p, mut q) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    produit(x, &mut q);
    let (mut rz, mut cible) = (0.0, 0.0);
    for i in 0..n {
        r[i] = b[i] - q[i];
        z[i] = r[i] / diag[i];
        p[i] = z[i];
        rz += r[i] * z[i];
        cible += b[i] * b[i] / diag[i];
    }
    cible *= TOLERANCE * TOLERANCE;
    let mut t = 0;
    while t < ITERATIONS && rz > cible {
        produit(&p, &mut q);
        let pq: f64 = p.iter().zip(&q).map(|(a, b)| a * b).sum();
        let a = rz / pq;
        let mut suite = 0.0;
        for i in 0..n {
            x[i] += a * p[i];
            r[i] -= a * q[i];
            z[i] = r[i] / diag[i];
            suite += r[i] * z[i];
        }
        let e = suite / rz;
        rz = suite;
        for i in 0..n {
            p[i] = z[i] + e * p[i];
        }
        t += 1;
    }
}

/// Poids de chaque sommet : `INFLUENCES` cases par sommet, les plus forts d'abord, un
/// poids nul fermant la liste.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Peau {
    pub ids: Vec<String>,
    pub indices: Vec<u16>,
    pub poids: Vec<f32>,
}

/// Garde les quatre plus forts par sommet, ramenés à une somme de 1 ; le premier poids
/// écarté est retiré aux autres : un os entre ou sort sans saut d'un sommet au voisin.
fn compacter(table: &[Vec<f32>], n: usize, ids: Vec<String>) -> Peau {
    let m = INFLUENCES;
    let (mut indices, mut poids) = (vec![0u16; n * m], vec![0f32; n * m]);
    for i in 0..n {
        let mut l: Vec<(f64, usize)> = table.iter().enumerate().map(|(j, c)| (c[i] as f64, j)).filter(|x| x.0 > MINIMUM).collect();
        l.sort_by(|u, v| v.0.total_cmp(&u.0));
        let ecarte = l.get(m).map_or(0.0, |x| x.0);
        let garde: Vec<(f64, usize)> = l.iter().take(m).map(|(w, j)| (w - ecarte, *j)).filter(|x| x.0 > 0.0).collect();
        let s: f64 = garde.iter().map(|x| x.0).sum();
        let s = if s != 0.0 { s } else { 1.0 };
        for (k, (w, j)) in garde.iter().enumerate() {
            indices[i * m + k] = *j as u16;
            poids[i * m + k] = (*w / s) as f32;
        }
        if garde.is_empty() {
            poids[i * m] = 1.0;
        }
    }
    Peau { ids, indices, poids }
}

/// Poids de sommets `sommets`, au repère de l'armature, de faces celles de `m`, entre les
/// os de `groupe`.
pub fn calculer(sommets: &[P3], m: &Maillage, a: &Armature, groupe: &[String]) -> Peau {
    let segs = segments(a, groupe);
    let (n, b) = (sommets.len(), segs.len());
    let mut d = vec![0f32; n * b];
    for (i, p) in sommets.iter().enumerate() {
        for (j, s) in segs.iter().enumerate() {
            d[i * b + j] = distance(*p, s) as f32;
        }
    }
    let os = attribuer(sommets, &segs, &d);
    let (l, aires) = laplacien(sommets, m);
    let (mut diag, mut source) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n {
        let somme: f64 = l.val[l.debut[i]..l.debut[i + 1]].iter().sum();
        let e = (d[i * b + os[i]] as f64).max(1e-3);
        source[i] = (if aires[i] != 0.0 { aires[i] } else { 1e-9 }) / (e * e);
        diag[i] = somme + source[i];
    }
    // Un os à la fois, sur quelques cœurs
    let suivant = std::sync::atomic::AtomicUsize::new(0);
    let fils = std::thread::available_parallelism().map_or(1, |x| x.get()).clamp(1, 4).min(b.max(1));
    let (l, diag, source, os) = (&l, &diag, &source, &os);
    let mut table: Vec<(usize, Vec<f32>)> = std::thread::scope(|sc| {
        let suivant = &suivant;
        let t: Vec<_> = (0..fils).map(|_| sc.spawn(move || {
            let mut fait = Vec::new();
            loop {
                let j = suivant.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if j >= b {
                    return fait;
                }
                let mut x: Vec<f64> = os.iter().map(|o| if *o == j { 1.0 } else { 0.0 }).collect();
                let rhs: Vec<f64> = (0..n).map(|i| source[i] * x[i]).collect();
                resoudre(l, diag, &rhs, &mut x);
                fait.push((j, x.into_iter().map(|v| v as f32).collect::<Vec<f32>>()));
            }
        })).collect();
        t.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    table.sort_by_key(|x| x.0);
    let table: Vec<Vec<f32>> = table.into_iter().map(|x| x.1).collect();
    compacter(&table, n, segs.into_iter().map(|s| s.id).collect())
}

impl Peau {
    /// Points au repos, déplacés par la pose de `a` (repère de l'armature).
    pub fn deformer(&self, points: &[P3], a: &Armature) -> Vec<P3> {
        let (repos, pose) = (a.repos(), a.matrices_pose());
        let table: Vec<Option<(P3, P3, [f64; 4])>> = self.ids.iter().map(|id| {
            let ((pr, qr), (pp, qp)) = (*repos.get(id)?, *pose.get(id)?);
            Some((pr, pp, qnorm(qmul(qp, qinv(qr)))))
        }).collect();
        points.iter().enumerate().map(|(i, p)| {
            let mut r = [0.0; 3];
            for k in 0..INFLUENCES {
                let w = self.poids[i * INFLUENCES + k] as f64;
                if w == 0.0 {
                    break;
                }
                let q = match table[self.indices[i * INFLUENCES + k] as usize] {
                    Some((pr, pp, q)) => {
                        let v = qappliquer(q, sous(*p, pr));
                        [v[0] + pp[0], v[1] + pp[1], v[2] + pp[2]]
                    }
                    None => *p,
                };
                for c in 0..3 {
                    r[c] += w * q[c];
                }
            }
            r
        }).collect()
    }

    /// Poids de l'os `id` pour chaque sommet.
    pub fn poids_de(&self, id: &str) -> Vec<f32> {
        let j = self.ids.iter().position(|x| x == id);
        (0..self.poids.len() / INFLUENCES).map(|i| {
            (0..INFLUENCES).find(|k| Some(self.indices[i * INFLUENCES + k] as usize) == j && self.poids[i * INFLUENCES + k] > 0.0).map_or(0.0, |k| self.poids[i * INFLUENCES + k])
        }).collect()
    }
}

/// Peau d'un objet lié, et ce dont elle part.
#[derive(Debug)]
pub struct Liee {
    pub cle: String,
    pub peau: Peau,
    /// Sommets au repos, au repère de l'armature.
    pub repos: Vec<P3>,
    /// Gardé en vie : la clé cite son adresse, qui ne doit pas resservir.
    pub base: Arc<Maillage>,
}

/// Ce dont les poids dépendent : maillage, place de l'objet par rapport à l'armature, repos
/// des os, groupe. La pose n'y entre pas, ni un déplacement de l'armature et de ses objets.
pub fn cle(base: &Arc<Maillage>, o: &Objet, x: &Objet, groupe: &[String]) -> String {
    let m = o.matrice();
    let place: Vec<String> = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].iter().map(|p| {
        let v = vers_local(x, vue::appliquer(&m, *p));
        format!("{:.6},{:.6},{:.6}", v[0], v[1], v[2])
    }).collect();
    let os: Vec<_> = x.armature.iter().flat_map(|a| &a.os).map(|b| (&b.id, &b.parent, b.position, b.rotation, b.longueur)).collect();
    format!("{:x}|{place:?}|{os:?}|{groupe:?}", Arc::as_ptr(base) as usize)
}

/// Calcule la peau de `o`, lié à l'armature de `x`.
pub fn lier(base: Arc<Maillage>, o: &Objet, x: &Objet, groupe: &[String], cle: String) -> Liee {
    let m = o.matrice();
    let repos: Vec<P3> = base.sommets.iter().map(|p| vers_local(x, vue::appliquer(&m, *p))).collect();
    let peau = x.armature.as_ref().map_or_else(Peau::default, |a| calculer(&repos, &base, a, groupe));
    Liee { cle, peau, repos, base }
}

/// Le maillage de l'objet dans la pose de son armature.
pub fn deforme(l: &Liee, o: &Objet, x: &Objet, a: &Armature) -> Maillage {
    let mx = x.matrice();
    let mut r = (*l.base).clone();
    for (s, p) in r.sommets.iter_mut().zip(l.peau.deformer(&l.repos, a)) {
        *s = vers_local(o, vue::appliquer(&mx, p));
    }
    r
}

/// Une liste d'os vide ou une peau échouée ne déforme rien.
pub fn valide(l: &Liee, n: usize) -> bool {
    l.repos.len() == n && l.peau.poids.len() == n * INFLUENCES
}


/// Ce que fait le pinceau au poids de l'os actif.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pinceau {
    Ajouter,
    Retirer,
    /// Vers la moyenne des sommets voisins.
    Lisser,
}

/// Poids peints à la main : une colonne par os, tous les sommets du maillage de base.
/// Chaque sommet garde une somme de 1 ; la peau en retient les quatre plus forts.
#[derive(Clone, Debug, Default)]
pub struct Main {
    pub ids: Vec<String>,
    /// Sommets du maillage peint : un autre nombre rend les poids caducs.
    pub n: usize,
    /// `w[i * ids.len() + j]` : poids de l'os `j` au sommet `i`.
    pub w: Vec<f32>,
    /// Change à chaque coup de pinceau : la peau se refait.
    pub version: u64,
}

impl PartialEq for Main {
    fn eq(&self, o: &Main) -> bool {
        (&self.ids, self.n, &self.w) == (&o.ids, o.n, &o.w)
    }
}

fn version() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SUITE: AtomicU64 = AtomicU64::new(1);
    SUITE.fetch_add(1, Ordering::Relaxed)
}

impl Main {
    /// Le point de départ du pinceau : les poids calculés.
    pub fn depuis_peau(p: &Peau, n: usize) -> Main {
        let b = p.ids.len();
        let mut w = vec![0f32; n * b];
        for i in 0..n.min(p.poids.len() / INFLUENCES) {
            for k in 0..INFLUENCES {
                let x = p.poids[i * INFLUENCES + k];
                if x == 0.0 {
                    break;
                }
                w[i * b + p.indices[i * INFLUENCES + k] as usize] += x;
            }
        }
        Main { ids: p.ids.clone(), n, w, version: version() }
    }

    /// Tout le poids à un seul os : un objet sans peau calculée.
    pub fn uniforme(ids: Vec<String>, n: usize, os: usize) -> Main {
        let b = ids.len();
        let mut w = vec![0f32; n * b];
        for i in 0..n {
            w[i * b + os.min(b.saturating_sub(1))] = 1.0;
        }
        Main { ids, n, w, version: version() }
    }

    /// Les colonnes suivent le groupe : un os retiré sort, un os ajouté entre à zéro,
    /// et un sommet sans poids revient au premier os.
    pub fn suivre(&mut self, groupe: &[String]) {
        if self.ids == groupe {
            return;
        }
        let (b, nb) = (self.ids.len(), groupe.len());
        let mut w = vec![0f32; self.n * nb];
        for (j, id) in groupe.iter().enumerate() {
            if let Some(a) = self.ids.iter().position(|x| x == id) {
                for i in 0..self.n {
                    w[i * nb + j] = self.w[i * b + a];
                }
            }
        }
        self.ids = groupe.to_vec();
        self.w = w;
        for i in 0..self.n {
            self.normaliser(i, None, None);
        }
        self.version = version();
    }

    pub fn colonne(&self, id: &str) -> Option<usize> {
        self.ids.iter().position(|x| x == id)
    }

    /// Poids de l'os `id` pour chaque sommet.
    pub fn poids_de(&self, id: &str) -> Vec<f32> {
        let b = self.ids.len();
        match self.colonne(id) {
            Some(j) => (0..self.n).map(|i| self.w[i * b + j]).collect(),
            None => vec![0.0; self.n],
        }
    }

    /// La peau : les quatre plus forts par sommet.
    pub fn peau(&self) -> Peau {
        let b = self.ids.len();
        let table: Vec<Vec<f32>> = (0..b).map(|j| (0..self.n).map(|i| self.w[i * b + j]).collect()).collect();
        compacter(&table, self.n, self.ids.clone())
    }

    /// Ramène le sommet `i` à une somme de 1 sans toucher à la colonne `fixe` : les autres
    /// se partagent le reste en proportion, ou, toutes nulles, il va à `repli`.
    fn normaliser(&mut self, i: usize, fixe: Option<usize>, repli: Option<usize>) {
        let b = self.ids.len();
        if b == 0 {
            return;
        }
        let l = &mut self.w[i * b..(i + 1) * b];
        let garde = fixe.map_or(0.0, |j| l[j].clamp(0.0, 1.0));
        if let Some(j) = fixe {
            l[j] = garde;
        }
        let autres: f32 = l.iter().enumerate().filter(|(k, _)| Some(*k) != fixe).map(|x| x.1.max(0.0)).sum();
        let reste = 1.0 - garde;
        if autres > 1e-6 {
            for (k, x) in l.iter_mut().enumerate() {
                if Some(k) != fixe {
                    *x = x.max(0.0) * reste / autres;
                }
            }
        } else if let Some(r) = repli.filter(|r| Some(*r) != fixe).or_else(|| (0..b).find(|k| Some(*k) != fixe)) {
            l[r] = reste;
        } else if let Some(j) = fixe {
            // Un seul os : il garde tout
            l[j] = 1.0;
        }
    }

    /// Un coup de pinceau sur la colonne `j` : chaque sommet touché, avec son intensité
    /// (0 à 1, déjà multipliée par la force) ; `voisins` sert au lissage, `repli` reçoit
    /// ce qu'on retire à un sommet qui n'avait que cet os.
    pub fn peindre(&mut self, j: usize, touches: &[(usize, f32)], mode: Pinceau, voisins: &[Vec<usize>], repli: Option<usize>) {
        let b = self.ids.len();
        if j >= b {
            return;
        }
        let avant: Vec<f32> = if mode == Pinceau::Lisser { (0..self.n).map(|i| self.w[i * b + j]).collect() } else { Vec::new() };
        for &(i, f) in touches {
            if i >= self.n {
                continue;
            }
            let x = self.w[i * b + j];
            let cible = match mode {
                Pinceau::Ajouter => 1.0,
                Pinceau::Retirer => 0.0,
                Pinceau::Lisser => {
                    let v = voisins.get(i).map_or(&[][..], |v| &v[..]);
                    if v.is_empty() {
                        continue;
                    }
                    v.iter().map(|k| avant[*k]).sum::<f32>() / v.len() as f32
                }
            };
            self.w[i * b + j] = x + (cible - x) * f.clamp(0.0, 1.0);
            self.normaliser(i, Some(j), repli);
        }
        self.version = version();
    }

    pub fn lire(j: &J) -> Option<Main> {
        let ids: Vec<String> = j.get("os").and_then(J::liste)?.iter().filter_map(J::txt).map(str::to_string).collect();
        let n = j.get("sommets").and_then(J::num)? as usize;
        let b = ids.len();
        let mut w = vec![0f32; n * b];
        // Par os : sommet, poids, sommet, poids…
        for (c, l) in j.get("poids").and_then(J::liste)?.iter().enumerate().take(b) {
            let l = l.liste()?;
            for p in l.chunks(2) {
                if let [i, x] = p {
                    let i = i.num()? as usize;
                    if i < n {
                        w[i * b + c] = x.num()? as f32;
                    }
                }
            }
        }
        Some(Main { ids, n, w, version: version() })
    }

    pub fn vers_donnees(&self) -> J {
        let b = self.ids.len();
        let poids = (0..b).map(|j| {
            let mut l = Vec::new();
            for i in 0..self.n {
                let x = self.w[i * b + j];
                if x > 1e-4 {
                    l.push(J::Num(i as f64));
                    l.push(J::Num((x as f64 * 1e4).round() / 1e4));
                }
            }
            J::Liste(l)
        }).collect();
        J::Objet(vec![
            ("os".into(), J::Liste(self.ids.iter().map(|i| J::Txt(i.clone())).collect())),
            ("sommets".into(), J::Num(self.n as f64)),
            ("poids".into(), J::Liste(poids)),
        ])
    }
}

/// Sommets voisins par les arêtes des faces : pour lisser.
pub fn voisins(m: &Maillage) -> Vec<Vec<usize>> {
    let mut v = vec![Vec::new(); m.sommets.len()];
    for f in &m.faces {
        let n = f.s.len();
        for k in 0..n {
            let (a, b) = (f.s[k], f.s[(k + 1) % n]);
            if a < v.len() && b < v.len() {
                if !v[a].contains(&b) {
                    v[a].push(b);
                }
                if !v[b].contains(&a) {
                    v[b].push(a);
                }
            }
        }
    }
    v
}

/// La peau d'un objet aux poids peints : sans calcul, tout de suite.
pub fn lier_main(base: Arc<Maillage>, o: &Objet, x: &Objet, main: &Main, cle: String) -> Liee {
    let m = o.matrice();
    let repos: Vec<P3> = base.sommets.iter().map(|p| vers_local(x, vue::appliquer(&m, *p))).collect();
    Liee { cle, peau: main.peau(), repos, base }
}

#[cfg(test)]
mod tests_main {
    use super::*;

    fn main3() -> Main {
        // Trois sommets, deux os : tout à a, moitié-moitié, tout à b
        Main { ids: vec!["a".into(), "b".into()], n: 3, w: vec![1.0, 0.0, 0.5, 0.5, 0.0, 1.0], version: 0 }
    }

    #[test]
    fn peindre_garde_une_somme_de_un() {
        let mut m = main3();
        m.peindre(0, &[(1, 0.5), (2, 1.0)], Pinceau::Ajouter, &[], None);
        for i in 0..3 {
            assert!((m.w[i * 2] + m.w[i * 2 + 1] - 1.0).abs() < 1e-6);
        }
        assert!((m.w[2] - 0.75).abs() < 1e-6 && m.w[4] == 1.0);
        // Retirer tout à un sommet qui n'a que cet os : le reste va au repli
        m.peindre(0, &[(0, 1.0)], Pinceau::Retirer, &[], Some(1));
        assert_eq!((m.w[0], m.w[1]), (0.0, 1.0));
    }

    #[test]
    fn lisser_tire_vers_les_voisins() {
        let mut m = main3();
        let v = vec![vec![1], vec![0, 2], vec![1]];
        m.peindre(0, &[(0, 1.0)], Pinceau::Lisser, &v, None);
        assert!((m.w[0] - 0.5).abs() < 1e-6 && (m.w[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn les_poids_peints_reviennent_des_donnees_et_suivent_le_groupe() {
        let m = main3();
        let r = Main::lire(&m.vers_donnees()).unwrap();
        assert_eq!(r, m);
        let mut g = r.clone();
        g.suivre(&["b".into(), "c".into()]);
        assert_eq!(g.ids, ["b", "c"]);
        assert_eq!(g.w, [1.0, 0.0, 1.0, 0.0, 1.0, 0.0], "le sommet de a seul revient au premier os");
    }

    #[test]
    fn la_peau_des_poids_peints_les_rend() {
        let p = main3().peau();
        let m = Main::depuis_peau(&p, 3);
        assert_eq!(m, main3());
    }
}
