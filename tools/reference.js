// Exécute le moteur de GDA sur des suites d'opérations et écrit, pour chacune,
// les paramètres et les résultats dans tests/reference.json.
// `tests/differentiel.rs` rejoue ces opérations avec le portage Rust et compare.
//
//   node tools/reference.js <dossier de GDA> tests/reference.json
const fs = require('fs'), vm = require('vm'), [GDA, SORTIE] = process.argv.slice(2);
global.window = global;
const charger = f => vm.runInThisContext(fs.readFileSync(f, 'utf8') + '\n;', { filename: f });
charger(GDA + '/lib/three.min.js');
charger(GDA + '/moteur/maillage.js');
vm.runInThisContext('globalThis.Maillage = Maillage');

// Tirages reproductibles : les paramètres sont écrits dans le fichier, Rust ne tire rien
let graine = 20261101;
const hasard = n => { graine = (graine * 48271) % 2147483647; return graine % n; };
const reel = (a, b) => a + (b - a) * (hasard(1e6) / 1e6);
const parmi = t => t[hasard(t.length)];

const etat = m => ({ v: m.sommets.map(p => p.slice()), f: m.faces.map(f => [f.m, ...f.s]), l: m.fils.map(a => a.slice()) });
const hex = b => Buffer.from(b).toString('hex');
const faceIdx = (m, l) => l.map(f => m.faces.indexOf(f));
const octets = s => new TextEncoder().encode(s).buffer;

const OPS = {
  subdiviser: (m, a, c) => [...m.subdiviser(a, c)].map(([k, l]) => [k.split(' ').map(Number), l]).sort((x, y) => x[0][0] - y[0][0] || x[0][1] - y[0][1]),
  couper: (m, i, j, c, g, u, inv) => {
    const an = m.anneau(i, j);
    if (!an) return null;
    const r = { rails: an.rails, quads: faceIdx(m, an.quads), depart: an.depart, ferme: an.ferme };
    r.aretes = m.couperBoucle(an, c, g, u, inv);
    return r;
  },
  reunir: (m, g) => m.reunir(g),
  fusionner: (m, l, c) => m.fusionner(l, c),
  proches: (m, t) => { const r = m.fusionnerProches(undefined, t); return [r.table, r.groupes, r.retires]; },
  supprimer: (m, l) => m.supprimerSommets(l),
  souder: (m, t) => { m.souder(t); return null; },
  revolution: (m, s, a, o) => {
    const r = m.revolution(s, a, o);
    return { faces: faceIdx(m, r.faces), fils: r.fils, copies: [...r.copies] };
  },
  contour: (m, l) => { const c = m.contourFaces(l.map(n => m.faces[n])); return c.erreur ? { erreur: c.erreur } : c; },
  dissoudre: (m, a) => {
    const r = m.dissolution(a);
    return r.erreur ? { erreur: r.erreur } : r.groupes.map(g => ({ faces: faceIdx(m, g.faces), s: g.s, plane: g.plane }));
  },
  cheminS: (m, a, b) => m.chemin('s', a, b),
  cheminA: (m, a, b) => m.chemin('a', a, b),
  cheminF: (m, a, b) => { const l = m.chemin('f', m.faces[a], m.faces[b]); return l && faceIdx(m, l); },
  connexe: (m, l) => { const r = m.connexe(l); return [r.sommets, faceIdx(m, r.faces)]; },
  aretes: m => m.aretes(),
  ilots: m => m.ilots(),
  boite: m => m.boite(),
  trianguler: m => m.faces.map(f => m.trianguler(f)),
  triangles: m => [...m.triangles()],
  normales: m => m.faces.map(f => m.normale(f)),
  parMateriau: m => m.parMateriau().map(x => [x.m, etat(x.maillage)]),
  obj: (m, nom) => m.versOBJ(nom, 'materiaux.mtl'),
  stl: (m, nom) => hex(m.versSTL(nom)),
  glb: (m, nom, mats) => hex(m.versGLB(nom, mats)),
  donnees: m => m.versDonnees(),
  relireObj: m => etat(Maillage.depuisOBJ(m.versOBJ('x'))),
  relireStl: m => etat(Maillage.depuisSTL(m.versSTL('x'))),
  relireGlb: m => etat(Maillage.depuisGLTF(m.versGLB('x'))),
};

const BASES = {
  boite: a => Maillage.boite(...a), cone: a => Maillage.cone(...a), cylindre: a => Maillage.cylindre(...a),
  sphereUV: a => Maillage.sphereUV(...a), icosphere: a => Maillage.icosphere(...a),
  obj: a => Maillage.depuisOBJ(a[0]),
};

