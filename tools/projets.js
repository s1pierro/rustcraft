// Pour chaque .gda donné : texte normalisé par GDA (ed_texteProjet) et bilan des
// objets, que `tests/projets.rs` compare à la lecture Rust.
//   node tools/projets.js <sortie> <fichiers .gda…>
const fs = require('fs'), path = require('path'), [SORTIE, ...F] = process.argv.slice(2);
const texte = p => JSON.stringify(p, null, 2).replace(/\[\s*([^\[\]{}]*?)\s*\]/g, (_, c) => '[' + c.replace(/,\n\s*/g, ', ') + ']');
F.forEach((f, n) => {
  const p = JSON.parse(fs.readFileSync(f, 'utf8'));
  fs.copyFileSync(f, `${SORTIE}/${n}.gda`);
  fs.writeFileSync(`${SORTIE}/${n}.norm`, texte(p));
  fs.writeFileSync(`${SORTIE}/${n}.bilan`, JSON.stringify(p.scene.objets.map(o => [o.nom, o.maillage.v.length, o.maillage.f.length, (o.modifs || []).length, !!o.armature])));
  console.log(n, path.basename(f), p.version, p.scene.objets.length);
});
