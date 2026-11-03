#!/usr/bin/env python3
"""Convertit des pictogrammes de GDA en tracés pour Rustcraft : src/icones.rs.

    python3 tools/icones.py <dossier de GDA> src/icones.rs

Les chemins SVG sont aplatis en lignes brisées ; `dl.line` a des bouts arrondis, comme
les `stroke-linecap="round"` de ces fichiers. Les formes pleines sont
des disques, ou des hachures jointives sous leur contour.
"""
import math, re, sys

CUBIQUE = 10   # segments par courbe
ARC = 8        # segments par quart de tour, au moins 2 par arc


def jetons(d):
    """Commandes et nombres d'un attribut `d`, drapeaux d'arc compris (« 0 0 1-.3 »)."""
    i, sortie, cmd, n = 0, [], None, 0
    nombre = re.compile(r'[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?')
    while i < len(d):
        c = d[i]
        if c in ' ,\t\n':
            i += 1
        elif c.isalpha():
            cmd, n = c, 0
            sortie.append(c)
            i += 1
        elif cmd in 'aA' and n % 7 in (3, 4):
            sortie.append(float(c))
            n += 1
            i += 1
        else:
            m = nombre.match(d, i)
            sortie.append(float(m.group()))
            n += 1
            i = m.end()
    return sortie


def arc(p, rx, ry, phi, grand, sens, q):
    """Arc SVG vers `q`, sans son point de départ."""
    if p == q or rx == 0 or ry == 0:
        return [q]
    c, s = math.cos(math.radians(phi)), math.sin(math.radians(phi))
    dx, dy = (p[0] - q[0]) / 2, (p[1] - q[1]) / 2
    x, y = c * dx + s * dy, -s * dx + c * dy
    k = x * x / (rx * rx) + y * y / (ry * ry)
    if k > 1:
        rx, ry = rx * math.sqrt(k), ry * math.sqrt(k)
    a = rx * rx * ry * ry - rx * rx * y * y - ry * ry * x * x
    b = rx * rx * y * y + ry * ry * x * x
    f = math.sqrt(max(0, a / b)) * (-1 if grand == sens else 1)
    cx, cy = f * rx * y / ry, -f * ry * x / rx
    ox, oy = c * cx - s * cy + (p[0] + q[0]) / 2, s * cx + c * cy + (p[1] + q[1]) / 2
    t0 = math.atan2((y - cy) / ry, (x - cx) / rx)
    t1 = math.atan2((-y - cy) / ry, (-x - cx) / rx)
    dt = t1 - t0
    if sens and dt < 0:
        dt += math.tau
    if not sens and dt > 0:
        dt -= math.tau
    n = max(2, math.ceil(abs(dt) / (math.pi / 2) * ARC))
    l = []
    for i in range(1, n + 1):
        t = t0 + dt * i / n
        u, v = rx * math.cos(t), ry * math.sin(t)
        l.append((c * u - s * v + ox, s * u + c * v + oy))
    l[-1] = q
    return l


def bezier(pts):
    n = len(pts) - 1
    l = []
    for i in range(1, CUBIQUE + 1):
        t = i / CUBIQUE
        x = sum(math.comb(n, k) * (1 - t) ** (n - k) * t ** k * pts[k][0] for k in range(n + 1))
        y = sum(math.comb(n, k) * (1 - t) ** (n - k) * t ** k * pts[k][1] for k in range(n + 1))
        l.append((x, y))
    return l


def chemin(d):
    """Lignes brisées d'un attribut `d`."""
    t, i, traits, cour, p, depart, cmd, ctrl = jetons(d), 0, [], [], (0, 0), (0, 0), None, None

    def lire(n):
        nonlocal i
        v = t[i:i + n]
        i += n
        return v

    while i < len(t):
        if isinstance(t[i], str):
            cmd = t[i]
            i += 1
            if cmd in 'zZ':
                cour.append(depart)
                traits.append(cour)
                cour, p = [], depart
                continue
        rel = cmd.islower()
        o = p if rel else (0, 0)
        c = cmd.lower()
        pt = lambda x, y: (x + o[0], y + o[1])
        if c == 'm':
            if cour:
                traits.append(cour)
            p = pt(*lire(2))
            depart, cour = p, [p]
            cmd = 'l' if rel else 'L'
            continue
        if not cour:
            cour = [p]
        if c == 'l':
            p = pt(*lire(2))
            cour.append(p)
        elif c == 'h':
            x, = lire(1)
            p = (x + o[0], p[1])
            cour.append(p)
        elif c == 'v':
            y, = lire(1)
            p = (p[0], y + o[1])
            cour.append(p)
        elif c == 'c':
            v = lire(6)
            ctrl = pt(v[2], v[3])
            q = pt(v[4], v[5])
            cour += bezier([p, pt(v[0], v[1]), ctrl, q])
            p = q
        elif c == 's':
            v = lire(4)
            a = (2 * p[0] - ctrl[0], 2 * p[1] - ctrl[1]) if ctrl else p
            ctrl = pt(v[0], v[1])
            q = pt(v[2], v[3])
            cour += bezier([p, a, ctrl, q])
            p = q
        elif c == 'q':
            v = lire(4)
            ctrl = pt(v[0], v[1])
            q = pt(v[2], v[3])
            cour += bezier([p, ctrl, q])
            p = q
        elif c == 'a':
            v = lire(7)
            q = pt(v[5], v[6])
            cour += arc(p, v[0], v[1], v[2], bool(v[3]), bool(v[4]), q)
            p = q
        else:
            raise ValueError('commande ' + cmd)
        if c not in 'csq':
            ctrl = None
    if len(cour) > 1:
        traits.append(cour)
    return traits


