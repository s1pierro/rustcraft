//! Dessine l'application sur le vrai processeur graphique, hors écran, et écrit
//! des images PPM. Sert à valider nuanceurs et mise en page sans installer l'APK.
//!
//!   cargo run --example apercu -- <dossier> [projet.gda]

use rustcraft::draw::{DrawList, Rect};
use rustcraft::edition::Mode;
use rustcraft::editeur::{App, Champ, Derniere, Contenu, Rendu};
use rustcraft::geometrie::{El, T};
use rustcraft::gles::{load_gl, Egl};
use rustcraft::renderer::Renderer;
use rustcraft::maillage::Maillage;
use rustcraft::projet::Projet;
use rustcraft::text::Text;
use rustcraft::theme::{self, C};
use rustcraft::ui::Ui;
use rustcraft::widgets::Cx;
use std::sync::Arc;

struct Vue {
    nom: &'static str,
    w: i32,
    h: i32,
    dp: f32,
    debut: bool,
    fin: bool,
    rendu: Rendu,
    choix: Option<usize>,
    mode: Mode,
    onglet: usize,
    pave: bool,
    /// 0 : scène ; 1 : faces ; 2 : sommets, Rayons X ; 3 : arêtes, opération réglable.
    geo: u8,
    /// Panneau Fichier ouvert.
    fichier: bool,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("dossier de sortie");
    let fichier = args.get(2).map(|f| (f.rsplit('/').next().unwrap().to_string(), std::fs::read(f).expect("projet")));
    let vues = [
        Vue { nom: "tablette", w: 1920, h: 1200, dp: 1.5, debut: true, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "tablette-jeu", w: 1920, h: 1200, dp: 1.5, debut: false, fin: false, rendu: Rendu::Jeu, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "tablette-portrait", w: 1200, h: 1920, dp: 1.5, debut: true, fin: true, rendu: Rendu::Solide, choix: Some(1), mode: Mode::Rotation, onglet: 1, pave: false, geo: 0, fichier: false },
        Vue { nom: "telephone", w: 2400, h: 1080, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: true },
        Vue { nom: "objet", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Rotation, onglet: 1, pave: false, geo: 0, fichier: false },
        Vue { nom: "telephone-portrait", w: 1080, h: 2400, dp: 2.5, debut: false, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Echelle, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "pave", w: 2400, h: 1080, dp: 2.5, debut: false, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: true, geo: 0, fichier: false },
        Vue { nom: "pave-texte", w: 1080, h: 2400, dp: 2.5, debut: false, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: true, geo: 0, fichier: false },
        Vue { nom: "aide", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "derniere", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Rotation, onglet: 2, pave: false, geo: 0, fichier: false },
        Vue { nom: "exporter", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: true },
        Vue { nom: "couleurs", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "sombre", w: 1920, h: 1200, dp: 1.5, debut: true, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 1, pave: false, geo: 0, fichier: false },
        Vue { nom: "texture", w: 1920, h: 1200, dp: 1.5, debut: false, fin: false, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "modifieurs", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "armature", w: 1920, h: 1200, dp: 1.5, debut: false, fin: false, rendu: Rendu::Solide, choix: Some(1), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "armature-os", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(1), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "liaison", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "liaison-fiche", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "pose", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "pose-scene", w: 1920, h: 1200, dp: 1.5, debut: false, fin: false, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "devant", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "peau", w: 1920, h: 1200, dp: 1.5, debut: false, fin: false, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "peau-poids", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "armature-vide", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "armature-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(1), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "sets", w: 1920, h: 1200, dp: 1.5, debut: true, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "sets-deplacer", w: 1920, h: 1200, dp: 1.5, debut: true, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "sets-telephone", w: 1080, h: 2400, dp: 2.5, debut: true, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "materiaux", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "materiaux-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-sphere", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-cube", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-page", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-deplie", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-page-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "uv-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "vis", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "vis-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: true, rendu: Rendu::Solide, choix: None, mode: Mode::Translation, onglet: 0, pave: false, geo: 0, fichier: false },
        Vue { nom: "geo-faces", w: 1920, h: 1200, dp: 1.5, debut: false, fin: true, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 1, pave: false, geo: 1, fichier: false },
        Vue { nom: "geo-rayons", w: 1920, h: 1200, dp: 1.5, debut: true, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Translation, onglet: 0, pave: false, geo: 2, fichier: false },
        Vue { nom: "geo-telephone", w: 1080, h: 2400, dp: 2.5, debut: false, fin: false, rendu: Rendu::Solide, choix: Some(0), mode: Mode::Rotation, onglet: 1, pave: false, geo: 3, fichier: false },
    ];
    let mut egl = Egl::new(true).expect("EGL");
    egl.attach_pbuffer(2400, 2400).expect("pbuffer");
    let mut render = Renderer::new(load_gl(true).expect("GL")).expect("nuanceurs");
    println!("nuanceurs compilés et liés");
    let mut text = Text::new();

