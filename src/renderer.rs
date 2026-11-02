//! Tout OpenGL est ici : la 3D dans le rectangle de l'espace de travail, puis
//! l'interface par-dessus, sur tout l'écran.

use crate::draw::{DrawList, Rect, Vertex};
use crate::editeur::{GeoVue, ObjetVue, Scene};
use crate::gles::GlApi;
use crate::math;
use crate::text::{Text, ATLAS};
use crate::vue::{BANDE_F32, FACE_F32};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CString};
use std::mem::{offset_of, size_of};

const TRIANGLES: u32 = 4;
const FLOAT: u32 = 0x1406;
const UBYTE: u32 = 0x1401;
const ARRAY_BUFFER: u32 = 0x8892;
const TEXTURE_2D: u32 = 0x0DE1;
const RED: u32 = 0x1903;
const BLEND: u32 = 0x0BE2;
const DEPTH_TEST: u32 = 0x0B71;
const SCISSOR: u32 = 0x0C11;
const CULL_FACE: u32 = 0x0B44;
const POLYGON_OFFSET: u32 = 0x8037;
const ELEMENT_BUFFER: u32 = 0x8893;
const UINT: u32 = 0x1405;
const COLOR_BIT: u32 = 0x4000;
const DEPTH_BIT: u32 = 0x0100;

const UI_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec2 a_pos;
layout(location=1) in vec2 a_local;
layout(location=2) in vec4 a_shape;
layout(location=3) in vec2 a_uv;
layout(location=4) in vec4 a_color;
uniform vec2 u_size;
out vec2 v_local; out vec4 v_shape; out vec2 v_uv; out vec4 v_color;
void main() {
  v_local = a_local; v_shape = a_shape; v_uv = a_uv; v_color = a_color;
  vec2 p = a_pos / u_size * 2.0 - 1.0;
  gl_Position = vec4(p.x, -p.y, 0.0, 1.0);
}";

// highp : en mediump (16 bits sur Mali), les distances en pixels manquent de précision.
const UI_FS: &str = "#version 300 es
precision highp float;
in vec2 v_local; in vec4 v_shape; in vec2 v_uv; in vec4 v_color;
uniform sampler2D u_atlas;
out vec4 o_color;
void main() {
  float a;
  if (v_shape.z < -1.5) {
    // Image en sRGB, lue en linéaire : retour au sRGB de l'interface
    vec4 t = texture(u_atlas, v_uv);
    vec3 c = 1.055 * pow(max(t.rgb, 0.0), vec3(1.0 / 2.4)) - 0.055;
    o_color = vec4(c * t.a, t.a);
    return;
  } else if (v_shape.z < 0.0) {
    a = texture(u_atlas, v_uv).r;
  } else {
    vec2 q = abs(v_local) - v_shape.xy + v_shape.z;
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - v_shape.z;
    float s = max(v_shape.w, 0.6);
    a = 1.0 - smoothstep(-s, s, d);
  }
  a *= v_color.a;
  o_color = vec4(v_color.rgb * a, a);
}";

// Fond radial de l'espace de travail : ellipse centrée à 50 % / 40 %, comme le
// dégradé CSS de GDA (rayon au coin le plus éloigné, couleur de bord à 70 %).
const FOND_VS: &str = "#version 300 es
precision highp float;
out vec2 v_p;
void main() {
  vec2 q = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
  v_p = vec2(q.x, 1.0 - q.y);
  gl_Position = vec4(q * 2.0 - 1.0, 0.0, 1.0);
}";

const FOND_FS: &str = "#version 300 es
precision highp float;
in vec2 v_p;
uniform vec2 u_taille;
uniform vec3 u_centre;
uniform vec3 u_bord;
out vec4 o_color;
void main() {
  vec2 c = vec2(0.5, 0.4) * u_taille;
  vec2 r = max(c, u_taille - c) * sqrt(2.0);
  float t = clamp(length((v_p * u_taille - c) / r) / 0.7, 0.0, 1.0);
  o_color = vec4(mix(u_centre, u_bord, t), 1.0);
}";

const LIGNE_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos;
uniform mat4 u_mvp;
void main() { gl_Position = u_mvp * vec4(a_pos, 1.0); }";

const LIGNE_FS: &str = "#version 300 es
precision highp float;
uniform vec4 u_couleur;
out vec4 o_color;
void main() { o_color = u_couleur; }";

const POINT_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos;
uniform mat4 u_mvp;
uniform float u_taille;
void main() { gl_Position = u_mvp * vec4(a_pos, 1.0); gl_PointSize = u_taille; }";

const POINT_FS: &str = "#version 300 es
precision highp float;
uniform vec4 u_couleur;
out vec4 o_color;
void main() {
  if (length(gl_PointCoord - 0.5) > 0.5) discard;
  o_color = u_couleur;
}";

