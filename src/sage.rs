//! Export vers S.A.G.E. : une armature et les objets qui lui sont liés, en un seul .glb
//! que tout moteur lit. Un nœud par os (repos), une peau (JOINTS_0, WEIGHTS_0, matrices
//! de liaison inverses), une animation glTF par animation de l'armature. Ce que glTF ne
//! sait pas dire (longueur et limites des os, ancre, poses nommées, boucle) va dans les
//! `extras`, que S.A.G.E. relira.

use crate::animation::Animation;
use crate::armature::{Armature, Ecart, Pose};
use crate::edition::{qappliquer, qinv, qmul, Q};
use crate::json::{self, J};
use crate::maillage::{glb_conteneur, materiau_gltf, Maillage, MatMtl, P3};
use crate::peau::{Peau, INFLUENCES};
use std::sync::Arc;

/// Un objet lié : son maillage dans le repère de l'armature au repos, et ses poids.
pub struct Piece {
    pub nom: String,
    pub maillage: Maillage,
    pub peau: Peau,
}

/// Les données binaires, leurs vues et leurs accesseurs.
#[derive(Default)]
struct Tampon {
    bin: Vec<u8>,
    vues: Vec<String>,
    acces: Vec<String>,
}

impl Tampon {
    fn vue(&mut self, octets: &[u8], cible: Option<u32>) -> usize {
        self.bin.resize((self.bin.len() + 3) & !3, 0);
        let c = cible.map_or(String::new(), |c| format!(",\"target\":{c}"));
        self.vues.push(format!("{{\"buffer\":0,\"byteOffset\":{},\"byteLength\":{}{c}}}", self.bin.len(), octets.len()));
        self.bin.extend_from_slice(octets);
        self.vues.len() - 1
    }

    /// Accesseur de flottants groupés par `n` (1, 2, 3, 4 ou 16).
    fn flottants(&mut self, v: &[f32], n: usize, cible: Option<u32>, bornes: bool) -> usize {
        let octets: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let vue = self.vue(&octets, cible);
        let t = match n { 1 => "SCALAR", 2 => "VEC2", 3 => "VEC3", 4 => "VEC4", _ => "MAT4" };
        let mut b = String::new();
        if bornes && !v.is_empty() {
            let (mut mn, mut mx) = (vec![f32::MAX; n], vec![f32::MIN; n]);
            for c in v.chunks(n) {
                for k in 0..n {
                    mn[k] = mn[k].min(c[k]);
                    mx[k] = mx[k].max(c[k]);
                }
            }
            let l = |x: &[f32]| x.iter().map(|v| json::nombre(*v as f64)).collect::<Vec<_>>().join(",");
            b = format!(",\"min\":[{}],\"max\":[{}]", l(&mn), l(&mx));
        }
        self.acces.push(format!("{{\"bufferView\":{vue},\"componentType\":5126,\"count\":{},\"type\":\"{t}\"{b}}}", v.len() / n));
        self.acces.len() - 1
    }

    fn entiers(&mut self, v: &[u32], cible: Option<u32>) -> usize {
        let octets: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let vue = self.vue(&octets, cible);
        self.acces.push(format!("{{\"bufferView\":{vue},\"componentType\":5125,\"count\":{},\"type\":\"SCALAR\"}}", v.len()));
        self.acces.len() - 1
    }

    fn articulations(&mut self, v: &[u16]) -> usize {
        let octets: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let vue = self.vue(&octets, Some(34962));
        self.acces.push(format!("{{\"bufferView\":{vue},\"componentType\":5123,\"count\":{},\"type\":\"VEC4\"}}", v.len() / 4));
        self.acces.len() - 1
    }
}

fn txt(s: &str) -> String {
    let mut o = String::new();
    json::texte(s, &mut o);
    o
}

fn nums(v: &[f64]) -> String {
    format!("[{}]", v.iter().map(|x| json::nombre(*x as f32 as f64)).collect::<Vec<_>>().join(","))
}

