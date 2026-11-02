//! Vrais projets GDA, s'ils sont fournis : `RUSTCRAFT_PROJETS=<dossier>` rempli
//! par `tools/projets.js`. Le JSON relu et réécrit doit être celui de GDA, à
//! l'octet près, et un projet réenregistré doit se relire à l'identique.

use rustcraft::json::{self, J};
use rustcraft::projet::Projet;

#[test]
fn vrais_projets() {
    let Ok(d) = std::env::var("RUSTCRAFT_PROJETS") else { return };
    let mut n = 0;
    while let Ok(t) = std::fs::read_to_string(format!("{d}/{n}.gda")) {
        let mut o = String::new();
        json::indente(&json::lire(&t).unwrap(), 0, &mut o);
        assert_eq!(o, std::fs::read_to_string(format!("{d}/{n}.norm")).unwrap(), "projet {n}");
        let p = Projet::lire(&t).unwrap();
        let bilan = json::lire(&std::fs::read_to_string(format!("{d}/{n}.bilan")).unwrap()).unwrap();
        let l = bilan.liste().unwrap();
        assert_eq!(l.len(), p.objets.len());
        for (b, x) in l.iter().zip(&p.objets) {
            let b = b.liste().unwrap();
            assert_eq!(b[0].txt().unwrap(), x.nom);
            assert_eq!(b[1].num().unwrap() as usize, x.maillage.sommets.len());
            assert_eq!(b[2].num().unwrap() as usize, x.maillage.faces.len());
            assert_eq!(b[4], J::Bool(x.armature.is_some()));
        }
        let r = Projet::lire(&p.ecrire()).unwrap();
        assert_eq!(r, p, "projet {n}");
        n += 1;
    }
    println!("{n} projets vérifiés");
    assert!(n > 0);
}
