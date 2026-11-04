//! Relit un projet et le réécrit : `cargo run --example aller_retour -- <entrée> <sortie>`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::fs::read_to_string(&a[1]).unwrap();
    let p = rustcraft::projet::Projet::lire(&t).unwrap();
    std::fs::write(&a[2], p.ecrire()).unwrap();
}
