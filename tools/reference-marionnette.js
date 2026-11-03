// Exécute la vraie saisie directe de GDA (moteur/marionnette.js) sur des humanoïdes, avec
// diverses ancres et articulations soudées, et écrit les poses obtenues dans
// tests/marionnette.json ; `tests/marionnette.rs` les rejoue et compare.
//
//   node tools/reference-marionnette.js <dossier de GDA> tests/marionnette.json
//
// Les limites partielles n'y figurent pas : GDA compose ses angles en X, Y, Z, Rustcraft
// en X, Z, Y, et un axe bloqué seul n'y tourne donc pas autour du même axe.
const fs = require('fs'), vm = require('vm'), [GDA, SORTIE] = process.argv.slice(2);
global.window = global;
const charger = f => vm.runInThisContext(fs.readFileSync(f, 'utf8') + '\n;', { filename: f });
charger(GDA + '/lib/three.min.js');
charger(GDA + '/moteur/maillage.js');
charger(GDA + '/moteur/armature.js');
charger(GDA + '/moteur/marionnette.js');
vm.runInThisContext('globalThis.Armature = Armature; globalThis.Marionnette = Marionnette');

let graine = 20261103;
const hasard = n => { graine = (graine * 48271) % 2147483647; return graine % n; };
const reel = (a, b) => a + (b - a) * (hasard(1e6) / 1e6);
const parmi = t => t[hasard(t.length)];

const matrices = a => {
  const r = {}, t = new THREE.Vector3(), q = new THREE.Quaternion(), e = new THREE.Vector3();
  for (const [id, m] of a.matricesPose()) { m.decompose(t, q, e); r[id] = [t.toArray(), q.toArray()]; }
  return r;
};

const cas = [];
for (const [ancre, soudes] of [[null, []], ['pied.G', []], ['main.D', ['colonne', 'thorax']], ['tete', ['jambe.D']], ['bras.G', ['cou']]]) {
  const a = Armature.humanoide(), id = n => a.os.find(b => b.nom === n).id;
  for (const n of soudes) for (const k of [0, 1, 2]) a.contraindreRotation(id(n), k, { etat: 'bloque' });
  if (ancre) a.ancre = id(ancre);
  const m = new Marionnette(a), donnees = a.versDonnees(), saisies = [];
  for (let n = 0; n < 6; n++) {
    const o = parmi(a.os.map(b => b.id)), s0 = a.segments().get(o), k = reel(0.2, 1);
    // Le point saisi, le long de l'os posé
    const mp = a.matricesPose().get(o), b = a.trouver(o);
    const point = new THREE.Vector3(0, b.longueur * k, 0).applyMatrix4(mp);
    const s = m.saisir(o, point), cibles = [], poses = [];
    let c = point.clone();
    for (let i = 0; i < 4; i++) {
      c = c.clone().add(new THREE.Vector3(reel(-0.3, 0.3), reel(-0.3, 0.3), reel(-0.3, 0.3)));
      m.tirer(s, c);
      cibles.push(c.toArray());
      poses.push(matrices(a));
    }
    saisies.push({ os: o, point: point.toArray(), chaine: s.chaine.map(x => [x.j, x.inverse]), cibles, poses });
    void s0;
  }
  cas.push({ donnees, ancre: a.ancre, saisies });
}
fs.writeFileSync(SORTIE, JSON.stringify({ cas }));
console.log(cas.length + ' cas, ' + cas.reduce((s, c) => s + c.saisies.length, 0) + ' saisies');
