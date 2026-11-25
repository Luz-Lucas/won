#!/usr/bin/env python3
"""Gera o conteúdo da Loja do WON (cursores, pacotes de ícones, temas).

Tudo é desenho ORIGINAL feito por este script (CC0 1.0): nada foi copiado de
terceiros, então não há créditos a repartir. Saída:

  catalog/loja/loja.zip        index.json + previews + pacotes (embutido no app)
  catalog/loja/remoto.json     catálogo online (itens extras baixados sob demanda)
  catalog/loja/remoto/*.zip    pacotes dos itens extras

Uso: python3 tools/loja/gerar.py   (precisa de Pillow)
"""
import base64, hashlib, io, json, math, os, struct, sys, zipfile
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

RAIZ = os.path.normpath(os.path.join(os.path.dirname(__file__), "..", ".."))
SAIDA = os.path.join(RAIZ, "catalog", "loja")
RAW = "https://raw.githubusercontent.com/Luz-Lucas/sentinela/main/catalog/loja/remoto/"
AUTOR = "WON"
LICENCA = "CC0 1.0"
FONTE = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
DATA = (2026, 11, 24, 0, 0, 0)


def hexrgb(h, a=255):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), a)


# ───────────────────────── cursores ─────────────────────────
SS = 4
ROLES_CUR = ["Arrow", "Help", "AppStarting", "Wait", "Crosshair", "IBeam", "NO",
             "SizeNS", "SizeWE", "SizeNWSE", "SizeNESW", "SizeAll", "UpArrow", "Hand"]
ANIMADOS = {"AppStarting", "Wait"}
HOT = {  # ponto ativo na grade de 32
    "Arrow": (4, 2), "Help": (4, 2), "AppStarting": (4, 2), "UpArrow": (16, 3), "Hand": (13.5, 3),
}


def seta(d, u, k=1.0, dx=0.0, dy=0.0):
    pts = [(4, 2), (4, 24), (10, 18.5), (14.5, 28), (18.5, 26.3), (14, 17), (22, 17)]
    d.polygon([((x * k + dx) * u, (y * k + dy) * u) for x, y in pts], fill=255)


def anel(d, u, cx, cy, r, w, ini=None, fim=None):
    box = [(cx - r) * u, (cy - r) * u, (cx + r) * u, (cy + r) * u]
    if ini is None:
        d.ellipse(box, outline=255, width=max(1, int(w * u)))
    else:
        d.arc(box, ini, fim, fill=255, width=max(1, int(w * u)))


def setas_duplas(d, u, curto=False):
    a = 11 if curto else 8
    d.rectangle([a * u, 14.8 * u, (32 - a) * u, 17.2 * u], fill=255)
    d.polygon([(3 * u, 16 * u), (10 * u, 10 * u), (10 * u, 22 * u)], fill=255)
    d.polygon([(29 * u, 16 * u), (22 * u, 10 * u), (22 * u, 22 * u)], fill=255)