const scenarios = [];
const lancer = (base, etapes) => {
  const m = BASES[base[0]](base.slice(1)), res = [];
  for (const e of etapes) {
    const [op, ...args] = typeof e === 'function' ? e(m) : e;
    const r = OPS[op](m, ...args);
    res.push({ op, args, r: r === undefined ? null : r, m: etat(m) });
  }
  scenarios.push({ base, etapes: res });
};

const INFOS = [['aretes'], ['ilots'], ['boite'], ['trianguler'], ['triangles'], ['normales'], ['donnees'], ['obj', 'objet é'], ['stl', 'objet é'],
  ['glb', 'objet é', { defaut: { couleur: '#c9c3d8', opacite: 0.5, emission: '#102030' }, bois: '#804020' }], ['parMateriau'], ['relireObj'], ['relireStl'], ['relireGlb']];

// Toutes les primitives, à tous leurs réglages
for (const b of [['boite', 1, 2, 3], ['boite', 0.5, 0.5, 0.5], ['cone', 0.5, 0, 1, 8, 'polygone'], ['cone', 0.5, 0.2, 1, 7, 'eventail'], ['cone', 1, 0, 2, 16, 'eventail'],
  ['cylindre', 0.5, 1, 16, 'polygone'], ['cylindre', 0.3, 2, 5, 'eventail'], ['sphereUV', 0.5, 16, 8], ['sphereUV', 1, 5, 3], ['icosphere', 0.5, 1], ['icosphere', 1, 3]]) lancer(b, INFOS);

// Formats et cas particuliers de l'OBJ, polygones concaves
const CONCAVE = 'v 0 0 0\nv 4 0 0\nv 4 4 0\nv 2 1 0\nv 0 4 0\nv 1 1 0\nf 1 2 3 4 5 6\n';
const OBJ = '# essai\nmtllib a.mtl\no un\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 5 5 5\nusemtl bois\nf 1/1/1 2/2 3//1\nusemtl pierre\nf -4 -2 -1\nl 1 5\nl 2 2\nl 1 2\n';
lancer(['obj', CONCAVE], INFOS);
lancer(['obj', OBJ], INFOS);

// Opérations réglées à la main
lancer(['boite', 1, 1, 1], [['subdiviser', [[0, 1], [1, 3]], 2], ['aretes'], ['subdiviser', [[0, 4]], 1]]);
lancer(['boite', 1, 1, 1], [['couper', 0, 1, 1, 0, false, false], ['couper', 0, 2, 3, 0.4, false, false], ['couper', 1, 3, 2, -0.5, true, true]]);
lancer(['obj', 'v 0 0 0\nv 1 0 0\nv 3 0 0\nv 0 1 0\nv 1 1 0\nv 3 2 0\nv 0 2 0\nf 1 2 5 4\nf 2 3 6 5\nf 4 5 7\n'], [['couper', 0, 3, 2, 0, true, false], ['couper', 0, 1, 1, 0.3, true, true]]);
lancer(['boite', 1, 1, 1], [['fusionner', [0, 1], 'premier'], ['fusionner', [0, 1], 'dernier'], ['fusionner', [0, 1, 2], 'centre']]);
lancer(['boite', 1, 1, 1], [['reunir', [[0, 3], [5, 6]]], ['supprimer', [0]], ['souder', 1e-5]]);
lancer(['cylindre', 0.5, 1, 8, 'polygone'], [['contour', [0, 1, 2]], ['contour', [0, 2]], ['contour', [0]], ['dissoudre', [[0, 1]]], ['dissoudre', []]]);
lancer(['boite', 1, 1, 1], [['contour', [0, 1, 2, 3, 4, 5]], ['dissoudre', [[0, 1], [1, 3]]], ['dissoudre', [[0, 7]]]]);
lancer(['obj', 'v 0 1 0\nv 1 1 0\nv 1 0 0\nv 2 0 0\nl 1 2\nl 2 3\nl 3 4\n'], [['revolution', [], [[0, 1], [1, 2]], { angle: 360, segments: 8 }], ['aretes'], ['ilots']]);
lancer(['obj', 'v 1 0 0\nv 2 0 0\nv 2 1 0\nv 1 1 0\nf 1 2 3 4\n'], [['revolution', [], [[0, 1], [1, 2]], { angle: 90, segments: 3, axe: [0, 1, 0], centre: [0, 0, 0], materiau: 'bois' }]]);
lancer(['obj', 'v 1 0 0\nv 0 0 0\nv 3 3 3\n'], [['revolution', [0, 1, 2], [], { angle: 180, segments: 4, axe: [1, 1, 0], centre: [0, 0.5, 0] }], ['revolution', [0], [], { angle: 360, segments: 4, fermer: false }]]);
lancer(['sphereUV', 1, 8, 4], [['cheminS', 0, 49], ['cheminA', [0, 1], [30, 31]], ['cheminF', 0, 20], ['connexe', [3]]]);
lancer(['obj', 'v 0 0 0\nv 1 0 0\nv 0 1 0\nv 5 0 0\nv 6 0 0\nv 5 1 0\nv 9 9 9\nf 1 2 3\nf 4 5 6\nl 3 4\n'], [['ilots'], ['connexe', [0]], ['cheminS', 0, 6]]);
lancer(['obj', 'v 0 0 0\nv 0.00001 0 0\nv 1 0 0\nv 0 1 0\nv 1 0.000011 0\nf 1 3 4\nf 2 5 4\n'], [['proches', 1e-4], ['aretes']]);