/// Matrice 4×4 (colonnes d'abord) d'une position et d'une rotation.
pub fn matrice(p: P3, q: Q) -> [f64; 16] {
    let c = [0, 1, 2].map(|j| qappliquer(q, [0, 1, 2].map(|i| if i == j { 1.0 } else { 0.0 })));
    [c[0][0], c[0][1], c[0][2], 0.0, c[1][0], c[1][1], c[1][2], 0.0, c[2][0], c[2][1], c[2][2], 0.0, p[0], p[1], p[2], 1.0]
}

/// L'inverse d'un repère rigide.
pub fn inverse_rigide(p: P3, q: Q) -> [f64; 16] {
    let qi = qinv(q);
    let t = qappliquer(qi, p);
    matrice([-t[0], -t[1], -t[2]], qi)
}

/// Le repos local d'un os, puis l'écart de la pose : ce que deviennent les TRS du nœud.
fn local(a: &Armature, id: &str, e: Option<&Ecart>) -> (P3, Q) {
    let b = a.trouver(id).expect("os");
    match e {
        Some(e) => ([0, 1, 2].map(|k| b.position[k] + e.t[k]), qmul(b.rotation, e.r)),
        None => (b.position, b.rotation),
    }
}

/// Les clés d'une animation à exporter : celles de l'animation, plus le début et la fin
/// de sa durée s'il n'y en a pas, pour que le clip couvre toute la durée.
fn instants(an: &Animation) -> Vec<(f64, Pose)> {
    let mut l: Vec<(f64, Pose)> = an.cles.iter().map(|c| (c.t, c.pose.clone())).collect();
    if l.first().is_none_or(|c| c.0 > 1e-6) {
        l.insert(0, (0.0, an.pose_a(0.0)));
    }
    if l.last().is_some_and(|c| c.0 < an.duree - 1e-6) {
        l.push((an.duree, an.pose_a(an.duree)));
    }
    l
}

fn extras_os(a: &Armature, id: &str) -> String {
    let b = a.trouver(id).expect("os");
    let mut x = format!("\"longueur\":{}", json::nombre(b.longueur));
    if let Some((_, c)) = b.extra.iter().find(|(k, _)| k == "contraintes") {
        let mut o = String::new();
        json::compact(c, &mut o);
        x.push_str(&format!(",\"contraintes\":{o}"));
    }
    format!("{{{x}}}")
}

/// Poses nommées et ancre, par nom d'os.
fn extras_armature(a: &Armature, animations: &[Animation]) -> String {
    let nom = |id: &str| a.trouver(id).map_or(id.to_string(), |b| b.nom.clone());
    let pose_j = |p: &Pose| {
        let l: Vec<String> = a.os.iter().filter_map(|b| {
            let e = p.get(&b.id)?;
            Some(format!("{}:{{\"r\":{},\"t\":{}}}", txt(&b.nom), nums(&e.r), nums(&e.t)))
        }).collect();
        format!("{{{}}}", l.join(","))
    };
    let poses: Vec<String> = a.poses_nommees().iter().map(|(n, p)| format!("{{\"nom\":{},\"os\":{}}}", txt(n), pose_j(p))).collect();
    let ancre = a.ancre().map_or("null".to_string(), |i| txt(&nom(&i)));
    format!("{{\"sage\":{{\"format\":1,\"ancre\":{ancre},\"poses\":[{}],\"animations\":{}}}}}", poses.join(","), animations.len())
}

