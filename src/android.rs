//! Intégration Android : NativeActivity et thread de rendu.
//!
//! Le thread principal ne fait que transmettre les événements ; toute l'interface
//! vit sur le thread de rendu, qui dort tant qu'il n'y a rien à redessiner.

#![allow(non_camel_case_types)]

use crate::draw::{DrawList, Rect};
use crate::editeur::{App, Selecteur, Sortie};
use crate::gles::{load_gl, Egl};
use crate::projet::Projet;
use crate::renderer::Renderer;
use crate::text::Text;
use crate::ui::{Event, Ui};
use crate::widgets::Cx;
use std::ffi::{c_void, CString};
use std::ptr::null_mut;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Condvar, Mutex, Once, OnceLock};
use std::time::{Duration, Instant};

type ANativeWindow = c_void;
type AInputQueue = c_void;
type AInputEvent = c_void;
type ALooper = c_void;

#[repr(C)]
pub struct ANativeActivity {
    callbacks: *mut ANativeActivityCallbacks,
    vm: *mut c_void,
    env: *mut c_void,
    clazz: *mut c_void,
    internal_data_path: *const i8,
    external_data_path: *const i8,
    sdk_version: i32,
    instance: *mut c_void,
    asset_manager: *mut c_void,
    obb_path: *const i8,
}

type Cb = Option<unsafe extern "C" fn(*mut ANativeActivity)>;
type CbWindow = Option<unsafe extern "C" fn(*mut ANativeActivity, *mut ANativeWindow)>;
type CbQueue = Option<unsafe extern "C" fn(*mut ANativeActivity, *mut AInputQueue)>;

#[repr(C)]
pub struct ANativeActivityCallbacks {
    on_start: Cb,
    on_resume: Cb,
    on_save_instance_state: Option<unsafe extern "C" fn(*mut ANativeActivity, *mut usize) -> *mut c_void>,
    on_pause: Cb,
    on_stop: Cb,
    on_destroy: Cb,
    on_window_focus_changed: Option<unsafe extern "C" fn(*mut ANativeActivity, i32)>,
    on_native_window_created: CbWindow,
    on_native_window_resized: CbWindow,
    on_native_window_redraw_needed: CbWindow,
    on_native_window_destroyed: CbWindow,
    on_input_queue_created: CbQueue,
    on_input_queue_destroyed: CbQueue,
    on_content_rect_changed: Option<unsafe extern "C" fn(*mut ANativeActivity, *const c_void)>,
    on_configuration_changed: Cb,
    on_low_memory: Cb,
}

const AINPUT_EVENT_TYPE_MOTION: i32 = 2;
const ACTION_DOWN: i32 = 0;
const ACTION_UP: i32 = 1;
const ACTION_MOVE: i32 = 2;
const ACTION_CANCEL: i32 = 3;
const ACTION_POINTER_DOWN: i32 = 5;
const ACTION_POINTER_UP: i32 = 6;
const ACTION_MASK: i32 = 0xff;
const ACTION_POINTER_INDEX_MASK: i32 = 0xff00;
const ACTION_POINTER_INDEX_SHIFT: i32 = 8;

type ALooperCallback = unsafe extern "C" fn(fd: i32, events: i32, data: *mut c_void) -> i32;

#[link(name = "android")]
extern "C" {
    fn ALooper_forThread() -> *mut ALooper;
    fn AInputQueue_attachLooper(q: *mut AInputQueue, l: *mut ALooper, ident: i32, cb: Option<ALooperCallback>, data: *mut c_void);
    fn AInputQueue_detachLooper(q: *mut AInputQueue);
    fn AInputQueue_getEvent(q: *mut AInputQueue, out: *mut *mut AInputEvent) -> i32;
    fn AInputQueue_preDispatchEvent(q: *mut AInputQueue, e: *mut AInputEvent) -> i32;
    fn AInputQueue_finishEvent(q: *mut AInputQueue, e: *mut AInputEvent, handled: i32);
    fn AInputEvent_getType(e: *const AInputEvent) -> i32;
    fn AMotionEvent_getAction(e: *const AInputEvent) -> i32;
    fn AMotionEvent_getPointerCount(e: *const AInputEvent) -> usize;
    fn AMotionEvent_getPointerId(e: *const AInputEvent, pointer_index: usize) -> i32;
    fn AMotionEvent_getX(e: *const AInputEvent, pointer_index: usize) -> f32;
    fn AMotionEvent_getY(e: *const AInputEvent, pointer_index: usize) -> f32;
    fn AConfiguration_new() -> *mut c_void;
    fn AConfiguration_fromAssetManager(c: *mut c_void, am: *mut c_void);
    fn AConfiguration_getDensity(c: *const c_void) -> i32;
    fn AConfiguration_delete(c: *mut c_void);
}

