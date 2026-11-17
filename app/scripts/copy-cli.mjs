// Copia o sentinela.exe (CLI, build release) para app/src-tauri/bin/, de onde o
// Tauri o empacota como recurso ao lado do executável da janela.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const raiz = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const alvo = resolve(process.env.CARGO_TARGET_DIR ?? resolve(raiz, "target"));
const origem = resolve(alvo, "release", "sentinela.exe");
const destino = resolve(raiz, "app", "src-tauri", "bin", "sentinela.exe");

if (!existsSync(origem)) {
  console.error(`sentinela.exe não encontrado em ${origem}`);
  process.exit(1);
}
mkdirSync(dirname(destino), { recursive: true });
copyFileSync(origem, destino);
console.log(`CLI copiado para ${destino}`);