const BANDE_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_autre;
layout(location=2) in float a_cote;
uniform mat4 u_mvp;
uniform vec2 u_res;
uniform float u_largeur;
void main() {
  vec4 a = u_mvp * vec4(a_pos, 1.0);
  vec4 b = u_mvp * vec4(a_autre, 1.0);
  vec2 d = (b.xy / b.w - a.xy / a.w) * u_res;
  vec2 n = length(d) > 1e-6 ? normalize(vec2(-d.y, d.x)) : vec2(0.0, 1.0);
  a.xy += n * a_cote * u_largeur / u_res * a.w;
  gl_Position = a;
}";

const OBJET_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_normale;
layout(location=2) in vec2 a_uv;
uniform mat4 u_vp;
uniform mat4 u_modele;
uniform mat3 u_nm;
out vec3 v_n;
out vec2 v_uv;
void main() {
  v_n = u_nm * a_normale;
  v_uv = a_uv;
  gl_Position = u_vp * u_modele * vec4(a_pos, 1.0);
}";

// Lambert comme three.js r159 (moteur de GDA) : ciel hémisphérique de 1,1 et
// soleil de 2,2, en espace linéaire, puis sortie sRGB.
const OBJET_FS: &str = "#version 300 es
precision highp float;
in vec3 v_n;
in vec2 v_uv;
uniform vec3 u_albedo;
uniform vec3 u_emission;
uniform vec3 u_lumiere;
uniform float u_opacite;
uniform sampler2D u_tex;
uniform float u_texturee;
out vec4 o_color;
const vec3 SOL = vec3(0.1022, 0.0887, 0.1384);
vec3 srgb(vec3 c) {
  vec3 a = 1.055 * pow(max(c, 0.0), vec3(1.0 / 2.4)) - 0.055;
  return mix(c * 12.92, a, step(0.0031308, c));
}
void main() {
  vec3 n = normalize(v_n);
  vec3 ciel = mix(SOL, vec3(1.0), 0.5 * n.y + 0.5) * 1.1;
  vec3 e = ciel + vec3(2.2 * max(dot(n, u_lumiere), 0.0));
  vec3 albedo = u_albedo, emission = u_emission;
  if (u_texturee > 0.5 && v_uv.x > -5000.0) {
    vec4 t = texture(u_tex, v_uv);
    if (t.a < 0.5) discard;
    albedo *= t.rgb;
    // Sans albedo, le matériau est plat : sa couleur passe par l'émission
    if (u_albedo == vec3(0.0)) emission *= t.rgb;
  }
  o_color = vec4(srgb(albedo / 3.14159265 * e + emission), u_opacite);
}";

// Silhouette (ed_silhouette) : l'arête n'est tracée que si l'une de ses faces est
// tournée vers l'œil et l'autre non ; sinon elle sort du volume de vue.
const CONTOUR_VS: &str = "#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos;
layout(location=1) in vec3 a_autre;
layout(location=2) in float a_cote;
layout(location=3) in vec3 a_n1;
layout(location=4) in vec3 a_n2;
uniform mat4 u_vp;
uniform mat4 u_modele;
uniform mat3 u_nm;
uniform vec3 u_oeil;
uniform vec2 u_res;
uniform float u_largeur;
void main() {
  vec4 m = u_modele * vec4((a_pos + a_autre) * 0.5, 1.0);
  vec3 o = u_oeil - m.xyz;
  float f1 = dot(u_nm * a_n1, o), f2 = dot(u_nm * a_n2, o);
  if (dot(a_n2, a_n2) > 0.5 && f1 * f2 > 0.0) { gl_Position = vec4(0.0, 0.0, 2.0, 1.0); return; }
  vec4 a = u_vp * u_modele * vec4(a_pos, 1.0);
  vec4 b = u_vp * u_modele * vec4(a_autre, 1.0);
  vec2 d = (b.xy / b.w - a.xy / a.w) * u_res;
  vec2 n = length(d) > 1e-6 ? normalize(vec2(-d.y, d.x)) : vec2(0.0, 1.0);
  a.xy += n * a_cote * u_largeur / u_res * a.w;
  gl_Position = a;
}";

const CONTOUR_FS: &str = "#version 300 es
precision highp float;
uniform vec3 u_couleur;
out vec4 o_color;
vec3 srgb(vec3 c) {
  vec3 a = 1.055 * pow(max(c, 0.0), vec3(1.0 / 2.4)) - 0.055;
  return mix(c * 12.92, a, step(0.0031308, c));
}
void main() { o_color = vec4(srgb(u_couleur), 1.0); }";

/// Grille de 20 cases de côté 1, puis les trois axes.
fn grille() -> Vec<f32> {
    let mut v = Vec::with_capacity((21 * 4 + 6) * 3);
    for i in -10..=10 {
        let k = i as f32;
        v.extend_from_slice(&[k, 0., -10., k, 0., 10., -10., 0., k, 10., 0., k]);
    }
    v.extend_from_slice(&[-10., 0., 0., 10., 0., 0., 0., -10., 0., 0., 10., 0., 0., 0., -10., 0., 0., 10.]);
    v
}

