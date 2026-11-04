//! Prépare la démonstration des couches sur `hb-01-repos.gda` : « Repos » devient
//! additive, les masques utiles sont créés, et le mélange « Repos vivant » pose la
//! respiration sur le haut du corps, par-dessus le transfert d'appui.
//!
//!   cargo run --example repos_vivant -- <entrée.gda> <sortie.gda>

use rustcraft::animation::{Mode, Reference};
use rustcraft::couches::{Couche, Masque, Melange};
use rustcraft::projet::Projet;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut p = Projet::lire(&std::fs::read_to_string(&a[1]).unwrap()).unwrap();
    let o = p.objets.iter_mut().find(|o| o.armature.is_some()).expect("une armature");
    let x = o.armature.as_mut().unwrap();
    let os = |x: &rustcraft::armature::Armature, nom: &str| x.os.iter().find(|b| b.nom == nom).map(|b| b.id.clone()).unwrap();

    let mut l = x.animations();
    let repos = l.iter().position(|an| an.nom == "Repos").expect("« Repos »");
    (l[repos].mode, l[repos].reference) = (Mode::Additif, Reference::Cle(0));
    x.regler_animations(&l);
    let id = |n: &str| l.iter().find(|an| an.nom == n).map(|an| an.id.clone()).unwrap();

    // Haut du corps : la colonne à moitié, pour que la respiration ne plie pas le bas du dos
    let mut haut = Masque::os_et_descendants(x, &os(x, "colonne"), 1.0);
    haut[0].1 = 0.5;
    let masques = [
        ("haut du corps", haut),
        ("bras G", Masque::os_et_descendants(x, &os(x, "clavicule.G"), 1.0)),
        ("bras D", Masque::os_et_descendants(x, &os(x, "clavicule.D"), 1.0)),
        ("tête et cou", Masque::os_et_descendants(x, &os(x, "cou"), 1.0)),
    ];
    x.regler_masques(&masques.map(|(nom, os)| Masque { id: String::new(), nom: nom.into(), os }));
    let haut = x.masques().into_iter().find(|m| m.nom == "haut du corps").unwrap().id;

    let mut souffle = Couche::de(&id("Repos"));
    souffle.masque = Some(haut);
    let vivant = Melange { couches: vec![Couche::de(&id("Repos – appui")), souffle], ..Melange::neuf("Repos vivant") };
    x.regler_melanges(&[vivant]);
    std::fs::write(&a[2], p.ecrire()).unwrap();
}
