//! Modifieurs : une pile par objet, appliquée dans l'ordre à son maillage sans le changer.
//! Répétition (copies décalées, éventuellement unies) et Booléen (avec un autre objet, la
//! « cible », désignée par son identifiant). Les réglages restent dans `Objet::modifs`.
//!
//! `evaluer` calcule, `cle` dit si un résultat est encore bon : l'application lance le
//! calcul dans un fil et affiche le dernier résultat en attendant.

use crate::booleens::{self, Op};
use crate::json::J;
use crate::maillage::Maillage;
use crate::projet::{Objet, Projet};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Maillage modifié d'un objet.
#[derive(Clone, Debug)]
pub struct Ev {
    pub cle: String,
    pub maillage: Arc<Maillage>,
    /// Un message par modifieur qui n'a pas pu s'appliquer.
    pub erreurs: Vec<Option<String>>,
    /// Gardés en vie : la clé cite leur adresse, qui ne doit pas resservir.
    _deps: Vec<Arc<Maillage>>,
}

impl Ev {
    /// Résultat d'un calcul qui a échoué tout entier : le maillage de base.
    pub fn echec(p: &Projet, k: usize) -> Ev {
        let o = &p.objets[k];
        let n = liste(o).len();
        Ev { cle: cle(p, k, &[]), maillage: o.maillage.clone(), erreurs: vec![Some("calcul impossible sur cette forme".into()); n], _deps: Vec::new() }
    }
}

/// Résultats, par identifiant d'objet.
pub type Cache = HashMap<String, Ev>;

pub const OPS: [(&str, &str); 3] = [("difference", "Différence"), ("union", "Union"), ("intersection", "Intersection")];

pub fn liste(o: &Objet) -> &[J] {
    o.modifs.liste().unwrap_or(&[])
}

pub fn genre(m: &J) -> &str {
    m.get("type").and_then(J::txt).unwrap_or("")
}

pub fn actif(m: &J) -> bool {
    matches!(m.get("actif"), Some(J::Bool(true)))
}

pub fn cible(m: &J) -> &str {
    m.get("cible").and_then(J::txt).unwrap_or("")
}

pub fn titre(m: &J) -> String {
    match genre(m) {
        "repetition" => "Répétition".into(),
        "booleen" => "Booléen".into(),
        g => g.to_string(),
    }
}

fn champ(k: &str, v: J) -> (String, J) {
    (k.to_string(), v)
}

/// Modifieur neuf, aux réglages par défaut de GDA.
pub fn neuf(genre: &str, cible: Option<&str>) -> J {
    let mut l = vec![champ("type", J::Txt(genre.into())), champ("actif", J::Bool(true))];
    if genre == "repetition" {
        l.push(champ("n", J::Num(3.0)));
        l.push(champ("relatif", J::Bool(true)));
        l.push(champ("decalage", J::Liste(vec![J::Num(1.2), J::Num(0.0), J::Num(0.0)])));
        l.push(champ("unir", J::Bool(false)));
    } else {
        l.push(champ("op", J::Txt("difference".into())));
        l.push(champ("cible", J::Txt(cible.unwrap_or("").into())));
    }
    J::Objet(l)
}

/// Change un champ d'un modifieur.
pub fn fixer(m: &mut J, k: &str, v: J) {
    if let J::Objet(l) = m {
        match l.iter_mut().find(|x| x.0 == k) {
            Some(x) => x.1 = v,
            None => l.push(champ(k, v)),
        }
    }
}

fn indice(p: &Projet, id: &str) -> Option<usize> {
    p.objets.iter().position(|o| o.id == id)
}

fn adresse(m: &Arc<Maillage>) -> usize {
    Arc::as_ptr(m) as usize
}

/// Ce dont dépend le résultat : maillages, réglages, et poses dès qu'une cible intervient.
pub fn cle(p: &Projet, k: usize, pile: &[usize]) -> String {
    let o = &p.objets[k];
    if liste(o).is_empty() {
        return format!("{:x}", adresse(&o.maillage));
    }
    let mut s = format!("{:x}{:?}", adresse(&o.maillage), o.modifs);
    let pile2: Vec<usize> = pile.iter().copied().chain([k]).collect();
    for m in liste(o).iter().filter(|m| genre(m) == "booleen" && actif(m)) {
        let Some(t) = indice(p, cible(m)) else { continue };
        if pile2.contains(&t) {
            s += "|boucle";
        } else {
            let c = &p.objets[t];
            s += &format!("|{}|{:?}{:?}{:?}|{:?}{:?}{:?}", cle(p, t, &pile2), c.position, c.rotation, c.echelle, o.position, o.rotation, o.echelle);
        }
    }
    s
}