const LIGNES: u32 = 1;
const STRIP: u32 = 5;
const NB_GRILLE: i32 = 21 * 4;

/// Un objet chez le processeur graphique.
struct Gpu {
    vao: u32,
    vbo: u32,
    nb: i32,
    vao_b: u32,
    vbo_b: u32,
    ibo: u32,
    nb_b: i32,
    vao_fil: u32,
    vbo_fil: u32,
    nb_fil: i32,
}

struct LocObjet {
    vp: i32,
    modele: i32,
    nm: i32,
    albedo: i32,
    emission: i32,
    lumiere: i32,
    opacite: i32,
    texturee: i32,
}

struct LocContour {
    vp: i32,
    modele: i32,
    nm: i32,
    oeil: i32,
    res: i32,
    largeur: i32,
    couleur: i32,
}

pub struct Renderer {
    gl: GlApi,
    ui_program: u32,
    u_size: i32,
    ui_vao: u32,
    ui_vbo: u32,
    atlas: u32,
    fond: u32,
    fond_taille: i32,
    fond_centre: i32,
    fond_bord: i32,
    vide_vao: u32,
    ligne: u32,
    ligne_mvp: i32,
    ligne_couleur: i32,
    grille_vao: u32,
    objet: u32,
    lo: LocObjet,
    contour: u32,
    lc: LocContour,
    /// Tampons par maillage : plusieurs objets peuvent en partager un.
    gpu: HashMap<usize, Gpu>,
    /// Textures chargées, par identifiant d'image.
    images: HashMap<String, u32>,
    point: u32,
    point_mvp: i32,
    point_taille: i32,
    point_couleur: i32,
    bande: u32,
    bande_mvp: i32,
    bande_res: i32,
    bande_largeur: i32,
    bande_couleur: i32,
    /// Fil, points et bandes de la copie de travail : (vao, vbo, sommets).
    geo: [(u32, u32, i32); 4],
    dp: f32,
}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

unsafe fn compile(gl: &GlApi, kind: u32, src: &str) -> Result<u32, String> {
    let s = (gl.glCreateShader)(kind);
    let c = cstr(src);
    let p = c.as_ptr();
    (gl.glShaderSource)(s, 1, &p, std::ptr::null());
    (gl.glCompileShader)(s);
    let mut ok = 0;
    (gl.glGetShaderiv)(s, 0x8B81, &mut ok);
    if ok == 0 {
        let mut log = [0u8; 1024];
        (gl.glGetShaderInfoLog)(s, 1024, std::ptr::null_mut(), log.as_mut_ptr() as *mut c_char);
        return Err(String::from_utf8_lossy(&log).trim_end_matches('\0').to_string());
    }
    Ok(s)
}

unsafe fn program(gl: &GlApi, vs: &str, fs: &str) -> Result<u32, String> {
    let p = (gl.glCreateProgram)();
    (gl.glAttachShader)(p, compile(gl, 0x8B31, vs)?);
    (gl.glAttachShader)(p, compile(gl, 0x8B30, fs)?);
    (gl.glLinkProgram)(p);
    let mut ok = 0;
    (gl.glGetProgramiv)(p, 0x8B82, &mut ok);
    if ok == 0 {
        let mut log = [0u8; 1024];
        (gl.glGetProgramInfoLog)(p, 1024, std::ptr::null_mut(), log.as_mut_ptr() as *mut c_char);
        return Err(String::from_utf8_lossy(&log).trim_end_matches('\0').to_string());
    }
    Ok(p)
}