#[link(name = "log")]
extern "C" {
    fn __android_log_write(prio: i32, tag: *const i8, text: *const i8) -> i32;
}

fn log(s: &str) {
    let text = CString::new(s).unwrap_or_default();
    unsafe {
        __android_log_write(6, c"Rustcraft".as_ptr() as *const _, text.as_ptr() as *const _);
    }
}

// ---------------------------------------------------------------------------
// État partagé entre le thread principal et le thread de rendu
// ---------------------------------------------------------------------------

struct Shared {
    window: *mut ANativeWindow,
    window_changed: bool,
    /// Fenêtre que le rendu utilise : Android ne peut la détruire qu'une fois lâchée.
    attached: *mut ANativeWindow,
    redraw: bool,
    events: Vec<(Event, f64)>,
    dp: f32,
    /// Fichier reçu d'une autre application, à ouvrir.
    /// Noms et contenus, et s'ils s'ajoutent à la scène au lieu de la remplacer.
    /// Documents reçus et la demande qui les a appelés (`Selecteur`, 0 : Ouvrir avec).
    fichier: Option<(Vec<(String, Vec<u8>)>, i32)>,
    /// Sélection multiple en cours de réception.
    lot: Vec<(String, Vec<u8>)>,
    /// Dossier privé de l'application.
    interne: Option<PathBuf>,
}
unsafe impl Send for Shared {}

static SHARED: Mutex<Shared> =
    Mutex::new(Shared { window: null_mut(), window_changed: false, attached: null_mut(), redraw: false, events: Vec::new(), dp: 1.0, fichier: None, lot: Vec::new(), interne: None });
static WAKE: Condvar = Condvar::new();
/// Accès à Java pour le fil de rendu : la machine virtuelle et le ContentResolver,
/// en référence globale. Pointeurs rangés en entiers : ils ne sont lus qu'en JNI.
static JAVA: Mutex<Option<(usize, usize)>> = Mutex::new(None);
/// Machine virtuelle et activité courante, pour ouvrir le sélecteur depuis le fil de rendu.
static ACTIVITE: Mutex<Option<(usize, usize)>> = Mutex::new(None);
static START: OnceLock<Instant> = OnceLock::new();

fn now() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

fn push(events: &[Event]) {
    let t = now();
    let mut s = SHARED.lock().unwrap();
    s.events.extend(events.iter().map(|e| (*e, t)));
    WAKE.notify_all();
}

// ---------------------------------------------------------------------------
// Thread de rendu
// ---------------------------------------------------------------------------

