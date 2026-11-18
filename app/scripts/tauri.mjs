// Envolve o CLI do Tauri. Em `tauri build`, acrescenta a configuração que empacota o
// sentinela.exe como recurso. Fica fora da configuração base porque o tauri-build copia os
// recursos para target/<perfil>/ e o `sentinela.exe` do recurso sobrescreveria o binário do
// CLI nesses builds (cargo clippy/test), além de exigir o arquivo em um checkout limpo.
import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
if (args[0] === "build") {
  args.push("--config", "src-tauri/tauri.bundle.conf.json");
}
const r = spawnSync("tauri", args, { stdio: "inherit", shell: true });
process.exit(r.status ?? 1);
