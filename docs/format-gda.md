# Format `.gda` 10 (Rustcraft)

Spécification du fichier de projet écrit par Rustcraft 0.51, tirée du code : `src/projet.rs`
et `src/modifs.rs` de Rustcraft, et la crate partagée `gda` (`maillage.rs`, `armature.rs`,
`animation.rs`, `couches.rs`, `peau.rs`). En cas de doute, le code fait foi.

Le format est né dans GDA, l'éditeur web, qui s'arrête au format 9. Le format 10 est
propre à Rustcraft et **GDA refuse de l'ouvrir**. C'est voulu : le `.gda` évolue
désormais librement pour les applications Rust et pour le moteur S.A.G.E., sans
compatibilité à garder vers GDA (voir la dernière partie).

## Conventions

- **Fichier** : texte JSON en UTF-8, indenté comme le faisait GDA. Extension `.gda`.
- **Repère** : Y vers le haut, main droite. Un personnage regarde vers +Z, et son côté
  gauche est en +X.
- **Unités** : le mètre. L'humanoïde de base mesure 1,75 m.
- **Quaternions** : `[x, y, z, w]`. Un quaternion nul ou illisible vaut l'identité
  `[0, 0, 0, 1]`. Un quaternion non unitaire est normalisé à la lecture. Un écart de
  moins de 4·10⁻⁶ sur la norme au carré compte comme unitaire, et le quaternion est
  gardé tel quel : un quaternion écrit avec six décimales se réécrit à l'identique.
- **Angles** : les limites des os sont en degrés. Partout ailleurs, les rotations sont
  des quaternions.
- **Couleurs** : chaînes `"#rrggbb"`.
- **Identifiants** : chaînes opaques.
  - Les objets en reçoivent un à leur création (`o<millisecondes en hexa>-<compteur>`),
    et ceux des os viennent de GDA ou de Rustcraft.
  - Dans une armature, les animations, les poses du catalogue, les masques et les
    mélanges ont leur identifiant, unique parmi les leurs : `a1`, `p1`, `m1`, `c1`… À la
    lecture, une animation ou une pose qui n'en a pas reçoit le premier libre (fichiers
    antérieurs à Rustcraft 0.49).
  - Les noms sont libres, peuvent changer et ne servent qu'à l'affichage : **toute
    référence passe par un identifiant** (liaison, cible d'un booléen, ancre, poses,
    animations, masques, mélanges, poids peints, export S.A.G.E.).
- **Clés inconnues** : un lecteur doit les ignorer. Rustcraft garde et réécrit telles
  quelles, et dans leur ordre, celles des os, des armatures, des animations, des poses,
  des mélanges et de leurs couches, mais pas celles des objets.
- **Écriture** : un fichier écrit par Rustcraft se relit et se réécrit à l'octet près. Un
  fichier produit par un autre outil peut changer une fois, à sa première réécriture :
  par exemple, un zéro écrit `0.0` devient `0`.

## Racine

```json
{
  "format": "gda",
  "version": 10,
  "application": "Rustcraft 0.48.0",
  "scene": { … },
  "materiaux": { … },
  "textures": { … }
}
```

| Clé | Type | Rôle |
|---|---|---|
| `format` | `"gda"` | Obligatoire. Sinon : « ce n'est pas un projet GDA ». |
| `version` | nombre ≥ 1 | Obligatoire. Un fichier plus récent que l'application est refusé. Les formats 1 à 9 de GDA se lisent : les champs absents prennent leur valeur par défaut. |
| `application` | texte | Signature de l'écrivain, pour information. |
| `scene` | objet | Obligatoire. |
| `materiaux` | objet | Bibliothèque des matériaux, par nom. |
| `textures` | objet | Images des matériaux, par identifiant. |

## `scene`

| Clé | Type | Rôle |
|---|---|---|
| `objets` | liste | Obligatoire. Les objets, dans l'ordre de la liste de scène. |
| `sets` | liste | Groupes d'objets : `{ "nom", "visible", "replie" }`. Un nom vide ou en double est ignoré. Le set par défaut s'appelle `"main set"`. |
| `actif` | nombre | Rang de l'objet actif, ou `-1`. Un rang invalide donne le dernier objet. |
| `curseur` | `[x, y, z]` | Position du curseur 3D, au monde. |

