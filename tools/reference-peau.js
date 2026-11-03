// Exécute le vrai `moteur/peau.js` de GDA sur des tubes et une armature humanoïde et écrit les
// poids et les déformations dans tests/peau.json ; `tests/peau.rs` les compare au portage.
//
//   node tools/reference-peau.js <dossier de GDA> tests/peau.json
const fs = require('fs'), vm = require('vm'), [GDA, SORTIE] = process.argv.slice(2);
global.window = global;
const charger = f => vm.runInThisContext(fs.readFileSync(f, 'utf8') + '\n;', { filename: f });
charger(GDA + '/lib/three.min.js');
charger(GDA + '/moteur/maillage.js');
charger(GDA + '/moteur/armature.js');
charger(GDA + '/moteur/peau.js');
vm.runInThisContext('globalThis.Armature = Armature; globalThis.Peau = Peau; globalThis.THREE = THREE');

let graine = 20261103;
const hasard = n => { graine = (graine * 48271) % 2147483647; return graine % n; };
const reel = (a, b) => a + (b - a) * (hasard(1e6) / 1e6);

// Tube fermé autour d'un segment de `a` à `b`, en quadrilatères
const tube = (a, b, rayon, anneaux, cotes) => {
  const d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]], l = Math.hypot(...d), u = d.map(v => v / l);
  const x = Math.abs(u[1]) > 0.9 ? [1, 0, 0] : [0, 1, 0];
  const w = [u[1] * x[2] - u[2] * x[1], u[2] * x[0] - u[0] * x[2], u[0] * x[1] - u[1] * x[0]], lw = Math.hypot(...w), e = w.map(v => v / lw);
  const f = [u[1] * e[2] - u[2] * e[1], u[2] * e[0] - u[0] * e[2], u[0] * e[1] - u[1] * e[0]];
  const v = [], faces = [], id = (i, j) => i * cotes + (j % cotes);
  for (let i = 0; i <= anneaux; i++) for (let j = 0; j < cotes; j++) {
    const t = i / anneaux, an = j / cotes * 2 * Math.PI, c = Math.cos(an) * rayon, s = Math.sin(an) * rayon;
    v.push([0, 1, 2].map(k => a[k] + d[k] * t + e[k] * c + f[k] * s));
  }
  for (let i = 0; i < anneaux; i++) for (let j = 0; j < cotes; j++) faces.push(['defaut', id(i, j), id(i, j + 1), id(i + 1, j + 1), id(i + 1, j)]);
  faces.push(['defaut', ...Array.from({ length: cotes }, (_, j) => id(0, cotes - 1 - j))], ['defaut', ...Array.from({ length: cotes }, (_, j) => id(anneaux, j))]);
  return { v, f: faces };
};

const segments = Peau._segments, cas = [];
const essai = (mesh, a, noms) => {
  const m = Maillage.depuisDonnees ? Maillage.depuisDonnees(mesh) : null;
  const mm = m || { sommets: mesh.v, faces: mesh.f.map(f => ({ s: f.slice(1), m: f[0] })) };
  const groupe = noms ? a.os.filter(b => noms.includes(b.nom)).map(b => b.id) : null;
  Peau._segments = groupe ? x => segments(x).filter(s => groupe.includes(s.id)) : segments;
  const p = Peau.calculer(mm, a);
  Peau._segments = segments;
  const pose = {};
  for (const b of a.os) if (hasard(2)) {
    const q = [reel(-1, 1), reel(-1, 1), reel(-1, 1), reel(-1, 1)], l = Math.hypot(...q);
    pose[b.id] = { r: q.map(v => v / l), t: [0, 0, 0] };
  }
  a.pose = pose;
  const r = a.matricesRepos(), mp = a.matricesPose(), table = new Map();
  for (const [id, x] of mp) table.set(id, x.clone().multiply(r.get(id).clone().invert()));
  const sortie = new Float32Array(mesh.v.length * 3);
  Peau.deformer(p, mesh.v, table, sortie);
  cas.push({ mesh, donnees: a.versDonnees(), groupe, ids: p.ids, indices: [...p.indices], poids: [...p.poids], deforme: [...sortie] });
  a.pose = {};
};

const h = Armature.humanoide(), s = h.segments(), os = n => s.get(h.os.find(b => b.nom === n).id);
essai(tube([0, 0.05, 0], [0, 1.75, 0], 0.22, 30, 12), h);
essai(tube([0, 0.05, 0], [0, 1.75, 0], 0.22, 30, 12), h, ['cuisse.G', 'jambe.G', 'cuisse.D', 'jambe.D', 'bassin']);
essai(tube(os('bras.G').tete, os('main.G').queue, 0.05, 24, 10), h, ['bras.G', 'avant-bras.G', 'main.G']);
essai(tube(os('bras.G').tete, os('main.G').queue, 0.05, 24, 10), h, ['bras.G', 'avant-bras.G']);
fs.writeFileSync(SORTIE, JSON.stringify(cas));
