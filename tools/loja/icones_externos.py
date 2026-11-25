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
from PIL import Image, ImageFilter

ROLES_SISTEMA = ["folder", "pc", "recycle_empty", "recycle_full", "user", "network"]
# unidade, tipos de arquivo e categorias de aplicativos (atalhos .lnk)
ROLES_EXTRA = ["drive", "file_text", "file_zip", "file_script", "app_browser", "app_terminal", "app_code",
               "app_chat", "app_music", "app_video", "app_game", "app_tools", "app_office", "app_generic"]
ROLES = ROLES_SISTEMA + ROLES_EXTRA
# "Quase preto": miolo #14161A com um fio claro em volta, para ler no fundo claro e no escuro.
PRETO = "#14161a"
HALO = (242, 244, 248, 235)

# Nomes-base (primeiro que existir vale) de cada papel extra, por conjunto.
BASE = {
    "fluent": {"drive": ["hard-drive"], "file_text": ["document"], "file_zip": ["archive"],
               "file_script": ["code-block"], "app_browser": ["globe"],
               "app_terminal": ["window-dev-tools", "window-console"], "app_code": ["code", "braces"],
               "app_chat": ["chat"], "app_music": ["headphones", "note"], "app_video": ["video"],
               "app_game": ["games"], "app_tools": ["settings"], "app_office": ["briefcase"],
               "app_generic": ["app-generic", "apps"]},
    "ph": {"drive": ["hard-drive"], "file_text": ["file-text"], "file_zip": ["file-zip"],
           "file_script": ["file-code"], "app_browser": ["globe"], "app_terminal": ["terminal-window"],
           "app_code": ["code"], "app_chat": ["chat-circle"], "app_music": ["music-note"],
           "app_video": ["video"], "app_game": ["game-controller"], "app_tools": ["gear"],
           "app_office": ["briefcase"], "app_generic": ["app-window"]},
    "tabler": {"drive": ["server", "database"], "file_text": ["file-text"], "file_zip": ["file-zip"],
               "file_script": ["file-code"], "app_browser": ["globe"], "app_terminal": ["terminal-2"],
               "app_code": ["code"], "app_chat": ["message-circle"], "app_music": ["music"],
               "app_video": ["video"], "app_game": ["device-gamepad-2"], "app_tools": ["settings"],
               "app_office": ["briefcase"], "app_generic": ["app-window"]},
    "lucide": {"drive": ["hard-drive"], "file_text": ["file-text"], "file_zip": ["file-archive"],
               "file_script": ["file-code"], "app_browser": ["globe"], "app_terminal": ["square-terminal"],
               "app_code": ["code-xml"], "app_chat": ["message-circle"], "app_music": ["music"],
               "app_video": ["video"], "app_game": ["gamepad-2"], "app_tools": ["settings"],
               "app_office": ["briefcase"], "app_generic": ["app-window"]},
    "bi": {"drive": ["hdd"], "file_text": ["file-text"], "file_zip": ["file-zip"], "file_script": ["file-code"],
           "app_browser": ["globe"], "app_terminal": ["terminal"], "app_code": ["code-slash"],
           "app_chat": ["chat"], "app_music": ["music-note-beamed"], "app_video": ["camera-video"],
           "app_game": ["controller"], "app_tools": ["gear"], "app_office": ["briefcase"],
           "app_generic": ["app"]},
    "mdi": {"drive": ["harddisk"], "file_text": ["file-document"], "file_zip": ["zip-box"],
            "file_script": ["script-text"], "app_browser": ["web"], "app_terminal": ["console"],
            "app_code": ["code-braces"], "app_chat": ["chat"], "app_music": ["music"],
            "app_video": ["video"], "app_game": ["gamepad-variant"], "app_tools": ["cog"],
            "app_office": ["briefcase"], "app_generic": ["application"]},
    "fa6-solid": {"drive": ["hard-drive"], "file_text": ["file-lines"], "file_zip": ["file-zipper"],
                  "file_script": ["file-code"], "app_browser": ["globe"], "app_terminal": ["terminal"],
                  "app_code": ["code"], "app_chat": ["comment"], "app_music": ["music"],
                  "app_video": ["video"], "app_game": ["gamepad"], "app_tools": ["gear"],
                  "app_office": ["briefcase"], "app_generic": ["window-maximize"]},
    "game-icons": {"drive": ["database"], "file_text": ["files"], "file_zip": ["archive-register"],
                   "file_script": ["scroll-unfurled", "tied-scroll"], "app_browser": ["world"],
                   "app_terminal": ["pc"], "app_code": ["processor"], "app_chat": ["chat-bubble"],
                   "app_music": ["musical-notes"], "app_video": ["video-camera"], "app_game": ["gamepad"],
                   "app_tools": ["cog"], "app_office": ["briefcase", "notebook"], "app_generic": ["window"]},
}
for _emoji in ("fluent-emoji", "twemoji"):
    BASE[_emoji] = {"drive": ["floppy-disk", "computer-disk"], "file_text": ["page-facing-up", "memo"],
                    "file_zip": ["package"], "file_script": ["scroll"], "app_browser": ["globe-with-meridians"],
                    "app_terminal": ["laptop"], "app_code": ["keyboard"], "app_chat": ["speech-balloon"],
                    "app_music": ["musical-note"], "app_video": ["movie-camera"], "app_game": ["video-game"],
                    "app_tools": ["gear"], "app_office": ["briefcase"], "app_generic": ["window", "puzzle-piece"]}