## Objet

```json
{
  "id": "o18f3a2c4b1-0",
  "nom": "corps",
  "position": [0, 0, 0],
  "rotation": [0, 0, 0, 1],
  "echelle": [1, 1, 1],
  "set": "main set",
  "visible": true,
  "modifs": [],
  "materiau": "peau",
  "armature": { … },
  "liaison": { … },
  "maillage": { … }
}
```

| Clé | Défaut | Rôle |
|---|---|---|
| `id` | un neuf | Identifiant stable. Un doublon est renouvelé à la lecture. |
| `nom` | `"objet"` | Nom affiché. |
| `position`, `rotation`, `echelle` | origine, identité, `[1,1,1]` | Place au monde : un sommet local `v` va en `position + rotation · (echelle × v)`. L'échelle est par axe ; celle d'une armature est uniforme (seul X compte). |
| `set` | premier set | Nom du set. Un set inconnu est créé. Un projet sans set reçoit `"main set"`. |
| `visible` | `true` | L'objet n'est montré que si son set l'est aussi. |
| `materiau` | celui de la 1re face, sinon `"defaut"` | Matériau de tout l'objet : Rustcraft affecte les matériaux par objet, pas par face. |
| `modifs` | `[]` | Pile de modifieurs (voir plus bas). |
| `armature` | absente | Présente si l'objet est une armature (voir plus bas). |
| `liaison` | absente | Attache de l'objet à une armature (voir plus bas). Une liaison vers une armature disparue est retirée à la lecture. |
| `maillage` | obligatoire | Géométrie, en coordonnées locales. Pour une armature, il est écrit vide et sa forme se déduit des os. |

### Maillage

```json
{
  "v": [[x, y, z], …],
  "f": [["peau", 0, 1, 2, 3], …],
  "l": [[4, 5], …],
  "uv": [[u0, v0, u1, v1, …], …],
  "c": [[0, 1], …]
}
```

| Clé | Rôle |
|---|---|
| `v` | Sommets, arrondis à 1e-5 à l'écriture. |
| `f` | Faces. Chacune est une liste dont le 1er élément est le nom du matériau, suivi des indices des sommets dans l'ordre du contour. La face avant est celle où le contour tourne dans le sens trigonométrique. Les polygones à plus de 3 sommets sont permis. Un indice hors limites rend le maillage illisible. |
| `l` | Arêtes libres (« fils ») : paires de sommets sans face. Rustcraft ne garde ni les doublons, ni les boucles sur un sommet, ni les arêtes qui bordent déjà une face. |
| `uv` | Facultatif, absent si aucune face n'a d'UV. Sinon, une liste par face (même ordre que `f`), à plat : deux nombres par coin, dans l'ordre des sommets de la face. Une liste vide signifie que la face n'a pas d'UV. L'origine (0, 0) est le coin haut gauche de l'image, comme en glTF. Des UV illisibles laissent la face sans texture. |
| `c` | Facultatif. Coutures du dépliage : paires de sommets reliés par une arête. Des coutures illisibles sont ignorées. |

Les UV sont portées par les coins et non par les sommets : une couture de texture
n'oblige donc pas à dédoubler les sommets.

### Modifieurs (`modifs`)

C'est une pile appliquée dans l'ordre au maillage, sans le changer. Chaque entrée a un
`type` et un `actif` (booléen). Une entrée de type inconnu est gardée et ignorée.

- **Répétition** : `{ "type": "repetition", "actif": true, "n": 3, "relatif": true, "decalage": [1.2, 0, 0], "unir": false }`
  - `n` copies, de 1 à 200, chacune décalée de `decalage` par rapport à la
    précédente.
  - Si `relatif` vaut `true`, le décalage est mesuré en tailles de la boîte englobante.
  - Si `unir` vaut `true`, les copies sont fusionnées en un seul solide.
- **Booléen** : `{ "type": "booleen", "actif": true, "op": "difference", "cible": "o18f3a2c4b1-1" }`
  - `op` vaut `difference`, `union` ou `intersection`.
  - `cible` est l'**identifiant** de l'autre objet, pris avec ses propres modifieurs.
    Renommer la cible ne change donc rien. Une dépendance circulaire est refusée.
  - **Migration** : jusqu'à Rustcraft 0.47 (et dans GDA), `cible` était un nom. À la
    lecture, une cible qui n'est l'identifiant d'aucun objet, mais le nom de l'un d'eux,
    prend l'identifiant de cet objet (le premier, si plusieurs portent ce nom).
  - Si l'on duplique un objet avec sa cible, la copie vise la copie de la cible ; seul, il
    garde l'original. Si l'on fusionne un projet dont un identifiant est déjà pris, il est
    renouvelé et les cibles le suivent.

