// Exécute la vraie visserie de GDA, avec le vrai Manifold, et écrit tests/visserie.json :
// `tests/visserie.rs` rejoue ces pièces avec le portage Rust et boolmesh, et compare.
//
//   node tools/reference-visserie.js <dossier de GDA> tests/visserie.json
const fs = require('fs'), [GDA, SORTIE] = process.argv.slice(2);
global.window = global;
global.self = global;
global.document = { currentScript: { src: 'file://' + GDA + '/lib/manifold.js' } };
const lire = f => fs.readFileSync(GDA + '/' + f, 'utf8');
(0, eval)(lire('lib/three.min.js'));
(0, eval)(lire('moteur/maillage.js') + '\n;globalThis.Maillage = Maillage;');
(0, eval)(lire('moteur/booleens.js'));
(0, eval)(lire('moteur/visserie.js'));
// Manifold se croirait dans Node et chercherait ses modules : on lui passe le wasm
(0, eval)(lire('lib/manifold.js').replace(/ENVIRONMENT_IS_NODE=[^;,]*/, 'ENVIRONMENT_IS_NODE=false') + '\n;globalThis.ManifoldModule = ManifoldModule;');

const etat = m => ({ v: m.sommets, f: m.faces.map(f => f.s) });
const bilan = m => {
  const b = m.boite();
  return { volume: Visserie._volume(m), min: b && b.min, max: b && b.max, sommets: m.sommets.length, faces: m.faces.length };
};
const VIS = ['vis', 'ecrou', 'rondelle', 'tige'];