// Suites tirées au hasard, paramètres choisis sur le maillage du moment
const arete = m => { const a = m.aretes(); return parmi(a); };
const TIRAGES = [
  m => ['subdiviser', [arete(m), arete(m)], 1 + hasard(3)],
  m => { const [i, j] = arete(m); return ['couper', i, j, 1 + hasard(3), reel(-0.8, 0.8), hasard(2) === 0, hasard(2) === 0]; },
  m => { const [i, j] = arete(m); return ['fusionner', [i, j], parmi(['centre', 'premier', 'dernier'])]; },
  m => ['dissoudre', [arete(m)]],
  m => ['contour', [hasard(m.faces.length), hasard(m.faces.length)]],
  m => ['cheminS', hasard(m.sommets.length), hasard(m.sommets.length)],
  m => ['cheminF', hasard(m.faces.length), hasard(m.faces.length)],
  m => ['proches', reel(0.001, 0.2)],
  m => { const [i, j] = arete(m); return ['revolution', [], [[i, j]], { angle: parmi([90, 180, 360]), segments: 2 + hasard(6), axe: parmi([[0, 1, 0], [1, 0, 0], [1, 1, 1]]), centre: [reel(-1, 1), 0, reel(-1, 1)] }]; },
];
for (let k = 0; k < 120; k++) {
  const b = parmi([['boite', 1, 2, 3], ['cylindre', 0.5, 1, 6, 'polygone'], ['sphereUV', 1, 6, 4], ['icosphere', 1, 2], ['cone', 1, 0.5, 2, 5, 'eventail']]);
  const et = Array.from({ length: 6 }, () => m => (m.faces.length && m.sommets.length ? parmi(TIRAGES)(m) : ['aretes']));
  lancer(b, [...et, ['aretes'], ['trianguler'], ['relireObj']]);
}

// MTL
const MTL = 'newmtl a b\nKd 1 0.5 0\nKa 0.2\nNs 96.1\nd 0.5\nillum 2.4\nmap_Kd x.png\nnewmtl vide\nTr 0.25\nKe 2 -1 0.5\nnewmtl\nNi 1.45\n';
const mtl = { texte: MTL, lu: Maillage.depuisMTL(MTL), ecrit: Maillage.versMTL({ un: { couleur: '#804020', brillance: 12.34567, illumination: 2.6 }, deux: {} }, 'titre') };

