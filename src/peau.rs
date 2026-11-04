//! Peau d'un objet lié à une armature : le cœur (poids calculés ou peints, déformation)
//! vient de la crate `gda` ; ici, ce qui dépend des objets du projet (place de l'objet par
//! rapport à l'armature, cache des peaux).

pub use gda::peau::*;

use crate::armature::Armature;
use crate::geometrie::vers_local;
use crate::maillage::{Maillage, P3};
use crate::projet::Objet;
use crate::vue;
use std::sync::Arc;

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

/// La peau d'un objet aux poids peints : sans calcul, tout de suite.
pub fn lier_main(base: Arc<Maillage>, o: &Objet, x: &Objet, main: &Main, cle: String) -> Liee {
    let m = o.matrice();
    let repos: Vec<P3> = base.sommets.iter().map(|p| vers_local(x, vue::appliquer(&m, *p))).collect();
    Liee { cle, peau: main.peau(), repos, base }
}
