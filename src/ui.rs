//! Tactile : à qui appartient chaque doigt.
//!
//! Chaque doigt reste à ce qu'il a touché en premier, jusqu'à ce qu'il se lève.
//! Les zones touchables sont celles déclarées à l'image précédente, dernière
//! déclarée au-dessus. Un doigt qui ne touche aucune zone part vers l'espace de
//! travail, où il devient un geste : un doigt tourne, deux doigts zooment et déplacent.

use crate::draw::Rect;
use std::collections::HashMap;

pub type Id = u64;

/// Identifiant stable tiré d'un texte (FNV-1a).
pub const fn id(s: &str) -> Id {
    let b = s.as_bytes();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < b.len() {
        h = (h ^ b[i] as u64).wrapping_mul(0x0000_0100_0000_01B3);
        i += 1;
    }
    h
}

pub fn sub_id(parent: Id, i: usize) -> Id {
    (parent ^ i as u64).wrapping_mul(0x0000_0100_0000_01B3)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Down { id: i32, x: f32, y: f32 },
    Move { id: i32, x: f32, y: f32 },
    Up { id: i32 },
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sense {
    /// Appui bref. Si `scroll` est donné, un glissement passe à cette zone défilante.
    Click { scroll: Option<Id> },
    /// Le doigt pilote directement la zone (curseur, poignée).
    Drag,
    Scroll,
    /// Absorbe le doigt sans rien faire : le fond d'un panneau.
    Sink,
}

#[derive(Clone, Copy)]
struct Region {
    id: Id,
    rect: Rect,
    clip: Option<Rect>,
    sense: Sense,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Owner {
    Widget(Id),
    Scroll(Id),
    Workspace,
    /// Doigt saisi par l'application : il pilote une poignée, pas la vue.
    Prise,
    Nobody,
}

#[derive(Clone, Copy)]
struct Pointer {
    id: i32,
    x: f32,
    y: f32,
    start: (f32, f32),
    owner: Owner,
    slid: bool,
    /// Temps passé posé, en secondes.
    age: f32,
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Drag {
    pub x: f32,
    pub y: f32,
    pub moved: bool,
    /// Où le doigt s'est posé : plusieurs événements peuvent précéder la première image.
    pub depart: (f32, f32),
}

/// Geste accumulé sur l'espace de travail depuis la dernière image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gesture {
    pub orbit: (f32, f32),
    pub pan: (f32, f32),
    pub zoom: f32,
}

impl Default for Gesture {
    fn default() -> Gesture {
        Gesture { orbit: (0.0, 0.0), pan: (0.0, 0.0), zoom: 1.0 }
    }
}

/// Appui bref sur l'espace de travail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tap {
    pub x: f32,
    pub y: f32,
    /// Deuxième appui au même endroit.
    pub double: bool,
}

const DOUBLE_TAP_S: f64 = 0.35;
const APPUI_LONG_S: f32 = 0.5;

pub struct Ui {
    pub dp: f32,
    regions: Vec<Region>,
    next: Vec<Region>,
    clip: Option<Rect>,
    pointers: Vec<Pointer>,
    clicks: Vec<Id>,
    double_taps: Vec<Id>,
    last_tap: Option<(Id, f64)>,
    drags: HashMap<Id, Drag>,
    /// Défilement en attente : (horizontal, vertical).
    scrolls: HashMap<Id, (f32, f32)>,
    gesture: Gesture,
    tap: Option<Tap>,
    dernier_appui: Option<(f32, f32, f64)>,
    /// Doigt seul qui vient de franchir le seuil : (id, point de départ).
    glisse: Option<(i32, (f32, f32))>,
    /// Dernière position du doigt saisi, et s'il est encore posé.
    prise: Option<((f32, f32), bool)>,
    /// Boutons qui écoutent l'appui long : déclarés à chaque image, comme les zones.
    ecoutes: Vec<Id>,
    ecoutes_next: Vec<Id>,
    longs: Vec<Id>,
    /// Un événement vient d'être traité : voir `a_redessiner`.
    agite: bool,
}

impl Ui {
    pub fn new(dp: f32) -> Ui {
        Ui {
            dp,
            regions: Vec::new(),
            next: Vec::new(),
            clip: None,
            pointers: Vec::new(),
            clicks: Vec::new(),
            double_taps: Vec::new(),
            last_tap: None,
            drags: HashMap::new(),
            scrolls: HashMap::new(),
            gesture: Gesture::default(),
            tap: None,
            dernier_appui: None,
            glisse: None,
            prise: None,
            ecoutes: Vec::new(),
            ecoutes_next: Vec::new(),
            longs: Vec::new(),
            agite: false,
        }
    }

    fn slop(&self) -> f32 {
        8.0 * self.dp
    }

    fn hit(&self, x: f32, y: f32) -> Option<Region> {
        self.regions.iter().rev().find(|r| r.rect.contains(x, y) && r.clip.is_none_or(|c| c.contains(x, y))).copied()
    }

    fn region(&self, id: Id) -> Option<Region> {
        self.regions.iter().rev().find(|r| r.id == id).copied()
    }

    /// `t` : instant de l'événement, en secondes.
    pub fn event(&mut self, ev: Event, t: f64) {
        self.agite = true;
        match ev {
            Event::Down { id, x, y } => {
                let owner = match self.hit(x, y) {
                    None => Owner::Workspace,
                    Some(r) => match r.sense {
                        Sense::Click { .. } => Owner::Widget(r.id),
                        Sense::Drag => {
                            self.drags.insert(r.id, Drag { x, y, moved: false, depart: (x, y) });
                            Owner::Widget(r.id)
                        }
                        Sense::Scroll => Owner::Scroll(r.id),
                        Sense::Sink => Owner::Nobody,
                    },
                };
                self.pointers.retain(|p| p.id != id);
                self.pointers.push(Pointer { id, x, y, start: (x, y), owner, slid: false, age: 0.0 });
            }
            Event::Move { id, x, y } => self.moved(id, x, y),
            Event::Up { id } => {
                if let Some(i) = self.pointers.iter().position(|p| p.id == id) {
                    let p = self.pointers.remove(i);
                    self.released(&p, t);
                }
            }
            Event::Cancel => self.pointers.clear(),
        }
    }

    fn moved(&mut self, id: i32, x: f32, y: f32) {
        let Some(i) = self.pointers.iter().position(|p| p.id == id) else { return };
        let (ox, oy) = (self.pointers[i].x, self.pointers[i].y);
        let (sx, sy) = self.pointers[i].start;
        self.pointers[i].x = x;
        self.pointers[i].y = y;
        if !self.pointers[i].slid && (x - sx).hypot(y - sy) > self.slop() {
            self.pointers[i].slid = true;
            if self.pointers[i].owner == Owner::Workspace && self.seul_sur_l_espace() {
                self.glisse = Some((id, (sx, sy)));
            }
        }
        let slid = self.pointers[i].slid;
        match self.pointers[i].owner {
            Owner::Widget(w) => match self.region(w).map(|r| r.sense) {
                Some(Sense::Click { scroll: Some(s) }) if slid => self.pointers[i].owner = Owner::Scroll(s),
                Some(Sense::Drag) => {
                    self.drags.insert(w, Drag { x, y, moved: slid, depart: (sx, sy) });
                }
                _ => {}
            },
            Owner::Scroll(s) if slid => {
                let e = self.scrolls.entry(s).or_default();
                e.0 += x - ox;
                e.1 += y - oy;
            }
            Owner::Workspace if !self.a_une_prise() => self.workspace_moved(i, ox, oy),
            Owner::Prise => self.prise = Some(((x, y), true)),
            _ => {}
        }
    }

    fn a_une_prise(&self) -> bool {
        self.pointers.iter().any(|p| p.owner == Owner::Prise)
    }

    fn seul_sur_l_espace(&self) -> bool {
        self.pointers.iter().filter(|p| p.owner == Owner::Workspace).count() == 1 && !self.a_une_prise()
    }

    fn workspace_moved(&mut self, i: usize, ox: f32, oy: f32) {
        let me = self.pointers[i];
        let others: Vec<Pointer> = self.pointers.iter().filter(|p| p.owner == Owner::Workspace && p.id != me.id).copied().collect();
        match others.first() {
            None if me.slid => {
                self.gesture.orbit.0 += me.x - ox;
                self.gesture.orbit.1 += me.y - oy;
            }
            None => {}
            Some(o) => {
                let before = (ox - o.x).hypot(oy - o.y).max(1.0);
                let after = (me.x - o.x).hypot(me.y - o.y).max(1.0);
                self.gesture.zoom *= after / before;
                self.gesture.pan.0 += (me.x - ox) / 2.0;
                self.gesture.pan.1 += (me.y - oy) / 2.0;
            }
        }
    }

    fn released(&mut self, p: &Pointer, t: f64) {
        if p.owner == Owner::Prise {
            if let Some(e) = &mut self.prise {
                e.1 = false;
            }
        }
        if p.owner == Owner::Workspace && !p.slid && !self.pointers.iter().any(|q| q.owner == Owner::Workspace) {
            let (x, y) = p.start;
            let double = self.dernier_appui.is_some_and(|(a, b, at)| t - at < DOUBLE_TAP_S && (x - a).hypot(y - b) < 3.0 * self.slop());
            self.tap = Some(Tap { x, y, double });
            self.dernier_appui = if double { None } else { Some((x, y, t)) };
        }
        let Owner::Widget(w) = p.owner else { return };
        if matches!(self.region(w).map(|r| r.sense), Some(Sense::Drag)) {
            self.drags.remove(&w);
        }
        if p.slid {
            return;
        }
        self.clicks.push(w);
        match self.last_tap {
            Some((prev, at)) if prev == w && t - at < DOUBLE_TAP_S => {
                self.double_taps.push(w);
                self.last_tap = None;
            }
            _ => self.last_tap = Some((w, t)),
        }
    }

    /// Vrai une fois après une image qui a reçu un événement. Un widget apprend qu'il
    /// est touché en se dessinant : l'état change donc après son dessin, et sans une
    /// image de plus, l'écran resterait en retard jusqu'au prochain événement.
    pub fn a_redessiner(&mut self) -> bool {
        std::mem::take(&mut self.agite)
    }

    pub fn set_clip(&mut self, clip: Option<Rect>) {
        self.clip = clip;
    }

    pub fn region_add(&mut self, id: Id, rect: Rect, sense: Sense) {
        self.next.push(Region { id, rect, clip: self.clip, sense });
    }

    /// Zone cliquable ; vrai à l'image qui suit l'appui.
    pub fn click(&mut self, id: Id, rect: Rect, scroll: Option<Id>) -> bool {
        self.region_add(id, rect, Sense::Click { scroll });
        self.clicked(id)
    }

    pub fn clicked(&self, id: Id) -> bool {
        self.clicks.contains(&id)
    }

    pub fn double_tapped(&self, id: Id) -> bool {
        self.double_taps.contains(&id)
    }

    /// Zone pilotée au doigt : position courante du doigt tant qu'il est posé.
    pub fn drag(&mut self, id: Id, rect: Rect) -> Option<Drag> {
        self.region_add(id, rect, Sense::Drag);
        self.drags.get(&id).copied()
    }

    pub fn pressed(&self, id: Id) -> bool {
        self.pointers.iter().any(|p| p.owner == Owner::Widget(id))
    }

    /// Zone déclarée à l'image précédente.
    pub fn zone(&self, id: Id) -> Option<Rect> {
        self.region(id).map(|r| r.rect)
    }

    pub fn take_scroll(&mut self, id: Id) -> f32 {
        self.scrolls.remove(&id).map_or(0.0, |d| d.1)
    }

    /// Comme `take_scroll`, pour une zone qui défile de gauche à droite.
    pub fn take_scroll_h(&mut self, id: Id) -> f32 {
        self.scrolls.remove(&id).map_or(0.0, |d| d.0)
    }

    /// Appui bref d'un seul doigt sur l'espace de travail, depuis la dernière image.
    pub fn take_tap(&mut self) -> Option<Tap> {
        self.tap.take()
    }

    /// Point de départ du doigt seul qui commence à glisser. L'application peut alors
    /// le saisir ; sinon il tourne la vue.
    pub fn take_glisse(&mut self) -> Option<(f32, f32)> {
        self.glisse.take().filter(|g| self.pointers.iter().any(|p| p.id == g.0)).map(|g| g.1)
    }

    /// Prend le dernier doigt annoncé par `take_glisse`, avec le départ de la vue qu'il avait donné.
    pub fn saisir(&mut self, depart: (f32, f32)) {
        if let Some(p) = self.pointers.iter_mut().find(|p| p.owner == Owner::Workspace && p.start == depart) {
            p.owner = Owner::Prise;
            self.prise = Some(((p.x, p.y), true));
            self.gesture.orbit = (0.0, 0.0);
        }
    }

    /// Position du doigt saisi, et s'il est encore posé.
    pub fn prise(&self) -> Option<((f32, f32), bool)> {
        self.prise
    }

    pub fn lacher_prise(&mut self) {
        self.prise = None;
    }

    /// Un doigt immobile sur l'espace de travail : les images doivent continuer pour
    /// voir l'appui long arriver.
    pub fn maintien(&self) -> bool {
        self.pointers.iter().any(|p| !p.slid && (p.owner == Owner::Workspace || matches!(p.owner, Owner::Widget(w) if self.ecoutes.contains(&w))))
    }

    /// Le bouton accepte l'appui long : vrai quand il aboutit, et l'appui bref n'a pas lieu.
    pub fn long_press(&mut self, id: Id) -> bool {
        self.ecoutes_next.push(id);
        self.longs.contains(&id)
    }

    /// Fait passer le temps ; rend l'endroit d'un appui long qui vient d'aboutir. Le
    /// doigt ne fait plus rien ensuite.
    pub fn avancer(&mut self, dt: f32) -> Option<(f32, f32)> {
        let seul = self.pointers.iter().filter(|p| p.owner == Owner::Workspace).count() == 1;
        let mut long = None;
        for p in &mut self.pointers {
            p.age += dt;
            if p.slid || p.age < APPUI_LONG_S {
                continue;
            }
            match p.owner {
                Owner::Workspace if seul => {
                    p.owner = Owner::Nobody;
                    long = Some(p.start);
                    self.agite = true;
                }
                Owner::Widget(w) if self.ecoutes.contains(&w) => {
                    p.owner = Owner::Nobody;
                    self.longs.push(w);
                    self.agite = true;
                }
                _ => {}
            }
        }
        long
    }

    pub fn take_gesture(&mut self) -> Gesture {
        std::mem::take(&mut self.gesture)
    }

    /// Fin d'image : les zones déclarées deviennent celles qui reçoivent les doigts.
    pub fn end_frame(&mut self) {
        std::mem::swap(&mut self.regions, &mut self.next);
        self.next.clear();
        self.clip = None;
        self.clicks.clear();
        self.double_taps.clear();
        self.longs.clear();
        std::mem::swap(&mut self.ecoutes, &mut self.ecoutes_next);
        self.ecoutes_next.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTN: Id = id("btn");
    const SCROLL: Id = id("scroll");
    const SLIDER: Id = id("slider");

    fn ui() -> Ui {
        let mut u = Ui::new(1.0);
        u.region_add(id("panel"), Rect::new(0., 0., 300., 300.), Sense::Sink);
        u.region_add(SCROLL, Rect::new(0., 0., 300., 200.), Sense::Scroll);
        u.region_add(BTN, Rect::new(10., 10., 100., 40.), Sense::Click { scroll: Some(SCROLL) });
        u.region_add(SLIDER, Rect::new(10., 100., 100., 40.), Sense::Drag);
        u.end_frame();
        u
    }

    fn down(u: &mut Ui, i: i32, x: f32, y: f32) {
        u.event(Event::Down { id: i, x, y }, 0.0);
    }

    #[test]
    fn appui_bref_sur_un_bouton() {
        let mut u = ui();
        down(&mut u, 0, 20., 20.);
        assert!(u.pressed(BTN));
        u.event(Event::Up { id: 0 }, 0.1);
        assert!(u.clicked(BTN));
        assert!(!u.pressed(BTN));
    }

    #[test]
    fn glisser_depuis_un_bouton_fait_defiler_sans_cliquer() {
        let mut u = ui();
        down(&mut u, 0, 20., 20.);
        u.event(Event::Move { id: 0, x: 20., y: 40. }, 0.1);
        u.event(Event::Move { id: 0, x: 20., y: 60. }, 0.1);
        assert_eq!(u.take_scroll(SCROLL), 20.0);
        u.event(Event::Up { id: 0 }, 0.2);
        assert!(!u.clicked(BTN));
    }

    #[test]
    fn curseur_capture_son_doigt_hors_de_la_zone() {
        let mut u = ui();
        down(&mut u, 0, 20., 110.);
        assert_eq!(u.drags.get(&SLIDER).map(|d| d.x), Some(20.));
        u.event(Event::Move { id: 0, x: 900., y: 900. }, 0.1);
        assert_eq!(u.drags.get(&SLIDER).map(|d| d.x), Some(900.));
        u.event(Event::Up { id: 0 }, 0.2);
        assert!(u.drags.is_empty());
    }

    #[test]
    fn double_appui() {
        let mut u = ui();
        for t in [0.0, 0.2] {
            down(&mut u, 0, 20., 20.);
            u.event(Event::Up { id: 0 }, t);
        }
        assert!(u.double_tapped(BTN));
    }

    #[test]
    fn un_doigt_tourne_deux_doigts_zooment() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        u.event(Event::Move { id: 0, x: 530., y: 500. }, 0.1);
        u.event(Event::Move { id: 0, x: 540., y: 510. }, 0.1);
        let g = u.take_gesture();
        assert_eq!(g.orbit, (40., 10.)); // le geste démarre avec le mouvement qui franchit le seuil
        down(&mut u, 1, 640., 510.);
        u.event(Event::Move { id: 1, x: 740., y: 510. }, 0.2);
        let g = u.take_gesture();
        assert_eq!(g.orbit, (0., 0.));
        assert!((g.zoom - 2.0).abs() < 1e-5);
    }

    #[test]
    fn saisir_un_doigt_lui_retire_la_vue() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        u.event(Event::Move { id: 0, x: 530., y: 500. }, 0.1);
        let d = u.take_glisse().unwrap();
        assert_eq!(d, (500., 500.));
        u.saisir(d);
        assert_eq!(u.take_gesture(), Gesture::default());
        u.event(Event::Move { id: 0, x: 560., y: 500. }, 0.1);
        assert_eq!(u.take_gesture(), Gesture::default());
        assert_eq!(u.prise(), Some(((560., 500.), true)));
        // Un second doigt ne détourne pas le geste en cours
        down(&mut u, 1, 700., 700.);
        u.event(Event::Move { id: 1, x: 800., y: 700. }, 0.2);
        assert_eq!(u.take_gesture(), Gesture::default());
        assert!(u.take_glisse().is_none());
        u.event(Event::Up { id: 0 }, 0.3);
        assert_eq!(u.prise(), Some(((560., 500.), false)));
        u.lacher_prise();
        assert_eq!(u.prise(), None);
    }

    #[test]
    fn glisser_sans_saisie_tourne_la_vue() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        u.event(Event::Move { id: 0, x: 530., y: 500. }, 0.1);
        assert!(u.take_glisse().is_some());
        assert_eq!(u.take_gesture().orbit, (30., 0.));
    }

    #[test]
    fn un_doigt_leve_aussitot_n_est_pas_saisi() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        u.event(Event::Move { id: 0, x: 530., y: 500. }, 0.1);
        u.event(Event::Up { id: 0 }, 0.1);
        assert!(u.take_glisse().is_none());
    }

    #[test]
    fn double_appui_sur_l_espace() {
        let mut u = ui();
        for (t, double) in [(0.0, false), (0.2, true), (0.3, false)] {
            down(&mut u, 0, 500., 500.);
            u.event(Event::Up { id: 0 }, t);
            assert_eq!(u.take_tap().unwrap().double, double);
        }
        down(&mut u, 0, 900., 500.);
        u.event(Event::Up { id: 0 }, 0.4);
        assert!(!u.take_tap().unwrap().double);
    }

    #[test]
    fn appui_long() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        assert!(u.maintien());
        assert!(u.avancer(0.3).is_none());
        assert_eq!(u.avancer(0.3), Some((500., 500.)));
        assert!(!u.maintien());
        u.event(Event::Up { id: 0 }, 0.7);
        assert!(u.take_tap().is_none());
    }

    #[test]
    fn une_image_de_plus_suit_chaque_evenement() {
        let mut u = ui();
        assert!(!u.a_redessiner());
        u.region_add(BTN, Rect::new(10., 10., 100., 40.), Sense::Click { scroll: None });
        u.end_frame();
        down(&mut u, 0, 20., 20.);
        u.event(Event::Up { id: 0 }, 0.1);
        assert!(u.clicked(BTN));
        u.end_frame();
        assert!(u.a_redessiner());
        // L'image suivante n'a rien reçu : la boucle peut s'endormir
        assert!(!u.a_redessiner());
    }

    #[test]
    fn une_zone_horizontale_defile_de_gauche_a_droite() {
        let mut u = ui();
        down(&mut u, 0, 20., 20.);
        u.event(Event::Move { id: 0, x: 60., y: 22. }, 0.1); // le bouton passe la main
        u.event(Event::Move { id: 0, x: 100., y: 22. }, 0.15);
        assert_eq!(u.take_scroll_h(SCROLL), 40.0);
        assert_eq!(u.take_scroll_h(SCROLL), 0.0);
        u.event(Event::Up { id: 0 }, 0.2);
        assert!(!u.clicked(BTN));
    }

    #[test]
    fn appui_long_sur_un_bouton_qui_l_ecoute() {
        let mut u = ui();
        u.long_press(BTN);
        u.region_add(BTN, Rect::new(10., 10., 100., 40.), Sense::Click { scroll: None });
        u.end_frame();
        down(&mut u, 0, 20., 20.);
        assert!(u.maintien());
        u.avancer(0.6);
        assert!(u.long_press(BTN));
        u.event(Event::Up { id: 0 }, 0.7);
        assert!(!u.clicked(BTN));
    }

    #[test]
    fn glisser_empeche_l_appui_long() {
        let mut u = ui();
        down(&mut u, 0, 500., 500.);
        u.event(Event::Move { id: 0, x: 530., y: 500. }, 0.1);
        assert!(u.avancer(1.0).is_none());
    }

    #[test]
    fn le_fond_d_un_panneau_ne_touche_pas_la_3d() {
        let mut u = ui();
        down(&mut u, 0, 250., 250.);
        u.event(Event::Move { id: 0, x: 290., y: 290. }, 0.1);
        assert_eq!(u.take_gesture(), Gesture::default());
    }

    #[test]
    fn deux_doigts_deux_boutons() {
        let mut u = ui();
        down(&mut u, 0, 20., 20.);
        down(&mut u, 1, 20., 110.);
        assert!(u.pressed(BTN) && u.pressed(SLIDER));
    }
}