fn render_loop() -> Result<(), String> {
    let mut egl = Egl::new(false)?;
    let mut gl = Some(load_gl(false)?);
    let mut renderer: Option<Renderer> = None;
    let mut app: Option<(App, Ui)> = None;
    let mut text = Text::new();
    let mut dl = DrawList::default();
    let mut last = Instant::now();
    let mut again = false; // la dernière image demande à être suivie d'une autre
    let interne = SHARED.lock().unwrap().interne.clone();
    let sauvegarde = interne.as_deref().map(sauvegardes);

    loop {
        let (window, changed, events, dp, fichier) = {
            let mut s = SHARED.lock().unwrap();
            loop {
                let drawing = !egl.surface.is_null();
                // Un doigt immobile doit laisser le temps passer : c'est peut-être un appui long
                let busy = drawing && (again || app.as_ref().is_some_and(|(a, u)| a.animating() || u.maintien()));
                if s.window_changed || s.redraw || busy || (drawing && (!s.events.is_empty() || s.fichier.is_some())) {
                    break;
                }
                s = WAKE.wait(s).unwrap();
            }
            s.redraw = false;
            let changed = std::mem::take(&mut s.window_changed);
            let fichier = if drawing_ok(&egl) { s.fichier.take() } else { None };
            (s.window, changed, std::mem::take(&mut s.events), s.dp, fichier)
        };
        again = false;

        if changed {
            if !egl.surface.is_null() {
                egl.detach();
            }
            SHARED.lock().unwrap().attached = null_mut();
            WAKE.notify_all();
            if window.is_null() {
                continue;
            }
            egl.attach_window(window)?;
            SHARED.lock().unwrap().attached = window;
            if let Some(api) = gl.take() {
                renderer = Some(Renderer::new(api)?);
            }
            last = Instant::now();
        }
        if egl.surface.is_null() {
            continue;
        }
        let (Some(render), (app, ui)) = (
            renderer.as_mut(),
            app.get_or_insert_with(|| {
                let mut a = App::new(dp);
                if let Some(d) = &interne {
                    relire(&mut a, d);
                }
                (a, Ui::new(dp))
            }),
        ) else {
            continue;
        };
        render.set_dp(dp);
        if let Some((fichiers, demande)) = fichier {
            if demande == Selecteur::Image as i32 {
                if let Some((nom, contenu)) = fichiers.first() {
                    app.recevoir_image(nom, contenu);
                }
            } else if demande == Selecteur::Importer as i32 {
                app.importer_fichiers(fichiers);
            } else if let Some((nom, contenu)) = fichiers.first() {
                app.ouvrir(nom, contenu);
            }
        }

        for (e, t) in events {
            ui.event(e, t);
        }
        let dt = last.elapsed().as_secs_f32().min(0.05);
        last = Instant::now();

        let (w, h) = egl.size();
        dl.clear();
        let mut cx = Cx { ui, text: &mut text, dl: &mut dl, dp };
        let scene = app.frame(&mut cx, Rect::new(0.0, 0.0, w, h), dt);
        ui.end_frame();
        render.draw(w, h, &scene, &dl, &mut text);
        egl.swap();
        let sorties = app.prendre_sorties();
        // Un export peut faire plusieurs fichiers : un seul message, et l'échec d'abord
        let mut echec = None;
        for s in &sorties {
            if let Err(e) = deposer(s) {
                echec = Some(format!("Enregistrement impossible de {} : {e}", s.nom));
            }
        }
        if let Some(s) = sorties.last() {
            app.dire(echec.unwrap_or_else(|| match sorties.len() {
                1 => format!("Dans Téléchargements : {}", s.nom),
                n => format!("Dans Téléchargements : {} et {} fichiers liés", s.nom, n - 1),
            }));
        }
        if let (Some(d), Some(t)) = (&interne, app.prendre_reglages()) {
            let r = std::fs::write(d.join("reglages.tmp"), t).and_then(|_| std::fs::rename(d.join("reglages.tmp"), d.join("reglages.txt")));
            if let Err(e) = r {
                log(&format!("réglages : {e}"));
            }
        }
        if let Some(d) = app.prendre_selecteur() {
            if let Err(e) = selecteur(d) {
                app.dire(format!("Sélecteur de fichiers indisponible : {e}"));
            }
        }
        if let (Some(t), Some(p)) = (&sauvegarde, app.prendre_sauvegarde()) {
            let _ = t.send(p);
        }
        // Après une rotation, la nouvelle taille n'est connue qu'ici ; et un appui change
        // l'état après le dessin du widget, qu'il faut redessiner à jour.
        again = egl.size() != (w, h) || ui.a_redessiner();
    }
}

/// Sauvegarde automatique : écrite à côté, puis renommée, pour qu'une coupure ne laisse
/// jamais un fichier tronqué. Sur son propre fil : un gros projet ne fige pas l'image.
fn sauvegardes(dossier: &Path) -> Sender<(String, Projet)> {
    let (tx, rx) = mpsc::channel::<(String, Projet)>();
    let d = dossier.to_path_buf();
    std::thread::spawn(move || {
        while let Ok(mut p) = rx.recv() {
            // Seule la dernière compte
            while let Ok(q) = rx.try_recv() {
                p = q;
            }
            let t = d.join("scene.tmp");
            let r = std::fs::write(&t, p.1.ecrire()).and_then(|_| std::fs::write(d.join("scene.nom"), &p.0)).and_then(|_| std::fs::rename(&t, d.join("scene.gda")));
            if let Err(e) = r {
                log(&format!("sauvegarde : {e}"));
            }
        }
    });
    tx
}