/// Maillage modifié de l'objet `k`, gardé en cache ; `pile` : objets en cours d'évaluation.
pub fn evaluer(p: &Projet, k: usize, pile: &[usize], cache: &mut Cache) -> Arc<Maillage> {
    let o = &p.objets[k];
    if liste(o).is_empty() {
        return o.maillage.clone();
    }
    let c = cle(p, k, pile);
    if let Some(e) = cache.get(&o.id).filter(|e| e.cle == c) {
        return e.maillage.clone();
    }
    let (mut mm, mut erreurs, mut deps) = (o.maillage.clone(), Vec::new(), vec![o.maillage.clone()]);
    for m in liste(o) {
        let r = if actif(m) { modifier(p, k, &mm, m, pile, cache, &mut deps) } else { Ok(mm.clone()) };
        match r {
            Ok(x) => {
                mm = x;
                erreurs.push(None);
            }
            Err(e) => erreurs.push(Some(e)),
        }
    }
    cache.insert(o.id.clone(), Ev { cle: c, maillage: mm.clone(), erreurs, _deps: deps });
    mm
}

/// Copie du maillage de `t` dans le repère de `o`.
fn dans_repere(o: &Objet, t: &Objet, m: &Maillage) -> Maillage {
    let mut r = m.clone();
    let mx = t.matrice();
    for p in &mut r.sommets {
        *p = crate::geometrie::vers_local(o, crate::vue::appliquer(&mx, *p));
    }
    r
}

/// Booléen tolérant un côté vide : un résultat vide peut suivre dans la pile.
fn operer(a: &Maillage, b: &Maillage, op: Op) -> Result<Maillage, String> {
    match (a.faces.is_empty(), b.faces.is_empty(), op) {
        (true, _, Op::Union) => Ok(b.clone()),
        (true, _, _) | (_, true, Op::Intersection) => Ok(Maillage::default()),
        (_, true, _) => Ok(a.clone()),
        _ => booleens::operation(a, b, op),
    }
}

fn modifier(p: &Projet, k: usize, mm: &Arc<Maillage>, m: &J, pile: &[usize], cache: &mut Cache, deps: &mut Vec<Arc<Maillage>>) -> Result<Arc<Maillage>, String> {
    match genre(m) {
        "repetition" => {
            let n = m.get("n").and_then(J::num).unwrap_or(1.0);
            let n = if n.is_finite() { ((n + 0.5).floor() as i64).clamp(1, 200) as usize } else { 1 };
            let relatif = matches!(m.get("relatif"), Some(J::Bool(true)));
            let d = m.get("decalage").and_then(J::nombres::<3>).unwrap_or([0.0; 3]);
            let b = mm.boite_englobante();
            let pas = [0, 1, 2].map(|c| if relatif { d[c] * b.as_ref().map_or(0.0, |b| b.max[c] - b.min[c]) } else { d[c] });
            let copies: Vec<Maillage> = (0..n)
                .map(|i| {
                    let mut c = (**mm).clone();
                    c.sommets.iter_mut().for_each(|s| *s = [0, 1, 2].map(|a| s[a] + pas[a] * i as f64));
                    c
                })
                .collect();
            if !matches!(m.get("unir"), Some(J::Bool(true))) || n < 2 {
                return Ok(Arc::new(Maillage::assembler(copies.iter())));
            }
            let mut l = copies.into_iter();
            let mut r = l.next().unwrap();
            for c in l {
                r = booleens::operation(&r, &c, Op::Union).map_err(lisible)?;
            }
            Ok(Arc::new(r))
        }
        "booleen" => {
            let id = cible(m);
            if id.is_empty() {
                return Err("choisissez une cible".into());
            }
            let t = indice(p, id).ok_or("la cible n’existe plus")?;
            let pile2: Vec<usize> = pile.iter().copied().chain([k]).collect();
            if pile2.contains(&t) {
                return Err("dépendance circulaire".into());
            }
            let ev = evaluer(p, t, &pile2, cache);
            deps.push(ev.clone());
            let autre = dans_repere(&p.objets[k], &p.objets[t], &ev);
            let op = match m.get("op").and_then(J::txt) {
                Some("union") => Op::Union,
                Some("intersection") => Op::Intersection,
                _ => Op::Difference,
            };
            operer(mm, &autre, op).map(Arc::new).map_err(lisible)
        }
        _ => Ok(mm.clone()),
    }
}