/// Le .glb d'une armature (`nom`, placée par `place` : position, rotation, échelle) et de
/// ses pièces, avec les matériaux et images de l'export ordinaire.
pub fn glb(nom: &str, a: &Armature, place: (P3, Q, f64), pieces: &[Piece], mats: &[(String, MatMtl)], images: &[(String, Arc<Vec<u8>>)]) -> Vec<u8> {
    let mut t = Tampon::default();
    let rang = |id: &str| a.os.iter().position(|b| b.id == id);
    // Nœud 0 : l'armature ; puis un nœud par os, dans l'ordre de l'armature
    let mut noeuds = Vec::new();
    let racines: Vec<String> = a.os.iter().enumerate().filter(|(_, b)| b.parent.as_ref().is_none_or(|p| rang(p).is_none())).map(|(k, _)| (k + 1).to_string()).collect();
    let (p, q, s) = place;
    noeuds.push(format!(
        "{{\"name\":{},\"translation\":{},\"rotation\":{},\"scale\":{},\"children\":[{}],\"extras\":{}}}",
        txt(nom), nums(&p), nums(&q), nums(&[s, s, s]), racines.join(","), extras_armature(a, &a.animations())
    ));
    for b in &a.os {
        let enfants: Vec<String> = a.os.iter().enumerate().filter(|(_, c)| c.parent.as_deref() == Some(&b.id)).map(|(k, _)| (k + 1).to_string()).collect();
        let e = if enfants.is_empty() { String::new() } else { format!(",\"children\":[{}]", enfants.join(",")) };
        noeuds.push(format!("{{\"name\":{},\"translation\":{},\"rotation\":{}{e},\"extras\":{}}}", txt(&b.nom), nums(&b.position), nums(&b.rotation), extras_os(a, &b.id)));
    }
    // La peau : une matrice de liaison inverse par os, son repos dans l'armature
    let repos = a.repos();
    let ibm: Vec<f32> = a.os.iter().flat_map(|b| {
        let (p, q) = repos[&b.id];
        inverse_rigide(p, q).map(|x| x as f32)
    }).collect();
    let acc_ibm = t.flottants(&ibm, 16, None, false);
    let joints: Vec<String> = (1..=a.os.len()).map(|k| k.to_string()).collect();
    // Les matériaux, partagés entre les pièces
    let mut noms_mats: Vec<String> = Vec::new();
    let mut l_mats: Vec<String> = Vec::new();
    let mut meshes = Vec::new();
    let mut scene = vec!["0".to_string()];
    for piece in pieces {
        let a_uv = piece.maillage.faces.iter().any(|f| !f.uv.is_empty());
        let (m, uv, origine) = if a_uv {
            piece.maillage.deplie_origine()
        } else {
            (piece.maillage.clone(), Vec::new(), (0..piece.maillage.sommets.len()).collect())
        };
        let pos: Vec<f32> = m.sommets.iter().flat_map(|p| p.map(|c| c as f32)).collect();
        let acc_pos = t.flottants(&pos, 3, Some(34962), true);
        let acc_uv = (!uv.is_empty()).then(|| t.flottants(&uv.concat(), 2, Some(34962), false));
        // Os et poids de chaque sommet, rangés comme les nœuds
        let (mut js, mut ws) = (Vec::with_capacity(origine.len() * 4), Vec::with_capacity(origine.len() * 4));
        for &i in &origine {
            let mut paire: Vec<(u16, f32)> = (0..INFLUENCES)
                .map(|k| (piece.peau.indices.get(i * INFLUENCES + k).copied().unwrap_or(0) as usize, piece.peau.poids.get(i * INFLUENCES + k).copied().unwrap_or(0.0)))
                .filter(|x| x.1 > 0.0)
                .filter_map(|(j, w)| Some((rang(piece.peau.ids.get(j)?)? as u16, w)))
                .collect();
            if paire.is_empty() {
                paire.push((piece.peau.ids.first().and_then(|i| rang(i)).unwrap_or(0) as u16, 1.0));
            }
            let somme: f32 = paire.iter().map(|x| x.1).sum();
            paire.resize(4, (0, 0.0));
            for (j, w) in paire {
                js.push(j);
                ws.push(w / somme);
            }
        }
        let acc_j = t.articulations(&js);
        let acc_w = t.flottants(&ws, 4, Some(34962), false);
        let mut prims = Vec::new();
        for (mat, l) in m.triangles().into_iter().filter(|(_, l)| !l.is_empty()) {
            let k = match noms_mats.iter().position(|n| **n == *mat) {
                Some(k) => k,
                None => {
                    let d = mats.iter().find(|(n, _)| **n == *mat).map(|(_, d)| d.clone()).unwrap_or_default();
                    let image = d.texture.as_ref().and_then(|x| images.iter().position(|(n, _)| n == x)).filter(|_| a_uv);
                    l_mats.push(materiau_gltf(&mat, &d, image));
                    noms_mats.push(mat.to_string());
                    noms_mats.len() - 1
                }
            };
            let idx: Vec<u32> = l.iter().map(|i| *i as u32).collect();
            let acc_i = t.entiers(&idx, Some(34963));
            let uv_j = acc_uv.map_or(String::new(), |u| format!(",\"TEXCOORD_0\":{u}"));
            prims.push(format!("{{\"attributes\":{{\"POSITION\":{acc_pos}{uv_j},\"JOINTS_0\":{acc_j},\"WEIGHTS_0\":{acc_w}}},\"indices\":{acc_i},\"material\":{k},\"mode\":4}}"));
        }
        meshes.push(format!("{{\"name\":{},\"primitives\":[{}]}}", txt(&piece.nom), prims.join(",")));
        scene.push(noeuds.len().to_string());
        noeuds.push(format!("{{\"name\":{},\"mesh\":{},\"skin\":0}}", txt(&piece.nom), meshes.len() - 1));
    }
    // Animations : un canal de rotation par os qui bouge, de translation s'il glisse
    let mut anims = Vec::new();
    for an in a.animations() {
        let cles = instants(&an);
        let temps: Vec<f32> = cles.iter().map(|c| c.0 as f32).collect();
        let acc_t = t.flottants(&temps, 1, None, true);
        let (mut samplers, mut canaux) = (Vec::new(), Vec::new());
        for (k, b) in a.os.iter().enumerate() {
            if cles.iter().all(|c| !c.1.contains_key(&b.id)) {
                continue;
            }
            let locaux: Vec<(P3, Q)> = cles.iter().map(|c| local(a, &b.id, c.1.get(&b.id))).collect();
            let r: Vec<f32> = locaux.iter().flat_map(|x| x.1.map(|v| v as f32)).collect();
            let acc_r = t.flottants(&r, 4, None, false);
            samplers.push(format!("{{\"input\":{acc_t},\"output\":{acc_r},\"interpolation\":\"LINEAR\"}}"));
            canaux.push(format!("{{\"sampler\":{},\"target\":{{\"node\":{},\"path\":\"rotation\"}}}}", samplers.len() - 1, k + 1));
            if cles.iter().any(|c| c.1.get(&b.id).is_some_and(|e| e.t.iter().any(|v| v.abs() > 1e-12))) {
                let p: Vec<f32> = locaux.iter().flat_map(|x| x.0.map(|v| v as f32)).collect();
                let acc_p = t.flottants(&p, 3, None, false);
                samplers.push(format!("{{\"input\":{acc_t},\"output\":{acc_p},\"interpolation\":\"LINEAR\"}}"));
                canaux.push(format!("{{\"sampler\":{},\"target\":{{\"node\":{},\"path\":\"translation\"}}}}", samplers.len() - 1, k + 1));
            }
        }
        if canaux.is_empty() {
            continue;
        }
        anims.push(format!(
            "{{\"name\":{},\"samplers\":[{}],\"channels\":[{}],\"extras\":{{\"boucle\":{},\"duree\":{}}}}}",
            txt(&an.nom), samplers.join(","), canaux.join(","), an.boucle, json::nombre(an.duree)
        ));
    }
    // Les images, après le reste
    let mut suite = String::new();
    if !images.is_empty() && pieces.iter().any(|p| p.maillage.faces.iter().any(|f| !f.uv.is_empty())) {
        let (mut li, mut lt) = (Vec::new(), Vec::new());
        for (k, (n, o)) in images.iter().enumerate() {
            let v = t.vue(o, None);
            li.push(format!("{{\"name\":{},\"bufferView\":{v},\"mimeType\":\"{}\"}}", txt(n), crate::texture::mime(o)));
            lt.push(format!("{{\"source\":{k}}}"));
        }
        suite = format!(",\"images\":[{}],\"textures\":[{}]", li.join(","), lt.join(","));
    }
    if !anims.is_empty() {
        suite.push_str(&format!(",\"animations\":[{}]", anims.join(",")));
    }
    if !l_mats.is_empty() {
        suite.push_str(&format!(",\"materials\":[{}]", l_mats.join(",")));
    }
    if !meshes.is_empty() {
        suite.push_str(&format!(",\"meshes\":[{}]", meshes.join(",")));
    }
    t.bin.resize((t.bin.len() + 3) & !3, 0);
    let js = format!(
        "{{\"asset\":{{\"version\":\"2.0\",\"generator\":{}}},\"scene\":0,\"scenes\":[{{\"nodes\":[{}]}}],\"nodes\":[{}],\"skins\":[{{\"name\":{},\"joints\":[{}],\"inverseBindMatrices\":{acc_ibm},\"skeleton\":0}}],\"buffers\":[{{\"byteLength\":{}}}],\"bufferViews\":[{}],\"accessors\":[{}]{suite}}}",
        txt(crate::maillage::SIGNATURE), scene.join(","), noeuds.join(","), txt(nom), joints.join(","), t.bin.len(), t.vues.join(","), t.acces.join(",")
    );
    glb_conteneur(&js, &t.bin)
}