fn relire(a: &mut App, d: &Path) {
    if let Ok(t) = std::fs::read_to_string(d.join("reglages.txt")) {
        a.reprendre_reglages(&t);
    }
    if let Ok(t) = std::fs::read_to_string(d.join("scene.gda")) {
        a.reprendre(&std::fs::read_to_string(d.join("scene.nom")).unwrap_or_default(), &t);
    }
}

fn drawing_ok(egl: &Egl) -> bool {
    !egl.surface.is_null()
}

fn spawn_render_thread() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::thread::spawn(|| {
            if let Err(e) = render_loop() {
                log(&e);
                // Personne ne doit rester bloqué sur une fenêtre à lâcher.
                SHARED.lock().unwrap().attached = null_mut();
                WAKE.notify_all();
            }
        });
    });
}

// ---------------------------------------------------------------------------
// Fichier reçu : Intent lu par JNI, sans une ligne de Java
// ---------------------------------------------------------------------------

type Jni = *mut *const *const c_void;
type Obj = *mut c_void;

/// Entrée de la table de fonctions JNI (rang fixé par la spécification).
unsafe fn jni_fn<T: Copy>(env: Jni, rang: usize) -> T {
    let f = *(*env).add(rang);
    std::mem::transmute_copy(&f)
}

unsafe fn exception(env: Jni) -> bool {
    let check: unsafe extern "C" fn(Jni) -> u8 = jni_fn(env, 228);
    if check(env) == 0 {
        return false;
    }
    let clear: unsafe extern "C" fn(Jni) = jni_fn(env, 17);
    clear(env);
    true
}

unsafe fn methode(env: Jni, o: Obj, nom: &std::ffi::CStr, sig: &std::ffi::CStr) -> Option<*mut c_void> {
    let classe: unsafe extern "C" fn(Jni, Obj) -> Obj = jni_fn(env, 31);
    let id: unsafe extern "C" fn(Jni, Obj, *const i8, *const i8) -> *mut c_void = jni_fn(env, 33);
    let c = classe(env, o);
    let m = id(env, c, nom.as_ptr() as *const _, sig.as_ptr() as *const _);
    if exception(env) || m.is_null() { None } else { Some(m) }
}

type AppelObj = unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> Obj;

unsafe fn objet0(env: Jni, o: Obj, nom: &std::ffi::CStr, sig: &std::ffi::CStr) -> Option<Obj> {
    let m = methode(env, o, nom, sig)?;
    let f: AppelObj = jni_fn(env, 34);
    let r = f(env, o, m);
    if exception(env) || r.is_null() { None } else { Some(r) }
}

unsafe fn chaine(env: Jni, s: &std::ffi::CStr) -> Obj {
    let f: unsafe extern "C" fn(Jni, *const i8) -> Obj = jni_fn(env, 167);
    f(env, s.as_ptr() as *const _)
}

unsafe fn texte(env: Jni, s: Obj) -> Option<String> {
    let lire: unsafe extern "C" fn(Jni, Obj, *mut u8) -> *const i8 = jni_fn(env, 169);
    let lacher: unsafe extern "C" fn(Jni, Obj, *const i8) = jni_fn(env, 170);
    let p = lire(env, s, null_mut());
    if p.is_null() {
        return None;
    }
    let t = std::ffi::CStr::from_ptr(p as *const _).to_string_lossy().into_owned();
    lacher(env, s, p);
    Some(t)
}