def surface(t):
    return abs(sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(t, t[1:] + t[:1]))) / 2


def disque(t):
    """(x, y, rayon) si la ligne fermée `t` est un disque."""
    xs, ys = [p[0] for p in t], [p[1] for p in t]
    l, h = max(xs) - min(xs), max(ys) - min(ys)
    if abs(l - h) > 0.01 or abs(surface(t) / (math.pi * l * l / 4) - 1) > 0.05:
        return None
    return (min(xs) + l / 2, min(ys) + h / 2, l / 2)


def hachures(t, pas=0.4, e=0.7):
    """Remplit un contour par des traits horizontaux jointifs."""
    ys = [p[1] for p in t]
    y, fin, sortie = min(ys), max(ys), []
    while y <= fin:
        xs = sorted(a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) for a, b in zip(t, t[1:] + t[:1]) if (a[1] <= y) != (b[1] <= y))
        for a, b in zip(xs[::2], xs[1::2]):
            if b - a > e:
                sortie.append((e, [(a + e / 2, y), (b - e / 2, y)]))
        y += pas
    return sortie


def tirets(t, plein, vide):
    """Découpe une ligne brisée en tirets."""
    sortie, cour, reste, trace = [], [t[0]], plein, True
    for a, b in zip(t, t[1:]):
        l, u = math.dist(a, b), 0.0
        while l - u > reste:
            u += reste
            p = (a[0] + (b[0] - a[0]) * u / l, a[1] + (b[1] - a[1]) * u / l)
            if trace:
                sortie.append(cour + [p])
            cour, trace = [p], not trace
            reste = plein if trace else vide
        reste -= l - u
        cour.append(b)
    if trace and len(cour) > 1:
        sortie.append(cour)
    return sortie


def forme(balise, attr):
    if balise == 'path':
        return chemin(attr('d'))
    if balise == 'rect':
        x, y, l, h, r = (float(attr(n) or 0) for n in ('x', 'y', 'width', 'height', 'rx'))
        return chemin(f'M{x + r} {y}h{l - 2 * r}a{r} {r} 0 0 1 {r} {r}v{h - 2 * r}a{r} {r} 0 0 1 {-r} {r}h{2 * r - l}a{r} {r} 0 0 1 {-r} {-r}v{2 * r - h}a{r} {r} 0 0 1 {r} {-r}z')
    c = (float(attr('cx')), float(attr('cy')))
    rx = float(attr('rx') or attr('r'))
    ry = float(attr('ry') or attr('r'))
    # Dans le sens de SVG : depuis (cx + r, cy), en sens horaire à l'écran
    return [[(c[0] + rx * math.cos(k * math.tau / 48), c[1] + ry * math.sin(k * math.tau / 48)) for k in range(49)]]


def fichier(racine, nom):
    """Traits (épaisseur, ligne) et disques d'un SVG, dans l'ordre du fichier."""
    s = re.sub(r'<!--.*?-->', '', open(f'{racine}/icons/{nom}.svg', encoding='utf8').read(), flags=re.S)
    defaut = float(re.search(r'<g[^>]*stroke-width="([\d.]+)"', s).group(1)) if '<g' in s else None
    traits, disques = [], []
    for balise, attrs in re.findall(r'<(path|circle|ellipse|rect)\b([^>]*?)/?>', s):
        def attr(n, a=attrs):
            m = re.search(rf'(?:^|\s){n}="([^"]+)"', a)
            return m.group(1) if m else None
        e = float(attr('stroke-width') or defaut)
        lignes = forme(balise, attr)
        if attr('fill') == 'currentColor':
            for t in lignes:
                ferme = t[:-1] if t[0] == t[-1] else t
                if (c := disque(ferme)):   # les disques ont aussi le contour
                    disques.append((c[0], c[1], c[2] + e / 2))
                else:
                    traits += hachures(ferme)
                    traits.append((e, ferme + ferme[:1]))
        elif (t := attr('stroke-dasharray')):
            plein, vide = map(float, t.split())
            traits += [(e, x) for l in lignes for x in tirets(l, plein, vide)]
        else:
            traits += [(e, l) for l in lignes]
    return traits, disques


