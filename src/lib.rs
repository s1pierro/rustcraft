//! Rustcraft : l'éditeur 3D de GDA en application Android native, en Rust.
//!
//! - `gles`, `renderer`, `android` : ce qui touche à OpenGL ES et à Android ;
//! - `draw`, `text`, `ui`, `widgets`, `shell` : l'interface, sans aucun appel
//!   OpenGL, donc testable sur n'importe quelle machine ;
//! - `maillage`, `json`, `projet`, `vue` : le modèle, sans OpenGL ;
//! - `editeur` : le contexte d'édition 3D.
//!
//! Le modèle partagé avec S.A.G.E. vient de la crate `gda`, ré-exportée sous les mêmes
//! chemins : `crate::maillage`, `crate::armature`, etc.

pub mod actions;
pub mod android;
pub mod draw;
pub mod edition;
pub mod geometrie;
pub mod icones;
pub mod editeur;
pub mod gles;
pub mod math;
pub mod projet;
pub mod renderer;
pub mod shell;
pub mod text;
pub mod texture;
pub mod booleens;
pub mod modifs;
pub mod peau;
pub mod primitives;
pub mod visserie;
pub mod theme;
pub mod ui;
pub mod uv;
pub mod vue;
pub mod widgets;

pub use gda::{animation, armature, couches, json, maillage, marionnette, sage};
