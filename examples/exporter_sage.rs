//! Exporte l'armature d'un projet pour S.A.G.E., en couches et aplatie :
//! `cargo run --example exporter_sage -- <projet.gda> <dossier>`.
use rustcraft::sage::Couches;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let mut app = rustcraft::editeur::App::new(1.0);
    let nom = a[1].rsplit('/').next().unwrap().to_string();
    app.ouvrir(&nom, &std::fs::read(&a[1]).unwrap());
    let k = app.projet.objets.iter().position(|o| o.armature.is_some()).expect("une armature");
    app.choisir(Some(k));
    app.attendre_calculs();
    for (c, suffixe) in [(Couches::EnCouches, "couches"), (Couches::Aplaties, "aplati")] {
        app.exporter_sage(c);
        for s in app.prendre_sorties() {
            let chemin = format!("{}/{}-{suffixe}.glb", a[2], s.nom.trim_end_matches(".glb"));
            std::fs::write(&chemin, &s.octets).unwrap();
            println!("{chemin} : {} Ko", s.octets.len() / 1024);
        }
    }
}