def deplacer():
    """3d-move.svg est un contour plein de traits de 2 : ses lignes médianes."""
    t = [
        [(12, 6), (12, 3)], [(10, 4.155), (12, 3), (14, 4.155)],
        [(7.304, 9.289), (4.206, 7.5)], [(6.206, 6.345), (4.206, 7.5), (4.206, 9.81)],
        [(16.696, 9.289), (19.794, 7.5)], [(17.794, 6.345), (19.794, 7.5), (19.794, 9.81)],
        [(9.402, 10.5), (12, 12), (14.598, 10.5)], [(12, 12), (12, 15)],
        [(7.304, 14.711), (4.206, 16.5)], [(4.206, 14.19), (4.206, 16.5), (6.206, 17.655)],
        [(16.696, 14.711), (19.794, 16.5)], [(19.794, 14.19), (19.794, 16.5), (17.794, 17.655)],
        [(12, 18), (12, 21)], [(10, 19.845), (12, 21), (14, 19.845)],
    ]
    return [(2.0, l) for l in t], []


def curseur():
    """ED_SVG_CURSEUR : cercle en tirets de 5,1 et réticule."""
    r, tiret, tour = 6.5, 5.1, math.tau * 6.5
    t = []
    s = 0.0
    while s < tour:
        f = min(s + tiret, tour)
        t.append([(12 + r * math.cos(u / r), 12 + r * math.sin(u / r)) for u in (s + (f - s) * i / 8 for i in range(9))])
        s += 2 * tiret
    t += [[(12, 2), (12, 7)], [(12, 17), (12, 22)], [(2, 12), (7, 12)], [(17, 12), (22, 12)]]
    return [(1.4, l) for l in t], []


def rust(nom, traits, disques):
    f = lambda v: f'{float(round(v, 3))}'
    l = ', '.join(f'({f(e)}, &[' + ', '.join(f'[{f(x)}, {f(y)}]' for x, y in t) + '])' for e, t in traits)
    d = ', '.join(f'[{f(x)}, {f(y)}, {f(r)}]' for x, y, r in disques)
    return f'pub const {nom}: Icone = Icone {{\n    traits: &[{l}],\n    disques: &[{d}],\n}};\n'


def main():
    racine, sortie = sys.argv[1], sys.argv[2]
    icones = [
        ('DEPLACER', deplacer()),
        ('TOURNER', fichier(racine, 'rotate-3d')),
        ('ECHELLE', fichier(racine, 'scaling')),
        ('PAS', fichier(racine, 'pas')),
        ('PIVOT_MEDIAN', fichier(racine, 'iconoir--box-3d-three-points')),
        ('PIVOT_BOITE', fichier(racine, 'iconoir--box-3d-center')),
        ('PIVOT_ACTIF', fichier(racine, 'iconoir--select-point-3d')),
        ('PIVOT_CURSEUR', curseur()),
        ('PIVOT_ORIGINE', fichier(racine, 'lucide--move-3d')),
        ('MODIFS', fichier(racine, 'lucide--wrench')),
        ('GLOBE', fichier(racine, 'scene')),
        ('OS', fichier(racine, 'os')),
        ('SCENE', fichier(racine, 'iconoir--3d-select-solid')),
        ('ARETES', fichier(racine, 'iconoir--select-edge-3d')),
        ('FACES', fichier(racine, 'iconoir--select-face-3d')),
        ('SELECTION_TOUCHER', fichier(racine, 'selection-toucher')),
        ('SELECTION_BOITE', fichier(racine, 'selection-boite')),
        ('SELECTION_CHEMIN', fichier(racine, 'selection-chemin')),
    ]
    o = ['//! Pictogrammes de GDA, dans leur carré de 24 unités.', '//! Généré par `tools/icones.py` : ne pas modifier à la main.', '',
         '/// Lignes brisées, chacune avec son épaisseur, et disques (x, y, rayon).', 'pub struct Icone {',
         "    pub traits: &'static [(f32, &'static [[f32; 2]])],", "    pub disques: &'static [[f32; 3]],", '}', '']
    open(sortie, 'w', encoding='utf8').write('\n'.join(o) + '\n'.join(rust(n, *i) for n, i in icones))


main()