impl Renderer {
    /// À créer avec un contexte courant.
    pub fn new(gl: GlApi) -> Result<Renderer, String> {
        unsafe {
            let ui_program = program(&gl, UI_VS, UI_FS)?;
            let (fond, ligne) = (program(&gl, FOND_VS, FOND_FS)?, program(&gl, LIGNE_VS, LIGNE_FS)?);
            let (objet, contour) = (program(&gl, OBJET_VS, OBJET_FS)?, program(&gl, CONTOUR_VS, CONTOUR_FS)?);
            let (point, bande) = (program(&gl, POINT_VS, POINT_FS)?, program(&gl, BANDE_VS, LIGNE_FS)?);
            let loc = |p: u32, n: &str| (gl.glGetUniformLocation)(p, cstr(n).as_ptr());
            let u_size = loc(ui_program, "u_size");
            let lo = LocObjet {
                vp: loc(objet, "u_vp"),
                modele: loc(objet, "u_modele"),
                nm: loc(objet, "u_nm"),
                albedo: loc(objet, "u_albedo"),
                emission: loc(objet, "u_emission"),
                lumiere: loc(objet, "u_lumiere"),
                opacite: loc(objet, "u_opacite"),
                texturee: loc(objet, "u_texturee"),
            };
            // Unité 0 : l'atlas de l'interface
            (gl.glUseProgram)(objet);
            (gl.glUniform1i)(loc(objet, "u_tex"), 1);
            let lc = LocContour {
                vp: loc(contour, "u_vp"),
                modele: loc(contour, "u_modele"),
                nm: loc(contour, "u_nm"),
                oeil: loc(contour, "u_oeil"),
                res: loc(contour, "u_res"),
                largeur: loc(contour, "u_largeur"),
                couleur: loc(contour, "u_couleur"),
            };

            let (mut ui_vao, mut ui_vbo) = (0, 0);
            (gl.glGenVertexArrays)(1, &mut ui_vao);
            (gl.glGenBuffers)(1, &mut ui_vbo);
            (gl.glBindVertexArray)(ui_vao);
            (gl.glBindBuffer)(ARRAY_BUFFER, ui_vbo);
            let stride = size_of::<Vertex>() as i32;
            let attrs: [(u32, i32, u32, u8, usize); 5] = [
                (0, 2, FLOAT, 0, offset_of!(Vertex, pos)),
                (1, 2, FLOAT, 0, offset_of!(Vertex, local)),
                (2, 4, FLOAT, 0, offset_of!(Vertex, shape)),
                (3, 2, FLOAT, 0, offset_of!(Vertex, uv)),
                (4, 4, UBYTE, 1, offset_of!(Vertex, color)),
            ];
            for (i, n, t, norm, off) in attrs {
                (gl.glEnableVertexAttribArray)(i);
                (gl.glVertexAttribPointer)(i, n, t, norm, stride, off as *const c_void);
            }

            let mut atlas = 0;
            (gl.glGenTextures)(1, &mut atlas);
            (gl.glBindTexture)(TEXTURE_2D, atlas);
            (gl.glPixelStorei)(0x0CF5, 1);
            (gl.glTexImage2D)(TEXTURE_2D, 0, 0x8229, ATLAS as i32, ATLAS as i32, 0, RED, UBYTE, std::ptr::null());
            for (k, v) in [(0x2801, 0x2601), (0x2800, 0x2601), (0x2802, 0x812F), (0x2803, 0x812F)] {
                (gl.glTexParameteri)(TEXTURE_2D, k, v);
            }

            let mut vide_vao = 0;
            (gl.glGenVertexArrays)(1, &mut vide_vao);
            let g = grille();
            let (mut grille_vao, mut grille_vbo) = (0, 0);
            (gl.glGenVertexArrays)(1, &mut grille_vao);
            (gl.glGenBuffers)(1, &mut grille_vbo);
            (gl.glBindVertexArray)(grille_vao);
            (gl.glBindBuffer)(ARRAY_BUFFER, grille_vbo);
            (gl.glBufferData)(ARRAY_BUFFER, (g.len() * 4) as isize, g.as_ptr() as *const c_void, 0x88E4);
            (gl.glEnableVertexAttribArray)(0);
            (gl.glVertexAttribPointer)(0, 3, FLOAT, 0, 12, std::ptr::null());
            let mut geo = [(0, 0, 0); 4];
            for (n, g) in geo.iter_mut().enumerate() {
                (gl.glGenVertexArrays)(1, &mut g.0);
                (gl.glGenBuffers)(1, &mut g.1);
                (gl.glBindVertexArray)(g.0);
                (gl.glBindBuffer)(ARRAY_BUFFER, g.1);
                if n < 3 {
                    (gl.glEnableVertexAttribArray)(0);
                    (gl.glVertexAttribPointer)(0, 3, FLOAT, 0, 12, std::ptr::null());
                } else {
                    for (i, c, off) in [(0u32, 3, 0usize), (1, 3, 12), (2, 1, 24)] {
                        (gl.glEnableVertexAttribArray)(i);
                        (gl.glVertexAttribPointer)(i, c, FLOAT, 0, 28, off as *const c_void);
                    }
                }
            }
            (gl.glBindVertexArray)(0);

            Ok(Renderer {
                ui_program: ui_program,
                gl,
                u_size,
                ui_vao,
                ui_vbo,
                atlas,
                fond,
                fond_taille: 0,
                fond_centre: 0,
                fond_bord: 0,
                vide_vao,
                ligne,
                ligne_mvp: 0,
                ligne_couleur: 0,
                grille_vao,
                objet,
                lo,
                contour,
                lc,
                gpu: HashMap::new(),
                images: HashMap::new(),
                point,
                point_mvp: 0,
                point_taille: 0,
                point_couleur: 0,
                bande,
                bande_mvp: 0,
                bande_res: 0,
                bande_largeur: 0,
                bande_couleur: 0,
                geo,
                dp: 1.0,
            }
            .localiser())
        }
    }