async function principal() {
  const M = await ManifoldModule({ wasmBinary: fs.readFileSync(GDA + '/lib/manifold.wasm') });
  M.setup();
  Booleens._M = M;
  Booleens.pret = true;

  // Pièces par anneaux : le maillage exact
  const anneaux = [];
  const tetes = ['hexagonale', 'cylindrique', 'fraisee'];
  for (const taille of ['M2', 'M2,5', 'M5', 'M8', 'M12', 'M24']) for (const tete of tetes) for (const filetage of ['total', 'partiel']) {
    const longueur = taille === 'M24' ? 70 : 3 * parseFloat(taille.slice(1).replace(',', '.')) + 8;
    anneaux.push({ piece: 'vis', o: { taille, tete, longueur, filetage, segments: 12, rangs: 4 } });
  }
  for (const taille of Object.keys(Visserie.TAILLES)) {
    anneaux.push({ piece: 'ecrou', o: { taille, segments: 18, rangs: 4 } });
    anneaux.push({ piece: 'rondelle', o: { taille, segments: 32 } });
    anneaux.push({ piece: 'tige', o: { taille, longueur: 12, segments: 24, rangs: 8 } });
  }
  const fin = { detail: true, d: 7.8, p: 1.5, profondeur: 0.7, sommet: 0.3, gorge: 0.1 };
  anneaux.push({ piece: 'vis', o: { taille: 'M8', tete: 'cylindrique', longueur: 20, filetage: 'total', segments: 24, rangs: 8, ...fin, filetee: 15, lisse: 4 } });
  anneaux.push({ piece: 'ecrou', o: { taille: 'M8', segments: 24, rangs: 8, ...fin, hauteur: 9, plats: 14 } });
  anneaux.push({ piece: 'rondelle', o: { taille: 'M8', segments: 24, detail: true, interieur: 9, exterieur: 20, epaisseur: 2.5 } });
  anneaux.push({ piece: 'tige', o: { taille: 'M8', longueur: 15, segments: 12, rangs: 6, ...fin } });
  for (const c of anneaux) Object.assign(c, etat(Visserie[c.piece](Object.assign({}, c.o))));

  // Outils de coupe
  const outils = [];
  for (const [taille, n, y0, y1, sortie, gorge] of [['M8', 24, -21, 0, true], ['M3', 12, -9, 3, false], ['M12', 48, -5, 5, true], ['M8', 24, -10, 10, false, 0]]) {
    const P = Visserie.parametres('tige', { taille, longueur: 20 });
    if (gorge === 0) P.gorge = 0;
    outils.push({ P, n, y0, y1, sortie, ...etat(Visserie.outil(P, n, y0, y1, sortie)) });
  }

  // Paramètres, problèmes, nombre de sommets : y compris les pièces impossibles
  const reglages = [];
  for (const piece of VIS) for (const taille of Object.keys(Visserie.TAILLES)) for (const tete of tetes) for (const longueur of [0.5, 2, 6, 30]) for (const methode of ['anneaux', 'soustraction']) {
    reglages.push({ piece, o: { taille, tete, longueur, filetage: longueur > 2 ? 'partiel' : 'total', segments: 24, rangs: 8, methode } });
  }
  for (const r of [{ p: 0 }, { sommet: 0.8, gorge: 0.8, p: 1.25 }, { profondeur: 5 }, { d: 30 }, { filetee: 0 }, { lisse: -1 }, { hauteur: 0 }, { plats: 2 }, { interieur: 20 }, { d: 1 }, { hauteur: 0.5 }]) {
    for (const piece of VIS) reglages.push({ piece, o: { taille: 'M8', tete: 'fraisee', longueur: 20, filetage: 'total', segments: 24, rangs: 8, methode: 'soustraction', detail: true, ...r } });
  }
  for (const c of reglages) {
    const P = Visserie.parametres(c.piece, c.o);
    c.parametres = P;
    c.probleme = Visserie.probleme(c.piece, c.o);
    c.sommets = Visserie.sommets(c.piece, c.o);
  }

  // Par soustraction, avec Manifold : volume et boîte, le maillage dépendant du moteur
  const coupes = [];
  const t0 = Date.now();
  for (const [piece, taille, tete, longueur, filetage, segments] of [
    ['vis', 'M3', 'hexagonale', 10, 'total', 24], ['vis', 'M8', 'hexagonale', 20, 'total', 24], ['vis', 'M8', 'cylindrique', 30, 'partiel', 24],
    ['vis', 'M8', 'fraisee', 25, 'total', 24], ['vis', 'M5', 'cylindrique', 16, 'total', 48], ['vis', 'M24', 'hexagonale', 70, 'partiel', 24],
    ['ecrou', 'M3', '', 0, '', 24], ['ecrou', 'M8', '', 0, '', 24], ['ecrou', 'M24', '', 0, '', 48],
    ['tige', 'M4', '', 15, '', 24], ['tige', 'M8', '', 50, '', 24], ['tige', 'M16', '', 40, '', 48]]) {
    const o = { taille, tete, longueur, filetage, segments, rangs: 8, methode: 'soustraction' };
    coupes.push({ piece, o, ...bilan(Visserie[piece](Object.assign({}, o))) });
  }
  console.log('soustraction :', coupes.length, 'pièces,', Date.now() - t0, 'ms');

  // Booléens entre primitives
  const formes = {
    cube: () => Maillage.cube(1), boite: () => Maillage.boite(2, 0.5, 1.5), cylindre: () => Maillage.cylindre(0.5, 2, 24), cone: () => Maillage.cone(0.8, 0.1, 1.4, 16),
    sphere: () => Maillage.sphereUV(0.7, 24, 12), ico: () => Maillage.icosphere(0.6, 3)
  };
  const deplace = (m, d, ech) => { m.sommets = m.sommets.map(p => p.map((v, k) => v * ech + d[k])); return m; };
  const booleens = [];
  for (const a of Object.keys(formes)) for (const b of Object.keys(formes)) for (const op of ['union', 'difference', 'intersection']) for (const d of [[0.13, 0.21, 0.07], [0, 0, 0], [0.5, 0, 0], [3, 0, 0]]) {
    const ech = 0.9;
    let r;
    try { r = bilan(Booleens.operation(formes[a](), deplace(formes[b](), d, ech), op)); } catch (e) { r = { erreur: e.message }; }
    booleens.push({ a, b, op, d, ech, ...r });
  }
  fs.writeFileSync(SORTIE, JSON.stringify({ anneaux, outils, reglages, coupes, booleens }));
}
principal().catch(e => { console.error(e); process.exit(1); });