/// Lecture d'un .glb : son JSON et son binaire. Pour les vérifications.
pub fn lire_glb(d: &[u8]) -> Option<(J, Vec<u8>)> {
    let u = |o: usize| -> Option<usize> { Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize) };
    let lj = u(12)?;
    let js = std::str::from_utf8(d.get(20..20 + lj)?).ok()?;
    let j = json::lire(js.trim_end())?;
    let lb = u(20 + lj)?;
    let bin = d.get(28 + lj..28 + lj + lb)?.to_vec();
    Some((j, bin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edition::qaxe;
    use crate::maillage::{defaut, Face};
    use std::collections::HashMap;

    /// Un accesseur du .glb, en nombres.
    fn lire_acces(j: &J, bin: &[u8], k: usize) -> Vec<f64> {
        let a = &j.get("accessors").unwrap().liste().unwrap()[k];
        let v = &j.get("bufferViews").unwrap().liste().unwrap()[a.get("bufferView").unwrap().num().unwrap() as usize];
        let debut = v.get("byteOffset").and_then(J::num).unwrap_or(0.0) as usize;
        let n = a.get("count").unwrap().num().unwrap() as usize;
        let c = match a.get("type").unwrap().txt().unwrap() { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => 16 };
        let ct = a.get("componentType").unwrap().num().unwrap() as u32;
        (0..n * c).map(|i| match ct {
            5126 => f32::from_le_bytes(bin[debut + 4 * i..debut + 4 * i + 4].try_into().unwrap()) as f64,
            5123 => u16::from_le_bytes(bin[debut + 2 * i..debut + 2 * i + 2].try_into().unwrap()) as f64,
            _ => u32::from_le_bytes(bin[debut + 4 * i..debut + 4 * i + 4].try_into().unwrap()) as f64,
        }).collect()
    }

    fn mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
        let mut r = [0.0; 16];
        for c in 0..4 {
            for l in 0..4 {
                r[c * 4 + l] = (0..4).map(|k| a[k * 4 + l] * b[c * 4 + k]).sum();
            }
        }
        r
    }

    fn appliquer(m: &[f64; 16], p: P3) -> P3 {
        [0, 1, 2].map(|l| m[l] * p[0] + m[4 + l] * p[1] + m[8 + l] * p[2] + m[12 + l])
    }

    /// Repères des nœuds des os, sous le nœud 0 (l'armature), avec des TRS locaux donnés.
    fn globaux(j: &J, locaux: &HashMap<usize, (P3, Q)>) -> HashMap<usize, [f64; 16]> {
        let noeuds = j.get("nodes").unwrap().liste().unwrap();
        let mut r = HashMap::new();
        fn visiter(i: usize, parent: [f64; 16], noeuds: &[J], locaux: &HashMap<usize, (P3, Q)>, r: &mut HashMap<usize, [f64; 16]>) {
            let n = &noeuds[i];
            let (t, q) = locaux.get(&i).copied().unwrap_or((
                n.get("translation").and_then(J::nombres::<3>).unwrap_or([0.0; 3]),
                n.get("rotation").and_then(J::nombres::<4>).unwrap_or([0.0, 0.0, 0.0, 1.0]),
            ));
            let g = mul(&parent, &matrice(t, q));
            r.insert(i, g);
            for c in n.get("children").and_then(J::liste).unwrap_or(&[]) {
                visiter(c.num().unwrap() as usize, g, noeuds, locaux, r);
            }
        }
        let id = matrice([0.0; 3], [0.0, 0.0, 0.0, 1.0]);
        for c in noeuds[0].get("children").unwrap().liste().unwrap() {
            visiter(c.num().unwrap() as usize, id, noeuds, locaux, &mut r);
        }
        r
    }

    /// Le bras gauche de l'humanoïde : un tube de l'épaule à la main.
    fn bras() -> (Armature, Piece) {
        let mut a = Armature::humanoide();
        let id = |a: &Armature, n: &str| a.os.iter().find(|b| b.nom == n).unwrap().id.clone();
        let (b, v) = (id(&a, "bras.G"), id(&a, "avant-bras.G"));
        let s = a.segments();
        let (d, f) = (s[&b].tete, s[&v].queue);
        let (anneaux, cotes) = (12, 6);
        let mut sommets = Vec::new();
        for r in 0..=anneaux {
            let t = r as f64 / anneaux as f64;
            for c in 0..cotes {
                let an = c as f64 / cotes as f64 * std::f64::consts::TAU;
                sommets.push([d[0] + (f[0] - d[0]) * t, d[1] + (f[1] - d[1]) * t + 0.05 * an.cos(), d[2] + (f[2] - d[2]) * t + 0.05 * an.sin()]);
            }
        }
        let mut faces = Vec::new();
        for r in 0..anneaux {
            for c in 0..cotes {
                let n = (c + 1) % cotes;
                faces.push(Face { s: vec![r * cotes + c, r * cotes + n, (r + 1) * cotes + n, (r + 1) * cotes + c], m: defaut(), uv: Vec::new() });
            }
        }
        let m = Maillage::new(sommets, faces, Vec::new());
        let peau = crate::peau::calculer(&m.sommets, &m, &a, &[b.clone(), v.clone()]);
        // Une animation : le coude plie, le bassin glisse
        let mut an = Animation::neuve("salut");
        an.duree = 1.0;
        an.poser_cle(0.0, Pose::new());
        let mut p = Pose::new();
        p.insert(v.clone(), Ecart { r: qaxe([0.0, 0.0, 1.0], 1.2), t: [0.0; 3] });
        p.insert(id(&a, "bassin"), Ecart { r: [0.0, 0.0, 0.0, 1.0], t: [0.0, 0.1, 0.05] });
        an.poser_cle(0.5, p);
        a.regler_animations(&[an]);
        (a, Piece { nom: "bras".into(), maillage: m, peau })
    }

    #[test]
    fn le_personnage_se_relit_au_repos_et_dans_son_animation() {
        let (a, piece) = bras();
        let d = glb("corps", &a, ([1.0, 0.0, 2.0], qaxe([0.0, 1.0, 0.0], 0.3), 2.0), std::slice::from_ref(&piece), &[], &[]);
        // Pour le passer au validateur de Khronos : SAGE_GLB=fichier cargo test
        if let Ok(f) = std::env::var("SAGE_GLB") {
            std::fs::write(f, &d).unwrap();
        }
        let (j, bin) = lire_glb(&d).expect("glb lisible");
        let skin = &j.get("skins").unwrap().liste().unwrap()[0];
        let joints: Vec<usize> = skin.get("joints").unwrap().liste().unwrap().iter().map(|x| x.num().unwrap() as usize).collect();
        assert_eq!(joints.len(), a.os.len());
        let ibm = lire_acces(&j, &bin, skin.get("inverseBindMatrices").unwrap().num().unwrap() as usize);
        let ibm: Vec<[f64; 16]> = ibm.chunks(16).map(|c| c.try_into().unwrap()).collect();
        // Au repos, chaque matrice de liaison inverse défait le repère de son os
        let g = globaux(&j, &HashMap::new());
        let id = matrice([0.0; 3], [0.0, 0.0, 0.0, 1.0]);
        for (k, n) in joints.iter().enumerate() {
            let m = mul(&g[n], &ibm[k]);
            assert!(m.iter().zip(id).all(|(a, b)| (a - b).abs() < 1e-5), "os {k}");
        }
        // La primitive : sommets, os et poids, autant les uns que les autres
        let prim = &j.get("meshes").unwrap().liste().unwrap()[0].get("primitives").unwrap().liste().unwrap()[0];
        let attr = prim.get("attributes").unwrap();
        let acc = |n: &str| lire_acces(&j, &bin, attr.get(n).unwrap().num().unwrap() as usize);
        let (pos, jo, w) = (acc("POSITION"), acc("JOINTS_0"), acc("WEIGHTS_0"));
        assert_eq!((pos.len() / 3, jo.len() / 4, w.len() / 4), (piece.maillage.sommets.len(), piece.maillage.sommets.len(), piece.maillage.sommets.len()));
        assert!(w.chunks(4).all(|c| (c.iter().sum::<f64>() - 1.0).abs() < 1e-5));
        let peau_glb = |g: &HashMap<usize, [f64; 16]>| -> Vec<P3> {
            (0..pos.len() / 3).map(|i| {
                let v = [pos[3 * i], pos[3 * i + 1], pos[3 * i + 2]];
                let mut r = [0.0; 3];
                for k in 0..4 {
                    let (jt, wt) = (jo[4 * i + k] as usize, w[4 * i + k]);
                    let p = appliquer(&mul(&g[&joints[jt]], &ibm[jt]), v);
                    for c in 0..3 {
                        r[c] += wt * p[c];
                    }
                }
                r
            }).collect()
        };
        let repos = peau_glb(&g);
        assert!(repos.iter().zip(&piece.maillage.sommets).all(|(a, b)| (0..3).all(|c| (a[c] - b[c]).abs() < 1e-5)), "au repos, le maillage tel quel");
        // À chaque clé, la déformation est celle de Rustcraft
        let an = &j.get("animations").unwrap().liste().unwrap()[0];
        assert_eq!(an.get("extras").unwrap().get("boucle"), Some(&J::Bool(true)));
        let samplers = an.get("samplers").unwrap().liste().unwrap();
        let temps = lire_acces(&j, &bin, samplers[0].get("input").unwrap().num().unwrap() as usize);
        assert_eq!(temps, [0.0, 0.5, 1.0], "la durée entière, fin comprise");
        let mut x = a.clone();
        let anim = &a.animations()[0];
        for (k, t) in temps.iter().enumerate() {
            let mut locaux: HashMap<usize, (P3, Q)> = HashMap::new();
            for c in an.get("channels").unwrap().liste().unwrap() {
                let s = &samplers[c.get("sampler").unwrap().num().unwrap() as usize];
                let o = lire_acces(&j, &bin, s.get("output").unwrap().num().unwrap() as usize);
                let cible = c.get("target").unwrap();
                let n = cible.get("node").unwrap().num().unwrap() as usize;
                let b = &a.os[n - 1];
                let e = locaux.entry(n).or_insert((b.position, b.rotation));
                match cible.get("path").unwrap().txt().unwrap() {
                    "rotation" => e.1 = [o[4 * k], o[4 * k + 1], o[4 * k + 2], o[4 * k + 3]],
                    _ => e.0 = [o[3 * k], o[3 * k + 1], o[3 * k + 2]],
                }
            }
            x.pose = anim.pose_a(*t);
            let attendu = piece.peau.deformer(&piece.maillage.sommets, &x);
            let obtenu = peau_glb(&globaux(&j, &locaux));
            assert!(obtenu.iter().zip(&attendu).all(|(a, b)| (0..3).all(|c| (a[c] - b[c]).abs() < 1e-4)), "à {t} s");
        }
        // Ce que glTF ne dit pas : la longueur des os, l'ancre et les poses, dans les extras
        let n1 = &j.get("nodes").unwrap().liste().unwrap()[1];
        assert!((n1.get("extras").unwrap().get("longueur").unwrap().num().unwrap() - a.os[0].longueur).abs() < 1e-12);
        assert!(j.get("nodes").unwrap().liste().unwrap()[0].get("extras").unwrap().get("sage").is_some());
    }
}