    /// Emplacements restants, une fois `gl` rangé.
    fn localiser(mut self) -> Renderer {
        let gl = &self.gl;
        unsafe {
            let loc = |p: u32, n: &str| (gl.glGetUniformLocation)(p, cstr(n).as_ptr());
            self.fond_taille = loc(self.fond, "u_taille");
            self.fond_centre = loc(self.fond, "u_centre");
            self.fond_bord = loc(self.fond, "u_bord");
            self.ligne_mvp = loc(self.ligne, "u_mvp");
            self.ligne_couleur = loc(self.ligne, "u_couleur");
            self.point_mvp = loc(self.point, "u_mvp");
            self.point_taille = loc(self.point, "u_taille");
            self.point_couleur = loc(self.point, "u_couleur");
            self.bande_mvp = loc(self.bande, "u_mvp");
            self.bande_res = loc(self.bande, "u_res");
            self.bande_largeur = loc(self.bande, "u_largeur");
            self.bande_couleur = loc(self.bande, "u_couleur");
        }
        self
    }

    pub fn set_dp(&mut self, dp: f32) {
        self.dp = dp;
    }

    /// Met à jour les tampons : ceux que la scène ne montre plus, puis les nouveaux.
    fn charger(&mut self, s: &Scene) {
        let gl = &self.gl;
        unsafe {
            // Une clé déjà connue : ses tampons sont remplacés (copie de travail)
            for g in s.retraits.iter().chain(s.ajouts.iter().map(|a| &a.0)).filter_map(|c| self.gpu.remove(c)) {
                (gl.glDeleteBuffers)(1, &g.vbo);
                (gl.glDeleteBuffers)(1, &g.vbo_b);
                (gl.glDeleteBuffers)(1, &g.ibo);
                (gl.glDeleteVertexArrays)(1, &g.vao);
                (gl.glDeleteVertexArrays)(1, &g.vao_b);
                (gl.glDeleteBuffers)(1, &g.vbo_fil);
                (gl.glDeleteVertexArrays)(1, &g.vao_fil);
            }
            for t in s.images_retraits.iter().filter_map(|id| self.images.remove(id)) {
                (gl.glDeleteTextures)(1, &t);
            }
            for (id, i) in &s.images {
                let mut t = 0;
                (gl.glGenTextures)(1, &mut t);
                (gl.glActiveTexture)(0x84C1);
                (gl.glBindTexture)(TEXTURE_2D, t);
                // SRGB8_ALPHA8 : le processeur graphique rend des couleurs linéaires
                (gl.glTexImage2D)(TEXTURE_2D, 0, 0x8C43, i.l as i32, i.h as i32, 0, 0x1908, UBYTE, i.rgba.as_ptr() as *const c_void);
                for (k, v) in [(0x2801, 0x2703), (0x2800, 0x2601), (0x2802, 0x2901), (0x2803, 0x2901)] {
                    (gl.glTexParameteri)(TEXTURE_2D, k, v);
                }
                (gl.glGenerateMipmap)(TEXTURE_2D);
                (gl.glActiveTexture)(0x84C0);
                self.images.insert(id.clone(), t);
            }
            if let Some(t) = s.geo.as_ref().and_then(|g| g.maj.as_ref()) {
                for (n, (l, taille)) in [(&t.lignes, 3), (&t.points, 3), (&t.choisis, 3), (&t.bandes, 7)].into_iter().enumerate() {
                    (gl.glBindBuffer)(ARRAY_BUFFER, self.geo[n].1);
                    (gl.glBufferData)(ARRAY_BUFFER, (l.len() * 4) as isize, l.as_ptr() as *const c_void, 0x88E8);
                    self.geo[n].2 = (l.len() / taille) as i32;
                }
            }
            for (cle, t) in &s.ajouts {
                let (mut vao, mut vbo, mut vao_b, mut vbo_b, mut ibo) = (0, 0, 0, 0, 0);
                (gl.glGenVertexArrays)(1, &mut vao);
                (gl.glGenBuffers)(1, &mut vbo);
                (gl.glBindVertexArray)(vao);
                (gl.glBindBuffer)(ARRAY_BUFFER, vbo);
                (gl.glBufferData)(ARRAY_BUFFER, (t.faces.len() * 4) as isize, t.faces.as_ptr() as *const c_void, 0x88E4);
                let pas = (FACE_F32 * 4) as i32;
                for (i, n, off) in [(0u32, 3, 0usize), (1, 3, 12), (2, 2, 24)] {
                    (gl.glEnableVertexAttribArray)(i);
                    (gl.glVertexAttribPointer)(i, n, FLOAT, 0, pas, off as *const c_void);
                }
                (gl.glGenVertexArrays)(1, &mut vao_b);
                (gl.glGenBuffers)(1, &mut vbo_b);
                (gl.glGenBuffers)(1, &mut ibo);
                (gl.glBindVertexArray)(vao_b);
                (gl.glBindBuffer)(ARRAY_BUFFER, vbo_b);
                (gl.glBufferData)(ARRAY_BUFFER, (t.bandes.len() * 4) as isize, t.bandes.as_ptr() as *const c_void, 0x88E4);
                (gl.glBindBuffer)(ELEMENT_BUFFER, ibo);
                (gl.glBufferData)(ELEMENT_BUFFER, (t.bandes_idx.len() * 4) as isize, t.bandes_idx.as_ptr() as *const c_void, 0x88E4);
                let pas = (BANDE_F32 * 4) as i32;
                for (i, n, off) in [(0u32, 3, 0usize), (1, 3, 12), (2, 1, 24), (3, 3, 28), (4, 3, 40)] {
                    (gl.glEnableVertexAttribArray)(i);
                    (gl.glVertexAttribPointer)(i, n, FLOAT, 0, pas, off as *const c_void);
                }
                let (mut vao_fil, mut vbo_fil) = (0, 0);
                (gl.glGenVertexArrays)(1, &mut vao_fil);
                (gl.glGenBuffers)(1, &mut vbo_fil);
                (gl.glBindVertexArray)(vao_fil);
                (gl.glBindBuffer)(ARRAY_BUFFER, vbo_fil);
                (gl.glBufferData)(ARRAY_BUFFER, (t.fil.len() * 4) as isize, t.fil.as_ptr() as *const c_void, 0x88E4);
                (gl.glEnableVertexAttribArray)(0);
                (gl.glVertexAttribPointer)(0, 3, FLOAT, 0, 12, std::ptr::null());
                self.gpu.insert(*cle, Gpu { vao, vbo, nb: (t.faces.len() / FACE_F32) as i32, vao_b, vbo_b, ibo, nb_b: t.bandes_idx.len() as i32, vao_fil, vbo_fil, nb_fil: (t.fil.len() / 3) as i32 });
            }
            (gl.glBindVertexArray)(0);
        }
    }