def forma(role, u, S, giro=0):
    """Devolve (corpo, destaque): máscaras 'L' de tamanho S."""
    corpo = Image.new("L", (S, S), 0)
    dest = Image.new("L", (S, S), 0)
    c, a = ImageDraw.Draw(corpo), ImageDraw.Draw(dest)
    if role == "Arrow":
        seta(c, u)
    elif role == "Help":
        seta(c, u, 0.78)
        f = ImageFont.truetype(FONTE, int(17 * u))
        a.text((22.5 * u, 22 * u), "?", font=f, fill=255, anchor="mm")
    elif role == "AppStarting":
        seta(c, u, 0.8)
        anel(a, u, 22, 22, 5.2, 2.4, giro, giro + 270)
    elif role == "Wait":
        anel(c, u, 16, 16, 10, 3.2)
        anel(a, u, 16, 16, 10, 3.2, giro, giro + 100)
    elif role == "Crosshair":
        for x0, y0, x1, y1 in [(3, 15, 12.5, 17), (19.5, 15, 29, 17), (15, 3, 17, 12.5), (15, 19.5, 17, 29)]:
            c.rectangle([x0 * u, y0 * u, x1 * u, y1 * u], fill=255)
        a.ellipse([14.6 * u, 14.6 * u, 17.4 * u, 17.4 * u], fill=255)
    elif role == "IBeam":
        c.rectangle([15 * u, 6 * u, 17 * u, 26 * u], fill=255)
        c.rectangle([11.5 * u, 4.5 * u, 20.5 * u, 6.6 * u], fill=255)
        c.rectangle([11.5 * u, 25.4 * u, 20.5 * u, 27.5 * u], fill=255)
    elif role == "NO":
        anel(c, u, 16, 16, 11, 3.2)
        k = 11 * math.cos(math.pi / 4)
        a.line([((16 - k) * u, (16 + k) * u), ((16 + k) * u, (16 - k) * u)], fill=255, width=int(3.4 * u))
    elif role in ("SizeWE", "SizeNS", "SizeNESW", "SizeNWSE"):
        t = Image.new("L", (S, S), 0)
        setas_duplas(ImageDraw.Draw(t), u)
        ang = {"SizeWE": 0, "SizeNS": 90, "SizeNESW": 45, "SizeNWSE": -45}[role]
        corpo = t.rotate(ang, resample=Image.BICUBIC, center=(S / 2, S / 2))
    elif role == "SizeAll":
        t = Image.new("L", (S, S), 0)
        setas_duplas(ImageDraw.Draw(t), u, curto=True)
        corpo = ImageChops.lighter(t, t.rotate(90, resample=Image.BICUBIC, center=(S / 2, S / 2)))
    elif role == "UpArrow":
        c.polygon([(x * u, y * u) for x, y in [(16, 3), (7, 14), (13, 14), (13, 27), (19, 27), (19, 14), (25, 14)]], fill=255)
    elif role == "Hand":
        r = lambda x0, y0, x1, y1, rr: c.rounded_rectangle([x0 * u, y0 * u, x1 * u, y1 * u], radius=rr * u, fill=255)
        r(11, 3, 16.2, 18, 2.6)
        r(8.2, 15, 26.5, 28.5, 4)
        r(16, 10, 19.8, 19, 1.9)
        r(19.6, 11.5, 23.4, 19, 1.9)
        r(23.2, 13.5, 26.8, 20, 1.9)
        c.polygon([(x * u, y * u) for x, y in [(8.5, 20), (4, 15.5), (6.6, 13.4), (10.5, 17.5)]], fill=255)
    return corpo, dest


def quadro_cursor(role, estilo, tam, giro=0):
    S = tam * SS
    u = S / 32
    corpo, dest = forma(role, u, S, giro)
    uniao = ImageChops.lighter(corpo, dest)
    k = int(estilo["borda"] * u) * 2 + 1
    contorno = uniao.filter(ImageFilter.MaxFilter(k))
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    img.paste(Image.new("RGBA", (S, S), estilo["contorno"]), mask=contorno)
    img.paste(Image.new("RGBA", (S, S), estilo["corpo"]), mask=corpo)
    img.paste(Image.new("RGBA", (S, S), estilo["destaque"]), mask=dest)
    return img.resize((tam, tam), Image.LANCZOS)


