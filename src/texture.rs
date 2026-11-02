//! Images des textures : gardées telles qu'elles arrivent (PNG ou JPEG), et décodées
//! seulement pour le processeur graphique.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

/// Plus grand côté envoyé au processeur graphique : assez pour tout appareil de l'ES 3.
const COTE_MAX: u32 = 4096;

/// Une image du projet, dans son format d'origine.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub nom: String,
    pub octets: Arc<Vec<u8>>,
}

/// Image décodée, sur quatre octets par pixel, première ligne en haut.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub l: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Identifiant du damier : n'est le contenu d'aucune image du projet.
pub const DAMIER: &str = "damier";

/// Damier de huit cases par côté : il montre l'étirement d'une projection sans texture.
pub fn damier() -> Image {
    const COTE: u32 = 256;
    let mut rgba = Vec::with_capacity((COTE * COTE * 4) as usize);
    for y in 0..COTE {
        for x in 0..COTE {
            let (cx, cy) = (x / 32, y / 32);
            let v = if (cx + cy) % 2 == 0 { 215 } else { 120 };
            // Un quart de tuile plus chaud : le haut gauche se reconnaît sur le modèle
            let c = if cx < 4 && cy < 4 { [v, v - 40, v - 70] } else { [v, v, v] };
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Image { l: COTE, h: COTE, rgba }
}

/// Identifiant tiré du contenu (FNV-1a) : une même image n'est gardée qu'une fois.
pub fn identifiant(octets: &[u8]) -> String {
    let h = octets.iter().fold(0xcbf29ce484222325u64, |h, o| (h ^ *o as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}")
}

/// Type de l'image, d'après son contenu.
pub fn mime(d: &[u8]) -> &'static str {
    if d.starts_with(&[0xFF, 0xD8]) { "image/jpeg" } else { "image/png" }
}

/// Nom de fichier de chaque texture, à l'export : sans chemin ni espace, l'extension
/// suivant le contenu, et unique même sans tenir compte de la casse.
pub fn noms_fichiers(textures: &BTreeMap<String, Texture>) -> BTreeMap<String, String> {
    let mut pris = HashSet::new();
    let mut r = BTreeMap::new();
    for (id, t) in textures {
        let n = t.nom.rsplit(['/', '\\']).next().unwrap_or("");
        let n = if est_image(n) { n.rsplit_once('.').map_or(n, |x| x.0) } else { n };
        let n = n.split_whitespace().collect::<Vec<_>>().join("_");
        let base = if n.is_empty() { "texture" } else { &n };
        let ext = if mime(&t.octets) == "image/png" { "png" } else { "jpg" };
        let mut nom = format!("{base}.{ext}");
        let mut k = 1;
        while !pris.insert(nom.to_lowercase()) {
            k += 1;
            nom = format!("{base}-{k}.{ext}");
        }
        r.insert(id.clone(), nom);
    }
    r
}

pub fn est_image(nom: &str) -> bool {
    matches!(crate::maillage::extension(nom).as_str(), "png" | "jpg" | "jpeg")
}

pub fn decoder(d: &[u8]) -> Result<Image, String> {
    let i = if d.starts_with(&[0x89, b'P', b'N', b'G']) {
        png(d)?
    } else if d.starts_with(&[0xFF, 0xD8]) {
        jpeg(d)?
    } else {
        return Err("image ni PNG ni JPEG".into());
    };
    Ok(reduire(i))
}

fn png(d: &[u8]) -> Result<Image, String> {
    use png::{ColorType, Decoder, Transformations};
    let mut p = Decoder::new(d);
    p.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut r = p.read_info().map_err(|e| format!("PNG : {e}"))?;
    let mut b = vec![0; r.output_buffer_size()];
    let i = r.next_frame(&mut b).map_err(|e| format!("PNG : {e}"))?;
    b.truncate(i.buffer_size());
    let rgba = match i.color_type {
        ColorType::Rgba => b,
        ColorType::Rgb => b.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        ColorType::GrayscaleAlpha => b.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        ColorType::Grayscale => b.iter().flat_map(|c| [*c, *c, *c, 255]).collect(),
        ColorType::Indexed => return Err("PNG : palette non développée".into()),
    };
    Ok(Image { l: i.width, h: i.height, rgba })
}

fn jpeg(d: &[u8]) -> Result<Image, String> {
    use jpeg_decoder::{Decoder, PixelFormat};
    let mut j = Decoder::new(d);
    let b = j.decode().map_err(|e| format!("JPEG : {e}"))?;
    let info = j.info().ok_or("JPEG sans en-tête")?;
    let rgba = match info.pixel_format {
        PixelFormat::RGB24 => b.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        PixelFormat::L8 => b.iter().flat_map(|c| [*c, *c, *c, 255]).collect(),
        PixelFormat::L16 => b.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], 255]).collect(),
        PixelFormat::CMYK32 => b
            .chunks_exact(4)
            .flat_map(|c| {
                let k = c[3] as u32;
                [(c[0] as u32 * k / 255) as u8, (c[1] as u32 * k / 255) as u8, (c[2] as u32 * k / 255) as u8, 255]
            })
            .collect(),
    };
    Ok(Image { l: info.width as u32, h: info.height as u32, rgba })
}