    for s in vues {
        let (w, h) = (s.w as f32, s.h as f32);
        let ecran = Rect::new(0.0, 0.0, w, h);
        let mut app = App::new(s.dp);
        let mut ui = Ui::new(s.dp);
        let mut dl = DrawList::default();
        app.shell.start.open = s.debut;
        app.shell.end.open = s.fin;
        app.rendu = s.rendu;
        app.mode = s.mode;
        if s.onglet > 0 {
            app.contenu = Contenu::Contexte;
            app.onglet_contexte = s.onglet - 1;
        }
        if s.onglet == 2 {
            app.derniere = Some(Derniere::Rotation { degres: 45.0, axe: 1, vue: Some([0.3, 0.8, 0.5]), pivot: [0.0, 0.5, 0.0] });
        }
        if s.fichier {
            app.contenu = Contenu::Fichier;
            app.onglet_fichier = if s.nom == "exporter" { 2 } else { 0 };
        }
        // Le thème reste d'une vue à l'autre : chacune repart du sien
        app.couleurs = [None; theme::NB];
        theme::appliquer(&app.couleurs);
        if s.nom == "couleurs" {
            app.contenu = Contenu::Reglages;
            app.onglet = 0;
            app.choisir_couleur(0, Some([0xc0, 0x30, 0x60, 255]));
            app.couleur_ouverte = Some((0, theme::vers_tsl(theme::coul(C::Accent))));
        }
        if s.nom == "sombre" {
            for (c, v) in [(C::Accent, 0x7a70ff), (C::Fond, 0x17161f), (C::Surface, 0x262533), (C::Pastilles, 0x262533), (C::Bordures, 0x3a3848), (C::Texte, 0xe8e6f0), (C::Secondaire, 0x9a98ac), (C::FondCentre, 0x2c2b3a), (C::Face, 0x8f8ba6)] {
                app.choisir_couleur(c as usize, Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]));
            }
        }
        if s.nom == "aide" {
            app.contenu = Contenu::Aide;
            app.onglet_aide = 1;
        }
        render.set_dp(s.dp);
        // Quelques images pour poser les panneaux, puis l'ouverture, qui cadre.
        let etape = |app: &mut App, ui: &mut Ui, text: &mut Text, dl: &mut DrawList| {
            dl.clear();
            let mut cx = Cx { ui, text, dl, dp: s.dp };
            let scene = app.frame(&mut cx, ecran, 0.05);
            ui.end_frame();
            scene
        };
        for _ in 0..40 {
            let sc = etape(&mut app, &mut ui, &mut text, &mut dl);
            render.draw(w, h, &sc, &dl, &mut text);
        }
        let t0 = std::time::Instant::now();
        if let Some((nom, c)) = &fichier {
            app.ouvrir(nom, c);
        }
        let ms_ouvrir = t0.elapsed().as_secs_f64() * 1e3;
        if s.nom == "texture" || s.nom == "uv" || s.nom.starts_with("materiaux") {
            // Quadrants rouge, vert, bleu, et un quart transparent : ni retournée ni écrasée
            app.ouvrir("cube-texture.glb", include_bytes!("../tests/donnees/cube-texture.glb"));
        }
        if s.nom.starts_with("armature") {
            // Un humanoïde et, à côté, un cube, estompé en mode Armature
            let mut p = Projet::neuf();
            p.objets[0].position = [1.3, 0.5, 0.0];
            app.remplacer(p, "");
            if s.nom == "armature-vide" {
                app.entrer_armature();
            } else {
                app.ajouter_armature("humanoide", rustcraft::armature::Armature::humanoide());
            }
            if s.nom != "armature" && s.nom != "armature-vide" {
                app.entrer_geometrie();
                let k = app.os.as_ref().unwrap().objet.unwrap();
                let a = app.projet.objets[k].armature.clone().unwrap();
                let id = a.os.iter().find(|b| b.nom == "bras.G").unwrap().id.clone();
                let e = app.os.as_mut().unwrap();
                e.actif = Some(id.clone());
                e.selectionner(&a, &[id], false);
            }
        }
        if s.nom.starts_with("liaison") {
            app.ajouter_armature("corps", rustcraft::armature::Armature::humanoide());
            app.projet.objets[0].position = [1.3, 0.5, 0.0];
            app.choisir(Some(0));
            app.ouvrir_liaison();
            let bras: Vec<String> = app.projet.objets[1].armature.as_ref().unwrap().os.iter().filter(|b| b.nom.contains("bras")).map(|b| b.id.clone()).collect();
            app.liaison.as_mut().unwrap().os = bras;
            if s.nom == "liaison-fiche" {
                app.lier();
            }
            app.recadrer();
        }
        if s.nom.starts_with("pose") {
            // Un bloc tenu par l'avant-bras gauche, levé
            app.ajouter_armature("corps", rustcraft::armature::Armature::humanoide());
            let k = app.projet.objets.len() - 1;
            let a = app.projet.objets[k].armature.clone().unwrap();
            let os = |n: &str| a.os.iter().find(|b| b.nom == n).unwrap().id.clone();
            let main = a.segments()[&os("main.G")].tete;
            let id = app.projet.objets[k].id.clone();
            let o = &mut app.projet.objets[0];
            o.position = main;
            o.echelle = [0.12, 0.12, 0.12];
            o.liaison = Some(rustcraft::projet::Liaison { armature: id, os: vec![os("avant-bras.G")] });
            let q = rustcraft::edition::qaxe([0.0, 0.0, 1.0], 1.2);
            app.projet.objets[k].armature.as_mut().unwrap().tourner(&[os("bras.G")], q);
            app.projet.objets[k].armature.as_mut().unwrap().tourner(&[os("avant-bras.G")], q);
            app.redessiner_armatures();
            app.choisir(Some(k));
            if s.nom == "pose" {
                app.entrer_armature();
                app.basculer_pose();
                let e = app.os.as_mut().unwrap();
                e.actif = Some(os("avant-bras.G"));
                e.selectionner(&a, &[os("avant-bras.G")], false);
                app.contenu = Contenu::Os;
            }
            app.recadrer();
        }
        if s.nom == "devant" {
            // Un humanoïde enfermé dans un bloc, montré au premier plan
            app.ajouter_armature("corps", rustcraft::armature::Armature::humanoide());
            let k = app.projet.objets.len() - 1;
            let b = app.projet.objets[k].maillage.boite_englobante().unwrap();
            let p = app.projet.objets[k].position;
            let o = &mut app.projet.objets[0];
            o.maillage = std::sync::Arc::new(rustcraft::maillage::Maillage::cube(1.0));
            o.position = [0, 1, 2].map(|c| p[c] + (b.min[c] + b.max[c]) / 2.0);
            o.rotation = [0.0, 0.0, 0.0, 1.0];
            o.echelle = [0, 1, 2].map(|c| (b.max[c] - b.min[c]) * 1.3);
            app.projet.objets[k].armature.as_mut().unwrap().regler_devant(true);
            app.choisir(Some(k));
            app.recadrer();
        }
        if s.nom.starts_with("peau") {
            // Un tube de l'épaule à la main, qui plie au coude
            app.ajouter_armature("corps", rustcraft::armature::Armature::humanoide());
            let k = app.projet.objets.len() - 1;
            let a = app.projet.objets[k].armature.clone().unwrap();
            let os = |n: &str| a.os.iter().find(|b| b.nom == n).unwrap().id.clone();
            let seg = a.segments();
            let (d, f) = (seg[&os("bras.G")].tete, seg[&os("avant-bras.G")].queue);
            let (anneaux, cotes) = (40usize, 16usize);
            let mut sommets = Vec::new();
            let mut faces = Vec::new();
            for r in 0..=anneaux {
                let t = r as f64 / anneaux as f64;
                for c in 0..cotes {
                    let an = c as f64 / cotes as f64 * std::f64::consts::TAU;
                    sommets.push([d[0] + (f[0] - d[0]) * t, d[1] + (f[1] - d[1]) * t + 0.05 * an.cos(), d[2] + (f[2] - d[2]) * t + 0.05 * an.sin()]);
                }
            }
            for r in 0..anneaux {
                for c in 0..cotes {
                    let n = (c + 1) % cotes;
                    faces.push(rustcraft::maillage::Face { s: vec![r * cotes + (c), r * cotes + n, (r + 1) * cotes + n, (r + 1) * cotes + c], m: rustcraft::maillage::defaut(), uv: Vec::new() });
                }
            }
            let id = app.projet.objets[k].id.clone();
            let o = &mut app.projet.objets[0];
            o.position = [0.0; 3];
            o.maillage = Arc::new(Maillage::new(sommets, faces, Vec::new()));
            o.liaison = Some(rustcraft::projet::Liaison { armature: id, os: vec![os("bras.G"), os("avant-bras.G")] });
            let q = rustcraft::edition::qaxe([0.0, 0.0, 1.0], 1.3);
            app.projet.objets[k].armature.as_mut().unwrap().tourner(&[os("avant-bras.G")], q);
            app.redessiner_armatures();
            app.choisir(Some(k));
            if s.nom == "peau-poids" {
                app.entrer_armature();
                app.basculer_pose();
                app.voir_poids = true;
                let e = app.os.as_mut().unwrap();
                e.actif = Some(os("avant-bras.G"));
                e.selectionner(&a, &[os("avant-bras.G")], false);
            }
            app.attendre_calculs();
            app.cadrer_cible([0.6, 1.4, 0.0], 1.6);
        }
        if s.nom == "modifieurs" {
            // Un bloc percé par un cylindre, cible montrée en fil de fer
            let mut p = Projet::neuf();
            p.objets[0].nom = "bloc".into();
            p.objets[0].maillage = Arc::new(Maillage::cube(2.0));
            p.objets[0].modifs = rustcraft::json::lire(r#"[{"type":"booleen","op":"difference","cible":"percage","actif":true}]"#).unwrap();
            let mut o = p.objets[0].clone();
            o.nom = "percage".into();
            o.maillage = Arc::new(Maillage::cylindre(0.5, 3.0, 32, false));
            o.modifs = rustcraft::json::J::Liste(Vec::new());
            o.position = [0.3, 0.0, 0.3];
            p.objets.push(o);
            app.remplacer(p, "percage");
            app.contenu = Contenu::Modifieurs;
        }
        app.choisir(s.choix.filter(|k| *k < app.projet.objets.len()));
        if s.nom.starts_with("armature") && s.nom != "armature-vide" {
            app.recadrer();
        }
        if s.nom.starts_with("vis") {
            app.ouvrir_ajout(rustcraft::primitives::Prim::Vis);
        }
        if s.nom.starts_with("sets") {
            let o = app.projet.objets[0].clone();
            for (n, set) in [("roue avant", "véhicule"), ("roue arrière", "véhicule"), ("arbre", "décor très long à lire dans la liste")] {
                let mut x = o.clone();
                x.nom = n.into();
                x.set = set.into();
                app.projet.objets.push(x);
            }
            for n in ["véhicule", "décor très long à lire dans la liste", "vide"] {
                app.projet.sets.push(rustcraft::projet::Set { nom: n.into(), visible: n != "vide", replie: false });
            }
            if s.nom == "sets-deplacer" {
                app.page_liste = rustcraft::editeur::PageListe::Deplacer(vec![1, 2]);
            }
        }
        if s.nom.starts_with("materiaux") {
            app.contenu = Contenu::Scene;
            app.onglet_scene = 1;
        }
        if s.nom.starts_with("uv") {
            if s.nom != "uv" {
                let mut p = Projet::neuf();
                p.objets[0].nom = "forme".into();
                p.objets[0].maillage = Arc::new(if s.nom == "uv-sphere" || s.nom.starts_with("uv-page") || s.nom == "uv-deplie" { Maillage::sphere_uv(1.0, 32, 16) } else { Maillage::boite(2.0, 1.0, 1.0) });
                app.remplacer(p, "projection");
                app.choisir(Some(0));
            }
            app.entrer_geometrie();
            match s.nom {
                "uv-sphere" => app.projeter(rustcraft::uv::Projection::Sphere { axe: 1 }),
                "uv-cube" => app.projeter(rustcraft::uv::Projection::Cube),
                "uv-deplie" => {
                    let t = app.travail.as_mut().unwrap();
                    let (m, mut aretes) = (&t.m, Vec::new());
                    for e in m.aretes() {
                        let (p, q) = (m.sommets[e[0]], m.sommets[e[1]]);
                        if (p[2].abs() < 1e-6 && p[0] >= -1e-6 && q[2].abs() < 1e-6 && q[0] >= -1e-6) || (p[1].abs() < 1e-6 && q[1].abs() < 1e-6) {
                            aretes.push(e);
                        }
                    }
                    t.sel.aretes = aretes;
                    t.marquer_coutures(true).unwrap();
                    t.sel.aretes.clear();
                    let t0 = std::time::Instant::now();
                    app.deplier();
                    println!("dépliage : {:.0} ms, {} faces", t0.elapsed().as_secs_f64() * 1e3, app.travail.as_ref().unwrap().m.faces.len());
                }
                "uv-page" | "uv-page-telephone" => {
                    app.projeter(rustcraft::uv::Projection::Sphere { axe: 1 });
                    let t = app.travail.as_mut().unwrap();
                    t.sel.faces = (0..t.m.faces.len() / 2).collect();
                    app.projeter(rustcraft::uv::Projection::Cube);
                    app.travail.as_mut().unwrap().sel.faces.clear();
                    app.mettre_en_page();
                }
                _ => app.ouvrir_uv(),
            }
        }
        if s.nom.starts_with("armature") || s.nom == "pose" || s.nom.starts_with("sets") || s.nom == "modifieurs" || s.nom.starts_with("vis") || s.nom.starts_with("uv") || s.nom.starts_with("materiaux") {
            let t0 = std::time::Instant::now();
            let sc = etape(&mut app, &mut ui, &mut text, &mut dl);
            render.draw(w, h, &sc, &dl, &mut text);
            app.attendre_calculs();
            println!("{} : calcul {:.0} ms", s.nom, t0.elapsed().as_secs_f64() * 1e3);
            for _ in 0..30 {
                let sc = etape(&mut app, &mut ui, &mut text, &mut dl);
                render.draw(w, h, &sc, &dl, &mut text);
            }
        }
        if s.geo > 0 {
            app.entrer_geometrie();
            let t = app.travail.as_mut().unwrap();
            match s.geo {
                1 => (0..3.min(t.m.faces.len())).for_each(|f| t.basculer(El::F(f))),
                2 => {
                    t.tout(T::S);
                    app.rayons = true;
                }
                _ => t.tout(T::A),
            }
        }
        if s.pave {
            app.saisir(if s.nom == "pave" { Champ::Position(0) } else { Champ::Nom });
        }
        let t0 = std::time::Instant::now();
        let sc = etape(&mut app, &mut ui, &mut text, &mut dl);
        let ms_tampons = t0.elapsed().as_secs_f64() * 1e3;
        let t0 = std::time::Instant::now();
        render.draw(w, h, &sc, &dl, &mut text);
        let gl = render.gl();
        let mut px = vec![0u8; (s.w * s.h * 4) as usize];
        unsafe {
            (gl.glReadPixels)(0, 0, s.w, s.h, 0x1908, 0x1401, px.as_mut_ptr() as *mut _);
            let e = (gl.glGetError)();
            println!("{} : {} objets, ouvrir {ms_ouvrir:.0} ms, tampons {ms_tampons:.0} ms, dessin {:.1} ms, erreur GL {e:#x}", s.nom, sc.objets.len(), t0.elapsed().as_secs_f64() * 1e3);
        }
        let mut out = format!("P6\n{} {}\n255\n", s.w, s.h).into_bytes();
        for y in (0..s.h as usize).rev() {
            for x in 0..s.w as usize {
                out.extend_from_slice(&px[(y * s.w as usize + x) * 4..][..3]);
            }
        }
        std::fs::write(format!("{dir}/{}.ppm", s.nom), out).unwrap();
    }
}