/// Nom affiché du document, sinon la fin de son adresse.
unsafe fn nom_document(env: Jni, resolver: Obj, uri: Obj) -> String {
    let query = c"(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;";
    let lu = (|| {
        let m = methode(env, resolver, c"query", query)?;
        let f: AppelObj = jni_fn(env, 34);
        let vide: Obj = null_mut();
        let cur = f(env, resolver, m, uri, vide, vide, vide, vide);
        if exception(env) || cur.is_null() {
            return None;
        }
        let premier = methode(env, cur, c"moveToFirst", c"()Z")?;
        let b: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> u8 = jni_fn(env, 37);
        if b(env, cur, premier) == 0 {
            return None;
        }
        let col = methode(env, cur, c"getColumnIndex", c"(Ljava/lang/String;)I")?;
        let i: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> i32 = jni_fn(env, 49);
        let k = i(env, cur, col, chaine(env, c"_display_name"));
        if exception(env) || k < 0 {
            return None;
        }
        let get = methode(env, cur, c"getString", c"(I)Ljava/lang/String;")?;
        let s = f(env, cur, get, k);
        if exception(env) || s.is_null() {
            return None;
        }
        texte(env, s)
    })();
    lu.or_else(|| {
        let s = objet0(env, uri, c"getLastPathSegment", c"()Ljava/lang/String;")?;
        Some(texte(env, s)?.rsplit(['/', ':']).next()?.to_string())
    })
    .unwrap_or_else(|| "fichier".to_string())
}

/// Fichier transmis à l'activité (Ouvrir avec, Partager), s'il y en a un.
unsafe fn fichier_recu(activity: *mut ANativeActivity) -> Option<(Vec<(String, Vec<u8>)>, i32)> {
    let (env, moi) = ((*activity).env as Jni, (*activity).clazz);
    let intent = objet0(env, moi, c"getIntent", c"()Landroid/content/Intent;")?;
    let uri = objet0(env, intent, c"getData", c"()Landroid/net/Uri;").or_else(|| {
        let m = methode(env, intent, c"getParcelableExtra", c"(Ljava/lang/String;)Landroid/os/Parcelable;")?;
        let f: AppelObj = jni_fn(env, 34);
        let r = f(env, intent, m, chaine(env, c"android.intent.extra.STREAM"));
        if exception(env) || r.is_null() { None } else { Some(r) }
    })?;
    Some((vec![lire_uri(env, moi, uri)?], 0))
}

/// Ouvre le document désigné et le lit en entier.
unsafe fn lire_uri(env: Jni, moi: Obj, uri: Obj) -> Option<(String, Vec<u8>)> {
    use std::io::Read;
    use std::os::fd::FromRawFd;
    let resolver = objet0(env, moi, c"getContentResolver", c"()Landroid/content/ContentResolver;")?;
    let nom = nom_document(env, resolver, uri);
    let m = methode(env, resolver, c"openFileDescriptor", c"(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;")?;
    let f: AppelObj = jni_fn(env, 34);
    let pfd = f(env, resolver, m, uri, chaine(env, c"r"));
    if exception(env) || pfd.is_null() {
        log("fichier reçu illisible");
        return None;
    }
    let get = methode(env, pfd, c"getFd", c"()I")?;
    let i: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> i32 = jni_fn(env, 49);
    let fd = dup(i(env, pfd, get));
    if let Some(close) = methode(env, pfd, c"close", c"()V") {
        let v: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) = jni_fn(env, 61);
        v(env, pfd, close);
        exception(env);
    }
    if fd < 0 {
        return None;
    }
    let mut contenu = Vec::new();
    std::fs::File::from_raw_fd(fd).read_to_end(&mut contenu).ok()?;
    Some((nom, contenu))
}

extern "C" {
    fn dup(fd: i32) -> i32;
}

/// Demande à l'activité Java d'ouvrir le sélecteur ; le choix revient par `fichier_choisi`.
fn selecteur(d: Selecteur) -> Result<(), String> {
    let (vm, moi) = ACTIVITE.lock().unwrap().ok_or("activité absente")?;
    let vm = vm as *mut *const *const c_void;
    unsafe {
        let attacher: unsafe extern "C" fn(*mut *const *const c_void, *mut Jni, *mut c_void) -> i32 = std::mem::transmute(*(*vm).add(4));
        let mut env: Jni = null_mut();
        if attacher(vm, &mut env, null_mut()) != 0 || env.is_null() {
            return Err("Java injoignable".into());
        }
        let m = methode(env, moi as Obj, c"choisir", c"(I)V").ok_or("méthode absente")?;
        let v: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) = jni_fn(env, 61);
        v(env, moi as Obj, m, d as i32);
        if exception(env) {
            return Err("appel refusé".into());
        }
    }
    Ok(())
}