/// Divise par deux tant qu'un côté dépasse `COTE_MAX`.
fn reduire(mut i: Image) -> Image {
    while i.l.max(i.h) > COTE_MAX {
        let (l, h) = ((i.l / 2).max(1), (i.h / 2).max(1));
        let mut r = Vec::with_capacity((l * h * 4) as usize);
        for y in 0..h {
            for x in 0..l {
                for c in 0..4 {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (px, py) = ((x * 2 + dx).min(i.l - 1), (y * 2 + dy).min(i.h - 1));
                        s += i.rgba[((py * i.l + px) * 4 + c) as usize] as u32;
                    }
                    r.push((s / 4) as u8);
                }
            }
        }
        i = Image { l, h, rgba: r };
    }
    i
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn vers_base64(d: &[u8]) -> String {
    let mut o = String::with_capacity(d.len() / 3 * 4 + 4);
    for c in d.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            o.push(if k <= c.len() { ALPHABET[(v >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    o
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Image PNG de `l` × `h`, chaque pixel donné par `f`.
    pub fn png_de(l: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut o = Vec::new();
        let mut e = ::png::Encoder::new(&mut o, l, h);
        e.set_color(::png::ColorType::Rgba);
        e.set_depth(::png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        let d: Vec<u8> = (0..h).flat_map(|y| (0..l).map(move |x| (x, y))).flat_map(|(x, y)| f(x, y)).collect();
        w.write_image_data(&d).unwrap();
        w.finish().unwrap();
        o
    }

    #[test]
    fn un_png_se_decode_premiere_ligne_en_haut() {
        let d = png_de(2, 3, |x, y| [x as u8 * 10, y as u8 * 10, 7, 200]);
        let i = decoder(&d).unwrap();
        assert_eq!((i.l, i.h, i.rgba.len()), (2, 3, 24));
        assert_eq!(i.rgba[..4], [0, 0, 7, 200]);
        assert_eq!(i.rgba[20..24], [10, 20, 7, 200]);
    }

    #[test]
    fn un_png_en_gris_ou_en_palette_donne_quatre_canaux() {
        for (couleur, octets) in [(::png::ColorType::Grayscale, vec![9, 9]), (::png::ColorType::Indexed, vec![0, 1])] {
            let mut o = Vec::new();
            let mut e = ::png::Encoder::new(&mut o, 2, 1);
            e.set_color(couleur);
            e.set_depth(::png::BitDepth::Eight);
            if couleur == ::png::ColorType::Indexed {
                e.set_palette(vec![1, 2, 3, 4, 5, 6]);
            }
            let mut w = e.write_header().unwrap();
            w.write_image_data(&octets).unwrap();
            w.finish().unwrap();
            let i = decoder(&o).unwrap();
            assert_eq!(i.rgba.len(), 8, "{couleur:?}");
            assert_eq!(i.rgba[3], 255);
        }
    }

    #[test]
    fn un_jpeg_reel_se_decode() {
        let i = decoder(include_bytes!("../tests/donnees/damier.jpg")).unwrap();
        assert_eq!((i.l, i.h), (16, 16));
        assert_eq!(i.rgba.len(), 16 * 16 * 4);
        // Damier rouge et blanc, à la perte du JPEG près
        assert!(i.rgba[0] > 200 && i.rgba[1] < 60, "{:?}", &i.rgba[..4]);
    }

    #[test]
    fn les_noms_de_fichiers_sont_propres_et_uniques() {
        let t = |nom: &str, octets: Vec<u8>| Texture { nom: nom.into(), octets: Arc::new(octets) };
        let png = vec![0x89, b'P'];
        let l = BTreeMap::from([
            ("a".to_string(), t("C:\\tex\\mon bois.PNG", png.clone())),
            ("b".to_string(), t("mon_bois.png", png.clone())),
            ("c".to_string(), t("image", vec![0xFF, 0xD8])),
            ("d".to_string(), t("", png)),
        ]);
        let n = noms_fichiers(&l);
        assert_eq!(n["a"], "mon_bois.png");
        assert_eq!(n["b"], "mon_bois-2.png");
        assert_eq!(n["c"], "image.jpg");
        assert_eq!(n["d"], "texture.png");
    }

    #[test]
    fn une_image_abimee_est_refusee_sans_panique() {
        let d = png_de(4, 4, |_, _| [0; 4]);
        for t in [&d[..20], &d[..d.len() - 8], &b"pas une image"[..], &[0xFF, 0xD8, 0xFF][..]] {
            assert!(decoder(t).is_err());
        }
    }

    #[test]
    fn une_image_trop_grande_est_reduite() {
        let d = png_de(4100, 2, |_, _| [100, 150, 200, 255]);
        let i = decoder(&d).unwrap();
        assert_eq!((i.l, i.h), (2050, 1));
        assert_eq!(i.rgba[..4], [100, 150, 200, 255]);
    }

    #[test]
    fn l_identifiant_suit_le_contenu_et_base64_fait_l_aller_retour() {
        assert_eq!(identifiant(b"abc"), identifiant(b"abc"));
        assert_ne!(identifiant(b"abc"), identifiant(b"abd"));
        for d in [&b""[..], b"a", b"ab", b"abc", b"abcd", &[0, 255, 128, 7, 9]] {
            assert_eq!(crate::maillage::base64(&vers_base64(d)), d);
        }
        assert_eq!(vers_base64(b"Man"), "TWFu");
        assert_eq!(vers_base64(b"Ma"), "TWE=");
    }
}
