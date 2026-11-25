"""Gera os conceitos de logo do WON (arte própria, MIT). Uso: python gerar.py <saida>"""
import math, sys, os
from PIL import Image, ImageDraw, ImageFilter, ImageFont

S = 1024
BG0, BG1 = (4, 6, 10), (11, 18, 34)
BLUE, BLUE_L, BLUE_D = (47, 123, 255), (110, 190, 255), (20, 70, 200)

def lerp(a, b, t): return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

def grad(size, c0, c1, vertical=True):
    im = Image.new("RGB", (size, size))
    px = im.load()
    for y in range(size):
        for x in range(size):
            t = (y if vertical else (x + y) / 2) / (size - 1)
            px[x, y] = lerp(c0, c1, t)
    return im

def base():
    """Quadrado arredondado preto com leve degradê azul-noite."""
    bg = grad(S, BG1, BG0)
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, S - 1, S - 1), radius=int(S * .225), fill=255)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(bg, (0, 0), m)
    return out, m

def blue_fill(mask):
    g = grad(S, BLUE_L, BLUE_D, vertical=False)
    out = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    out.paste(g, (0, 0), mask)
    return out

def glow(mask, r=40, a=.55):
    g = mask.filter(ImageFilter.GaussianBlur(r)).point(lambda v: int(v * a))
    out = Image.new("RGBA", (S, S), BLUE + (0,))
    out.putalpha(g)
    return out

def compose(layers_fn):
    img, m = base()
    mask = layers_fn()
    img.alpha_composite(glow(mask))
    img.alpha_composite(blue_fill(mask))
    # borda fina azul
    edge = Image.new("L", (S, S), 0)
    ImageDraw.Draw(edge).rounded_rectangle((6, 6, S - 7, S - 7), radius=int(S * .22), outline=255, width=6)
    e = Image.new("RGBA", (S, S), BLUE + (0,)); e.putalpha(edge.point(lambda v: int(v * .35)))
    img.alpha_composite(e)
    return img

def thick_poly(d, pts, w):
    d.line(pts, fill=255, width=w, joint="curve")
    for p in (pts[0], pts[-1]):
        d.ellipse((p[0] - w / 2, p[1] - w / 2, p[0] + w / 2, p[1] + w / 2), fill=255)

# A: W de duas "setas" de otimização (chevrons) = velocidade
def conceito_a():
    m = Image.new("L", (S, S), 0); d = ImageDraw.Draw(m)
    w = 118
    pts = [(210, 330), (360, 700), (512, 430), (664, 700), (814, 330)]
    thick_poly(d, pts, w)
    # barra de "pico" acima do vértice central
    d.rounded_rectangle((452, 250, 572, 300), radius=25, fill=255)
    return m

# B: anel de "ninho" aberto (órbita) envolvendo um W compacto
def conceito_b():
    m = Image.new("L", (S, S), 0); d = ImageDraw.Draw(m)
    c = (512, 512); r = 340
    d.arc((c[0] - r, c[1] - r, c[0] + r, c[1] + r), 140, 40 + 360 - 0, fill=255, width=70)
    for ang in (140, 40):
        x = c[0] + r * math.cos(math.radians(ang)); y = c[1] + r * math.sin(math.radians(ang))
        d.ellipse((x - 35, y - 35, x + 35, y + 35), fill=255)
    thick_poly(d, [(360, 420), (436, 610), (512, 480), (588, 610), (664, 420)], 62)
    return m

# C: hexágono (nó de rede) com W vazado
def conceito_c():
    m = Image.new("L", (S, S), 0); d = ImageDraw.Draw(m)
    c = (512, 512); R = 340
    hexp = [(c[0] + R * math.cos(math.radians(60 * i - 90)), c[1] + R * math.sin(math.radians(60 * i - 90))) for i in range(6)]
    d.polygon(hexp, fill=255)
    cut = Image.new("L", (S, S), 0); dc = ImageDraw.Draw(cut)
    thick_poly(dc, [(340, 400), (426, 640), (512, 470), (598, 640), (684, 400)], 66)
    m.paste(0, (0, 0), cut)
    return m

CONCEITOS = {"A-pico": conceito_a, "B-ninho": conceito_b, "C-no": conceito_c}

def ico(img, path):
    img.resize((256, 256), Image.LANCZOS).save(path, sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

if __name__ == "__main__":
    out = sys.argv[1]; os.makedirs(out, exist_ok=True)
    imgs = {}
    for nome, fn in CONCEITOS.items():
        im = compose(fn); imgs[nome] = im
        im.save(f"{out}/won-{nome}.png")
    sheet = Image.new("RGB", (3 * 520 + 40, 640), (8, 10, 14))
    f = ImageFont.load_default()
    for i, (nome, im) in enumerate(imgs.items()):
        sheet.paste(im.resize((480, 480), Image.LANCZOS), (20 + i * 520, 40), im.resize((480, 480), Image.LANCZOS))
        ImageDraw.Draw(sheet).text((20 + i * 520, 540), nome, fill=(140, 190, 255), font=f)
        small = im.resize((48, 48), Image.LANCZOS); sheet.paste(small, (20 + i * 520, 570), small)
        small = im.resize((24, 24), Image.LANCZOS); sheet.paste(small, (80 + i * 520, 582), small)
    sheet.save(f"{out}/won-conceitos.png")