/// `Activite.recu`, sur le fil principal : un document choisi, et combien en restent.
/// S'il est illisible, un contenu vide fait dire à l'application pourquoi elle n'a
/// rien ouvert.
unsafe extern "C" fn fichier_choisi(env: Jni, moi: Obj, uri: Obj, demande: i32, reste: i32) {
    let f = lire_uri(env, moi, uri).unwrap_or_else(|| ("fichier".to_string(), Vec::new()));
    let mut s = SHARED.lock().unwrap();
    s.lot.push(f);
    if reste == 0 {
        s.fichier = Some((std::mem::take(&mut s.lot), demande));
        WAKE.notify_all();
    }
}

/// Relie `Activite.recu` : la bibliothèque est chargée par NativeActivity, hors de
/// `System.loadLibrary`, et la recherche par nom de symbole ne la trouverait pas.
unsafe fn relier_java(activity: *mut ANativeActivity) {
    #[repr(C)]
    struct Methode {
        nom: *const std::ffi::c_char,
        sig: *const std::ffi::c_char,
        f: *mut c_void,
    }
    let (env, moi) = ((*activity).env as Jni, (*activity).clazz);
    let classe: unsafe extern "C" fn(Jni, Obj) -> Obj = jni_fn(env, 31);
    let inscrire: unsafe extern "C" fn(Jni, Obj, *const Methode, i32) -> i32 = jni_fn(env, 215);
    let m = Methode { nom: c"recu".as_ptr(), sig: c"(Landroid/net/Uri;II)V".as_ptr(), f: fichier_choisi as *mut c_void };
    if inscrire(env, classe(env, moi), &m, 1) != 0 || exception(env) {
        log("Activite.recu non reliée");
    }
    let globale: unsafe extern "C" fn(Jni, Obj) -> Obj = jni_fn(env, 21);
    let lacher: unsafe extern "C" fn(Jni, Obj) = jni_fn(env, 22);
    let nouvelle = ((*activity).vm as usize, globale(env, moi) as usize);
    if let Some((_, ancienne)) = ACTIVITE.lock().unwrap().replace(nouvelle) {
        lacher(env, ancienne as Obj);
    }
}

/// Chaîne Java ; un octet nul la couperait, il est retiré.
fn jstr(env: Jni, s: &str) -> Obj {
    let c = CString::new(s.replace('\0', "")).unwrap_or_default();
    unsafe { chaine(env, &c) }
}

/// Dépose un fichier dans Téléchargements par MediaStore, sans permission (Android 10+).
fn deposer(s: &Sortie) -> Result<(), String> {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    let (vm, resolver) = JAVA.lock().unwrap().ok_or("Android 10 ou plus est nécessaire")?;
    let (vm, resolver) = (vm as *mut *const *const c_void, resolver as Obj);
    unsafe {
        let attacher: unsafe extern "C" fn(*mut *const *const c_void, *mut Jni, *mut c_void) -> i32 = std::mem::transmute(*(*vm).add(4));
        let mut env: Jni = null_mut();
        if attacher(vm, &mut env, null_mut()) != 0 || env.is_null() {
            return Err("Java injoignable".into());
        }
        // Un fil natif n'a pas de cadre : ce cadre libère toutes les références locales
        let pousser: unsafe extern "C" fn(Jni, i32) -> i32 = jni_fn(env, 19);
        let retirer: unsafe extern "C" fn(Jni, Obj) -> Obj = jni_fn(env, 20);
        pousser(env, 32);
        let r = ecrire_media(env, resolver, s).and_then(|fd| {
            let mut f = std::fs::File::from_raw_fd(fd);
            f.write_all(&s.octets).map_err(|e| e.to_string())
        });
        retirer(env, null_mut());
        r
    }
}

