//! Mode Géométrie, sans interface : copie de travail au monde, sélection typée,
//! opérations. Portage de `moteur/selection.js` et des méthodes de `ctx-editeur.js`,
//! vérifié face à l'original par `tests/differentiel.rs`.
//!
//! GDA tient les faces choisies par référence ; ici ce sont des indices, que chaque
//! opération renumérote par une table ancien → nouveau.

use crate::edition::{qappliquer, qinv, qnorm, Transfo, Vue};
use crate::maillage::{Arete, Cible, Face, Maillage, Mat, Revolution, P3};
use crate::projet::{Objet, Projet};
use crate::vue;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Distance de capture d'un sommet sous le doigt, en dp.
pub const RAYON: f64 = 28.0;

/// Type d'élément : sommet, arête, face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum T {
    S,
    A,
    F,
}

impl T {
    pub fn rang(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum El {
    S(usize),
    A(Arete),
    F(usize),
}

impl El {
    pub fn t(self) -> T {
        match self {
            El::S(_) => T::S,
            El::A(_) => T::A,
            El::F(_) => T::F,
        }
    }
}

pub fn arete(i: usize, j: usize) -> Arete {
    if i < j { [i, j] } else { [j, i] }
}

/// Sélection d'un objet, en indices locaux.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Donnees {
    pub s: Vec<usize>,
    pub a: Vec<Arete>,
    pub f: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sel {
    pub sommets: Vec<usize>,
    pub aretes: Vec<Arete>,
    pub faces: Vec<usize>,
    pub dernier: Option<El>,
    /// Éléments mis de côté par un filtre éteint.
    pub cache: Option<Box<Sel>>,
}

impl Sel {
    pub fn vide(&self) -> bool {
        self.sommets.is_empty() && self.aretes.is_empty() && self.faces.is_empty()
    }

    fn elements(&self, t: T) -> Vec<El> {
        match t {
            T::S => self.sommets.iter().map(|i| El::S(*i)).collect(),
            T::A => self.aretes.iter().map(|a| El::A(*a)).collect(),
            T::F => self.faces.iter().map(|f| El::F(*f)).collect(),
        }
    }

    fn vider_liste(&mut self, t: T) {
        match t {
            T::S => self.sommets.clear(),
            T::A => self.aretes.clear(),
            T::F => self.faces.clear(),
        }
    }

    pub fn masquer(&mut self, t: T) {
        let c = self.cache.get_or_insert_with(Default::default);
        let l = match t {
            T::S => self.sommets.iter().map(|i| El::S(*i)).collect::<Vec<_>>(),
            T::A => self.aretes.iter().map(|a| El::A(*a)).collect(),
            T::F => self.faces.iter().map(|f| El::F(*f)).collect(),
        };
        c.ajouter_lot(&l);
        self.vider_liste(t);
        if self.dernier.is_some_and(|d| d.t() == t) {
            self.dernier = None;
        }
    }

    pub fn montrer(&mut self, t: T) {
        let Some(c) = &mut self.cache else { return };
        let l = c.elements(t);
        c.vider(Some(t));
        self.ajouter_lot(&l);
    }

    pub fn contient(&self, el: El) -> bool {
        match el {
            El::S(i) => self.sommets.contains(&i),
            El::F(f) => self.faces.contains(&f),
            El::A(a) => self.aretes.contains(&arete(a[0], a[1])),
        }
    }

    pub fn ajouter(&mut self, el: El) {
        if self.contient(el) {
            return;
        }
        match el {
            El::S(i) => self.sommets.push(i),
            El::A(a) => self.aretes.push(arete(a[0], a[1])),
            El::F(f) => self.faces.push(f),
        }
        self.dernier = Some(el);
    }

    /// Comme `ajouter` sur chaque élément, sans parcourir la liste à chaque fois.
    pub fn ajouter_lot(&mut self, l: &[El]) {
        let mut s: HashSet<usize> = self.sommets.iter().copied().collect();
        let mut a: HashSet<Arete> = self.aretes.iter().copied().collect();
        let mut f: HashSet<usize> = self.faces.iter().copied().collect();
        for &el in l {
            let neuf = match el {
                El::S(i) => s.insert(i) && { self.sommets.push(i); true },
                El::A(x) => {
                    let c = arete(x[0], x[1]);
                    a.insert(c) && { self.aretes.push(c); true }
                }
                El::F(k) => f.insert(k) && { self.faces.push(k); true },
            };
            if neuf {
                self.dernier = Some(el);
            }
        }
    }

    pub fn retirer(&mut self, el: El) {
        if !self.contient(el) {
            return;
        }
        match el {
            El::S(i) => self.sommets.retain(|x| *x != i),
            El::F(f) => self.faces.retain(|x| *x != f),
            El::A(a) => {
                let c = arete(a[0], a[1]);
                self.aretes.retain(|x| *x != c)
            }
        }
        if let Some(d) = self.dernier {
            if d.t() == el.t() && !self.contient(d) {
                self.dernier = None;
            }
        }
    }

    pub fn basculer(&mut self, el: El) -> bool {
        if self.contient(el) {
            self.retirer(el);
            false
        } else {
            self.ajouter(el);
            true
        }
    }

    pub fn vider(&mut self, t: Option<T>) {
        match t {
            Some(t) => {
                self.vider_liste(t);
                if self.dernier.is_some_and(|d| d.t() == t) {
                    self.dernier = None;
                }
            }
            None => {
                for t in [T::S, T::A, T::F] {
                    self.vider_liste(t);
                }
                self.dernier = None;
            }
        }
        if let Some(c) = &mut self.cache {
            c.vider(t);
        }
    }

    pub fn tout(&mut self, m: &Maillage, t: T) {
        match t {
            T::S => self.sommets = (0..m.sommets.len()).collect(),
            T::A => self.aretes = m.aretes().into_iter().map(|[i, j]| arete(i, j)).collect(),
            T::F => self.faces = (0..m.faces.len()).collect(),
        }
        self.dernier = None;
    }

    pub fn sommets_de(el: El, m: &Maillage) -> Vec<usize> {
        match el {
            El::S(i) => vec![i],
            El::A(a) => a.to_vec(),
            El::F(f) => m.faces[f].s.clone(),
        }
    }

    pub fn impliques(&self, m: &Maillage) -> Vec<usize> {
        let (mut vus, mut r) = (HashSet::new(), Vec::new());
        let mut plus = |i: usize| {
            if vus.insert(i) {
                r.push(i);
            }
        };
        self.sommets.iter().for_each(|i| plus(*i));
        for a in &self.aretes {
            a.iter().for_each(|i| plus(*i));
        }
        for f in &self.faces {
            m.faces[*f].s.iter().for_each(|i| plus(*i));
        }
        r
    }

    pub fn renumeroter(&mut self, table: &[isize]) {
        let n = |i: usize| table.get(i).copied().unwrap_or(i as isize);
        let mut vus = HashSet::new();
        self.sommets = self.sommets.iter().map(|i| n(*i)).filter(|i| *i >= 0).map(|i| i as usize).filter(|i| vus.insert(*i)).collect();
        // Une arête dont un bout disparaît n'existe plus
        self.aretes = self.aretes.iter().filter_map(|a| { let (i, j) = (n(a[0]), n(a[1])); (i >= 0 && j >= 0).then(|| arete(i as usize, j as usize)) }).collect();
        self.dernier = match self.dernier {
            Some(El::S(i)) => (n(i) >= 0).then(|| El::S(n(i) as usize)),
            Some(El::A(a)) => (n(a[0]) >= 0 && n(a[1]) >= 0).then(|| El::A(arete(n(a[0]) as usize, n(a[1]) as usize))),
            d => d,
        };
        if let Some(c) = &mut self.cache {
            c.renumeroter(table);
        }
    }

    /// Faces renumérotées : `None` si la face a disparu.
    pub fn renumeroter_faces(&mut self, table: &[Option<usize>]) {
        let n = |f: usize| table.get(f).copied().flatten();
        self.faces = self.faces.iter().filter_map(|f| n(*f)).collect();
        if let Some(El::F(f)) = self.dernier {
            self.dernier = n(f).map(El::F);
        }
        if let Some(c) = &mut self.cache {
            c.renumeroter_faces(table);
        }
    }

    pub fn nettoyer(&mut self, m: &Maillage) {
        let nb = m.sommets.len();
        let aretes: HashSet<Arete> = m.aretes().into_iter().map(|[i, j]| arete(i, j)).collect();
        let (mut vs, mut va, mut vf) = (HashSet::new(), HashSet::new(), HashSet::new());
        self.sommets.retain(|i| *i < nb && vs.insert(*i));
        self.aretes.retain(|a| aretes.contains(a) && va.insert(*a));
        self.faces.retain(|f| *f < m.faces.len() && vf.insert(*f));
        if self.dernier.is_some_and(|d| !self.contient(d)) {
            self.dernier = None;
        }
        if let Some(c) = &mut self.cache {
            c.nettoyer(m);
        }
    }

    /// Indices, éléments masqués compris.
    pub fn vers_donnees(&self) -> Donnees {
        let mut d = Donnees { s: self.sommets.clone(), a: self.aretes.clone(), f: self.faces.clone() };
        if let Some(c) = &self.cache {
            let e = c.vers_donnees();
            let (mut s, mut a, mut f): (HashSet<usize>, HashSet<Arete>, HashSet<usize>) = (d.s.iter().copied().collect(), d.a.iter().copied().collect(), d.f.iter().copied().collect());
            d.s.extend(e.s.into_iter().filter(|x| s.insert(*x)));
            d.a.extend(e.a.into_iter().filter(|x| a.insert(*x)));
            d.f.extend(e.f.into_iter().filter(|x| f.insert(*x)));
        }
        d
    }
}

/// Réglages d'une révolution. `axe` est déjà résolu ; `angle` signé, en degrés.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RegleRevolution {
    pub axe: P3,
    pub angle: f64,
    pub segments: usize,
    pub fermer: bool,
}

/// Réussite d'une opération, et ce qu'elle a à dire. `Err` : rien n'a changé.
pub type Bilan = Result<Option<String>, String>;

fn pl(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

fn face(m: &Mat, s: Vec<usize>) -> Face {
    Face { s, m: m.clone(), uv: Vec::new() }
}

/// Point du monde ramené dans le repère de l'objet.
pub fn vers_local(o: &Objet, p: P3) -> P3 {
    let r = qinv(qnorm(o.rotation));
    let v = qappliquer(r, [p[0] - o.position[0], p[1] - o.position[1], p[2] - o.position[2]]);
    [v[0] / o.echelle[0], v[1] / o.echelle[1], v[2] / o.echelle[2]]
}

/// Faces survivant à une renumérotation de sommets (suppression, fusion) : celles qui
/// gardent trois sommets distincts, dans le même ordre.
fn survivantes(avant: &[Vec<usize>], n: &[isize], degenerees: bool) -> Vec<Option<usize>> {
    let mut k = 0;
    avant
        .iter()
        .map(|s| {
            if s.iter().any(|i| n[*i] < 0) {
                return None;
            }
            if degenerees {
                let u: HashSet<isize> = s.iter().map(|i| n[*i]).collect();
                if u.len() < 3 {
                    return None;
                }
            }
            k += 1;
            Some(k - 1)
        })
        .collect()
}

/// Copie de travail, au monde : les maillages réunis des objets édités.
#[derive(Clone, Debug, PartialEq)]
pub struct Travail {
    pub m: Arc<Maillage>,
    /// Objet de chaque sommet.
    pub proprio: Vec<usize>,
    pub sel: Sel,
    pub edites: Vec<usize>,
    /// Sommets, arêtes, faces.
    pub filtres: [bool; 3],
    /// Le maillage diffère de celui des objets.
    pub sale: bool,
    /// Change à chaque modification : le dessin s'y accroche.
    pub version: u64,
}

impl Travail {
    /// `edites` : objets demandés ; l'actif passe en dernier.
    pub fn ouvrir(objets: &[Objet], edites: &[usize], actif: usize, filtres: [bool; 3]) -> Travail {
        let mut l: Vec<usize> = Vec::new();
        for &k in edites {
            if k < objets.len() && objets[k].armature.is_none() && k != actif && !l.contains(&k) {
                l.push(k);
            }
        }
        l.push(actif);
        let (mut proprio, mut mondes) = (Vec::new(), Vec::new());
        for &k in &l {
            proprio.extend(std::iter::repeat_n(k, objets[k].maillage.sommets.len()));
            mondes.push(crate::edition::vers_monde(&objets[k]));
        }
        let m = Maillage::assembler(mondes.iter());
        let mut sel = Sel::default();
        let (mut ds, mut df) = (0, 0);
        for (n, &k) in l.iter().enumerate() {
            let d = &objets[k].sel;
            let (nb, nf) = (mondes[n].sommets.len(), mondes[n].faces.len());
            sel.sommets.extend(d.s.iter().filter(|i| **i < nb).map(|i| i + ds));
            sel.aretes.extend(d.a.iter().filter(|a| a[0] < nb && a[1] < nb).map(|a| arete(a[0] + ds, a[1] + ds)));
            sel.faces.extend(d.f.iter().filter(|i| **i < nf).map(|i| i + df));
            sel.nettoyer(&m);
            ds += nb;
            df += nf;
        }
        sel.dernier = None;
        for (i, t) in [T::S, T::A, T::F].into_iter().enumerate() {
            if !filtres[i] {
                sel.masquer(t);
            }
        }
        Travail { m: Arc::new(m), proprio, sel, edites: l, filtres, sale: false, version: 0 }
    }

    /// Projette les faces données : celles qui n'avaient pas d'UV en reçoivent.
    pub fn projeter(&mut self, faces: &[usize], p: crate::uv::Projection, a: crate::uv::Aspect) -> Bilan {
        if faces.is_empty() {
            return Err("Aucune face à projeter".into());
        }
        let l = crate::uv::projeter(&self.m, faces, p, a);
        let m = self.mm();
        for &f in faces {
            let n = m.faces[f].s.len();
            m.faces[f].uv.resize(n, [0.0; 2]);
        }
        for ((f, k), uv) in l {
            m.faces[f].uv[k] = uv;
        }
        self.maj();
        Ok(Some(format!("{} face{} projetée{}", faces.len(), pl(faces.len()), pl(faces.len()))))
    }

    /// Range les îlots des faces données dans leur tuile.
    pub fn mettre_en_page(&mut self, faces: &[usize], page: crate::uv::Page, a: crate::uv::Aspect) -> Bilan {
        let Some(d) = crate::uv::mettre_en_page(&self.m, faces, page, a) else {
            return Err("Trop d'îlots pour cette marge".into());
        };
        self.poser_uv(&d.coins);
        Ok(Some(format!("{} îlot{} rangé{} · {:.0} % de l'image", d.ilots, pl(d.ilots), pl(d.ilots), d.remplissage * 100.0)))
    }

    /// Marque les arêtes choisies comme coutures, ou les efface.
    pub fn marquer_coutures(&mut self, marquer: bool) -> Bilan {
        let l = self.sel.aretes.clone();
        if l.is_empty() {
            return Err("Choisissez d'abord des arêtes".into());
        }
        let cles: HashSet<Arete> = l.iter().map(|a| arete(a[0], a[1])).collect();
        let m = self.mm();
        m.coutures.retain(|a| !cles.contains(&arete(a[0], a[1])));
        if marquer {
            m.coutures.extend(l.iter().copied());
        }
        self.maj();
        let n = l.len();
        Ok(Some(format!("{n} couture{} {}", pl(n), if marquer { format!("marquée{}", pl(n)) } else { format!("effacée{}", pl(n)) })))
    }

    /// Une couture là où les UV de deux faces voisines se séparent déjà.
    pub fn coutures_depuis_uv(&mut self, faces: &[usize]) -> Bilan {
        let mut par_arete: HashMap<Arete, Vec<crate::uv::Coin>> = HashMap::new();
        for &f in faces.iter().filter(|f| crate::uv::complete(&self.m, **f)) {
            let s = &self.m.faces[f].s;
            for k in 0..s.len() {
                par_arete.entry(arete(s[k], s[(k + 1) % s.len()])).or_default().push((f, k));
            }
        }
        let uv_de = |(f, k): crate::uv::Coin, i: usize| {
            let x = &self.m.faces[f];
            (0..2).map(|d| (k + d) % x.s.len()).find(|c| x.s[*c] == i).map(|c| x.uv[c])
        };
        let mut neuves = Vec::new();
        for (a, l) in &par_arete {
            let [x, y] = l[..] else { continue };
            if a.iter().any(|i| match (uv_de(x, *i), uv_de(y, *i)) {
                (Some(p), Some(q)) => (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-6,
                _ => false,
            }) {
                neuves.push(*a);
            }
        }
        if neuves.is_empty() {
            return Err("Aucune séparation dans ces UV".into());
        }
        let n = neuves.len();
        self.mm().coutures.extend(neuves);
        self.maj();
        Ok(Some(format!("{n} couture{} tirée{} des UV", pl(n), pl(n))))
    }

    /// Déplie chaque groupe de faces dans sa tuile. Rien ne change si l'un d'eux échoue.
    pub fn deplier(&mut self, groupes: &[(Vec<usize>, crate::uv::Aspect)], page: crate::uv::Page) -> Bilan {
        if groupes.iter().all(|g| g.0.is_empty()) {
            return Err("Aucune face à déplier".into());
        }
        let mut faits = Vec::with_capacity(groupes.len());
        for (faces, a) in groupes {
            faits.push(crate::uv::deplier(&self.m, faces, page, *a)?);
        }
        let m = self.mm();
        for f in groupes.iter().flat_map(|g| &g.0) {
            let n = m.faces[*f].s.len();
            m.faces[*f].uv.resize(n, [0.0; 2]);
        }
        for d in &faits {
            self.poser_uv(&d.disposition.coins);
        }
        let (n, r) = (faits.iter().map(|d| d.disposition.ilots).sum::<usize>(), faits.iter().map(|d| d.retournes).sum::<usize>());
        let couvert = faits[0].disposition.remplissage;
        Ok(Some(if r > 0 {
            format!("{n} morceau{} · {r} triangle{} retourné{} : ajoutez des coutures", if n > 1 { "x" } else { "" }, pl(r), pl(r))
        } else {
            format!("{n} morceau{} déplié{} · {:.0} % de l'image", if n > 1 { "x" } else { "" }, pl(n), couvert * 100.0)
        }))
    }

    /// Nouvelles coordonnées des coins donnés. Toucher aux UV ne change pas la sélection.
    pub fn poser_uv(&mut self, valeurs: &[(crate::uv::Coin, crate::uv::Uv)]) {
        let m = self.mm();
        for ((f, k), uv) in valeurs {
            m.faces[*f].uv[*k] = *uv;
        }
        self.maj();
    }

    fn maj(&mut self) {
        self.version += 1;
        self.sel.nettoyer(&self.m);
    }

    fn mm(&mut self) -> &mut Maillage {
        self.sale = true;
        Arc::make_mut(&mut self.m)
    }

    fn par_objet(&self, local: &[usize]) -> HashMap<usize, Donnees> {
        let mut r: HashMap<usize, Donnees> = self.edites.iter().map(|k| (*k, Donnees::default())).collect();
        let d = self.sel.vers_donnees();
        let (mut local_f, mut compte) = (Vec::with_capacity(self.m.faces.len()), HashMap::new());
        for f in &self.m.faces {
            let n = compte.entry(self.proprio[f.s[0]]).or_insert(0);
            local_f.push(*n);
            *n += 1;
        }
        for i in d.s {
            r.get_mut(&self.proprio[i]).unwrap().s.push(local[i]);
        }
        for [i, j] in d.a {
            if self.proprio[i] == self.proprio[j] {
                r.get_mut(&self.proprio[i]).unwrap().a.push([local[i], local[j]]);
            }
        }
        for n in d.f {
            r.get_mut(&self.proprio[self.m.faces[n].s[0]]).unwrap().f.push(local_f[n]);
        }
        r
    }

    /// Range la copie dans les objets édités, chacun dans son repère. Une copie
    /// inchangée ne touche pas aux maillages : le retour au monde n'est pas exact.
    pub fn ranger(&mut self, objets: &mut [Objet]) {
        self.sel.nettoyer(&self.m);
        let m = &self.m;
        let mut parts: HashMap<usize, Maillage> = self.edites.iter().map(|k| (*k, Maillage::default())).collect();
        let mut local = Vec::with_capacity(m.sommets.len());
        for (i, p) in m.sommets.iter().enumerate() {
            let c = parts.get_mut(&self.proprio[i]).unwrap();
            local.push(c.sommets.len());
            c.sommets.push(*p);
        }
        for f in &m.faces {
            parts.get_mut(&self.proprio[f.s[0]]).unwrap().faces.push(Face { s: f.s.iter().map(|i| local[*i]).collect(), m: f.m.clone(), uv: f.uv.clone() });
        }
        for a in &m.fils {
            parts.get_mut(&self.proprio[a[0]]).unwrap().fils.push([local[a[0]], local[a[1]]]);
        }
        for a in m.coutures_valides() {
            if self.proprio[a[0]] == self.proprio[a[1]] {
                parts.get_mut(&self.proprio[a[0]]).unwrap().coutures.push([local[a[0]], local[a[1]]]);
            }
        }
        let mut sels = self.par_objet(&local);
        for k in self.edites.clone() {
            let o = &mut objets[k];
            o.sel = sels.remove(&k).unwrap_or_default();
            if self.sale {
                let mut c = parts.remove(&k).unwrap();
                for p in &mut c.sommets {
                    *p = vers_local(o, *p);
                }
                o.maillage = Arc::new(c);
            }
        }
        self.sale = false;
    }

    pub fn a_cheval(&self, l: &[usize]) -> bool {
        l.iter().map(|i| self.proprio[*i]).collect::<HashSet<_>>().len() > 1
    }

    pub fn rendre_selectionnable(&mut self, t: T) {
        if !self.filtres[t.rang()] {
            self.filtres[t.rang()] = true;
            self.sel.montrer(t);
        }
    }

    /// Filtre ajouté ou retiré ; au moins un reste actif.
    pub fn basculer_filtre(&mut self, t: T) -> Result<(), String> {
        let mut f = self.filtres;
        if f[t.rang()] && f.iter().filter(|x| **x).count() == 1 {
            return Err("Au moins un type d’élément doit rester sélectionnable".into());
        }
        f[t.rang()] = !f[t.rang()];
        self.regler_filtres(f);
        Ok(())
    }

    pub fn regler_filtres(&mut self, f: [bool; 3]) {
        for (i, t) in [T::S, T::A, T::F].into_iter().enumerate() {
            if self.filtres[i] != f[i] {
                self.filtres[i] = f[i];
                if f[i] { self.sel.montrer(t) } else { self.sel.masquer(t) }
            }
        }
        self.version += 1;
    }

    pub fn basculer(&mut self, el: El) {
        self.sel.basculer(el);
        self.version += 1;
    }

    /// L'élément seul, ou plus rien.
    pub fn seul(&mut self, el: Option<El>) {
        self.sel.vider(None);
        if let Some(e) = el {
            self.sel.ajouter(e);
        }
        self.version += 1;
    }

    pub fn resume(&self) -> String {
        let s = &self.sel;
        let mut l = Vec::new();
        for (n, nom) in [(s.faces.len(), "face"), (s.aretes.len(), "arête"), (s.sommets.len(), "sommet")] {
            if n > 0 {
                l.push(format!("{n} {nom}{}", pl(n)));
            }
        }
        l.join(", ")
    }

    pub fn impliques(&self) -> Vec<usize> {
        self.sel.impliques(&self.m)
    }

    /// Points que transforme la poignée.
    pub fn points(&self) -> Vec<P3> {
        self.impliques().iter().map(|i| self.m.sommets[*i]).collect()
    }

    /// Centre de l'élément choisi en dernier (pivot « élément actif »).
    pub fn centre_actif(&self) -> Option<P3> {
        let l = Sel::sommets_de(self.sel.dernier?, &self.m);
        let mut c = [0.0; 3];
        for i in &l {
            for k in 0..3 {
                c[k] += self.m.sommets[*i][k] / l.len() as f64;
            }
        }
        Some(c)
    }

    /// Pose les sommets `indices` : leurs positions de départ, transformées.
    pub fn poser(&mut self, indices: &[usize], departs: &[P3], t: Transfo) {
        let m = self.mm();
        for (&i, &v) in indices.iter().zip(departs) {
            m.sommets[i] = match t {
                Transfo::Translation(d) => [v[0] + d[0], v[1] + d[1], v[2] + d[2]],
                Transfo::Rotation { q, c } => {
                    let r = qappliquer(q, [v[0] - c[0], v[1] - c[1], v[2] - c[2]]);
                    [r[0] + c[0], r[1] + c[1], r[2] + c[2]]
                }
                Transfo::Echelle { s, c } => [0, 1, 2].map(|k| (v[k] - c[k]) * s[k] + c[k]),
            };
        }
        self.version += 1;
    }

    // ----- Opérations --------------------------------------------------------

    pub fn ajouter_sommet(&mut self, curseur: P3, actif: usize) -> Bilan {
        let l = self.impliques();
        self.proprio.push(l.last().map_or(actif, |i| self.proprio[*i]));
        let m = self.mm();
        m.sommets.push(curseur);
        let i = m.sommets.len() - 1;
        self.sel.vider(None);
        self.sel.ajouter(El::S(i));
        self.rendre_selectionnable(T::S);
        self.maj();
        Ok(Some("Sommet ajouté : glissez-le pour le placer".into()))
    }

    pub fn creer_face(&mut self) -> Bilan {
        let s = self.sel.sommets.clone();
        if s.len() < 3 {
            return Err("Sélectionnez au moins 3 sommets".into());
        }
        let cle = |l: &[usize]| {
            let mut c = l.to_vec();
            c.sort();
            c
        };
        let k = cle(&s);
        if self.m.faces.iter().any(|f| cle(&f.s) == k) {
            return Err("Cette face existe déjà".into());
        }
        if self.a_cheval(&s) {
            return Err("Ces sommets appartiennent à des objets différents".into());
        }
        let choisi: HashSet<usize> = s.iter().copied().collect();
        let mat = self.m.faces.iter().find(|f| f.s.iter().any(|i| choisi.contains(i))).map_or_else(|| Arc::from("defaut"), |f| f.m.clone());
        let m = self.mm();
        m.faces.push(Face { s, m: mat, uv: Vec::new() });
        m.nettoyer_fils();
        let n = m.faces.len() - 1;
        self.sel.vider(Some(T::S));
        self.sel.ajouter(El::F(n));
        self.rendre_selectionnable(T::F);
        self.maj();
        Ok(None)
    }

    pub fn creer_arete(&mut self) -> Bilan {
        let s = self.sel.sommets.clone();
        let &[a, b] = &s[..] else { return Err("Sélectionnez exactement 2 sommets".into()) };
        if self.m.aretes().iter().any(|e| *e == [a, b] || *e == [b, a]) {
            return Err("Cette arête existe déjà".into());
        }
        if self.a_cheval(&s) {
            return Err("Ces sommets appartiennent à des objets différents".into());
        }
        let m = self.mm();
        let (mut faces, mut table, mut coupees) = (Vec::with_capacity(m.faces.len()), Vec::with_capacity(m.faces.len()), 0);
        for f in std::mem::take(&mut m.faces) {
            let (Some(ia), Some(ib)) = (f.s.iter().position(|i| *i == a), f.s.iter().position(|i| *i == b)) else {
                table.push(Some(faces.len()));
                faces.push(f);
                continue;
            };
            // De a à b, puis de b à a, dans le sens de la face
            let tour = |d: usize, fin: usize| {
                let (mut r, mut k) = (Vec::new(), d);
                loop {
                    r.push(f.s[k]);
                    if k == fin {
                        return r;
                    }
                    k = (k + 1) % f.s.len();
                }
            };
            let (p, q) = (tour(ia, ib), tour(ib, ia));
            table.push(Some(faces.len()));
            faces.push(face(&f.m, p));
            faces.push(face(&f.m, q));
            coupees += 1;
        }
        m.faces = faces;
        if coupees == 0 {
            m.fils.push([a, b]);
        }
        self.sel.renumeroter_faces(&table);
        self.sel.vider(Some(T::S));
        self.sel.ajouter(El::A([a, b]));
        self.rendre_selectionnable(T::A);
        self.maj();
        Ok(Some(if coupees > 0 { format!("Face{p} coupée{p} en deux", p = pl(coupees)) } else { "Arête libre créée (sans face)".into() }))
    }

    pub fn supprimer_aretes(&mut self) -> Bilan {
        if self.sel.aretes.is_empty() {
            return Ok(None);
        }
        let retirees: HashSet<Arete> = self.sel.aretes.iter().copied().collect();
        let m = self.mm();
        let (mut table, mut n) = (Vec::with_capacity(m.faces.len()), 0);
        let sort = |f: &Face| (0..f.s.len()).any(|k| retirees.contains(&arete(f.s[k], f.s[(k + 1) % f.s.len()])));
        for f in &m.faces {
            table.push((!sort(f)).then(|| { n += 1; n - 1 }));
        }
        m.faces.retain(|f| !sort(f));
        m.fils.retain(|a| !retirees.contains(&arete(a[0], a[1])));
        self.sel.renumeroter_faces(&table);
        self.sel.vider(Some(T::A));
        self.maj();
        Ok(None)
    }

    pub fn inverser(&mut self) -> Bilan {
        if self.sel.faces.is_empty() {
            return Err("Aucune face sélectionnée".into());
        }
        let l = self.sel.faces.clone();
        let m = self.mm();
        for f in l {
            m.faces[f].s.reverse();
            m.faces[f].uv.reverse();
        }
        self.maj();
        Ok(None)
    }

    pub fn trianguler(&mut self) -> Bilan {
        let fs: HashSet<usize> = self.sel.faces.iter().copied().filter(|f| self.m.faces[*f].s.len() > 3).collect();
        if fs.is_empty() {
            return Err("Aucune face de plus de 3 sommets sélectionnée".into());
        }
        let m = self.mm();
        let (mut faces, mut table, mut nouvelles) = (Vec::new(), Vec::new(), Vec::new());
        for (k, f) in m.faces.iter().enumerate() {
            if !fs.contains(&k) {
                table.push(Some(faces.len()));
                faces.push(f.clone());
                continue;
            }
            table.push(None);
            for t in m.trianguler(&f.s) {
                nouvelles.push(faces.len());
                let mut n = face(&f.m, t.to_vec());
                if f.uv.len() == f.s.len() {
                    n.uv = t.iter().filter_map(|i| f.s.iter().position(|x| x == i).map(|k| f.uv[k])).collect();
                }
                faces.push(n);
            }
        }
        m.faces = faces;
        self.sel.renumeroter_faces(&table);
        let n = nouvelles.len();
        for f in nouvelles {
            self.sel.ajouter(El::F(f));
        }
        self.maj();
        Ok(Some(format!("{} face{} → {n} triangles", fs.len(), pl(fs.len()))))
    }

    pub fn dissoudre(&mut self) -> Bilan {
        let na = self.sel.aretes.len();
        let g = self.m.dissolution(&self.sel.aretes)?;
        let (mut nouvelle, mut retirees) = (HashMap::new(), HashSet::new());
        for (n, x) in g.iter().enumerate() {
            nouvelle.insert(x.faces[0], n);
            retirees.extend(x.faces.iter().copied());
        }
        let m = self.mm();
        let (mut faces, mut table, mut places) = (Vec::new(), Vec::new(), vec![0; g.len()]);
        for (k, f) in m.faces.iter().enumerate() {
            if let Some(&n) = nouvelle.get(&k) {
                places[n] = faces.len();
                table.push(None);
                faces.push(face(&f.m, g[n].s.clone()));
            } else if retirees.contains(&k) {
                table.push(None);
            } else {
                table.push(Some(faces.len()));
                faces.push(f.clone());
            }
        }
        m.faces = faces;
        self.sel.renumeroter_faces(&table);
        for p in places {
            self.sel.ajouter(El::F(p));
        }
        self.sel.nettoyer(&self.m);
        self.rendre_selectionnable(T::F);
        self.maj();
        let fin = if g.len() > 1 { format!("{} faces", g.len()) } else { format!("1 face de {} sommets", g[0].s.len()) };
        Ok(Some(format!("{na} arête{p} dissoute{p} → {fin}{}", if g.iter().all(|x| x.plane) { "" } else { " (non plane)" }, p = pl(na))))
    }

    pub fn subdiviser(&mut self, coupes: usize) -> Bilan {
        let aretes = self.sel.aretes.clone();
        if aretes.is_empty() {
            return Err("Aucune arête sélectionnée".into());
        }
        let nouveaux = self.mm().subdiviser(&aretes, coupes);
        for [i, j] in &aretes {
            let p = self.proprio[*i];
            let l = &nouveaux[&(*i, *j)];
            if self.proprio.len() <= l[l.len() - 1] {
                self.proprio.resize(l[l.len() - 1] + 1, p);
            }
            for n in l {
                self.proprio[*n] = p;
            }
        }
        self.sel.vider(Some(T::A));
        for &[i, j] in &aretes {
            let mut c = vec![i];
            c.extend_from_slice(&nouveaux[&(i, j)]);
            c.push(j);
            for w in c.windows(2) {
                self.sel.ajouter(El::A([w[0], w[1]]));
            }
        }
        self.maj();
        Ok(None)
    }

    pub fn arete_depart(&self) -> Option<Arete> {
        match (self.sel.dernier, &self.sel.aretes[..]) {
            (Some(El::A(a)), _) => Some(a),
            (_, [a]) => Some(*a),
            _ => None,
        }
    }

    pub fn boucle(&mut self, arete: Option<Arete>, coupes: usize, glissement: f64, uniforme: bool, inverser: bool) -> Bilan {
        let an = arete.or(self.arete_depart()).and_then(|[i, j]| self.m.anneau(i, j));
        let Some(an) = an else { return Err("Touchez une arête qui borde un quadrilatère".into()) };
        let (nouvelles, table) = self.mm().couper_boucle_suivi(&an, coupes, glissement, uniforme, inverser);
        for [a, _] in &an.rails {
            for _ in 0..coupes {
                self.proprio.push(self.proprio[*a]);
            }
        }
        self.sel.renumeroter_faces(&table);
        self.sel.vider(Some(T::A));
        for a in nouvelles {
            self.sel.ajouter(El::A(a));
        }
        self.maj();
        let n = an.rails.len();
        Ok(Some(format!("Boucle de coupe : {n} arête{p} coupée{p}{}", if an.ferme { " (tour complet)" } else { "" }, p = pl(n))))
    }

    /// Faces et sommets après une renumérotation du moteur.
    fn apres_table(&mut self, avant: &[Vec<usize>], table: &[isize], degenerees: bool) {
        self.sel.renumeroter_faces(&survivantes(avant, table, degenerees));
        self.sel.renumeroter(table);
    }

    pub fn fusionner(&mut self, cible: Cible) -> Bilan {
        let s = self.sel.sommets.clone();
        if s.len() < 2 {
            return Err("Sélectionnez au moins 2 sommets".into());
        }
        if self.a_cheval(&s) {
            return Err("Ces sommets appartiennent à des objets différents".into());
        }
        let autres: HashSet<usize> = s[1..].iter().copied().collect();
        let mut i = 0;
        self.proprio.retain(|_| { i += 1; !autres.contains(&(i - 1)) });
        let avant: Vec<Vec<usize>> = self.m.faces.iter().map(|f| f.s.clone()).collect();
        let table = self.mm().fusionner(&s, cible);
        self.apres_table(&avant, &table, true);
        self.maj();
        Ok(None)
    }

    pub fn fusion_proches(&mut self, tolerance: f64) -> Bilan {
        let tous = self.sel.vide();
        let indices: Vec<usize> = if tous { (0..self.m.sommets.len()).collect() } else { self.impliques() };
        let proprio = self.proprio.clone();
        let avant: Vec<Vec<usize>> = self.m.faces.iter().map(|f| f.s.clone()).collect();
        let (table, r) = self.mm().fusionner_proches(Some(&indices), tolerance, &|i| proprio[i]);
        if r.retires > 0 {
            let mut p = vec![0; self.m.sommets.len()];
            for (i, j) in table.iter().enumerate() {
                if *j >= 0 {
                    p[*j as usize] = proprio[i];
                }
            }
            self.proprio = p;
            self.apres_table(&avant, &table, true);
        }
        self.maj();
        let ou = if tous { "tout le maillage" } else { "la sélection" };
        Ok(Some(if r.retires > 0 {
            format!("{} sommets réunis en {} ({} retiré{}), dans {ou}", r.retires + r.groupes, r.groupes, r.retires, pl(r.retires))
        } else {
            format!("Aucun sommet confondu à cette tolérance dans {ou}")
        }))
    }

    pub fn supprimer_faces(&mut self) -> Bilan {
        if self.sel.faces.is_empty() {
            return Ok(None);
        }
        let fs: HashSet<usize> = self.sel.faces.iter().copied().collect();
        let mut k = 0;
        let table: Vec<Option<usize>> = (0..self.m.faces.len()).map(|f| (!fs.contains(&f)).then(|| { k += 1; k - 1 })).collect();
        let mut i = 0;
        self.mm().faces.retain(|_| { i += 1; !fs.contains(&(i - 1)) });
        self.sel.renumeroter_faces(&table);
        self.maj();
        Ok(None)
    }

    pub fn supprimer_sommets(&mut self) -> Bilan {
        let s = self.sel.sommets.clone();
        if s.is_empty() {
            return Ok(None);
        }
        let retires: HashSet<usize> = s.iter().copied().collect();
        let mut i = 0;
        self.proprio.retain(|_| { i += 1; !retires.contains(&(i - 1)) });
        let avant: Vec<Vec<usize>> = self.m.faces.iter().map(|f| f.s.clone()).collect();
        let table = self.mm().supprimer_sommets(&s);
        self.apres_table(&avant, &table, false);
        self.maj();
        Ok(None)
    }

    /// Faces choisies séparées en nouveaux objets. Rend vrai s'il ne reste aucun objet
    /// édité : `self` n'a alors plus de sens et le mode Scène reprend, sur `choix`.
    pub fn separer(&mut self, p: &mut Projet, choix: &mut Vec<usize>) -> Result<(String, bool), String> {
        if self.sel.faces.is_empty() {
            return Err("Aucune face sélectionnée".into());
        }
        let fs: HashSet<usize> = self.sel.faces.iter().copied().collect();
        let donneurs = |t: &Travail| {
            let mut l: Vec<usize> = Vec::new();
            for f in &t.sel.faces {
                let k = t.proprio[t.m.faces[*f].s[0]];
                if !l.contains(&k) {
                    l.push(k);
                }
            }
            l
        };
        let modifies: Vec<String> = donneurs(self).iter().filter(|k| matches!(&p.objets[**k].modifs, crate::json::J::Liste(l) if !l.is_empty())).map(|k| format!("« {} »", p.objets[*k].nom)).collect();
        if !modifies.is_empty() {
            return Err(format!("{}{} des modifieurs : appliquez-les ou retirez-les avant de séparer", modifies.join(", "), if modifies.len() > 1 { " ont" } else { " a" }));
        }
        // Objet donneur → maillage et table de ses sommets, dans l'ordre des faces
        let mut parts: Vec<(usize, Maillage, HashMap<usize, usize>)> = Vec::new();
        for (n, f) in self.m.faces.iter().enumerate() {
            if !fs.contains(&n) {
                continue;
            }
            let k = self.proprio[f.s[0]];
            if !parts.iter().any(|x| x.0 == k) {
                parts.push((k, Maillage::default(), HashMap::new()));
            }
            let x = parts.iter_mut().find(|x| x.0 == k).unwrap();
            let s = f.s.iter().map(|i| {
                *x.2.entry(*i).or_insert_with(|| {
                    x.1.sommets.push(self.m.sommets[*i]);
                    x.1.sommets.len() - 1
                })
            }).collect();
            x.1.faces.push(face(&f.m, s));
        }
        let (mut noms, mut nouveaux) = (Vec::new(), Vec::new());
        for (k, c, _) in &parts {
            let o = p.objets[*k].clone();
            let mut n = Objet::neuf(&crate::edition::nom_libre(&p.objets, &o.nom), Maillage::default());
            let mut l = c.clone();
            for q in &mut l.sommets {
                *q = vers_local(&o, *q);
            }
            n.maillage = Arc::new(l);
            (n.position, n.rotation, n.echelle, n.set, n.materiau) = (o.position, o.rotation, o.echelle, o.set, o.materiau);
            noms.push(format!("« {} »", n.nom));
            p.objets.push(n);
            nouveaux.push(p.objets.len() - 1);
        }
        // Donneurs : faces retirées, puis les sommets qu'elles étaient seules à utiliser
        let mut k = 0;
        let table: Vec<Option<usize>> = (0..self.m.faces.len()).map(|f| (!fs.contains(&f)).then(|| { k += 1; k - 1 })).collect();
        let mut i = 0;
        let m = self.mm();
        m.faces.retain(|_| { i += 1; !fs.contains(&(i - 1)) });
        let mut gardes: HashSet<usize> = m.fils.iter().flatten().copied().collect();
        for f in &m.faces {
            gardes.extend(f.s.iter().copied());
        }
        let mut orphelins: Vec<usize> = Vec::new();
        for x in &parts {
            let mut l: Vec<usize> = x.2.keys().copied().filter(|i| !gardes.contains(i)).collect();
            l.sort();
            orphelins.extend(l);
        }
        let hors: HashSet<usize> = orphelins.iter().copied().collect();
        let mut i = 0;
        self.proprio.retain(|_| { i += 1; !hors.contains(&(i - 1)) });
        self.sel.renumeroter_faces(&table);
        let avant: Vec<Vec<usize>> = self.m.faces.iter().map(|f| f.s.clone()).collect();
        let t = self.mm().supprimer_sommets(&orphelins);
        self.apres_table(&avant, &t, false);
        self.maj();
        let restants: HashSet<usize> = self.proprio.iter().copied().collect();
        let vides: Vec<usize> = parts.iter().map(|x| x.0).filter(|k| !restants.contains(k)).collect();
        let mut msg = format!("{n} face{s} séparée{s} → {}", noms.join(", "), n = fs.len(), s = pl(fs.len()));
        let mut fin = false;
        if !vides.is_empty() {
            let v: Vec<String> = vides.iter().map(|k| format!("« {} »", p.objets[*k].nom)).collect();
            msg += &format!(" ; {} vidé{s}, supprimé{s}", v.join(", "), s = pl(v.len()));
            fin = self.retirer_vides(p, choix, &vides, &nouveaux);
        }
        Ok((msg, fin))
    }

    /// Objets édités `vides` supprimés, la copie rouverte sur les autres. Rend vrai s'il
    /// n'en reste aucun : `nouveaux` sont alors choisis pour le mode Scène.
    fn retirer_vides(&mut self, p: &mut Projet, choix: &mut Vec<usize>, vides: &[usize], nouveaux: &[usize]) -> bool {
        self.sale = true;
        self.ranger(&mut p.objets);
        let n = |k: usize| k - vides.iter().filter(|v| **v < k).count();
        let edites: Vec<usize> = self.edites.iter().copied().filter(|k| !vides.contains(k)).collect();
        let actif = p.actif;
        let mut i = 0;
        p.objets.retain(|_| { i += 1; !vides.contains(&(i - 1)) });
        if edites.is_empty() {
            *choix = nouveaux.iter().map(|k| n(*k)).collect();
            p.actif = choix.last().copied();
            return true;
        }
        *choix = choix.iter().copied().filter(|k| !vides.contains(k)).map(n).collect();
        let e: Vec<usize> = edites.iter().map(|k| n(*k)).collect();
        let a = actif.filter(|k| !vides.contains(k)).map(n).unwrap_or(e[e.len() - 1]);
        p.actif = Some(a);
        let (f, v) = (self.filtres, self.version);
        *self = Travail::ouvrir(&p.objets, &e, a, f);
        self.version = v + 1;
        false
    }

    pub fn revolution(&mut self, r: RegleRevolution, centre: P3) -> Bilan {
        let n = self.m.sommets.len();
        let sommets = self.sel.sommets.clone();
        let choisis: HashSet<usize> = sommets.iter().copied().collect();
        let mut aretes = self.sel.aretes.clone();
        let mut deja: HashSet<Arete> = aretes.iter().copied().collect();
        for [i, j] in self.m.aretes() {
            let a = arete(i, j);
            if choisis.contains(&i) && choisis.contains(&j) && deja.insert(a) {
                aretes.push(a);
            }
        }
        let profil: HashSet<usize> = sommets.iter().copied().chain(aretes.iter().flatten().copied()).collect();
        let materiau = self.m.faces.iter().find(|f| f.s.iter().any(|i| profil.contains(i))).map_or_else(|| Arc::from("defaut"), |f| f.m.clone());
        let o = Revolution { axe: r.axe, centre, angle: r.angle, segments: r.segments, fermer: r.fermer, materiau };
        let x = self.mm().revolution(&sommets, &aretes, &o);
        self.proprio.resize(self.m.sommets.len(), 0);
        for (i, l) in &x.copies {
            for &j in l {
                if j >= n {
                    self.proprio[j] = self.proprio[*i];
                }
            }
        }
        self.sel.vider(None);
        for f in &x.faces {
            self.sel.ajouter(El::F(*f));
        }
        for a in &x.fils {
            self.sel.ajouter(El::A(*a));
        }
        if !x.faces.is_empty() {
            self.rendre_selectionnable(T::F);
        }
        if !x.fils.is_empty() {
            self.rendre_selectionnable(T::A);
        }
        self.maj();
        let (nf, na, ns) = (x.faces.len(), x.fils.len(), self.m.sommets.len() - n);
        let mut l = Vec::new();
        if nf > 0 {
            l.push(format!("{nf} face{}", pl(nf)));
        }
        if na > 0 {
            l.push(format!("{na} arête{p} libre{p}", p = pl(na)));
        }
        l.push(format!("{ns} sommet{p} ajouté{p}", p = pl(ns)));
        Ok(Some(l.join(" · ")))
    }

    // ----- Sélection ---------------------------------------------------------

    pub fn convertir(&mut self, de: T, vers: T) -> Bilan {
        let (m, s) = (&self.m, &self.sel);
        let cotes = |f: &Face| (0..f.s.len()).map(|k| [f.s[k], f.s[(k + 1) % f.s.len()]]).collect::<Vec<_>>();
        let mut el = Vec::new();
        match (de, vers) {
            (T::S, T::A) => {
                let c: HashSet<usize> = s.sommets.iter().copied().collect();
                el.extend(m.aretes().into_iter().filter(|a| a.iter().all(|i| c.contains(i))).map(El::A));
            }
            (T::S, _) => {
                let c: HashSet<usize> = s.sommets.iter().copied().collect();
                el.extend((0..m.faces.len()).filter(|f| m.faces[*f].s.iter().all(|i| c.contains(i))).map(El::F));
            }
            (T::A, T::S) => el.extend(s.aretes.iter().flatten().map(|i| El::S(*i))),
            (T::F, T::S) => el.extend(s.faces.iter().flat_map(|f| m.faces[*f].s.iter()).map(|i| El::S(*i))),
            (T::F, _) => el.extend(s.faces.iter().flat_map(|f| cotes(&m.faces[*f])).map(El::A)),
            (T::A, _) => {
                let c: HashSet<Arete> = s.aretes.iter().copied().collect();
                el.extend((0..m.faces.len()).filter(|f| cotes(&m.faces[*f]).iter().any(|a| c.contains(&arete(a[0], a[1])))).map(El::F));
            }
        }
        self.sel.vider(Some(de));
        self.sel.ajouter_lot(&el);
        self.rendre_selectionnable(vers);
        self.maj();
        Ok(Some(if self.sel.vide() { "Plus rien de sélectionné".into() } else { format!("Sélection : {}", self.resume()) }))
    }

    pub fn tout(&mut self, t: T) {
        let m = self.m.clone();
        self.sel.tout(&m, t);
        self.rendre_selectionnable(t);
        self.maj();
    }

    pub fn aucun(&mut self, t: Option<T>) {
        self.sel.vider(t);
        self.maj();
    }

    pub fn lies(&mut self) -> Bilan {
        let Some(actif) = self.sel.dernier else { return Err("Aucun élément actif : touchez un sommet, une arête ou une face".into()) };
        let (sommets, faces) = self.m.connexe(&Sel::sommets_de(actif, &self.m));
        let dans: HashSet<usize> = sommets.iter().copied().collect();
        let mut part = Maillage::default();
        part.faces = faces.iter().map(|f| self.m.faces[*f].clone()).collect();
        part.fils = self.m.fils.iter().copied().filter(|a| dans.contains(&a[0]) && dans.contains(&a[1])).collect();
        let aretes = part.aretes();
        let s: Vec<El> = sommets.iter().map(|i| El::S(*i)).collect();
        let a: Vec<El> = aretes.iter().map(|x| El::A(*x)).collect();
        let f: Vec<El> = faces.iter().map(|x| El::F(*x)).collect();
        let mut caches = 0;
        for (i, l) in [s, a, f].iter().enumerate() {
            if self.filtres[i] {
                self.sel.ajouter_lot(l);
            } else {
                caches += 1;
                self.sel.cache.get_or_insert_with(Default::default).ajouter_lot(l);
            }
        }
        self.sel.dernier = Some(actif);
        self.maj();
        Ok(Some(format!(
            "Liés : {} sommet{}, {} arête{}, {} face{}{}",
            sommets.len(), pl(sommets.len()), aretes.len(), pl(aretes.len()), faces.len(), pl(faces.len()),
            if caches > 0 { " (filtres éteints : mis de côté)" } else { "" }
        )))
    }

    /// Plus court chemin de l'élément actif à `el`, ajouté à la sélection. `None` :
    /// `el` doit simplement être basculé.
    pub fn chemin(&mut self, el: El) -> Option<String> {
        let a = self.sel.dernier?;
        if a.t() != el.t() || !self.sel.contient(a) {
            return None;
        }
        let meme = match (a, el) {
            (El::A(x), El::A(y)) => arete(x[0], x[1]) == arete(y[0], y[1]),
            _ => a == el,
        };
        if meme {
            return None;
        }
        let l: Option<Vec<El>> = match (a, el) {
            (El::S(x), El::S(y)) => self.m.chemin_sommets(x, y).map(|l| l.into_iter().map(El::S).collect()),
            (El::A(x), El::A(y)) => self.m.chemin_aretes(x, y).map(|l| l.into_iter().map(El::A).collect()),
            (El::F(x), El::F(y)) => self.m.chemin_faces(x, y).map(|l| l.into_iter().map(El::F).collect()),
            _ => None,
        };
        let Some(l) = l else { return Some("Aucun chemin : ces éléments ne sont pas reliés".into()) };
        let n = l.len();
        self.sel.ajouter_lot(&l);
        self.sel.ajouter(el);
        self.sel.dernier = Some(el);
        self.version += 1;
        let nom = match el { El::S(_) => ["sommet", "sommets"], El::A(_) => ["arête", "arêtes"], El::F(_) => ["face", "faces"] };
        Some(format!("Chemin : {n} {}", nom[(n > 1) as usize]))
    }

    // ----- Sous le doigt -----------------------------------------------------

    /// Triangles de la copie, tournés vers le rayon ou non, avec leur face.
    fn triangles(&self) -> Vec<(usize, [P3; 3])> {
        let m = &self.m;
        let mut l = Vec::with_capacity(m.faces.len() * 2);
        for (k, f) in m.faces.iter().enumerate() {
            for [a, b, c] in m.trianguler(&f.s) {
                l.push((k, [m.sommets[a], m.sommets[b], m.sommets[c]]));
            }
        }
        l
    }

    fn visible(tris: &[(usize, [P3; 3])], o: P3, p: P3) -> bool {
        let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        let lg = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let u = d.map(|x| x / lg.max(1e-12));
        match vue::touche_faces(tris, o, u) {
            Some((t, _)) => t >= lg - (lg * 0.002).max(1e-4),
            None => true,
        }
    }

    /// Sommet le plus proche du pixel, parmi `parmi` s'il est donné.
    pub fn sommet_sous(&self, x: f64, y: f64, vue_: &Vue, rayon: f64, parmi: Option<&[usize]>, rayons_x: bool) -> Option<usize> {
        let tous: Vec<usize>;
        let l = match parmi {
            Some(l) => l,
            None => {
                tous = (0..self.m.sommets.len()).collect();
                &tous
            }
        };
        let mut cand: Vec<(f64, usize)> = l.iter().filter_map(|&i| {
            let e = vue_.ecran(self.m.sommets[i])?;
            let d = (e[0] - x).hypot(e[1] - y);
            (d <= rayon).then_some((d, i))
        }).collect();
        cand.sort_by(|a, b| a.0.total_cmp(&b.0));
        if rayons_x {
            return cand.first().map(|c| c.1);
        }
        let tris = self.triangles();
        cand.into_iter().find(|c| Self::visible(&tris, vue_.oeil, self.m.sommets[c.1])).map(|c| c.1)
    }

    pub fn arete_sous(&self, x: f64, y: f64, vue_: &Vue, rayon: f64, rayons_x: bool) -> Option<Arete> {
        let m = &self.m;
        let ecran: Vec<Option<[f64; 2]>> = m.sommets.iter().map(|p| vue_.ecran(*p)).collect();
        let mut cand = Vec::new();
        for [i, j] in m.aretes() {
            let (Some(a), Some(b)) = (ecran[i], ecran[j]) else { continue };
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l2 = dx * dx + dy * dy;
            let t = if l2 > 0.0 { (((x - a[0]) * dx + (y - a[1]) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
            let d = (a[0] + t * dx - x).hypot(a[1] + t * dy - y);
            if d <= rayon * 0.75 {
                cand.push((d, i, j, t));
            }
        }
        cand.sort_by(|a, b| a.0.total_cmp(&b.0));
        if rayons_x {
            return cand.first().map(|c| [c.1, c.2]);
        }
        let tris = self.triangles();
        cand.into_iter()
            .find(|&(_, i, j, t)| {
                let (a, b) = (m.sommets[i], m.sommets[j]);
                Self::visible(&tris, vue_.oeil, [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * t))
            })
            .map(|c| [c.1, c.2])
    }

    pub fn face_sous(&self, x: f64, y: f64, vue_: &Vue) -> Option<usize> {
        vue::touche_faces(&self.triangles(), vue_.oeil, vue_.rayon(x, y)).map(|c| c.1)
    }

    /// Élément sous le pixel selon les filtres : sommet, sinon arête, sinon face.
    pub fn element_sous(&self, x: f64, y: f64, vue_: &Vue, rayon: f64, rayons_x: bool) -> Option<El> {
        let f = self.filtres;
        if f[0] {
            if let Some(i) = self.sommet_sous(x, y, vue_, rayon, None, rayons_x) {
                return Some(El::S(i));
            }
        }
        if f[1] {
            if let Some(a) = self.arete_sous(x, y, vue_, rayon, rayons_x) {
                return Some(El::A(a));
            }
        }
        if f[2] {
            return self.face_sous(x, y, vue_).map(El::F);
        }
        None
    }

    /// Le doigt est sur la sélection : un sommet concerné, une arête ou une face choisie.
    pub fn sur_selection(&self, x: f64, y: f64, vue_: &Vue, rayon: f64, rayons_x: bool) -> bool {
        if self.sommet_sous(x, y, vue_, rayon, Some(&self.impliques()), rayons_x).is_some() {
            return true;
        }
        self.element_sous(x, y, vue_, rayon, rayons_x).is_some_and(|e| !matches!(e, El::S(_)) && self.sel.contient(e))
    }

    /// Sélection par une boîte en pixels [x0, y0, x1, y1] : ajoutée, ou retirée.
    pub fn boite(&mut self, r: [f64; 4], vue_: &Vue, retire: bool, rayons_x: bool) -> String {
        let m = self.m.clone();
        let ecran = |p: P3| vue_.ecran_q(p);
        let pts: Vec<Option<[f64; 3]>> = m.sommets.iter().map(|p| ecran(*p)).collect();
        let dans = |e: Option<[f64; 3]>| e.is_some_and(|e| e[0] >= r[0] && e[0] <= r[2] && e[1] >= r[1] && e[1] <= r[3]);
        let z = (!rayons_x).then(|| {
            let mut t = Vec::new();
            for f in &m.faces {
                for [a, b, c] in m.trianguler(&f.s) {
                    t.push([a, b, c]);
                }
            }
            Profondeurs::new(&pts, &t, r, 600000)
        });
        let visible = |p: P3| match &z {
            None => true,
            Some(t) => ecran(p).is_some_and(|e| t.visible(e[0], e[1], e[2])),
        };
        let centre = |l: &[usize]| [0, 1, 2].map(|c| l.iter().map(|i| m.sommets[*i][c]).sum::<f64>() / l.len() as f64);
        let f = self.filtres;
        let mut pris = Vec::new();
        if f[0] {
            pris.extend((0..pts.len()).filter(|i| dans(pts[*i]) && visible(m.sommets[*i])).map(El::S));
        }
        if f[1] {
            pris.extend(m.aretes().into_iter().filter(|a| dans(pts[a[0]]) && dans(pts[a[1]]) && visible(centre(a))).map(El::A));
        }
        if f[2] {
            pris.extend((0..m.faces.len()).filter(|k| m.faces[*k].s.iter().all(|i| dans(pts[*i])) && visible(centre(&m.faces[*k].s))).map(El::F));
        }
        let mut n = [0; 3];
        for el in pris {
            let deja = self.sel.contient(el);
            if retire && deja {
                self.sel.retirer(el);
                n[el.t().rang()] += 1;
            }
            if !retire && !deja {
                self.sel.ajouter(el);
                n[el.t().rang()] += 1;
            }
        }
        self.version += 1;
        let signe = if retire { "−" } else { "+" };
        let l: Vec<String> = [(0, "sommet"), (1, "arête"), (2, "face")].iter().filter(|(j, _)| f[*j]).map(|(j, nom)| format!("{signe}{} {nom}{}", n[*j], pl(n[*j]))).collect();
        format!("Boîte : {}", l.join(", "))
    }
}

/// Tampon de profondeur logiciel, limité à une boîte : ce qui s'y voit.
pub struct Profondeurs {
    tampon: Vec<f64>,
    w: usize,
    h: usize,
    e: f64,
    x0: f64,
    y0: f64,
}

impl Profondeurs {
    /// `pts` : pixel et inverse de la profondeur de chaque sommet.
    pub fn new(pts: &[Option<[f64; 3]>], tris: &[[usize; 3]], r: [f64; 4], max: usize) -> Profondeurs {
        let marge = 2.0;
        let (x0, y0) = (r[0].floor() - marge, r[1].floor() - marge);
        let (lw, lh) = (r[2].ceil() + marge - x0, r[3].ceil() + marge - y0);
        let e = (max as f64 / (lw * lh).max(1.0)).sqrt().min(1.0);
        let (w, h) = (((lw * e).ceil() as usize).max(1), ((lh * e).ceil() as usize).max(1));
        let mut tampon = vec![f64::NEG_INFINITY; w * h];
        for t in tris {
            let (Some(a), Some(b), Some(c)) = (pts[t[0]], pts[t[1]], pts[t[2]]) else { continue };
            let (ax, ay, bx, by, cx, cy) = ((a[0] - x0) * e, (a[1] - y0) * e, (b[0] - x0) * e, (b[1] - y0) * e, (c[0] - x0) * e, (c[1] - y0) * e);
            let aire = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
            if aire.abs() < 1e-12 {
                continue;
            }
            let xmin = ax.min(bx).min(cx).floor().max(0.0) as usize;
            let xmax = ax.max(bx).max(cx).ceil().min(w as f64 - 1.0);
            let ymin = ay.min(by).min(cy).floor().max(0.0) as usize;
            let ymax = ay.max(by).max(cy).ceil().min(h as f64 - 1.0);
            if xmax < 0.0 || ymax < 0.0 {
                continue;
            }
            for py in ymin..=ymax as usize {
                for px in xmin..=xmax as usize {
                    let (x, y) = (px as f64 + 0.5, py as f64 + 0.5);
                    let u = ((bx - x) * (cy - y) - (by - y) * (cx - x)) / aire;
                    let v = ((cx - x) * (ay - y) - (cy - y) * (ax - x)) / aire;
                    let t = 1.0 - u - v;
                    if u < -1e-9 || v < -1e-9 || t < -1e-9 {
                        continue;
                    }
                    let q = u * a[2] + v * b[2] + t * c[2];
                    let k = py * w + px;
                    if q > tampon[k] {
                        tampon[k] = q;
                    }
                }
            }
        }
        Profondeurs { tampon, w, h, e, x0, y0 }
    }

    pub fn visible(&self, x: f64, y: f64, q: f64) -> bool {
        let (px, py) = (((x - self.x0) * self.e).floor() as i64, ((y - self.y0) * self.e).floor() as i64);
        let mut loin = f64::INFINITY;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (i, j) = (px + dx, py + dy);
                if i >= 0 && j >= 0 && (i as usize) < self.w && (j as usize) < self.h {
                    loin = loin.min(self.tampon[j as usize * self.w + i as usize]);
                }
            }
        }
        q >= loin - q.abs() * 0.004
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube() -> Travail {
        Travail::ouvrir(&[Objet::neuf("c", Maillage::cube(2.0))], &[0], 0, [true; 3])
    }

    fn vue() -> Vue {
        Vue { oeil: [0.0, 0.0, 10.0], droite: [1.0, 0.0, 0.0], haut: [0.0, 1.0, 0.0], dos: [0.0, 0.0, 1.0], largeur: 1000.0, hauteur: 1000.0, tan: 0.5, pres: 0.05 }
    }

    #[test]
    fn la_selection_masquee_revient() {
        let mut s = Sel::default();
        s.ajouter(El::S(3));
        s.ajouter(El::F(1));
        s.masquer(T::S);
        assert!(s.sommets.is_empty());
        assert_eq!(s.dernier, Some(El::F(1)));
        assert_eq!(s.vers_donnees().s, [3]);
        s.montrer(T::S);
        assert_eq!(s.sommets, [3]);
    }

    /// Cube dont chaque face couvre toute l'image, dans l'ordre de ses sommets.
    fn cube_uv() -> Travail {
        let mut m = Maillage::cube(2.0);
        for f in &mut m.faces {
            f.uv = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        }
        Travail::ouvrir(&[Objet::neuf("c", m)], &[0], 0, [true; 3])
    }

    #[test]
    fn subdiviser_interpole_les_uv() {
        let mut t = cube_uv();
        let a = t.m.faces[0].s[0..2].try_into().unwrap();
        t.sel.ajouter(El::A(a));
        t.subdiviser(1).unwrap();
        let f = &t.m.faces[0];
        assert_eq!(f.s.len(), 5);
        assert_eq!(f.uv, vec![[0.0, 0.0], [0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        // Toutes les faces gardent une UV par coin
        assert!(t.m.faces.iter().all(|f| f.uv.len() == f.s.len()));
    }

    #[test]
    fn la_boucle_de_coupe_garde_les_uv_et_suit_le_glissement() {
        for g in [0.0, 0.5] {
            let mut t = cube_uv();
            let a = t.m.faces[0].s[0..2].try_into().unwrap();
            t.boucle(Some(a), 1, g, false, false).unwrap();
            assert!(t.m.faces.iter().all(|f| f.uv.len() == f.s.len()), "{g}");
            // Chaque nouveau sommet garde, sur ses coins, la place qu'il a sur l'arête
            for f in &t.m.faces {
                for (k, i) in f.s.iter().enumerate() {
                    if *i >= 8 {
                        let u = f.uv[k];
                        assert!(u.iter().all(|x| (0.0..=1.0).contains(x)));
                        assert!(u.iter().any(|x| *x > 0.0 && *x < 1.0), "{u:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn trianguler_garde_les_uv() {
        let mut t = cube_uv();
        t.sel.ajouter(El::F(0));
        t.trianguler().unwrap();
        assert!(t.m.faces.iter().all(|f| f.uv.len() == f.s.len()));
    }

    #[test]
    fn les_uv_posees_reviennent_a_l_objet() {
        let mut t = cube_uv();
        let mut o = vec![Objet::neuf("c", Maillage::cube(2.0))];
        t.poser_uv(&[((0, 0), [0.25, 0.75])]);
        assert!(t.sale);
        t.ranger(&mut o);
        assert_eq!(o[0].maillage.faces[0].uv[0], [0.25, 0.75]);
    }

    #[test]
    fn renumeroter_oublie_ce_qui_disparait() {
        let mut s = Sel { sommets: vec![0, 1, 2], aretes: vec![[0, 2], [1, 2]], ..Default::default() };
        s.renumeroter(&[0, -1, 1]);
        assert_eq!(s.sommets, [0, 1]);
        assert_eq!(s.aretes, [[0, 1]]);
    }

    #[test]
    fn une_copie_inchangee_ne_touche_pas_au_maillage() {
        let mut o = [Objet::neuf("c", Maillage::cube(1.0))];
        o[0].rotation = crate::edition::qaxe([0.0, 1.0, 0.0], 0.7);
        let a = o[0].maillage.clone();
        let mut t = Travail::ouvrir(&o, &[0], 0, [true; 3]);
        t.ranger(&mut o);
        assert!(Arc::ptr_eq(&o[0].maillage, &a));
    }

    #[test]
    fn ranger_rend_le_maillage_a_son_repere() {
        let mut o = [Objet::neuf("c", Maillage::cube(1.0))];
        o[0].position = [3.0, 0.0, 0.0];
        o[0].echelle = [2.0, 1.0, 1.0];
        o[0].rotation = crate::edition::qaxe([0.0, 0.0, 1.0], 0.7);
        let mut t = Travail::ouvrir(&o, &[0], 0, [true; 3]);
        t.sel.ajouter(El::S(0));
        let d = [t.m.sommets[0]];
        t.poser(&[0], &d, Transfo::Translation([0.0; 3]));
        t.ranger(&mut o);
        for (a, b) in o[0].maillage.sommets.iter().zip(&Maillage::cube(1.0).sommets) {
            assert!((0..3).all(|k| (a[k] - b[k]).abs() < 1e-12), "{a:?}");
        }
        assert_eq!(o[0].sel.s, [0]);
    }

    #[test]
    fn deux_objets_ne_se_melangent_pas() {
        let mut o = [Objet::neuf("a", Maillage::cube(1.0)), Objet::neuf("b", Maillage::cube(1.0))];
        o[1].position = [5.0, 0.0, 0.0];
        let mut t = Travail::ouvrir(&o, &[0, 1], 1, [true; 3]);
        assert_eq!(t.edites, [0, 1]);
        t.sel.ajouter(El::S(0));
        t.sel.ajouter(El::S(8));
        assert!(t.fusionner(Cible::Centre).is_err());
        t.sel.vider(None);
        t.sel.ajouter(El::S(9));
        t.sel.ajouter(El::S(10));
        t.fusionner(Cible::Centre).unwrap();
        t.ranger(&mut o);
        assert_eq!(o[0].maillage.sommets.len(), 8);
        assert_eq!(o[1].maillage.sommets.len(), 7);
    }

    #[test]
    fn le_sommet_cache_ne_se_touche_pas() {
        let t = cube();
        let v = vue();
        let (avant, arriere) = ((0..8).max_by(|a, b| t.m.sommets[*a][2].total_cmp(&t.m.sommets[*b][2])).unwrap(), [-1.0, -1.0, -1.0]);
        let e = v.ecran(t.m.sommets[avant]).unwrap();
        assert_eq!(t.sommet_sous(e[0], e[1], &v, 28.0, None, false).map(|i| t.m.sommets[i][2]), Some(1.0));
        // Le coin arrière, invisible, ne répond qu'en rayons X, si on le vise seul
        let i = t.m.sommets.iter().position(|p| *p == arriere).unwrap();
        let e = v.ecran(arriere).unwrap();
        assert!(t.sommet_sous(e[0], e[1], &v, 2.0, None, false).is_none());
        assert_eq!(t.sommet_sous(e[0], e[1], &v, 2.0, None, true), Some(i));
    }

    #[test]
    fn la_face_devant_est_prise() {
        let t = cube();
        let f = t.face_sous(500.0, 500.0, &vue()).unwrap();
        assert!(t.m.faces[f].s.iter().all(|i| t.m.sommets[*i][2] == 1.0));
    }

    #[test]
    fn la_boite_prend_ce_qui_se_voit() {
        let mut t = cube();
        let m = t.boite([0.0, 0.0, 1000.0, 1000.0], &vue(), false, false);
        assert_eq!(t.sel.sommets.len(), 4, "{m}");
        assert!(m.starts_with("Boîte : +4 sommets, +4 arêtes, +1 face"), "{m}");
        let mut x = cube();
        x.boite([0.0, 0.0, 1000.0, 1000.0], &vue(), false, true);
        assert_eq!((x.sel.sommets.len(), x.sel.aretes.len(), x.sel.faces.len()), (8, 12, 6));
        x.boite([0.0, 0.0, 1000.0, 1000.0], &vue(), true, true);
        assert!(x.sel.vide());
    }

    #[test]
    fn un_filtre_eteint_ne_se_touche_plus() {
        let mut t = cube();
        t.regler_filtres([false, false, true]);
        assert_eq!(t.element_sous(500.0, 500.0, &vue(), 28.0, false).map(|e| e.t()), Some(T::F));
        assert!(t.basculer_filtre(T::F).is_err());
    }

    #[test]
    fn chemin_entre_deux_sommets() {
        let mut t = cube();
        let a = 0;
        let b = (0..8).find(|i| (0..3).all(|k| t.m.sommets[*i][k] == -t.m.sommets[a][k])).unwrap();
        t.sel.ajouter(El::S(a));
        assert!(t.chemin(El::S(a)).is_none());
        assert_eq!(t.chemin(El::S(b)).unwrap(), "Chemin : 4 sommets");
        assert_eq!(t.sel.sommets.len(), 4);
    }

    #[test]
    fn separer_tout_quitte_le_mode() {
        let mut p = Projet::neuf();
        let mut t = Travail::ouvrir(&p.objets, &[0], 0, [true; 3]);
        t.tout(T::F);
        let mut choix = vec![0];
        let (m, fin) = t.separer(&mut p, &mut choix).unwrap();
        assert!(fin, "{m}");
        assert_eq!(p.objets.len(), 1);
        assert_eq!(p.objets[0].nom, "cube 2");
        assert_eq!(choix, [0]);
    }
}