## Armature (`objet.armature`)

```json
{
  "os": [ { … }, … ],
  "pose": { "<id os>": { "r": [x, y, z, w], "t": [x, y, z] }, … },
  "poses": [ { "id": "p1", "nom": "Garde", "os": { "<id os>": { "r": …, "t": … } } }, … ],
  "ancre": "<id os>" | null,
  "devant": true,
  "animations": [ … ],
  "masques": [ … ],
  "melanges": [ … ]
}
```

### Os

```json
{
  "id": "b3", "nom": "avant-bras.G", "parent": "b2",
  "position": [0, 0.28, 0], "rotation": [0, 0, 0, 1], "longueur": 0.26,
  "contraintes": { "r": [ … 3 limites … ], "t": [ … 3 limites … ] },
  "couleur": "#ff8800"
}
```

- **Repos** :
  - Un os s'étend de sa tête le long de son axe +Y, sur `longueur` (> 0, sinon 1).
  - `position` et `rotation` placent sa tête et son orientation **dans le repère de son
    parent**, c'est-à-dire relativement à la tête et à la rotation du parent. Un os
    sans parent est placé dans le repère de l'objet armature.
  - Le repère d'un os s'obtient donc en composant ceux de ses ancêtres depuis la racine.
- **Parenté** : un parent inconnu, ou une boucle de parenté, fait de l'os une racine. Les
  identifiants vides ou en double sont ignorés. Une armature sans os valable reçoit un
  os unique.
