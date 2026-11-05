//! Prépare pour S.A.G.E. un personnage dont le squelette (les os de hb-01, mêmes noms)
//! a été posé sur son corps : bras baissés, tourné vers où regarde le corps.
//!
//!   cargo run --release --example adapter_personnage -- <personnage.gda> <hb-01.gda> <sortie.gda> [repos°] [marche°]
//!
//! 1. le personnage est tourné face à +z, comme S.A.G.E. l'attend ;
//! 2. il reprend de hb-01 ses poses, animations, masques, mélanges et limites, et ses
//!    pieds s'écartent de `repos°` au repos (3° par défaut), de `marche°` en marche et en
//!    course (-4° par défaut : ils se rapprochent) ;
//! 3. son corps est lié à l'armature, poids calculés bras écartés (`calculer_dans_la_pose`) ;
//! 4. le projet est écrit, et le `.glb` pour S.A.G.E. à côté (en couches et aplati).

use gda::armature::{Armature, Ecart, Pose};
use gda::json;
use gda::quat::{qappliquer, qaxe, qinv, qmul, qnorm};
use rustcraft::projet::Liaison;
use rustcraft::sage::Couches;
use std::sync::Arc;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (perso, source, sortie) = (&a[1], &a[2], &a[3]);
    let degres = |i: usize, d: f64| a.get(i).map_or(d, |x| x.parse().expect("un angle en degrés"));
    let (ecart_repos, ecart_marche) = (degres(4, 3.0), degres(5, -4.0));
    let mut app = rustcraft::editeur::App::new(1.0);
    let nom = perso.rsplit('/').next().unwrap().to_string();
    app.ouvrir(&nom, &std::fs::read(perso).unwrap());

    // La source : l'armature de hb-01, avec ses animations
    let t = std::fs::read_to_string(source).unwrap();
    let j = json::lire(&t).unwrap();
    let hb = j.get("scene").unwrap().get("objets").unwrap().liste().unwrap().iter().find_map(|o| o.get("armature")).expect("une armature dans la source").clone();
    let hb = Armature::depuis_donnees(&hb);

    let p = &mut app.projet;
    let k = p.objets.iter().position(|o| o.armature.is_some()).expect("une armature dans le personnage");
    for o in &p.objets {
        assert!(o.position == [0.0; 3] && o.rotation == [0.0, 0.0, 0.0, 1.0] && o.echelle == [1.0; 3], "« {} » est déplacé : appliquez sa transformation d'abord", o.nom);
    }
    let mut arm = p.objets[k].armature.clone().unwrap();

    // 1. Face à +z : la gauche est de clavicule.D vers clavicule.G
    let repos = arm.repos();
    let pos = |n: &str| repos[&arm.os.iter().find(|b| b.nom == n).unwrap_or_else(|| panic!("pas d'os « {n} »")).id].0;
    let (g, d) = (pos("clavicule.G"), pos("clavicule.D"));
    let gauche = [g[0] - d[0], 0.0, g[2] - d[2]];
    let devant = [-gauche[2], 0.0, gauche[0]]; // gauche × haut
    let f = qaxe([0.0, 1.0, 0.0], -devant[0].atan2(devant[2]));
    println!("devant {:?} → {:?}", devant, qappliquer(f, devant));
    for b in arm.os.iter_mut().filter(|b| b.parent.is_none()) {
        b.position = qappliquer(f, b.position);
        b.rotation = qnorm(qmul(f, b.rotation));
    }
    let corps: Vec<usize> = (0..p.objets.len()).filter(|&i| p.objets[i].armature.is_none()).collect();
    for &i in &corps {
        let mut m = (*p.objets[i].maillage).clone();
        for s in &mut m.sommets {
            *s = qappliquer(f, *s);
        }
        p.objets[i].maillage = Arc::new(m);
    }

    // 2. Les animations de hb-01, converties à ce repos
    let r = arm.reprendre(&hb).unwrap();
    println!("reprise : {} os pareils, {} vrillés, {} poses, {} animations, seuls {:?}", r.os, r.vrilles, r.poses, r.animations, r.seuls);

    // L'écart des pieds : selon le nom, repos ou allure ; une animation additive n'a que
    // des écarts à sa référence, elle reste telle quelle
    let ecart = |nom: &str| {
        let n = nom.to_lowercase();
        if ["marche", "course", "walk", "run"].iter().any(|m| n.contains(m)) {
            ecart_marche
        } else if ["repos", "idle"].iter().any(|m| n.contains(m)) {
            ecart_repos
        } else {
            0.0
        }
    };
    let mut poses = arm.poses_nommees();
    for p in &mut poses {
        gda::ik::ecarter_jambes(&arm, &mut p.pose, ecart(&p.nom));
    }
    arm.regler_poses(&poses);
    let mut anims = arm.animations();
    for an in anims.iter_mut().filter(|an| an.mode != gda::animation::Mode::Additif) {
        let e = ecart(&an.nom);
        for k in &mut an.cles {
            gda::ik::ecarter_jambes(&arm, &mut k.pose, e);
        }
        println!("« {} » : pieds écartés de {e}°", an.nom);
    }
    arm.regler_animations(&anims);

    // 3. La croix de hb-01 vue par ce squelette : les poids s'y calculent, bras écartés
    let croix = arm.pose_de(&hb, &Pose::new());
    let mut x = arm.clone();
    x.pose = croix.clone();
    let mut x = x.figee();
    // Pose qui ramène la croix au repos du personnage
    x.pose = croix.iter().map(|(id, e)| (id.clone(), Ecart { r: qinv(e.r), t: [0.0; 3] })).collect();
    let ids: Vec<String> = arm.os.iter().map(|b| b.id.clone()).collect();
    let id_arm = p.objets[k].id.clone();
    for &i in &corps {
        let m = p.objets[i].maillage.clone();
        let peau = gda::peau::calculer_dans_la_pose(&m.sommets, &m, &x, &ids);
        // Vérification : les poids redéforment la croix exactement en ce corps
        let n = m.sommets.len();
        p.objets[i].liaison = Some(Liaison { armature: id_arm.clone(), os: ids.clone(), poids: Some(Arc::new(gda::peau::Main::depuis_peau(&peau, n))) });
        println!("« {} » lié : {n} sommets", p.objets[i].nom);
    }
    p.objets[k].maillage = Arc::new(arm.enveloppe());
    p.objets[k].armature = Some(arm);
    std::fs::write(sortie, p.ecrire()).unwrap();
    println!("{sortie}");

    // 4. Le .glb pour S.A.G.E.
    app.choisir(Some(k));
    app.attendre_calculs();
    for (c, suffixe) in [(Couches::EnCouches, "couches"), (Couches::Aplaties, "aplati")] {
        app.exporter_sage(c);
        for s in app.prendre_sorties() {
            let chemin = format!("{}-{suffixe}.glb", sortie.trim_end_matches(".gda"));
            std::fs::write(&chemin, &s.octets).unwrap();
            println!("{chemin} : {} Ko", s.octets.len() / 1024);
        }
    }
}