def entrada_bmp(img):
    w, h = img.size
    px = img.load()
    linhas = b"".join(
        b"".join(struct.pack("<BBBB", px[x, y][2], px[x, y][1], px[x, y][0], px[x, y][3]) for x in range(w))
        for y in range(h - 1, -1, -1)
    )
    mascara = bytes(((w + 31) // 32) * 4 * h)
    cab = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(linhas) + len(mascara), 0, 0, 0, 0)
    return cab + linhas + mascara


def cur(imagens, hot):
    """`imagens`: lista de quadros RGBA quadrados (tamanhos diferentes)."""
    dados = [entrada_bmp(i) for i in imagens]
    saida = struct.pack("<HHH", 0, 2, len(imagens))
    desloc = 6 + 16 * len(imagens)
    for img, d in zip(imagens, dados):
        w = img.size[0]
        hx, hy = round(hot[0] * w / 32), round(hot[1] * w / 32)
        saida += struct.pack("<BBBBHHII", w % 256, w % 256, 0, 0, hx, hy, len(d), desloc)
        desloc += len(d)
    return saida + b"".join(dados)


def ani(quadros_cur, jif=4):
    n = len(quadros_cur)
    anih = struct.pack("<IIIIIIIII", 36, n, n, 0, 0, 0, 0, jif, 1)
    lista = b"fram"
    for q in quadros_cur:
        lista += b"icon" + struct.pack("<I", len(q)) + q + (b"\0" if len(q) % 2 else b"")
    corpo = b"ACON" + b"anih" + struct.pack("<I", 36) + anih + b"LIST" + struct.pack("<I", len(lista)) + lista
    return b"RIFF" + struct.pack("<I", len(corpo)) + corpo


def pacote_cursor(estilo):
    tams = estilo.get("tamanhos", (32, 48, 64))
    arqs = {}
    for role in ROLES_CUR:
        hot = HOT.get(role, (16, 16))
        if role in ANIMADOS:
            n = 12
            quadros = [cur([quadro_cursor(role, estilo, t, giro=i * 360 // n) for t in tams[:2]], hot) for i in range(n)]
            arqs[role + ".ani"] = ani(quadros)
        else:
            arqs[role + ".cur"] = cur([quadro_cursor(role, estilo, t) for t in tams], hot)
    manifesto = {"tipo": "cursores", "papeis": {r: (r + (".ani" if r in ANIMADOS else ".cur")) for r in ROLES_CUR}}
    return arqs, manifesto


def previa_cursor(estilo):
    papeis = ["Arrow", "Help", "Wait", "Crosshair", "IBeam", "NO", "SizeAll", "Hand"]
    t = 44
    img = Image.new("RGBA", (len(papeis) * (t + 14) + 14, t + 28), hexrgb("#1c2030"))
    claro = Image.new("RGBA", (img.width // 2, img.height), hexrgb("#eef0f6"))
    img.paste(claro, (img.width // 2, 0))
    for i, r in enumerate(papeis):
        q = quadro_cursor(r, estilo, t)
        img.alpha_composite(q, (14 + i * (t + 14), 14))
    return img


# ───────────────────────── ícones ─────────────────────────
ROLES_ICO = ["folder", "pc", "recycle_empty", "recycle_full", "user", "network"]
W = 256


def glifo(nome):
    """Primitivas: (tipo, coords, papel, opções). Grade 256."""
    if nome == "folder":
        return [
            ("rr", (22, 52, 118, 112), "dark", {"r": 14}),
            ("rr", (22, 80, 234, 208), "main", {"r": 20}),
            ("rr", (22, 100, 234, 208), "light", {"r": 20}),
            ("ln", (50, 124, 120, 124), "accent", {"w": 8}),
        ]
    if nome == "pc":
        return [
            ("rr", (26, 38, 230, 172), "main", {"r": 18}),
            ("rr", (42, 54, 214, 156), "light", {"r": 8}),
            ("rr", (58, 70, 112, 100), "accent", {"r": 5}),
            ("pg", [(112, 172), (144, 172), (154, 208), (102, 208)], "dark", {}),
            ("rr", (80, 204, 176, 226), "dark", {"r": 9}),
        ]
    if nome in ("recycle_empty", "recycle_full"):
        p = []
        if nome == "recycle_full":
            p += [
                ("pg", [(78, 76), (108, 28), (134, 56), (122, 82)], "accent", {}),
                ("pg", [(118, 76), (166, 26), (186, 60), (160, 84)], "light", {}),
            ]
        p += [
            ("rr", (104, 38, 152, 66), "dark", {"r": 9, "o": 1}),
            ("rr", (46, 62, 210, 88), "dark", {"r": 10}),
            ("pg", [(64, 92), (192, 92), (180, 226), (76, 226)], "main", {}),
            ("ln", (104, 112, 108, 204), "light", {"w": 9}),
            ("ln", (128, 112, 128, 204), "light", {"w": 9}),
            ("ln", (152, 112, 148, 204), "light", {"w": 9}),
        ]
        return p
    if nome == "user":
        return [
            ("el", (86, 34, 170, 118), "main", {}),
            ("el", (40, 138, 216, 300), "light", {}),
        ]
    if nome == "network":
        return [
            ("el", (32, 32, 224, 224), "main", {}),
            ("el", (84, 32, 172, 224), "light", {"o": 1, "w": 9}),
            ("ln", (32, 128, 224, 128), "light", {"w": 9}),
            ("ln", (50, 82, 206, 82), "light", {"w": 8}),
            ("ln", (50, 174, 206, 174), "light", {"w": 8}),
        ]
    raise KeyError(nome)


def pintar(d, prim, cor, preenche, w_linha):
    tipo, co, papel, op = prim
    c = cor.get(papel)
    if c is None:
        return
    contorno_so = op.get("o") or not preenche
    w = op.get("w", w_linha)
    kw = dict(outline=c, width=w) if contorno_so else dict(fill=c)
    if tipo == "rr":
        d.rounded_rectangle(co, radius=op.get("r", 8), **kw)
    elif tipo == "el":
        d.ellipse(co, **kw)
    elif tipo == "pg":
        if contorno_so:
            d.line(co + [co[0]], fill=c, width=w, joint="curve")
        else:
            d.polygon(co, fill=c)
    elif tipo == "ln":
        d.line(co, fill=c, width=op.get("w", 8))


def desenhar_icone(nome, estilo, tam):
    f = 2
    S = W * f
    esc = lambda co: [v * f for v in co] if isinstance(co[0], (int, float)) else [(x * f, y * f) for x, y in co]
    camada = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(camada)
    tipo_estilo = estilo["tipo"]
    if estilo.get("tile"):
        d.rounded_rectangle([8 * f, 8 * f, 248 * f, 248 * f], radius=52 * f, fill=estilo["tile"],
                            outline=estilo.get("tile_borda"), width=4 * f if estilo.get("tile_borda") else 0)
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    # com tile, o glifo encolhe para 70% e fica centralizado
    k = 0.66 if estilo.get("tile") else 1.0
    desloc = (W * (1 - k) / 2)
    for p in glifo(nome):
        t, co, papel, op = p
        if isinstance(co[0], (int, float)):
            co2 = [(v * k + desloc) * f for v in co]
        else:
            co2 = [((x * k + desloc) * f, (y * k + desloc) * f) for x, y in co]
        op2 = dict(op)
        for chave in ("r", "w"):
            if chave in op2:
                op2[chave] = max(2, int(op2[chave] * k * f / (1 if chave == "r" else 1)))
        base_w = int(estilo.get("traco", 11) * k * f)
        pintar(d, (t, co2, papel, op2), estilo["cores"], tipo_estilo != "linha", base_w)
        if estilo.get("brilho"):
            pintar(gd, (t, co2, papel, op2), estilo["cores"], False, base_w)
    if estilo.get("brilho"):
        g = glow.filter(ImageFilter.GaussianBlur(6 * f))
        base = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        base.alpha_composite(camada, (0, 0)) if False else None
        camada = Image.alpha_composite(Image.alpha_composite(camada, Image.new("RGBA", (S, S), (0, 0, 0, 0))), Image.new("RGBA", (S, S), (0, 0, 0, 0)))
        # tile primeiro, brilho, depois o traço
        fundo = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        if estilo.get("tile"):
            dd = ImageDraw.Draw(fundo)
            dd.rounded_rectangle([8 * f, 8 * f, 248 * f, 248 * f], radius=52 * f, fill=estilo["tile"],
                                 outline=estilo.get("tile_borda"), width=4 * f)
        fundo = Image.alpha_composite(fundo, g)
        fundo = Image.alpha_composite(fundo, g)
        tracos = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        td = ImageDraw.Draw(tracos)
        for p in glifo(nome):
            t, co, papel, op = p
            if isinstance(co[0], (int, float)):
                co2 = [(v * k + desloc) * f for v in co]
            else:
                co2 = [((x * k + desloc) * f, (y * k + desloc) * f) for x, y in co]
            op2 = dict(op)
            for chave in ("r", "w"):
                if chave in op2:
                    op2[chave] = max(2, int(op2[chave] * k * f))
            pintar(td, (t, co2, papel, op2), estilo["cores"], False, int(estilo.get("traco", 11) * k * f))
        camada = Image.alpha_composite(fundo, tracos)
    return camada.resize((tam, tam), Image.LANCZOS)


def ico(nome, estilo, tams=(16, 24, 32, 48, 64, 256)):
    pngs = []
    for t in tams:
        b = io.BytesIO()
        desenhar_icone(nome, estilo, t).save(b, "PNG")
        pngs.append((t, b.getvalue()))
    saida = struct.pack("<HHH", 0, 1, len(pngs))
    desloc = 6 + 16 * len(pngs)
    for t, d in pngs:
        saida += struct.pack("<BBBBHHII", t % 256, t % 256, 0, 0, 1, 32, len(d), desloc)
        desloc += len(d)
    return saida + b"".join(d for _, d in pngs)


def pacote_icones(estilo):
    arqs = {f"{r}.ico": ico(r, estilo) for r in ROLES_ICO}
    return arqs, {"tipo": "icones", "papeis": {r: f"{r}.ico" for r in ROLES_ICO}}


def previa_icones(estilo):
    t = 64
    img = Image.new("RGBA", (len(ROLES_ICO) * (t + 14) + 14, t + 28), hexrgb("#1c2030"))
    claro = Image.new("RGBA", (img.width // 2, img.height), hexrgb("#eef0f6"))
    img.paste(claro, (img.width // 2, 0))
    for i, r in enumerate(ROLES_ICO):
        img.alpha_composite(desenhar_icone(r, estilo, t), (14 + i * (t + 14), 14))
    return img


# ───────────────────────── estilos ─────────────────────────
def est_cur(corpo, contorno, destaque, borda=0.9, **x):
    return dict(corpo=hexrgb(corpo), contorno=hexrgb(contorno), destaque=hexrgb(destaque), borda=borda, **x)


CURSORES = [
    # id, nome, descrição, embutido, estilo, tags
    ("cursor-noite", "Noite", "Seta escura com contorno branco fino; a mais discreta em fundos claros e escuros.", True,
     est_cur("#14161d", "#f5f7ff", "#5b8cff"), ["escuro", "clássico"]),
    ("cursor-claro", "Claro", "Seta branca com contorno escuro, no estilo do tema claro do Windows.", True,
     est_cur("#ffffff", "#15171d", "#e5484d"), ["claro", "clássico"]),
    ("cursor-neon-ciano", "Neon ciano", "Corpo escuro com borda ciano brilhante e giro de espera em ciano.", True,
     est_cur("#0a1019", "#22e5ff", "#22e5ff", borda=1.0), ["neon", "escuro"]),
    ("cursor-magenta", "Magenta pulso", "Corpo escuro com borda magenta e destaques rosa vibrante.", True,
     est_cur("#170d1c", "#ff3ea5", "#ff7ac6", borda=1.0), ["neon", "escuro"]),
    ("cursor-ambar-grande", "Âmbar grande (alto contraste)", "Ponteiros 50% maiores, amarelo com contorno preto grosso: fácil de achar na tela.", True,
     est_cur("#ffc400", "#000000", "#ff6a00", borda=1.5, tamanhos=(48, 72, 96)), ["acessibilidade", "grande"]),
]
CURSORES_EXTRA = [
    ("cursor-verde-matrix", "Verde terminal", "Corpo preto com borda verde fosforescente, para quem vive no terminal.", False,
     est_cur("#050a06", "#39ff7a", "#39ff7a", borda=1.0), ["neon", "escuro"]),
    ("cursor-grafite", "Grafite", "Cinza médio com contorno claro: sóbrio, sem cor.", False,
     est_cur("#4a4f5c", "#eceff6", "#9aa3b8"), ["neutro", "clássico"]),
    ("cursor-sunset", "Pôr do sol", "Laranja queimado com contorno creme e destaque coral.", False,
     est_cur("#ff7a45", "#fff3e6", "#ffb199"), ["quente", "claro"]),
]


def est_ico(tipo, cores, **x):
    return dict(tipo=tipo, cores={k: hexrgb(v) if v else None for k, v in cores.items()}, **x)


ICONES = [
    ("icones-flat", "Flat colorido", "Ícones planos com cores quentes (pasta âmbar, monitor azul).", True,
     est_ico("flat", {"main": "#f5a524", "light": "#ffc94d", "dark": "#c97a0a", "accent": "#fff1c2"}), ["colorido", "plano"]),
    ("icones-linha", "Linha azul", "Só contorno, em azul-lavanda: leve e legível no claro e no escuro.", True,
     est_ico("linha", {"main": "#6f93f5", "light": "#6f93f5", "dark": "#6f93f5", "accent": "#f08bb4"}, traco=11), ["minimalista", "contorno"]),
    ("icones-neon", "Neon", "Ladrilho escuro com traço ciano e magenta e brilho suave.", True,
     est_ico("linha", {"main": "#22e5ff", "light": "#22e5ff", "dark": "#22e5ff", "accent": "#ff3ea5"},
             tile="#0d1220", tile_borda="#22e5ff", brilho=True, traco=12), ["neon", "escuro"]),
    ("icones-pastel", "Pastel", "Ladrilho rosa-claro com glifos brancos e detalhes suaves.", True,
     est_ico("flat", {"main": "#ffffff", "light": "#ffe3f0", "dark": "#f4a3c4", "accent": "#a5b4fc"}, tile="#f7b4d2"), ["suave", "claro"]),
]
ICONES_EXTRA = [
    ("icones-oceano", "Oceano", "Ladrilho azul-petróleo com glifos brancos e detalhes turquesa.", False,
     est_ico("flat", {"main": "#ffffff", "light": "#bfeaf5", "dark": "#7fd0e6", "accent": "#00b4d8"}, tile="#0a6a8a"), ["frio", "escuro"]),
    ("icones-ouro", "Ouro e grafite", "Ladrilho grafite com traço dourado.", False,
     est_ico("linha", {"main": "#f2c14e", "light": "#f2c14e", "dark": "#f2c14e", "accent": "#ffffff"},
             tile="#1e2027", tile_borda="#f2c14e", traco=12), ["elegante", "escuro"]),
]

TEMAS = [
    # id, nome, descrição, modo, acento, ajustes, cursor, ícones
    ("tema-noite-neon", "Noite neon", "Escuro, barra centralizada, destaque ciano, ponteiro neon e ícones neon.", "escuro", "#22e5ff",
     ["theme-apps-dark", "theme-system-dark", "taskbar-align-center", "taskbar-search-icon", "taskbar-taskview-off"],
     "cursor-neon-ciano", "icones-neon"),
    ("tema-magenta", "Magenta pulso", "Escuro com destaque magenta, ponteiro e ícones combinando.", "escuro", "#ff3ea5",
     ["theme-apps-dark", "theme-system-dark", "taskbar-align-center", "start-recommendations-off"],
     "cursor-magenta", "icones-neon"),
    ("tema-claro-limpo", "Claro limpo", "Claro, extensões visíveis, destaque azul, ponteiro claro e ícones planos.", "claro", "#3b82f6",
     ["theme-apps-light", "theme-system-light", "explorer-show-extensions", "taskbar-align-left"],
     "cursor-claro", "icones-flat"),
    ("tema-pastel", "Pastel suave", "Claro e delicado, destaque rosa, ícones pastel.", "claro", "#f08bb4",
     ["theme-apps-light", "theme-system-light", "taskbar-align-center", "taskbar-search-icon"],
     "cursor-claro", "icones-pastel"),
    ("tema-foco-total", "Foco total (alto contraste)", "Escuro, explorador compacto, ponteiro âmbar grande e destaque amarelo.", "escuro", "#ffc400",
     ["theme-apps-dark", "theme-system-dark", "explorer-show-extensions", "explorer-compact", "taskbar-widgets-off"],
     "cursor-ambar-grande", "icones-flat"),
    ("tema-linha-minimal", "Linha minimal", "Escuro enxuto com ícones de linha e ponteiro discreto.", "escuro", "#6f93f5",
     ["theme-apps-dark", "theme-system-dark", "taskbar-search-hidden", "taskbar-taskview-off", "start-recommendations-off"],
     "cursor-noite", "icones-linha"),
]


def previa_tema(modo, acento, cur_id, ico_id, estilos):
    w, h = 360, 200
    esc = modo == "escuro"
    img = Image.new("RGBA", (w, h), hexrgb("#10131b" if esc else "#e8ecf5"))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        c = (16 + int(20 * t), 19 + int(10 * t), 27 + int(30 * t)) if esc else (232 - int(30 * t), 236 - int(20 * t), 245 - int(10 * t))
        d.line([(0, y), (w, y)], fill=c + (255,))
    d.rounded_rectangle([40, 24, 250, 130], radius=10, fill=hexrgb("#1f2330" if esc else "#ffffff"))
    d.rounded_rectangle([40, 24, 250, 44], radius=10, fill=hexrgb(acento))
    d.rectangle([40, 34, 250, 44], fill=hexrgb(acento))
    for i in range(3):
        d.rounded_rectangle([54, 58 + i * 20, 54 + 90 - i * 18, 66 + i * 20], radius=4, fill=hexrgb("#3a4056" if esc else "#cfd6e6"))
    d.rectangle([0, h - 28, w, h], fill=hexrgb("#0a0c12" if esc else "#f6f8fc"))
    for i in range(5):
        d.rounded_rectangle([w // 2 - 60 + i * 26, h - 22, w // 2 - 42 + i * 26, h - 6], radius=5, fill=hexrgb(acento if i == 1 else ("#2b3144" if esc else "#c9d1e3")))
    ei = estilos[ico_id]
    for i, r in enumerate(["folder", "pc", "recycle_full"]):
        img.alpha_composite(desenhar_icone(r, ei, 40), (270 + 0, 22 + i * 48))
    img.alpha_composite(quadro_cursor("Arrow", estilos[cur_id], 40), (190, 100))
    return img


# ───────────────────────── montagem ─────────────────────────
def zip_bytes(arqs):
    b = io.BytesIO()
    with zipfile.ZipFile(b, "w", zipfile.ZIP_STORED) as z:
        for nome in sorted(arqs):
            zi = zipfile.ZipInfo(nome, DATA)
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, arqs[nome])
    return b.getvalue()


def png(img):
    b = io.BytesIO()
    img.save(b, "PNG", optimize=True)
    return b.getvalue()


def tema_toml(nome, desc, acento, ajustes):
    ids = ", ".join(f'"{a}"' for a in ajustes)
    return (f'version = 1\n\n[theme]\nname = "{nome}"\ndescription = "{desc}"\n\n'
            f'[tweaks]\nids = [{ids}]\n\n[colors]\naccent = "{acento}"\n')


def itens_externos():
    """Pacotes de ícones feitos de conjuntos abertos de terceiros (ver icones_externos.py)."""
    sys.path.insert(0, os.path.dirname(__file__))
    import icones_externos as ext
    saida = []
    os.makedirs(os.path.join(SAIDA, "remoto"), exist_ok=True)
    for id_, nome, desc, conj, nomes, tags, preto in ext.PACOTES:
        _, nome_fonte, autor, lic, attr, url = ext.FONTES[conj]
        conteudo, manifesto = ext.pacote(id_, conj, nomes, preto)
        conteudo["manifest.json"] = json.dumps(manifesto, indent=1).encode()
        z = zip_bytes(conteudo)
        with open(os.path.join(SAIDA, "remoto", f"{id_}.zip"), "wb") as f:
            f.write(z)
        prev = png(ext.previa(id_, conj, nomes, preto))
        saida.append(dict(id=id_, tipo="icones", nome=nome, autor=f"{autor} ({nome_fonte})", licenca=lic,
                          descricao=f"{desc} Fonte: {url}", tags=tags + ["atalhos", "unidades", "arquivos"],
                          sha256=hashlib.sha256(z).hexdigest(), bytes=len(z), url=RAW + f"{id_}.zip",
                          previa_b64=base64.b64encode(prev).decode()))
    return saida


def main():
    estilos = {i[0]: i[4] for i in CURSORES + CURSORES_EXTRA + ICONES + ICONES_EXTRA}
    arqs = {}
    itens = []
    extras = []
    for grupo, fn_pacote, fn_prev, tipo in [(CURSORES + CURSORES_EXTRA, pacote_cursor, previa_cursor, "cursores"),
                                             (ICONES + ICONES_EXTRA, pacote_icones, previa_icones, "icones")]:
        for id_, nome, desc, embut, est, tags in grupo:
            conteudo, manifesto = fn_pacote(est)
            conteudo["manifest.json"] = json.dumps(manifesto, indent=1).encode()
            z = zip_bytes(conteudo)
            prev = png(fn_prev(est))
            item = dict(id=id_, tipo=tipo, nome=nome, autor=AUTOR, licenca=LICENCA, descricao=desc, tags=tags,
                        sha256=hashlib.sha256(z).hexdigest(), bytes=len(z))
            if embut:
                arqs[f"pacotes/{id_}.zip"] = z
                arqs[f"previas/{id_}.png"] = prev
                item.update(pacote=f"pacotes/{id_}.zip", previa=f"previas/{id_}.png")
                itens.append(item)
            else:
                os.makedirs(os.path.join(SAIDA, "remoto"), exist_ok=True)
                with open(os.path.join(SAIDA, "remoto", f"{id_}.zip"), "wb") as f:
                    f.write(z)
                item.update(url=RAW + f"{id_}.zip", previa_b64=base64.b64encode(prev).decode())
                extras.append(item)
    extras += itens_externos()
    for id_, nome, desc, modo, acento, ajustes, cur_id, ico_id in TEMAS:
        prev = png(previa_tema(modo, acento, cur_id, ico_id, estilos))
        arqs[f"previas/{id_}.png"] = prev
        itens.append(dict(id=id_, tipo="temas", nome=nome, autor=AUTOR, licenca=LICENCA, descricao=desc,
                          tags=[modo, "combo"], previa=f"previas/{id_}.png", toml=tema_toml(nome, desc, acento, ajustes),
                          cursor=cur_id, icones=ico_id))
    arqs["index.json"] = json.dumps(dict(versao=1, itens=itens), indent=1, ensure_ascii=False).encode()
    with open(os.path.join(SAIDA, "loja.zip"), "wb") as f:
        f.write(zip_bytes(arqs))
    with open(os.path.join(SAIDA, "remoto.json"), "w", encoding="utf-8") as f:
        json.dump(dict(versao=1, itens=extras), f, indent=1, ensure_ascii=False)
    print(f"{len(itens)} itens embutidos, {len(extras)} online, loja.zip = {os.path.getsize(os.path.join(SAIDA, 'loja.zip'))//1024} KB")


if __name__ == "__main__":
    main()