- **`contraintes.r`** : limites de rotation pour les angles X, Y et Z de l'os, en
  degrés d'écart au repos.
  - Les angles se lisent ainsi : l'os plie autour de son axe X, puis de son axe Z, puis
    vrille autour de son axe Y (le long de l'os).
  - Chaque limite est `{ "etat": "libre" }`, `{ "etat": "bloque" }` ou
    `{ "etat": "limite", "min": -30, "max": 60 }`.
  - Si la clé est absente, les trois axes sont libres.
- **`contraintes.t`** : limites de translation le long des axes X, Y et Z de l'os au
  repos, en mètres, sous la même forme.
  - Si la clé est absente, **un os racine est libre et les autres sont bloqués**.
    C'est un écart voulu avec GDA, qui bloque tout. Seules les limites qui diffèrent de
    ce défaut sont écrites.
  - Un état inconnu compte comme bloqué.
- **Autres clés** de l'os (par exemple `couleur`) : gardées telles quelles.

### Pose et écarts

Un **écart** `{ "r": quaternion, "t": [x, y, z] }` décrit une pose relative au repos,
dans le repère du parent :

- rotation posée = `rotation × r` ;
- position posée = `position + t`.

Un os absent d'une pose est au repos. Les écarts des os disparus sont retirés à
l'écriture.

| Clé | Rôle |
|---|---|
| `pose` | Pose courante de l'armature. |
| `poses` | Poses nommées (catalogue) : `{ "id", "nom", "os": { id: écart } }`. |
| `ancre` | Os fixé au monde pour la manipulation « Tirer », ou `null`. Une ancre vers un os disparu devient `null`. |
| `devant` | `true` : l'armature est dessinée au premier plan en mode Scène. Clé absente sinon. |
| `animations` | Animations à clés (voir ci-dessous). Clé absente s'il n'y en a pas. |
| `masques`, `melanges` | Couches d'animation (voir plus bas). Clés absentes s'il n'y en a pas. |

Rustcraft garde et réécrit toutes les autres clés de l'armature.

### Animations

```json
{
  "id": "a3",
  "nom": "Marche",
  "duree": 1,
  "boucle": true,
  "vitesse": 1.44,
  "marqueurs": [{ "t": 0, "nom": "contact.G" }, { "t": 0.5, "nom": "contact.D" }],
  "cles": [
    { "t": 0,   "pose": "p5", "os": { "<id os>": { "r": […], "t": […] } } },
    { "t": 0.5, "os": { … } }
  ]
}
```

- `duree` est en secondes (> 0, sinon 2). `boucle` vaut `true` par défaut.
- Les clés sont triées par temps `t` (secondes, de 0 à `duree`, calées sur une grille de
  24 images par seconde dans l'éditeur). Une clé contient la pose de tous les os qui
  ne sont pas au repos.
- **Entre deux clés**, chaque os passe linéairement de l'une à l'autre : interpolation
  sphérique (slerp) pour `r`, linéaire pour `t`. Un os absent d'une clé y est au repos.
- **Après la dernière clé** :
  - en boucle, l'animation revient vers la première clé, atteinte à
    `t₀ + duree` ;
  - sinon, la dernière pose est tenue.
- **`pose`**, facultatif dans une clé : l'id de la pose du catalogue dont la clé est la
  copie. « Reprendre les poses du catalogue » recopie cette pose dans la clé. La
  clé garde toujours ses `os` : sans catalogue, elle se lit seule.
- Une clé sans `t` rend son animation illisible : l'animation est ignorée. Les écarts
  vers des os disparus sont oubliés.
- **`mode`** : `"normal"` (par défaut, clé absente) ou `"additif"`. Une animation normale
  donne une pose complète. Une animation additive donne un écart par rapport à sa
  `reference`, à poser par-dessus d'autres couches.
- **`reference`**, pour une animation additive : `{ "cle": n }`, le rang d'une de ses
  clés (0 par défaut), ou `{ "pose": "<id de pose>" }`, une pose du catalogue.
  Introuvable, c'est le repos. Les deux clés ne sont écrites que pour une animation
  additive.
- **Autres clés**, gardées telles quelles et dans leur ordre, entre `boucle` (ou
  `reference`) et `cles` :
  - `vitesse` : la vitesse au sol d'une allure, en m/s ;
  - `marqueurs` : des instants nommés, `{ "t", "nom" }`, par exemple le contact d'un
    pied.

### Couches : masques et mélanges

Un **mélange** joue plusieurs animations ensemble, en couches, chacune avec un poids,
un masque d'os et son propre temps. L'éditeur et S.A.G.E. partagent ainsi la même
recette.

```json
"masques": [
  { "id": "m1", "nom": "haut du corps", "os": { "o2": 0.5, "o3": 1, "o4": 1, … } }
],
"melanges": [
  {
    "id": "c1",
    "nom": "Repos vivant",
    "couches": [
      { "animation": "a2", "poids": 1 },
      { "animation": "a1", "poids": 1, "masque": "m1", "vitesse": 1, "decalage": 0 }
    ]
  }
]
```

- **Masque** : le poids de chaque os, de 0 à 1, par identifiant. Un os absent pèse 0.
  Les os disparus sont retirés à l'écriture.
- **Couche** :
  - `animation` : l'identifiant de l'animation. Une couche vers une animation disparue
    est oubliée.
  - `poids` : de 0 à 1, 1 par défaut.
  - `masque` : l'identifiant d'un masque, facultatif. Sans masque, tous les os pèsent 1.
  - `vitesse` et `decalage` : le temps de l'animation vaut `t × vitesse + decalage`,
    ramené dans sa durée si elle boucle. Par défaut, 1 et 0, et non écrits.
  - `actif` : `false` éteint la couche. Absent, elle est allumée.
- **Évaluation**, couche par couche dans l'ordre de la liste, en partant du repos. Pour
  chaque os, `k = poids × masque(os)`, et un os absent d'une pose y est au repos :
  - couche normale : `r = slerp(r_dessous, r_couche, k)` et
    `t = t_dessous + k·(t_couche − t_dessous)` ;
  - couche additive, de référence `ref` : `Δr = r_ref⁻¹ × r_couche`,
    `Δt = t_couche − t_ref`, puis `r = r_dessous × slerp(identité, Δr, k)` et
    `t = t_dessous + k·Δt`. La multiplication à droite place l'écart dans le repère de
    l'os.
  - Le slerp prend toujours le plus court chemin : `q` et `−q` sont la même rotation.
- **Durée d'un mélange** : il se répète au plus petit multiple commun des durées de ses
  couches, compté en images à 24 par seconde. Par exemple, 10 s et 4 s donnent 20 s. La
  durée est bornée à 10 minutes.
- **Aplatir** (Rustcraft 0.50) : le mélange échantillonné à chaque image, de 0 à sa
  durée incluse, devient une animation normale en boucle, avec une clé par image. C'est
  la forme la plus sûre pour un lecteur qui ne connaît pas les couches.
- **Clé posée sur une couche** : l'éditeur montre tout le mélange. La clé qu'il range
  dans l'animation de la couche est la pose qui, recomposée avec les autres couches,
  redonne ce qui est montré. Le fichier n'en garde que la clé.
- La mise en solo d'une couche n'est qu'un état de l'éditeur : elle ne s'écrit pas.
- **Compatibilité** : ces ajouts restent dans le format 10. Un lecteur ancien les
  ignore. Attention, Rustcraft avant 0.49 les perd s'il réenregistre le fichier, avec
  les autres clés des animations (`vitesse`, `marqueurs`).

## Liaison (`objet.liaison`)

Elle attache un objet (maillage) à une armature.

```json
{
  "armature": "<id de l'objet armature>",
  "os": ["<id os>", …],
  "poids": { "os": ["<id os>", …], "sommets": 1234, "poids": [[i, w, i, w, …], …] }
}
```

- **Un seul os** : l'objet le suit rigidement.
- **Plusieurs os** : les sommets se répartissent entre eux (peau).
- **Pose de liaison** : elle n'est pas stockée. C'est la place relative présente de
  l'objet et de l'armature au repos.
- **`poids`** (facultatif) : les poids peints à la main.
  - `os` donne l'ordre des colonnes, et `sommets` le nombre de sommets de l'objet.
  - `poids` contient une liste par os, dans l'ordre de `os` : des paires `indice de
    sommet, poids`, et seulement les poids > 1e-4, arrondis à 1e-4.
  - Les poids d'un sommet ont pour somme 1.
  - Sans cette clé, les poids sont calculés automatiquement à partir des os de la
    liaison.

## `materiaux`

Un objet dont les clés sont les noms de matériau :

```json
"peau": {
  "couleur": "#d9a066", "ambiante": "#000000", "speculaire": "#000000",
  "emission": "#000000", "brillance": 10, "opacite": 1, "refraction": 1,
  "illumination": 1, "texture": "<id texture>", "deux_cotes": true
}
```

Seuls ces champs sont lus. Ils reprennent ceux du MTL et sont bornés :

- `brillance` : 0 à 1000 ;
- `opacite` : 0 à 1 ;
- `refraction` : 0,001 à 10 ;
- `illumination` : entier de 0 à 10.

La `texture` est facultative. Le matériau `"defaut"` est partagé et ne porte jamais
d'image.

`deux_cotes` est facultatif et vaut `false` s'il manque : il n'est écrit que s'il est
vrai. Vrai, les faces du matériau se voient des deux côtés en rendu En jeu, et
l'export glTF écrit `"doubleSided": true`. Le MTL ne le connaît pas.

## `textures`

```json
"textures": { "<id>": { "nom": "peau.png", "donnees": "<base64>" } }
```

Ce sont les images PNG ou JPEG des matériaux, encodées en base64. L'identifiant dépend
du contenu de l'image : une même image n'est donc stockée qu'une fois. Seules les images
encore utilisées par un matériau sont écrites.

## Ce qui n'est pas dans le fichier

- La sélection, la vue et l'historique d'annulation.
- Les réglages de l'application, qui sont dans `reglages.txt`.
- Le maillage déformé par une pose, et les poids calculés : ils se recalculent.

---

# S.A.G.E.

S.A.G.E. est le futur moteur de jeu, avec pour but final un « anti GTA ». Son logo est
orné d'un papillon sur le S. Son cahier des charges n'est pas encore écrit.

## Ce qui est décidé

- **Liberté du format** : GDA n'a servi à produire aucun contenu, donc le `.gda` peut
  évoluer sans compatibilité avec GDA. GDA reste utile pour des maillages (impression
  3D), mais le travail se concentre sur les applications Rust.
- **Données d'animation** : armatures, peaux, poses et animations sont pensées pour être
  consommées par S.A.G.E.

## Export actuel : GLB animé (Rustcraft 0.46 et suivantes, extras au format 3 depuis 0.51)

On le trouve dans Fichier › Exporter › « Personnage pour S.A.G.E. », sous deux formes
(`gda/src/sage.rs`) :

- **« GLB animé, en couches »** : chaque animation telle quelle, en pose complète, et les
  `extras` décrivent le reste (mode, référence, masques, mélanges). S.A.G.E. recompose
  les couches au chargement.
- **« GLB animé, mélanges aplatis »** : de plus, chaque mélange devient une animation glTF
  ordinaire, échantillonnée à chaque image (24 par seconde) sur sa durée commune. Tout
  moteur la lit ; les masques et mélanges ne sont alors pas écrits.

 Il exporte l'armature active et ses objets liés en un fichier glTF 2.0
binaire standard, validé sans erreur ni avertissement par le validateur Khronos.

- **Nœud 0** : l'armature, à la place de l'objet (échelle uniforme). C'est aussi la
  racine (`skeleton`) de la peau.
- **Un nœud par os**, au repos : `translation` et `rotation` dans le repère du parent,
  comme dans le `.gda`.
- **Une peau** :
  - `JOINTS_0` en u16 et `WEIGHTS_0` (4 influences par sommet) ;
  - les matrices de liaison inverses, qui sont l'inverse du repère de chaque os au repos
    dans l'armature.

  Les sommets sont écrits dans le repère de l'armature.
- **Une animation glTF par animation** :
  - échantillonneurs LINEAR ;
  - rotation = repos × `r` ; translation seulement pour les os qui glissent ;
  - des clés ajoutées à 0 et à `duree` si elles manquent ;
  - une animation qui laisse tout au repos garde un canal, la rotation de repos du
    premier os, pour qu'un mélange puisse la désigner.
- **`extras`**, pour ce que glTF ne sait pas porter :
  - sur le nœud 0 :
    `"sage": { "format": 3, "id": <id de l'objet armature>, "ancre": <id d'os> | null, "poses": [{ "id", "nom", "os": { <id d'os>: { "r", "t" } } }], "animations": <nombre>, "masques": […], "melanges": […] }`.
    Les poses portent leur identifiant depuis Rustcraft 0.49. `masques` et `melanges`
    ont la forme du `.gda` (voir « Couches »), n'apparaissent qu'en couches, et seulement
    s'il y en a ;
  - sur chaque os : `id`, `longueur` et, s'il y en a, `contraintes` (même forme que
    dans le `.gda`) ;
  - sur chaque objet lié (nœud à `mesh`) : `id`, l'identifiant de l'objet ;
  - sur chaque animation : `id`, `boucle` et `duree` ; pour une additive, `mode`
    (`"additif"`) et `reference` ; puis ses autres clés du `.gda` (`vitesse`,
    `marqueurs`…). Une référence `{ "cle": k }` donne le rang de la clé **parmi les clés
    du clip glTF**, qui peut en avoir une de plus au début ; `{ "pose": <id> }` désigne une
    pose des extras du nœud 0. Une animation aplatie porte `melange`, l'identifiant du
    mélange dont elle vient, et un identifiant d'animation libre.

  Comme dans le `.gda`, **os et objets sont désignés par leur identifiant**. Le `name`
  glTF de chaque nœud porte le nom, pour l'affichage seulement. Le format 1 des extras
  (Rustcraft 0.46 et 0.47) désignait les os par leur nom et n'avait pas d'`id`. Le format
  2 (0.48 à 0.50) n'avait ni identifiant ni mode d'animation, ni masques, ni mélanges :
  relu, ses animations reçoivent un identifiant neuf et sont normales.
- **Relecture** (`gda::sage::lire`, utilisée par S.A.G.E.) : l'armature retrouve ses
  animations avec leurs identifiants, modes, références et autres clés, ses masques et
  ses mélanges. Un mélange relu se joue comme l'original, à la précision des flottants du
  `.glb` (moins de 10⁻⁵ radian sur `Repos vivant`).

Ce choix d'un GLB standard a été fait faute de format S.A.G.E. défini. Il changera quand
le moteur exprimera ses besoins.

## Pistes ouvertes

- Un format propre à S.A.G.E., ou la lecture directe du `.gda` par le moteur.
- L'import de skins glTF dans Rustcraft.
- La description des environnements (décors, niveaux), pas encore abordée.
