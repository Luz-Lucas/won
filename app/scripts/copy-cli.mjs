// Copia o sentinela.exe (CLI) e o sentinela-desktop.exe (build release) para app/src-tauri/bin/, de onde o
// Tauri o empacota como recurso ao lado do executável da janela.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const alvo = resolve(process.env.CARGO_TARGET_DIR ?? resolve(raiz, "target"));
for (const [nome, pacote] of [
  ["sentinela.exe", "sentinela"],
  ["sentinela-desktop.exe", "sentinela-desktop"],
  ["sentinela-mods.exe", "sentinela-mods"],
]) {
  const origem = resolve(alvo, "release", nome);
  const destino = resolve(raiz, "app", "src-tauri", "bin", nome);
  if (!existsSync(origem)) {
    console.error(`${nome} não encontrado em ${origem}; rode ${"`"}cargo build --release -p ${pacote}${"`"} antes`);
    process.exit(1);
  }
  mkdirSync(dirname(destino), { recursive: true });
  copyFileSync(origem, destino);
  console.log(`${nome} copiado para ${destino}`);
}