    pub fn gl(&self) -> &GlApi {
        &self.gl
    }

    /// Rectangle en pixels (origine en haut) vers OpenGL (origine en bas).
    fn gl_rect(r: Rect, h: f32) -> (i32, i32, i32, i32) {
        (r.x.round() as i32, (h - r.bottom()).round() as i32, r.w.round().max(0.0) as i32, r.h.round().max(0.0) as i32)
    }

    unsafe fn dessiner_objet(&self, g: &Gpu, o: &ObjetVue) {
        let gl = &self.gl;
        let l = &self.lo;
        (gl.glUniformMatrix4fv)(l.modele, 1, 0, o.modele.as_ptr());
        (gl.glUniformMatrix3fv)(l.nm, 1, 0, o.normale.as_ptr());
        (gl.glUniform3f)(l.albedo, o.albedo[0], o.albedo[1], o.albedo[2]);
        (gl.glUniform3f)(l.emission, o.emission[0], o.emission[1], o.emission[2]);
        (gl.glUniform1f)(l.opacite, o.opacite);
        let image = o.texture.as_ref().and_then(|t| self.images.get(t));
        (gl.glUniform1f)(l.texturee, if image.is_some() { 1.0 } else { 0.0 });
        if let Some(t) = image {
            (gl.glActiveTexture)(0x84C1);
            (gl.glBindTexture)(TEXTURE_2D, *t);
            (gl.glActiveTexture)(0x84C0);
        }
        (gl.glBindVertexArray)(g.vao);
        (gl.glDrawArrays)(TRIANGLES, 0, g.nb);
        // L'envers d'une face de la copie de travail se voit, teinté, sans éclairage
        if let Some(e) = o.envers {
            (gl.glCullFace)(0x0404);
            (gl.glUniform3f)(l.albedo, 0.0, 0.0, 0.0);
            (gl.glUniform3f)(l.emission, e[0], e[1], e[2]);
            (gl.glDrawArrays)(TRIANGLES, 0, g.nb);
            (gl.glCullFace)(0x0405);
        }
    }

