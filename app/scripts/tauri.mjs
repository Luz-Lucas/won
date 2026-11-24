// Envolve o CLI do Tauri. Em `tauri build`, acrescenta a configuração que empacota o
// sentinela.exe como recurso. Fica fora da configuração base porque o tauri-build copia os
// recursos para target/<perfil>/ e o `sentinela.exe` do recurso sobrescreveria o binário do
// CLI nesses builds (cargo clippy/test), além de exigir o arquivo em um checkout limpo.
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const args = process.argv.slice(2);
if (args[0] === "build") {
  args.push("--config", "src-tauri/tauri.bundle.conf.json");
  // Assinatura de código opcional: só com um certificado PFX (base64) e a senha, vindos de
  // segredos do CI. Sem eles nada muda e o instalador sai sem assinatura.
  const { SIGN_PFX_BASE64: pfx, SIGN_PFX_PASSWORD: password } = process.env;
  if (pfx && password) {
    const dir = mkdtempSync(join(tmpdir(), "sentinela-sign-"));
    const file = join(dir, "cert.pfx");
    writeFileSync(file, Buffer.from(pfx, "base64"));
    const config = join(dir, "sign.conf.json");
    // A senha vai por variável de ambiente (SIGN_PFX_PASSWORD), nunca para o arquivo.
    const signCommand = {
      cmd: "powershell",
      args: [
        "-NoProfile",
        "-Command",
        `& signtool sign /fd sha256 /tr http://timestamp.digicert.com /td sha256 /f '${file}' /p $env:SIGN_PFX_PASSWORD '%1'`,
      ],
    };
    writeFileSync(config, JSON.stringify({ bundle: { windows: { signCommand } } }));
    args.push("--config", config);
    console.log("Assinatura de código ativada.");
  }
}
const r = spawnSync("tauri", args, { stdio: "inherit", shell: true });
process.exit(r.status ?? 1);