/// Crée l'entrée et ouvre son fichier ; rend un descripteur à nous.
unsafe fn ecrire_media(env: Jni, resolver: Obj, s: &Sortie) -> Result<i32, String> {
    let classe: unsafe extern "C" fn(Jni, *const i8) -> Obj = jni_fn(env, 6);
    let id: unsafe extern "C" fn(Jni, Obj, *const i8, *const i8) -> *mut c_void = jni_fn(env, 33);
    let neuf: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> Obj = jni_fn(env, 28);
    let vide: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) = jni_fn(env, 61);
    let f: AppelObj = jni_fn(env, 34);

    let cv_c = classe(env, c"android/content/ContentValues".as_ptr() as *const _);
    if exception(env) || cv_c.is_null() {
        return Err("ContentValues absent".into());
    }
    let ctor = id(env, cv_c, c"<init>".as_ptr() as *const _, c"()V".as_ptr() as *const _);
    let cv = neuf(env, cv_c, ctor);
    let put = id(env, cv_c, c"put".as_ptr() as *const _, c"(Ljava/lang/String;Ljava/lang/String;)V".as_ptr() as *const _);
    if exception(env) || cv.is_null() || put.is_null() {
        return Err("ContentValues inutilisable".into());
    }
    vide(env, cv, put, jstr(env, "_display_name"), jstr(env, &s.nom));
    vide(env, cv, put, jstr(env, "mime_type"), jstr(env, s.mime));

    let dl = classe(env, c"android/provider/MediaStore$Downloads".as_ptr() as *const _);
    if exception(env) || dl.is_null() {
        return Err("Android 10 ou plus est nécessaire".into());
    }
    let champ: unsafe extern "C" fn(Jni, Obj, *const i8, *const i8) -> *mut c_void = jni_fn(env, 144);
    let lire: unsafe extern "C" fn(Jni, Obj, *mut c_void) -> Obj = jni_fn(env, 145);
    let u = champ(env, dl, c"EXTERNAL_CONTENT_URI".as_ptr() as *const _, c"Landroid/net/Uri;".as_ptr() as *const _);
    if exception(env) || u.is_null() {
        return Err("dossier Téléchargements introuvable".into());
    }
    let dossier = lire(env, dl, u);

    let m = methode(env, resolver, c"insert", c"(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;").ok_or("insert absent")?;
    let item = f(env, resolver, m, dossier, cv);
    if exception(env) || item.is_null() {
        return Err("création du fichier refusée".into());
    }
    let m = methode(env, resolver, c"openFileDescriptor", c"(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;").ok_or("ouverture absente")?;
    let pfd = f(env, resolver, m, item, chaine(env, c"w"));
    if exception(env) || pfd.is_null() {
        return Err("ouverture du fichier refusée".into());
    }
    let get = methode(env, pfd, c"getFd", c"()I").ok_or("descripteur absent")?;
    let i: unsafe extern "C" fn(Jni, Obj, *mut c_void, ...) -> i32 = jni_fn(env, 49);
    let fd = dup(i(env, pfd, get));
    if let Some(close) = methode(env, pfd, c"close", c"()V") {
        vide(env, pfd, close);
        exception(env);
    }
    if fd < 0 { Err("descripteur illisible".into()) } else { Ok(fd) }
}

/// Garde de quoi joindre Java depuis le fil de rendu, à partir d'Android 10.
unsafe fn retenir_java(activity: *mut ANativeActivity) {
    if (*activity).sdk_version < 29 {
        return;
    }
    let env = (*activity).env as Jni;
    let Some(resolver) = objet0(env, (*activity).clazz, c"getContentResolver", c"()Landroid/content/ContentResolver;") else { return };
    let globale: unsafe extern "C" fn(Jni, Obj) -> Obj = jni_fn(env, 21);
    *JAVA.lock().unwrap() = Some(((*activity).vm as usize, globale(env, resolver) as usize));
}

// ---------------------------------------------------------------------------
// Callbacks de l'activité
// ---------------------------------------------------------------------------

unsafe extern "C" fn on_window_created(_: *mut ANativeActivity, window: *mut ANativeWindow) {
    let mut s = SHARED.lock().unwrap();
    s.window = window;
    s.window_changed = true;
    WAKE.notify_all();
}

unsafe extern "C" fn on_window_changed(_: *mut ANativeActivity, _: *mut ANativeWindow) {
    SHARED.lock().unwrap().redraw = true;
    WAKE.notify_all();
}