    /// Fil, points et arêtes choisies de la copie de travail.
    unsafe fn dessiner_geo(&self, s: &Scene, g: &GeoVue, vw: i32, vh: i32) {
        let gl = &self.gl;
        if g.rayons {
            (gl.glDisable)(DEPTH_TEST);
        }
        (gl.glUseProgram)(self.ligne);
        (gl.glUniformMatrix4fv)(self.ligne_mvp, 1, 0, s.vue_proj.as_ptr());
        let c = s.couleurs.arete;
        (gl.glUniform4f)(self.ligne_couleur, c[0], c[1], c[2], 0.75);
        (gl.glBindVertexArray)(self.geo[0].0);
        (gl.glDrawArrays)(LIGNES, 0, self.geo[0].2);

        (gl.glUseProgram)(self.bande);
        (gl.glUniformMatrix4fv)(self.bande_mvp, 1, 0, s.vue_proj.as_ptr());
        (gl.glUniform2f)(self.bande_res, vw as f32, vh as f32);
        (gl.glUniform1f)(self.bande_largeur, 4.0 * self.dp);
        let c = s.couleurs.arete_choisie;
        (gl.glUniform4f)(self.bande_couleur, c[0], c[1], c[2], 1.0);
        (gl.glBindVertexArray)(self.geo[3].0);
        (gl.glDrawArrays)(TRIANGLES, 0, self.geo[3].2);

        (gl.glUseProgram)(self.point);
        (gl.glUniformMatrix4fv)(self.point_mvp, 1, 0, s.vue_proj.as_ptr());
        (gl.glUniform1f)(self.point_taille, 8.0 * self.dp);
        let c = s.couleurs.sommet;
        (gl.glUniform4f)(self.point_couleur, c[0], c[1], c[2], 1.0);
        (gl.glBindVertexArray)(self.geo[1].0);
        (gl.glDrawArrays)(0, 0, self.geo[1].2);
        (gl.glUniform1f)(self.point_taille, 13.0 * self.dp);
        let c = s.couleurs.sommet_choisi;
        (gl.glUniform4f)(self.point_couleur, c[0], c[1], c[2], 1.0);
        (gl.glBindVertexArray)(self.geo[2].0);
        (gl.glDrawArrays)(0, 0, self.geo[2].2);
        if g.rayons {
            (gl.glEnable)(DEPTH_TEST);
        }
    }

    unsafe fn dessiner_3d(&self, s: &Scene, vw: i32, vh: i32) {
        let gl = &self.gl;
        // Fond
        (gl.glDisable)(DEPTH_TEST);
        (gl.glUseProgram)(self.fond);
        (gl.glUniform2f)(self.fond_taille, vw as f32, vh as f32);
        let (c, b) = (s.couleurs.fond_centre, s.couleurs.fond_bord);
        (gl.glUniform3f)(self.fond_centre, c[0], c[1], c[2]);
        (gl.glUniform3f)(self.fond_bord, b[0], b[1], b[2]);
        (gl.glBindVertexArray)(self.vide_vao);
        (gl.glDrawArrays)(STRIP, 0, 4);
        (gl.glEnable)(DEPTH_TEST);

        let k = s.pas_grille;
        let grille_mvp = math::mul(&s.vue_proj, &[k, 0., 0., 0., 0., k, 0., 0., 0., 0., k, 0., 0., 0., 0., 1.]);
        (gl.glLineWidth)(self.dp.max(1.0));

        // Axes
        (gl.glUseProgram)(self.ligne);
        (gl.glUniformMatrix4fv)(self.ligne_mvp, 1, 0, grille_mvp.as_ptr());
        (gl.glBindVertexArray)(self.grille_vao);
        for (i, c) in s.couleurs.axes.iter().enumerate() {
            (gl.glUniform4f)(self.ligne_couleur, c[0], c[1], c[2], 1.0);
            (gl.glDrawArrays)(LIGNES, NB_GRILLE + 2 * i as i32, 2);
        }

        // Objets opaques ; envers cachés, comme dans le mode Scène de GDA
        (gl.glEnable)(CULL_FACE);
        (gl.glEnable)(POLYGON_OFFSET);
        (gl.glPolygonOffset)(1.0, 1.0);
        (gl.glUseProgram)(self.objet);
        (gl.glUniformMatrix4fv)(self.lo.vp, 1, 0, s.vue_proj.as_ptr());
        (gl.glUniform3f)(self.lo.lumiere, s.lumiere[0], s.lumiere[1], s.lumiere[2]);
        for o in s.objets.iter().filter(|o| o.opacite >= 1.0 && !o.filaire) {
            if let Some(g) = self.gpu.get(&o.tampon) {
                self.dessiner_objet(g, o);
            }
        }
        (gl.glDisable)(POLYGON_OFFSET);
        (gl.glDisable)(CULL_FACE);

        // Cibles de booléens : leur fil de fer
        (gl.glUseProgram)(self.ligne);
        let c = s.couleurs.arete;
        (gl.glUniform4f)(self.ligne_couleur, c[0], c[1], c[2], 1.0);
        for o in s.objets.iter().filter(|o| o.filaire) {
            if let Some(g) = self.gpu.get(&o.tampon) {
                let mvp = crate::math::mul(&s.vue_proj, &o.modele);
                (gl.glUniformMatrix4fv)(self.ligne_mvp, 1, 0, mvp.as_ptr());
                (gl.glBindVertexArray)(g.vao_fil);
                (gl.glDrawArrays)(LIGNES, 0, g.nb_fil);
            }
        }

        // Silhouette des objets choisis
        (gl.glUseProgram)(self.contour);
        let l = &self.lc;
        (gl.glUniformMatrix4fv)(l.vp, 1, 0, s.vue_proj.as_ptr());
        (gl.glUniform3f)(l.oeil, s.oeil[0], s.oeil[1], s.oeil[2]);
        (gl.glUniform2f)(l.res, vw as f32, vh as f32);
        (gl.glUniform1f)(l.largeur, s.largeur_contour);
        (gl.glUniform3f)(l.couleur, s.contour[0], s.contour[1], s.contour[2]);
        for o in s.objets.iter().filter(|o| o.contour) {
            if let Some(g) = self.gpu.get(&o.tampon) {
                (gl.glUniformMatrix4fv)(l.modele, 1, 0, o.modele.as_ptr());
                (gl.glUniformMatrix3fv)(l.nm, 1, 0, o.normale.as_ptr());
                (gl.glBindVertexArray)(g.vao_b);
                (gl.glDrawElements)(TRIANGLES, g.nb_b, UINT, std::ptr::null());
            }
        }

        // Transparences : grille, puis objets translucides, sans écrire la profondeur
        (gl.glEnable)(BLEND);
        (gl.glBlendFunc)(0x0302, 0x0303);
        (gl.glDepthMask)(0);
        (gl.glUseProgram)(self.ligne);
        (gl.glUniformMatrix4fv)(self.ligne_mvp, 1, 0, grille_mvp.as_ptr());
        let c = s.couleurs.grille;
        (gl.glUniform4f)(self.ligne_couleur, c[0], c[1], c[2], 0.35);
        (gl.glBindVertexArray)(self.grille_vao);
        (gl.glDrawArrays)(LIGNES, 0, NB_GRILLE);
        (gl.glEnable)(CULL_FACE);
        (gl.glUseProgram)(self.objet);
        for o in s.objets.iter().filter(|o| o.opacite < 1.0 && !o.filaire) {
            if let Some(g) = self.gpu.get(&o.tampon) {
                self.dessiner_objet(g, o);
            }
        }
        (gl.glDisable)(CULL_FACE);
        if let Some(g) = &s.geo {
            self.dessiner_geo(s, g, vw, vh);
        }
        (gl.glDepthMask)(1);
    }

