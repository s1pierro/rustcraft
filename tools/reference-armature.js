// Exécute le vrai moteur d'armature de GDA sur des suites d'opérations et écrit leurs
// résultats dans tests/armature.json ; `tests/armature.rs` les rejoue et compare.
//
//   node tools/reference-armature.js <dossier de GDA> tests/armature.json
const fs = require('fs'), vm = require('vm'), [GDA, SORTIE] = process.argv.slice(2);
global.window = global;
const charger = f => vm.runInThisContext(fs.readFileSync(f, 'utf8') + '\n;', { filename: f });
charger(GDA + '/lib/three.min.js');
charger(GDA + '/moteur/maillage.js');
charger(GDA + '/moteur/armature.js');
vm.runInThisContext('globalThis.Armature = Armature');

let graine = 20261102;
const hasard = n => { graine = (graine * 48271) % 2147483647; return graine % n; };
const reel = (a, b) => a + (b - a) * (hasard(1e6) / 1e6);
const parmi = t => t[hasard(t.length)];

const etat = a => {
  const s = a.segments(), sd = a.soudures();
  return {
    donnees: a.versDonnees().os.map(b => [b.id, b.nom, b.parent, b.position, b.rotation, b.longueur]),
    ordre: a.ordre().map(b => b.id),
    segments: Object.fromEntries([...s].map(([id, x]) => [id, [x.tete, x.queue, x.q]])),
    soudures: Object.fromEntries([...sd].map(([c, l]) => [c, [...l].sort()])),
    enveloppe: { v: a.enveloppe().sommets, f: a.enveloppe().faces.map(f => f.s) }
  };
};

const cas = [];
for (const depart of ['un', 'humanoide', 'humanoide', 'un', 'humanoide']) {
  const a = depart === 'un' ? new Armature() : Armature.humanoide();
  if (!a.os.length) a.ajouter();
  const ops = [], resultats = [etat(a)];
  for (let n = 0; n < 40; n++) {
    const ids = a.os.map(b => b.id), id = parmi(ids);
    let op;
    switch (hasard(6)) {
      case 0: op = ['extruder', id]; a.extruder(id); break;
      case 1: if (ids.length < 2) continue; op = ['retirer', id]; a.retirer(id); break;
      case 2: {
        const p = hasard(4) === 0 ? null : parmi(ids);
        op = ['parent', id, p, a.changerParent(id, p)];
        break;
      }
      case 3: {
        const tete = [reel(-2, 2), reel(-2, 2), reel(-2, 2)], queue = [reel(-2, 2), reel(-2, 2), reel(-2, 2)], parent = hasard(2) ? id : null;
        op = ['segment', parent, tete, queue];
        a.ajouterSegment({ nom: 'neuf', parent, tete, queue });
        break;
      }
      default: {
        const queue = hasard(2) === 0, d = [reel(-1, 1), reel(-1, 1), reel(-1, 1)];
        op = ['bout', id, queue, d];
        const s = a.segments(), x = s.get(id), k = queue ? 'queue' : 'tete';
        x[k] = x[k].map((v, i) => v + d[i]);
        a.appliquerSegments(s);
      }
    }
    ops.push(op);
    resultats.push(etat(a));
  }
  cas.push({ depart, ops, resultats });
}
fs.writeFileSync(SORTIE, JSON.stringify(cas));

// Poses au hasard, écrites à côté : <sortie>-poses.json
const poses = [];
for (let n = 0; n < 12; n++) {
  const a = Armature.humanoide();
  for (let k = 0; k < 3; k++) a.extruder(parmi(a.os).id);
  const pose = {};
  for (const b of a.os) {
    if (hasard(3) === 0) continue;
    const q = [reel(-1, 1), reel(-1, 1), reel(-1, 1), reel(-1, 1)], l = Math.hypot(...q);
    pose[b.id] = { r: q.map(v => v / l), t: hasard(2) ? [0, 0, 0] : [reel(-1, 1), reel(-1, 1), reel(-1, 1)] };
  }
  a.pose = pose;
  const m = a.matricesPose(), s = a.segments(m);
  poses.push({ donnees: a.versDonnees(), segments: Object.fromEntries([...s].map(([id, x]) => [id, [x.tete, x.queue, x.q]])), enveloppe: a.enveloppe(undefined, m).sommets });
}
fs.writeFileSync(SORTIE.replace(/\.json$/, '-poses.json'), JSON.stringify(poses));