/// Android détruit la fenêtre au retour : le rendu doit l'avoir lâchée avant. Un
/// fichier reçu ouvre une nouvelle activité : sa fenêtre peut arriver avant la
/// destruction de l'ancienne, qui ne doit alors pas l'effacer.
unsafe extern "C" fn on_window_destroyed(_: *mut ANativeActivity, window: *mut ANativeWindow) {
    let mut s = SHARED.lock().unwrap();
    if s.window == window {
        s.window = null_mut();
        s.window_changed = true;
        WAKE.notify_all();
    }
    let (_guard, _) = WAKE.wait_timeout_while(s, Duration::from_secs(2), |s| s.attached == window).unwrap();
}

unsafe extern "C" fn on_input(_fd: i32, _events: i32, data: *mut c_void) -> i32 {
    let queue = data as *mut AInputQueue;
    let mut event: *mut AInputEvent = null_mut();
    while AInputQueue_getEvent(queue, &mut event) >= 0 {
        if AInputQueue_preDispatchEvent(queue, event) != 0 {
            continue;
        }
        let mut handled = 0;
        if AInputEvent_getType(event) == AINPUT_EVENT_TYPE_MOTION {
            handled = 1;
            let action = AMotionEvent_getAction(event);
            let idx = ((action & ACTION_POINTER_INDEX_MASK) >> ACTION_POINTER_INDEX_SHIFT) as usize;
            let id = |i: usize| AMotionEvent_getPointerId(event, i);
            match action & ACTION_MASK {
                ACTION_DOWN | ACTION_POINTER_DOWN => {
                    push(&[Event::Down { id: id(idx), x: AMotionEvent_getX(event, idx), y: AMotionEvent_getY(event, idx) }])
                }
                ACTION_MOVE => {
                    let moves: Vec<Event> = (0..AMotionEvent_getPointerCount(event))
                        .map(|i| Event::Move { id: id(i), x: AMotionEvent_getX(event, i), y: AMotionEvent_getY(event, i) })
                        .collect();
                    push(&moves);
                }
                ACTION_UP | ACTION_POINTER_UP => push(&[Event::Up { id: id(idx) }]),
                ACTION_CANCEL => push(&[Event::Cancel]),
                _ => handled = 0,
            }
        }
        AInputQueue_finishEvent(queue, event, handled);
    }
    1
}

unsafe extern "C" fn on_queue_created(_: *mut ANativeActivity, queue: *mut AInputQueue) {
    AInputQueue_attachLooper(queue, ALooper_forThread(), 1, Some(on_input), queue);
}

unsafe extern "C" fn on_queue_destroyed(_: *mut ANativeActivity, queue: *mut AInputQueue) {
    AInputQueue_detachLooper(queue);
}

/// Un pixel indépendant de la densité (dp), en pixels réels.
unsafe fn density(activity: *mut ANativeActivity) -> f32 {
    let c = AConfiguration_new();
    AConfiguration_fromAssetManager(c, (*activity).asset_manager);
    let d = AConfiguration_getDensity(c);
    AConfiguration_delete(c);
    // 0 : non précisée, 0xFFFF / 0xFFFE : valeurs spéciales
    if (1..=1000).contains(&d) {
        d as f32 / 160.0
    } else {
        2.0
    }
}

/// Point d'entrée appelé par `android.app.NativeActivity`.
///
/// # Safety
/// Appelé uniquement par Android, avec une activité valide.
#[no_mangle]
pub unsafe extern "C" fn ANativeActivity_onCreate(activity: *mut ANativeActivity, _: *mut c_void, _: usize) {
    let fichier = fichier_recu(activity);
    retenir_java(activity);
    relier_java(activity);
    {
        let mut s = SHARED.lock().unwrap();
        s.dp = density(activity);
        let p = (*activity).internal_data_path;
        if !p.is_null() {
            s.interne = Some(PathBuf::from(std::ffi::CStr::from_ptr(p as *const _).to_string_lossy().into_owned()));
        }
        if fichier.is_some() {
            s.fichier = fichier;
        }
    }
    let cb = &mut *(*activity).callbacks;
    cb.on_native_window_created = Some(on_window_created);
    cb.on_native_window_redraw_needed = Some(on_window_changed);
    cb.on_native_window_resized = Some(on_window_changed);
    cb.on_native_window_destroyed = Some(on_window_destroyed);
    cb.on_input_queue_created = Some(on_queue_created);
    cb.on_input_queue_destroyed = Some(on_queue_destroyed);
    spawn_render_thread();
}