    pub fn draw(&mut self, w: f32, h: f32, scene: &Scene, dl: &DrawList, text: &mut Text) {
        self.charger(scene);
        let gl = &self.gl;
        unsafe {
            (gl.glDisable)(SCISSOR);
            (gl.glViewport)(0, 0, w as i32, h as i32);
            let f = scene.couleurs.fond;
            (gl.glClearColor)(f[0], f[1], f[2], 1.0);
            (gl.glClear)(COLOR_BIT | DEPTH_BIT);

            // 3D, limitée à l'espace de travail
            let (x, y, vw, vh) = Self::gl_rect(scene.workspace, h);
            if vw > 0 && vh > 0 {
                (gl.glDisable)(BLEND);
                (gl.glEnable)(SCISSOR);
                (gl.glScissor)(x, y, vw, vh);
                (gl.glViewport)(x, y, vw, vh);
                self.dessiner_3d(scene, vw, vh);
            }

            // Interface, sur tout l'écran
            (gl.glDisable)(DEPTH_TEST);
            (gl.glDisable)(SCISSOR);
            (gl.glViewport)(0, 0, w as i32, h as i32);
            (gl.glEnable)(BLEND);
            (gl.glBlendFunc)(1, 0x0303); // couleurs prémultipliées
            (gl.glUseProgram)(self.ui_program);
            (gl.glUniform2f)(self.u_size, w, h);
            (gl.glActiveTexture)(0x84C0);
            (gl.glBindTexture)(TEXTURE_2D, self.atlas);
            if text.dirty {
                (gl.glTexSubImage2D)(TEXTURE_2D, 0, 0, 0, ATLAS as i32, ATLAS as i32, RED, UBYTE, text.atlas.as_ptr() as *const c_void);
                text.dirty = false;
            }
            (gl.glBindVertexArray)(self.ui_vao);
            (gl.glBindBuffer)(ARRAY_BUFFER, self.ui_vbo);
            (gl.glBufferData)(ARRAY_BUFFER, (dl.vertices.len() * size_of::<Vertex>()) as isize, dl.vertices.as_ptr() as *const c_void, 0x88E0);
            for c in &dl.cmds {
                match c.clip {
                    Some(r) => {
                        let (x, y, cw, ch) = Self::gl_rect(r, h);
                        (gl.glEnable)(SCISSOR);
                        (gl.glScissor)(x, y, cw, ch);
                    }
                    None => (gl.glDisable)(SCISSOR),
                }
                // Une image pas encore chargée laisse voir le fond
                let image = c.image.and_then(|i| dl.images.get(i)).and_then(|i| self.images.get(i));
                if c.image.is_some() && image.is_none() {
                    continue;
                }
                if let Some(t) = image {
                    (gl.glBindTexture)(TEXTURE_2D, *t);
                }
                (gl.glDrawArrays)(TRIANGLES, c.first as i32, c.count as i32);
                if image.is_some() {
                    (gl.glBindTexture)(TEXTURE_2D, self.atlas);
                }
            }
            (gl.glBindVertexArray)(0);
        }
    }
}