// Mode Scène : les vraies méthodes de l'éditeur, sur un `this` réduit
global.Context = class {};
global.document = { createElement: () => ({}) };
// Les tables de l'éditeur lisent quelques globales au chargement : des bouchons suffisent
global.Visserie = { TAILLES: {} };
charger(GDA + '/moteur/selection.js');
charger(GDA + '/ctx-editeur.js');
const E = vm.runInThisContext('EditeurContext').prototype;
const q0 = () => [reel(-1, 1), reel(-1, 1), reel(-1, 1), reel(-1, 1)];
const norm = q => { const l = Math.hypot(...q); return q.map(c => c / l); };
const QUARTS = [[0, 0, 0, 1], [0, Math.SQRT1_2, 0, Math.SQRT1_2], [Math.SQRT1_2, 0, 0, Math.SQRT1_2], [0, 0, 1, 0], [0.5, 0.5, 0.5, 0.5]];
const poses = [];
for (let k = 0; k < 200; k++) {
  const base = Maillage.boite(1, 2, 3), n = 1 + hasard(3);
  const objets = Array.from({ length: n }, () => ({
    maillage: base, armature: hasard(10) === 0 ? {} : null,
    position: [reel(-5, 5), reel(-5, 5), reel(-5, 5)], echelle: [reel(0.5, 2), reel(0.5, 2), reel(0.5, 2)],
    rotation: hasard(2) ? parmi(QUARTS).slice() : norm(q0())
  }));
  const ctx = {
    objets, state: { pivot: parmi(['median', 'boite', 'actif', 'curseur', 'origine']), curseur: [reel(-3, 3), reel(-3, 3), reel(-3, 3)], edition: 'scene' },
    choix: objets.map((_, i) => i), sel: {}, _arm: () => false, _geo: () => false,
  };
  ctx._points = E._points; ctx._indices = E._indices; ctx._centre = E._centre; ctx._depart = E._depart;
  const avant = JSON.parse(JSON.stringify(objets.map(o => ({ a: !!o.armature, p: o.position, q: o.rotation, e: o.echelle }))));
  const c = [reel(-3, 3), reel(-3, 3), reel(-3, 3)], nature = hasard(3), indices = objets.map((_, i) => i);
  const t = nature === 0 ? { d: [reel(-3, 3), reel(-3, 3), reel(-3, 3)] }
    : nature === 1 ? { q: norm(q0()), c } : { s: [reel(0.2, 3), hasard(2) ? 1 : reel(0.2, 3), 1], c };
  const r = { pivot: ctx.state.pivot, curseur: ctx.state.curseur, attendu: E._pivot.call(ctx), centre: E._centre.call(ctx) };
  const V = a => a && new THREE.Vector3().fromArray(a);
  E._poser.call(ctx, indices, ctx._depart(indices), V(t.d), t.q && new THREE.Quaternion().fromArray(t.q), V(t.c), V(t.s));
  poses.push({ avant, t, ...r, apres: objets.map(o => ({ p: o.position, q: o.rotation, e: o.echelle, v: o.maillage === base ? null : o.maillage.sommets })) });
}
const eulers = Array.from({ length: 50 }, () => { const q = hasard(5) ? norm(q0()) : parmi(QUARTS); return { q, deg: E._euler.call({}, { rotation: q }) }; });