fn lisible(e: String) -> String {
    if e == "maillage non fermé" { "l’objet ou la cible n’est pas un volume fermé".into() } else { e }
}

/// Maillage de base avec le seul modifieur `i` : ce que fige « Appliquer ».
pub fn appliquer(p: &Projet, k: usize, i: usize) -> Result<Maillage, String> {
    let o = &p.objets[k];
    let m = liste(o).get(i).ok_or("modifieur absent")?;
    let mut m = m.clone();
    fixer(&mut m, "actif", J::Bool(true));
    let mut cache = Cache::new();
    modifier(p, k, &o.maillage, &m, &[], &mut cache, &mut Vec::new()).map(|x| (*x).clone())
}

/// Objets servant de cible à un booléen actif d'un objet affiché.
pub fn cibles(p: &Projet) -> HashSet<usize> {
    let mut c = HashSet::new();
    for (k, o) in p.objets.iter().enumerate().filter(|(_, o)| p.affiche(o)) {
        for m in liste(o).iter().filter(|m| genre(m) == "booleen" && actif(m)) {
            c.extend(indice(p, cible(m)).filter(|t| *t != k));
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visserie::volume;

    fn scene() -> Projet {
        let mut p = Projet::neuf();
        p.objets.clear();
        for (n, x) in [("a", 0.0), ("b", 0.5)] {
            let mut o = Objet::neuf(n, Maillage::cube(1.0));
            o.position = [x, 0.0, 0.0];
            o.set = p.sets[0].nom.clone();
            p.objets.push(o);
        }
        let b = p.objets[1].id.clone();
        p.objets[0].modifs = J::Liste(vec![neuf("booleen", Some(&b))]);
        p
    }

    fn id(p: &Projet, k: usize) -> String {
        p.objets[k].id.clone()
    }

    fn ev(p: &Projet, k: usize) -> Ev {
        let mut c = Cache::new();
        evaluer(p, k, &[], &mut c);
        c.remove(&p.objets[k].id).unwrap()
    }

    #[test]
    fn un_booleen_tient_compte_des_poses_et_garde_la_base() {
        let mut p = scene();
        assert!((volume(&ev(&p, 0).maillage) - 0.5).abs() < 1e-9);
        assert_eq!(p.objets[0].maillage.faces.len(), 6);
        p.objets[1].echelle = [2.0, 2.0, 2.0];
        p.objets[1].position = [1.5, 0.0, 0.0];
        assert!((volume(&ev(&p, 0).maillage) - 1.0).abs() < 1e-9, "cible hors de l'objet");
        p.objets[1].position = [1.25, 0.0, 0.0];
        p.objets[0].position = [0.0, 0.0, 0.0];
        assert!((volume(&ev(&p, 0).maillage) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn le_resultat_est_dans_le_repere_de_l_objet() {
        let mut p = scene();
        p.objets[0].echelle = [2.0, 1.0, 1.0];
        p.objets[1].position = [1.0, 0.0, 0.0];
        // Au monde, a va de -1 à 1 et b de 0,5 à 1,5 ; il reste de -1 à 0,5
        let e = ev(&p, 0);
        let b = e.maillage.boite_englobante().unwrap();
        assert!((b.min[0] + 0.5).abs() < 1e-9 && (b.max[0] - 0.25).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn la_cle_suit_cible_pose_et_reglages_et_le_cache_sert() {
        let mut p = scene();
        let mut c = Cache::new();
        let a = evaluer(&p, 0, &[], &mut c);
        assert!(Arc::ptr_eq(&a, &evaluer(&p, 0, &[], &mut c)));
        p.objets[1].position[1] = 0.1;
        assert!(!Arc::ptr_eq(&a, &evaluer(&p, 0, &[], &mut c)));
        let b = evaluer(&p, 0, &[], &mut c);
        if let J::Liste(l) = &mut p.objets[0].modifs {
            fixer(&mut l[0], "op", J::Txt("union".into()));
        }
        assert!(!Arc::ptr_eq(&b, &evaluer(&p, 0, &[], &mut c)));
    }

    #[test]
    fn les_erreurs_sont_dites_sans_arreter_la_pile() {
        let mut p = scene();
        let rep = neuf("repetition", None);
        let l = |c: &str| J::Liste(vec![neuf("booleen", Some(c)), rep.clone()]);
        p.objets[0].modifs = l("");
        let e = ev(&p, 0);
        assert_eq!(e.erreurs[0].as_deref(), Some("choisissez une cible"));
        assert_eq!(e.maillage.faces.len(), 18, "la répétition s'applique quand même");
        p.objets[0].modifs = l("zz");
        assert_eq!(ev(&p, 0).erreurs[0].as_deref(), Some("la cible n’existe plus"));
        p.objets[0].modifs = l("b");
        assert_eq!(ev(&p, 0).erreurs[0].as_deref(), Some("la cible n’existe plus"), "un nom ne désigne pas");
        p.objets[0].modifs = l(&id(&p, 0));
        assert_eq!(ev(&p, 0).erreurs[0].as_deref(), Some("dépendance circulaire"));
        p.objets[0].modifs = l(&id(&p, 1));
        p.objets[1].modifs = J::Liste(vec![neuf("booleen", Some(&id(&p, 0)))]);
        assert_eq!(ev(&p, 1).erreurs[0], None);
        assert_eq!(ev(&p, 0).erreurs[0], None, "b, évalué sous a, voit la boucle et reste entier");
        let mut c = Cache::new();
        evaluer(&p, 1, &[0], &mut c);
        assert_eq!(c[&id(&p, 1)].erreurs[0].as_deref(), Some("dépendance circulaire"));
    }

    #[test]
    fn un_volume_ouvert_est_explique() {
        let mut p = scene();
        let mut m = Maillage::cube(1.0);
        m.faces.pop();
        p.objets[1].maillage = Arc::new(m);
        assert_eq!(ev(&p, 0).erreurs[0].as_deref(), Some("l’objet ou la cible n’est pas un volume fermé"));
    }

    #[test]
    fn la_repetition_relative_constante_et_unie() {
        let mut p = scene();
        p.objets[0].modifs = J::Liste(vec![neuf("repetition", None)]);
        let b = ev(&p, 0).maillage.boite_englobante().unwrap();
        assert!((b.max[0] - (0.5 + 2.4)).abs() < 1e-9);
        let J::Liste(l) = &mut p.objets[0].modifs else { panic!() };
        fixer(&mut l[0], "relatif", J::Bool(false));
        fixer(&mut l[0], "decalage", J::Liste(vec![J::Num(0.5), J::Num(0.0), J::Num(0.0)]));
        fixer(&mut l[0], "unir", J::Bool(true));
        assert!((volume(&ev(&p, 0).maillage) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn un_resultat_vide_peut_continuer_la_pile() {
        let mut p = scene();
        p.objets[1].position = [5.0, 0.0, 0.0];
        let mut m = neuf("booleen", Some(&id(&p, 1)));
        fixer(&mut m, "op", J::Txt("intersection".into()));
        p.objets[0].modifs = J::Liste(vec![m, neuf("repetition", None)]);
        let e = ev(&p, 0);
        assert!(e.maillage.faces.is_empty());
        assert_eq!(e.erreurs, vec![None, None]);
    }

    #[test]
    fn appliquer_ne_prend_qu_un_modifieur_meme_eteint() {
        let mut p = scene();
        let mut m = neuf("booleen", Some(&id(&p, 1)));
        fixer(&mut m, "actif", J::Bool(false));
        p.objets[0].modifs = J::Liste(vec![neuf("repetition", None), m]);
        assert!((volume(&appliquer(&p, 0, 1).unwrap()) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn seuls_les_booleens_actifs_d_objets_affiches_font_une_cible() {
        let mut p = scene();
        assert_eq!(cibles(&p), HashSet::from([1]));
        p.objets[0].visible = false;
        assert!(cibles(&p).is_empty());
        p.objets[0].visible = true;
        let J::Liste(l) = &mut p.objets[0].modifs else { panic!() };
        fixer(&mut l[0], "actif", J::Bool(false));
        assert!(cibles(&p).is_empty());
    }
}
