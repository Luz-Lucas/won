//! Script de recuperação: desfaz tudo mesmo que o WON não abra (inclusive no modo seguro).

use std::path::Path;

/// Texto do `desfazer-mods.cmd`. `exe` é o `sentinela-mods.exe`, `dir` a pasta de dados dos mods.
pub fn script(exe: &Path, dir: &Path) -> String {
    let exe = exe.display();
    let dir = dir.display();
    format!(
        "@echo off\r\n\
rem Gerado pelo WON. Desfaz todos os mods carregados e impede que voltem.\r\n\
rem Pode ser executado a qualquer momento, inclusive no modo seguro.\r\n\
echo Desfazendo os mods do WON...\r\n\
if exist \"{exe}\" \"{exe}\" eject-all\r\n\
taskkill /f /im sentinela-mods.exe >nul 2>&1\r\n\
reg delete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\" /v SentinelaMods /f >nul 2>&1\r\n\
if exist \"{dir}\\mods.toml\" move /y \"{dir}\\mods.toml\" \"{dir}\\mods.toml.desativado\" >nul\r\n\
if exist \"{dir}\\injected.json\" del /q \"{dir}\\injected.json\"\r\n\
echo Pronto. Nenhum mod ficou ativo. Reinicie os programas afetados.\r\n\
pause\r\n"
    )
}

/// Grava o script em `<dir>/recuperacao/desfazer-mods.cmd` e devolve o caminho.
pub fn write(exe: &Path, dir: &Path) -> anyhow::Result<std::path::PathBuf> {
    let folder = dir.join("recuperacao");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("desfazer-mods.cmd");
    std::fs::write(&path, script(exe, dir))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn script_cobre_todos_os_passos() {
        let s = script(
            &PathBuf::from("C:/s/sentinela-mods.exe"),
            &PathBuf::from("C:/d"),
        );
        assert!(s.contains("eject-all"));
        assert!(s.contains("taskkill"));
        assert!(s.contains("SentinelaMods"));
        assert!(s.contains("mods.toml.desativado"));
        assert!(s.contains("\r\n"));
    }

    #[test]
    fn grava_na_pasta_recuperacao() {
        let d = tempfile::tempdir().unwrap();
        let p = write(&PathBuf::from("x.exe"), d.path()).unwrap();
        assert!(
            p.ends_with("recuperacao/desfazer-mods.cmd")
                || p.ends_with("recuperacao\\desfazer-mods.cmd")
        );
        assert!(p.exists());
    }
}