// Mode Géométrie : les vraies méthodes, sur un éditeur sans interface
const ED_OPS = {
  'face': c => c._creerFace(), 'arete': c => c._creerArete(), 'sup-a': c => c._supprimerAretes(), 'inverser': c => c._inverser(),
  'trianguler': c => c._trianguler(), 'dissoudre': c => c._dissoudre(), 'sup-f': c => c._supprimerFaces(), 'sup-s': c => c._supprimerSommets(),
  'sommet': c => c._ajouterSommet(), 'lies': c => c._selectionnerLies(), 'tout-s': c => c._toutChoisir(true, 's'), 'tout-a': c => c._toutChoisir(true, 'a'),
  'tout-f': c => c._toutChoisir(true, 'f'), 'aucun-s': c => c._toutChoisir(false, 's'), 'aucun': c => c._toutChoisir(false),
  'boucle': c => c._boucleCoupe(), 'separer': c => c._separer()
};
for (const [de, vers] of [['s', 'a'], ['s', 'f'], ['a', 's'], ['a', 'f'], ['f', 's'], ['f', 'a']]) ED_OPS['c-' + de + vers] = c => c._convertirSelection(de, vers);
for (const n of [1, 2, 3]) ED_OPS['subdiviser-' + n] = c => c._subdiviser(n);
for (const cible of ['centre', 'premier', 'dernier']) ED_OPS['fusion-' + cible] = c => c._fusionner(cible);
ED_OPS['proches'] = c => c._message(c._fusionProches({ tolerance: 0.3 }));
ED_OPS['revolution'] = c => c._message(c._revolution({ axe: 'y', angle: 90, sens: 1, segments: 3, fermer: false }));
ED_OPS['revolution-ferme'] = c => c._message(c._revolution({ axe: 'x', angle: 360, sens: -1, segments: 6, fermer: true }));
const SEL = global.SelectionMaillage || vm.runInThisContext('SelectionMaillage');
// Sans l'arrondi de versDonnees
const brut = m => ({ v: m.sommets, f: m.faces.map(f => [f.m, ...f.s]), l: m.fils });
const ops = Object.keys(ED_OPS), geometrie = [];
for (let k = 0; k < 400; k++) {
  const faites = [], c = Object.create(E), n = 1 + hasard(2);
  const formes = [() => Maillage.cube(2), () => Maillage.boite(1, 2, 3), () => Maillage.cylindre(1, 2, 5, false), () => Maillage.sphereUV(1, 6, 4)];
  const objets = Array.from({ length: n }, (_, i) => ({ nom: 'o' + i, maillage: parmi(formes)(), armature: null, modifs: [], set: 'main set', visible: true, materiau: 'defaut', sel: [],
    position: [reel(-3, 3), reel(-3, 3), reel(-3, 3)], echelle: [reel(0.5, 2), reel(0.5, 2), reel(0.5, 2)], rotation: norm(q0()) }));
  const avant = objets.map(o => ({ v: JSON.parse(JSON.stringify(brut(o.maillage))), p: o.position, q: o.rotation, e: o.echelle }));
  let msg = null;
  Object.assign(c, { objets, sets: [{ nom: 'main set', visible: true }], choix: objets.map((_, i) => i), bouts: [], edites: objets.map((_, i) => i),
    state: { edition: 'geometrie', actif: n - 1, curseur: [reel(-2, 2), reel(-2, 2), reel(-2, 2)], filtres: { sommets: true, aretes: true, faces: true } },
    save() {}, _afficherReglages() {}, _sauver() { if (this.maillage) this.sel.nettoyer(this.maillage); }, _rafraichir() {}, _garderSelection() {}, _memoriser() {}, _montrerAjuster() {}, _message(m) { msg = m; }, _affiche: () => true,
    _objet(mm, nom, position, rotation, sel, set, visible, modifs, materiau, echelle) { return { nom, maillage: mm, position, rotation, sel, set, visible, modifs, materiau, echelle, armature: null }; } });
  const curseur = c.state.curseur.slice();
  c._ouvrirEdition();
  const etat = () => {
    const d = c.sel.versDonnees(c.maillage), rang = new Map(c.maillage.faces.map((f, i) => [f, i])), e = c.sel.dernier;
    return { m: JSON.parse(JSON.stringify(brut(c.maillage))), proprio: c.proprio.slice(), sel: { s: c.sel.sommets.slice(), a: c.sel.aretes.map(a => a.slice()), f: c.sel.faces.map(f => rang.get(f)) }, tout: d, filtres: [c.state.filtres.sommets, c.state.filtres.aretes, c.state.filtres.faces],
      dernier: e ? (e.t === 's' ? ['s', e.i] : e.t === 'a' ? ['a', ...e.a] : ['f', rang.get(e.f)]) : null };
  };
  for (let e = 0; e < 5 && c.state.edition === 'geometrie'; e++) {
    const mm = c.maillage, choisis = [], nb = hasard(5);
    const toutes = mm.aretes();
    for (let i = 0; i < nb; i++) {
      const t = hasard(3);
      if (t === 0 && mm.sommets.length) { const x = hasard(mm.sommets.length); choisis.push(['s', x]); }
      if (t === 1 && toutes.length) { const a = toutes[hasard(toutes.length)]; choisis.push(['a', a[0], a[1]]); }
      if (t === 2 && mm.faces.length) { const x = hasard(mm.faces.length); choisis.push(['f', x]); }
    }
    if (hasard(3) === 0 && mm.sommets.length > 1) { const f = mm.faces.length ? mm.faces[hasard(mm.faces.length)].s : [0, 1]; for (const i of f.slice(0, 2 + hasard(2))) choisis.push(['s', i]); }
    for (const x of choisis) c.sel.basculer(x[0] === 's' ? { t: 's', i: x[1] } : x[0] === 'a' ? { t: 'a', a: [x[1], x[2]] } : { t: 'f', f: mm.faces[x[1]] });
    const filtre = hasard(6) === 0 ? hasard(2) : -1;
    if (filtre >= 0) c._reglerFiltres({ sommets: filtre !== 0, aretes: filtre !== 1, faces: true });
    const op = parmi(ops);
    msg = null;
    ED_OPS[op](c);
    faites.push({ choisis, filtre, op, msg, apres: c.state.edition === 'geometrie' ? etat() : null, objets: c.objets.length });
  }
  if (c.state.edition === 'geometrie') c._rangerEdition();
  geometrie.push({ avant, actif: n - 1, curseur, faites, fin: c.objets.map(o => ({ nom: o.nom, sel: o.sel, m: brut(o.maillage) })) });
}
fs.writeFileSync(SORTIE, JSON.stringify({ scenarios, mtl, poses, eulers, geometrie }));
console.log(scenarios.length, 'suites,', scenarios.reduce((n, s) => n + s.etapes.length, 0), 'opérations');