# sufixos de estilo por pacote: (conjunto, estilo) -> lista de sufixos tentados em ordem
ESTILO = {
    "fluent-preto-cheio": ["-{n}-filled", "-{n}-regular"], "fluent-preto-linha": ["-{n}-regular"],
    "phosphor-preto-linha": [""], "phosphor-preto-negrito": ["-bold", ""], "phosphor-preto-cheio": ["-fill", ""],
    "tabler-preto-linha": [""], "tabler-preto-cheio": ["-filled", ""],
    "bootstrap-preto-linha": [""], "bootstrap-preto-cheio": ["-fill", ""],
    "material-preto-cheio": [""], "material-preto-linha": ["-outline", ""],
}

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


_cache = {}


def nomes_extra(id_, conjunto):
    """Nomes dos ícones dos papéis extras para o pacote (primeiro que existir)."""
    pasta = FONTES[conjunto][0]
    if pasta not in _cache:
        _cache[pasta] = _carrega(pasta)
    j = _cache[pasta]
    existe = set(j["icons"]) | set(j.get("aliases", {}))
    sufixos = ESTILO.get(id_, [""])
    saida = []
    for r in ROLES_EXTRA:
        achou = None
        for base in BASE[conjunto][r]:
            for suf in sufixos:
                tams = (48, 32, 24, 20) if "{n}" in suf else (0,)
                for n in tams:
                    cand = base + suf.replace("{n}", str(n))
                    if cand in existe:
                        achou = cand
                        break
                if achou:
                    break
            if achou:
                break
        if not achou:
            raise SystemExit(f"{id_}: sem ícone para {r} em {conjunto}")
        saida.append(achou)
    return saida


def _carrega(pasta):
    base = os.environ.get("ICONIFY_DIR", "/tmp/ic/node_modules/@iconify-json")
    with open(os.path.join(base, pasta, "icons.json"), encoding="utf-8") as f:
        return json.load(f)


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


def _png(s, tam, halo=False):
    miolo = int(tam * 0.84)  # margem ~8% de cada lado
    b = cairosvg.svg2png(bytestring=s.encode(), output_width=miolo, output_height=miolo)
    im = Image.open(io.BytesIO(b)).convert("RGBA")
    tela = Image.new("RGBA", (tam, tam), (0, 0, 0, 0))
    if halo:
        # fio claro em volta do glifo: aparece no fundo escuro sem sumir no claro
        r = max(1, round(tam / 42))
        alfa = im.getchannel("A")
        grande = alfa.filter(ImageFilter.MaxFilter(2 * r + 1))
        fio = Image.new("RGBA", im.size, HALO[:3] + (0,))
        fio.putalpha(grande.point(lambda v: v * HALO[3] // 255))
        tela.alpha_composite(fio, ((tam - im.width) // 2, (tam - im.height) // 2))
    tela.alpha_composite(im, ((tam - im.width) // 2, (tam - im.height) // 2))
    o = io.BytesIO()
    tela.save(o, "PNG")
    return o.getvalue()


def ico_de_svg(s, tams=(16, 24, 32, 48, 64, 256), halo=False):
    pngs = [(t, _png(s, t, halo)) for t in tams]
    saida = struct.pack("<HHH", 0, 1, len(pngs))
    desloc = 6 + 16 * len(pngs)
    for t, d in pngs:
        saida += struct.pack("<BBBBHHII", t % 256, t % 256, 0, 0, 1, 32, len(d), desloc)
        desloc += len(d)
    return saida + b"".join(d for _, d in pngs)


def pacote(id_, conjunto, nomes, preto):
    nomes = list(nomes) + nomes_extra(id_, conjunto)
    arqs = {f"{r}.ico": ico_de_svg(svg(conjunto, n, preto), halo=preto) for r, n in zip(ROLES, nomes)}
    return arqs, {"tipo": "icones", "papeis": {r: f"{r}.ico" for r in ROLES}}


def previa(id_, conjunto, nomes, preto):
    """Faixa com 10 papéis: metade sobre fundo escuro, metade sobre claro."""
    todos = dict(zip(ROLES, list(nomes) + nomes_extra(id_, conjunto)))
    ordem = ["folder", "pc", "recycle_empty", "user", "drive", "file_text", "file_zip", "app_browser",
             "app_terminal", "app_chat", "app_music", "app_game"]
    t, m = 56, 12
    larg = len(ordem) * (t + m) + m
    img = Image.new("RGBA", (larg, 2 * (t + 2 * m)), (28, 32, 48, 255))
    img.paste(Image.new("RGBA", (larg, t + 2 * m), (238, 240, 246, 255)), (0, t + 2 * m))
    for linha in range(2):
        for i, r in enumerate(ordem):
            p = Image.open(io.BytesIO(_png(svg(conjunto, todos[r], preto), t, preto))).convert("RGBA")
            img.alpha_composite(p, (m + i * (t + m), m + linha * (t + 2 * m)))
    return img
