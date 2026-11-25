#!/usr/bin/env python3
"""Embute no loja.zip (dentro do executável) os itens de catalog/loja/remoto.json
e os pacotes extras de cursor, para aparecerem na Loja sem internet.
Idempotente: roda sobre o loja.zip atual e ignora ids que já estão lá."""
import base64, hashlib, json, os, sys, zipfile
RAIZ = os.path.join(os.path.dirname(__file__), "..", "..", "catalog", "loja")
EXTRA = sys.argv[1] if len(sys.argv) > 1 else None  # pasta com itens.json, pacotes/, previas/

zin = zipfile.ZipFile(os.path.join(RAIZ, "loja.zip"))
arqs = {n: zin.read(n) for n in zin.namelist()}
idx = json.loads(arqs["index.json"])
ids = {i["id"] for i in idx["itens"]}

def add(item, pacote_bytes, previa_bytes):
    if item["id"] in ids:
        return
    item = {k: v for k, v in item.items() if k not in ("url", "previa_b64")}
    item["pacote"] = f"pacotes/{item['id']}.zip"
    item["sha256"] = hashlib.sha256(pacote_bytes).hexdigest()
    item["bytes"] = len(pacote_bytes)
    arqs[item["pacote"]] = pacote_bytes
    if previa_bytes:
        item["previa"] = f"previas/{item['id']}.png"
        arqs[item["previa"]] = previa_bytes
    idx["itens"].append(item); ids.add(item["id"])

remoto = json.load(open(os.path.join(RAIZ, "remoto.json")))
for i in remoto["itens"]:
    pb = open(os.path.join(RAIZ, "remoto", i["url"].rsplit("/", 1)[1]), "rb").read()
    add(i, pb, base64.b64decode(i["previa_b64"]) if i.get("previa_b64") else None)
if EXTRA:
    for i in json.load(open(os.path.join(EXTRA, "itens.json"))):
        add(i, open(os.path.join(EXTRA, i["pacote"]), "rb").read(),
            open(os.path.join(EXTRA, i["previa"]), "rb").read())
arqs["index.json"] = json.dumps(idx, indent=1, ensure_ascii=False).encode()
with zipfile.ZipFile(os.path.join(RAIZ, "loja.zip"), "w", zipfile.ZIP_DEFLATED) as z:
    for n, b in arqs.items():
        z.writestr(zipfile.ZipInfo(n, (2026, 11, 24, 0, 0, 0)), b, zipfile.ZIP_DEFLATED)
print(len(idx["itens"]), "itens embutidos")
