#!/usr/bin/env python3
"""Pacotes de ícones da Loja feitos a partir de conjuntos abertos de terceiros.

Fonte: pacotes npm `@iconify-json/*` (SVGs + metadados de licença de cada conjunto).
Os SVGs são rasterizados em .ico (16..256 px) com cairosvg. Cada item do catálogo leva
autor e licença do conjunto original; os textos/avisos estão em docs/loja-creditos.md.

Uso (chamado por gerar.py):  ICONIFY_DIR=/caminho/node_modules/@iconify-json python3 gerar.py
Precisa de: cairosvg, Pillow.
"""
import io, json, os, re, struct

import cairosvg
from PIL import Image

ROLES = ["folder", "pc", "recycle_empty", "recycle_full", "user", "network"]
PRETO = "#000000"

# conjunto -> (pasta npm, nome exibido, autor, licença, atribuição obrigatória?, url)
FONTES = {
    "fluent": ("fluent", "Fluent UI System Icons", "Microsoft Corporation", "MIT", False,
               "https://github.com/microsoft/fluentui-system-icons"),
    "ph": ("ph", "Phosphor Icons", "Phosphor Icons", "MIT", False, "https://github.com/phosphor-icons/core"),
    "tabler": ("tabler", "Tabler Icons", "Paweł Kuna", "MIT", False, "https://github.com/tabler/tabler-icons"),
    "lucide": ("lucide", "Lucide", "Lucide Contributors", "ISC", False, "https://github.com/lucide-icons/lucide"),
    "mdi": ("mdi", "Material Design Icons", "Pictogrammers", "Apache 2.0", False,
            "https://github.com/Templarian/MaterialDesign"),
    "bi": ("bi", "Bootstrap Icons", "The Bootstrap Authors", "MIT", False, "https://github.com/twbs/icons"),
    "fluent-emoji": ("fluent-emoji", "Fluent Emoji", "Microsoft Corporation", "MIT", False,
                     "https://github.com/microsoft/fluentui-emoji"),
    "twemoji": ("twemoji", "Twemoji", "Twitter, Inc. e colaboradores", "CC BY 4.0", True,
                "https://github.com/jdecked/twemoji"),
    "fa6-solid": ("fa6-solid", "Font Awesome Free", "Fonticons, Inc. (Dave Gandy)", "CC BY 4.0", True,
                  "https://github.com/FortAwesome/Font-Awesome"),
    "game-icons": ("game-icons", "game-icons.net", "Lorc, Delapouite e colaboradores", "CC BY 3.0", True,
                   "https://game-icons.net"),
}

# id, nome, descrição, conjunto, [folder, pc, recycle_empty, recycle_full, user, network], tags, preto?
PACOTES = [
    ("fluent-preto-cheio", "Fluent preto (cheio)", "Estilo Windows 11, glifos pretos preenchidos.", "fluent",
     ["folder-48-filled", "desktop-48-filled", "delete-48-regular", "delete-48-filled", "person-48-filled", "globe-48-filled"],
     ["preto", "monocromático", "windows 11"], True),
    ("fluent-preto-linha", "Fluent preto (linha)", "Estilo Windows 11, só contorno preto.", "fluent",
     ["folder-48-regular", "desktop-48-regular", "delete-48-regular", "delete-48-filled", "person-48-regular", "globe-48-regular"],
     ["preto", "monocromático", "windows 11"], True),
    ("phosphor-preto-linha", "Phosphor preto (regular)", "Traço preto regular do Phosphor.", "ph",
     ["folder", "desktop", "trash", "trash-simple-bold", "user", "globe"], ["preto", "monocromático", "linha"], True),
    ("phosphor-preto-negrito", "Phosphor preto (negrito)", "Traço preto grosso do Phosphor.", "ph",
     ["folder-bold", "desktop-bold", "trash-bold", "trash-fill", "user-bold", "globe-bold"],
     ["preto", "monocromático", "negrito"], True),
    ("phosphor-preto-cheio", "Phosphor preto (cheio)", "Glifos pretos preenchidos do Phosphor.", "ph",
     ["folder-fill", "desktop-fill", "trash-simple-fill", "trash-fill", "user-fill", "globe-fill"],
     ["preto", "monocromático", "cheio"], True),
    ("tabler-preto-linha", "Tabler preto (linha)", "Traço fino preto do Tabler.", "tabler",
     ["folder", "device-desktop", "trash", "trash-x", "user", "world"], ["preto", "monocromático", "linha"], True),
    ("tabler-preto-cheio", "Tabler preto (cheio)", "Glifos pretos preenchidos do Tabler.", "tabler",
     ["folder-filled", "device-desktop", "trash", "trash-filled", "user-filled", "world-filled"],
     ["preto", "monocromático", "cheio"], True),
    ("lucide-preto", "Lucide preto", "Traço limpo e preto do Lucide.", "lucide",
     ["folder", "monitor", "trash", "trash-2", "user", "globe"], ["preto", "monocromático", "linha"], True),
    ("bootstrap-preto-linha", "Bootstrap preto (linha)", "Contorno preto simples do Bootstrap Icons.", "bi",
     ["folder", "pc-display", "trash", "trash2", "person", "globe"], ["preto", "monocromático", "linha"], True),
    ("bootstrap-preto-cheio", "Bootstrap preto (cheio)", "Glifos pretos preenchidos do Bootstrap Icons.", "bi",
     ["folder-fill", "pc-display", "trash3", "trash3-fill", "person-fill", "hdd-network-fill"],
     ["preto", "monocromático", "cheio"], True),
    ("material-preto-cheio", "Material preto (cheio)", "Estilo Material Design, preto preenchido.", "mdi",
     ["folder", "desktop-classic", "delete-empty", "delete", "account", "lan"],
     ["preto", "monocromático", "material"], True),
    ("material-preto-linha", "Material preto (contorno)", "Estilo Material Design, contorno preto.", "mdi",
     ["folder-outline", "monitor", "delete-empty-outline", "delete-outline", "account-outline", "earth"],
     ["preto", "monocromático", "material"], True),
    ("fontawesome-preto", "Font Awesome preto", "Glifos clássicos pretos do Font Awesome Free (atribuição CC BY 4.0).",
     "fa6-solid", ["folder", "computer", "trash-can", "trash", "user", "network-wired"],
     ["preto", "monocromático", "clássico"], True),
    ("gameicons-preto", "Game Icons preto", "Silhuetas pretas temáticas do game-icons.net (atribuição CC BY 3.0).",
     "game-icons", ["open-folder", "pc", "trash-can", "cardboard-box", "person", "earth-america"],
     ["preto", "monocromático", "jogos"], True),
    # coloridos (extra)
    ("fluent-emoji-cor", "Fluent Emoji (colorido)", "Emojis coloridos da Microsoft como ícones.", "fluent-emoji",
     ["file-folder", "desktop-computer", "wastebasket", "recycling-symbol", "bust-in-silhouette", "globe-with-meridians"],
     ["colorido", "emoji"], False),
    ("twemoji-cor", "Twemoji (colorido)", "Emojis coloridos do Twemoji (atribuição CC BY 4.0).", "twemoji",
     ["file-folder", "desktop-computer", "wastebasket", "recycling-symbol", "bust-in-silhouette", "globe-with-meridians"],
     ["colorido", "emoji"], False),
]


def _carrega(pasta):
    base = os.environ.get("ICONIFY_DIR", "/tmp/ic/node_modules/@iconify-json")
    with open(os.path.join(base, pasta, "icons.json"), encoding="utf-8") as f:
        return json.load(f)


_cache = {}


def svg(conjunto, nome, preto):
    pasta = FONTES[conjunto][0]
    if pasta not in _cache:
        _cache[pasta] = _carrega(pasta)
    j = _cache[pasta]
    ic = j["icons"].get(nome)
    if ic is None:
        nome = j["aliases"][nome]["parent"]
        ic = j["icons"][nome]
    w = ic.get("width", j.get("width", 16))
    h = ic.get("height", j.get("height", 16))
    corpo = ic["body"]
    if preto:
        corpo = corpo.replace("currentColor", PRETO)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
            f'viewBox="0 0 {w} {h}" color="{PRETO}">{corpo}</svg>')


def _png(s, tam):
    miolo = int(tam * 0.84)  # margem ~8% de cada lado
    b = cairosvg.svg2png(bytestring=s.encode(), output_width=miolo, output_height=miolo)
    im = Image.open(io.BytesIO(b)).convert("RGBA")
    tela = Image.new("RGBA", (tam, tam), (0, 0, 0, 0))
    tela.alpha_composite(im, ((tam - im.width) // 2, (tam - im.height) // 2))
    o = io.BytesIO()
    tela.save(o, "PNG")
    return o.getvalue()


def ico_de_svg(s, tams=(16, 24, 32, 48, 64, 256)):
    pngs = [(t, _png(s, t)) for t in tams]
    saida = struct.pack("<HHH", 0, 1, len(pngs))
    desloc = 6 + 16 * len(pngs)
    for t, d in pngs:
        saida += struct.pack("<BBBBHHII", t % 256, t % 256, 0, 0, 1, 32, len(d), desloc)
        desloc += len(d)
    return saida + b"".join(d for _, d in pngs)


def pacote(conjunto, nomes, preto):
    arqs = {f"{r}.ico": ico_de_svg(svg(conjunto, n, preto)) for r, n in zip(ROLES, nomes)}
    return arqs, {"tipo": "icones", "papeis": {r: f"{r}.ico" for r in ROLES}}


def previa(conjunto, nomes, preto):
    t = 64
    img = Image.new("RGBA", (len(ROLES) * (t + 14) + 14, t + 28), (238, 240, 246, 255))
    for i, n in enumerate(nomes):
        p = Image.open(io.BytesIO(_png(svg(conjunto, n, preto), t))).convert("RGBA")
        img.alpha_composite(p, (14 + i * (t + 14), 14))
    return img
