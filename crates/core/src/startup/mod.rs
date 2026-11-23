//! Gerenciador de inicialização: lista os programas que iniciam com o Windows
//! (chaves `Run` e pastas Inicializar) e liga ou desliga cada um gravando só o
//! estado "aprovado/desativado" de `Explorer\StartupApproved`, exatamente como
//! o Gerenciador de Tarefas. Nunca apaga a entrada nem o atalho.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

#[cfg(windows)]
pub mod windows;

/// Origem de uma entrada de inicialização.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupSource {
    /// `HKCU\…\CurrentVersion\Run`.
    RunHkcu,
    /// `HKLM\…\CurrentVersion\Run` (visão de 64 bits).
    RunHklm,
    /// `HKLM\…\WOW6432Node\…\CurrentVersion\Run` (programas de 32 bits).
    RunHklm32,
    /// Pasta Inicializar do usuário.
    StartupFolderUser,
    /// Pasta Inicializar de todos os usuários.
    StartupFolderCommon,
}

impl StartupSource {
    pub const ALL: [StartupSource; 5] = [
        StartupSource::RunHkcu,
        StartupSource::RunHklm,
        StartupSource::RunHklm32,
        StartupSource::StartupFolderUser,
        StartupSource::StartupFolderCommon,
    ];

    /// Prefixo do id da entrada.
    fn key(self) -> &'static str {
        match self {
            StartupSource::RunHkcu => "run_hkcu",
            StartupSource::RunHklm => "run_hklm",
            StartupSource::RunHklm32 => "run_hklm32",
            StartupSource::StartupFolderUser => "folder_user",
            StartupSource::StartupFolderCommon => "folder_common",
        }
    }

    fn from_key(k: &str) -> Option<StartupSource> {
        StartupSource::ALL.into_iter().find(|s| s.key() == k)
    }

    /// Entradas de todos os usuários: mudar exige administrador.
    pub fn needs_admin(self) -> bool {
        !matches!(
            self,
            StartupSource::RunHkcu | StartupSource::StartupFolderUser
        )
    }

    fn is_folder(self) -> bool {
        matches!(
            self,
            StartupSource::StartupFolderUser | StartupSource::StartupFolderCommon
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupEntry {
    /// `<origem>:<nome do valor ou do arquivo>`.
    pub id: String,
    pub name: String,
    pub command: String,
    pub source: StartupSource,
    pub enabled: bool,
    pub file_exists: bool,
    pub needs_admin: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct StartupList {
    pub entries: Vec<StartupEntry>,
    /// Origens que não puderam ser lidas (as demais aparecem normalmente).
    pub warnings: Vec<String>,
}

pub trait StartupBackend {
    fn list(&self) -> Result<StartupList, String>;
    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String>;
}

/// Acesso bruto ao sistema; o Windows real fica em `windows.rs` e os testes
/// usam um falso em memória.
pub trait StartupStore {
    /// `(nome, comando)` dos valores de texto da chave `Run` da origem.
    fn run_values(&self, source: StartupSource) -> Result<Vec<(String, String)>, String>;
    /// Arquivos da pasta Inicializar da origem.
    fn folder_files(&self, source: StartupSource) -> Result<Vec<PathBuf>, String>;
    /// Bytes de `StartupApproved` para a entrada; `None` se não há valor.
    fn approved(&self, source: StartupSource, name: &str) -> Result<Option<Vec<u8>>, String>;
    /// Grava o valor de `StartupApproved` da entrada (REG_BINARY).
    fn set_approved(&self, source: StartupSource, name: &str, data: &[u8]) -> Result<(), String>;
    fn file_exists(&self, path: &Path) -> bool;
    fn env_var(&self, name: &str) -> Option<String>;
}

pub const SENTINELA_NOTE: &str = "Este é o Sentinela. Para desligar, use Ajustes → Inicialização.";

/// A entrada que o próprio Sentinela registra (ligada em Ajustes).
pub fn is_sentinela(e: &StartupEntry) -> bool {
    e.source == StartupSource::RunHkcu
        && (e.name.eq_ignore_ascii_case("Sentinela")
            || e.command.to_ascii_lowercase().contains("sentinela-gui.exe"))
}

/// Estado gravado em `StartupApproved`: ausente ou primeiro byte par = ligado;
/// primeiro byte ímpar (03, 07…) = desligado.
pub fn parse_approved(bytes: Option<&[u8]>) -> bool {
    match bytes.and_then(|b| b.first()) {
        None => true,
        Some(first) => first % 2 == 0,
    }
}

/// FILETIME (intervalos de 100 ns desde 1601-01-01) do instante dado.
pub fn filetime(t: SystemTime) -> u64 {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(d.subsec_nanos()) / 100
}

/// O valor que o Gerenciador de Tarefas grava: `02 00 00 00` + 8 zeros para
/// ligar; `03 00 00 00` + FILETIME de quando foi desligado.
pub fn approved_value(enabled: bool, now: SystemTime) -> [u8; 12] {
    let mut v = [0u8; 12];
    if enabled {
        v[0] = 2;
    } else {
        v[0] = 3;
        v[4..].copy_from_slice(&filetime(now).to_le_bytes());
    }
    v
}

/// Troca `%VAR%` pelo valor; variáveis desconhecidas ficam como estão.
pub fn expand_env(s: &str, var: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match var(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// O executável de uma linha de comando: o trecho entre aspas, ou até o
/// primeiro `.exe` seguido de espaço ou fim (caminhos sem aspas com espaços),
/// ou a primeira palavra.
pub fn command_executable(command: &str, var: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let s = expand_env(command.trim(), var);
    if let Some(rest) = s.strip_prefix('"') {
        return rest
            .split('"')
            .next()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string);
    }
    // to_ascii_lowercase mantém os índices de byte de `s`.
    let lower = s.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(".exe") {
        let end = from + pos + 4;
        if end == s.len() || s[end..].starts_with(char::is_whitespace) {
            return Some(s[..end].to_string());
        }
        from = end;
    }
    s.split_whitespace().next().map(str::to_string)
}

/// Implementação de `StartupBackend` sobre um `StartupStore`.
pub struct Startup<S: StartupStore> {
    pub store: S,
}

impl<S: StartupStore> Startup<S> {
    pub fn new(store: S) -> Self {
        Startup { store }
    }

    /// Nome solto (`rundll32.exe`) é procurado em System32, no Windows e no PATH.
    fn executable_exists(&self, exe: &str) -> bool {
        if exe.contains('\\') || exe.contains('/') || exe.contains(':') {
            return self.store.file_exists(Path::new(exe));
        }
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(root) = self.store.env_var("SystemRoot") {
            dirs.push(Path::new(&root).join("System32"));
            dirs.push(PathBuf::from(root));
        }
        if let Some(path) = self.store.env_var("PATH") {
            dirs.extend(path.split(';').filter(|d| !d.is_empty()).map(PathBuf::from));
        }
        let names: Vec<String> = if exe.to_ascii_lowercase().ends_with(".exe") {
            vec![exe.to_string()]
        } else {
            vec![exe.to_string(), format!("{exe}.exe")]
        };
        dirs.iter()
            .any(|d| names.iter().any(|n| self.store.file_exists(&d.join(n))))
    }

    fn entry(
        &self,
        source: StartupSource,
        name: String,
        approved_name: &str,
        command: String,
        exists: bool,
    ) -> Result<StartupEntry, String> {
        let approved = self.store.approved(source, approved_name)?;
        Ok(StartupEntry {
            id: format!("{}:{approved_name}", source.key()),
            name,
            command,
            source,
            enabled: parse_approved(approved.as_deref()),
            file_exists: exists,
            needs_admin: source.needs_admin(),
        })
    }

    fn list_source(&self, source: StartupSource) -> Result<Vec<StartupEntry>, String> {
        let var = |n: &str| self.store.env_var(n);
        let mut out = Vec::new();
        if source.is_folder() {
            for path in self.store.folder_files(source)? {
                let Some(file) = path.file_name().map(|f| f.to_string_lossy().into_owned()) else {
                    continue;
                };
                if file.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file.clone());
                let exists = self.store.file_exists(&path);
                out.push(self.entry(
                    source,
                    name,
                    &file,
                    path.to_string_lossy().into_owned(),
                    exists,
                )?);
            }
        } else {
            for (name, command) in self.store.run_values(source)? {
                let exists = command_executable(&command, &var)
                    .is_some_and(|exe| self.executable_exists(&exe));
                out.push(self.entry(source, name.clone(), &name, command, exists)?);
            }
        }
        Ok(out)
    }
}

/// Nome da origem em pt-BR, para listas e avisos.
pub fn source_label(s: StartupSource) -> &'static str {
    match s {
        StartupSource::RunHkcu => "Registro (usuário)",
        StartupSource::RunHklm => "Registro (todos os usuários)",
        StartupSource::RunHklm32 => "Registro (todos os usuários, 32 bits)",
        StartupSource::StartupFolderUser => "Pasta Inicializar (usuário)",
        StartupSource::StartupFolderCommon => "Pasta Inicializar (todos os usuários)",
    }
}

impl<S: StartupStore> StartupBackend for Startup<S> {
    fn list(&self) -> Result<StartupList, String> {
        let mut list = StartupList::default();
        for source in StartupSource::ALL {
            match self.list_source(source) {
                Ok(mut entries) => list.entries.append(&mut entries),
                Err(e) => list.warnings.push(format!(
                    "Não foi possível ler {}: {e}",
                    source_label(source)
                )),
            }
        }
        Ok(list)
    }

    fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        let unknown = || format!("entrada de inicialização desconhecida: {id}");
        let (key, name) = id.split_once(':').ok_or_else(unknown)?;
        let source = StartupSource::from_key(key).ok_or_else(unknown)?;
        // Só entradas que existem agora: nunca cria estado para algo que não está lá.
        let entry = self
            .list_source(source)?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(unknown)?;
        if is_sentinela(&entry) {
            return Err(SENTINELA_NOTE.to_string());
        }
        self.store
            .set_approved(source, name, &approved_value(enabled, SystemTime::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct FakeStore {
        runs: HashMap<&'static str, Vec<(String, String)>>,
        folders: HashMap<&'static str, Vec<PathBuf>>,
        approved: RefCell<HashMap<(String, String), Vec<u8>>>,
        files: HashSet<PathBuf>,
        env: HashMap<&'static str, String>,
        failing: Option<StartupSource>,
    }

    impl FakeStore {
        fn run(mut self, s: StartupSource, name: &str, cmd: &str) -> Self {
            self.runs
                .entry(s.key())
                .or_default()
                .push((name.into(), cmd.into()));
            self
        }
        fn folder(mut self, s: StartupSource, path: &str) -> Self {
            self.folders.entry(s.key()).or_default().push(path.into());
            self
        }
        fn file(mut self, p: &str) -> Self {
            self.files.insert(p.into());
            self
        }
        fn approved_bytes(self, s: StartupSource, name: &str, b: &[u8]) -> Self {
            self.approved
                .borrow_mut()
                .insert((s.key().into(), name.into()), b.to_vec());
            self
        }
        fn get(&self, s: StartupSource, name: &str) -> Option<Vec<u8>> {
            self.approved
                .borrow()
                .get(&(s.key().to_string(), name.to_string()))
                .cloned()
        }
    }

    impl StartupStore for FakeStore {
        fn run_values(&self, s: StartupSource) -> Result<Vec<(String, String)>, String> {
            if self.failing == Some(s) {
                return Err("acesso negado".into());
            }
            Ok(self.runs.get(s.key()).cloned().unwrap_or_default())
        }
        fn folder_files(&self, s: StartupSource) -> Result<Vec<PathBuf>, String> {
            Ok(self.folders.get(s.key()).cloned().unwrap_or_default())
        }
        fn approved(&self, s: StartupSource, name: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.get(s, name))
        }
        fn set_approved(&self, s: StartupSource, name: &str, data: &[u8]) -> Result<(), String> {
            self.approved
                .borrow_mut()
                .insert((s.key().into(), name.into()), data.to_vec());
            Ok(())
        }
        fn file_exists(&self, p: &Path) -> bool {
            self.files.contains(p)
        }
        fn env_var(&self, n: &str) -> Option<String> {
            self.env.get(n).cloned()
        }
    }

    fn by_id<'a>(l: &'a StartupList, id: &str) -> &'a StartupEntry {
        l.entries.iter().find(|e| e.id == id).unwrap()
    }

    #[test]
    fn approved_binary_parsing() {
        let tail = [0u8; 11];
        let with = |first: u8| {
            let mut v = vec![first];
            v.extend_from_slice(&tail);
            v
        };
        assert!(parse_approved(Some(&with(0x02))));
        assert!(!parse_approved(Some(&with(0x03))));
        assert!(parse_approved(Some(&with(0x06))));
        assert!(!parse_approved(Some(&with(0x07))));
        assert!(parse_approved(None));
        assert!(parse_approved(Some(&[])));
    }

    #[test]
    fn set_enabled_writes_task_manager_format() {
        let on = approved_value(true, SystemTime::now());
        assert_eq!(on, [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // 2026-01-01T00:00:00Z.
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_767_225_600);
        let off = approved_value(false, t);
        assert_eq!(&off[..4], &[3, 0, 0, 0]);
        assert_eq!(
            u64::from_le_bytes(off[4..].try_into().unwrap()),
            134_116_992_000_000_000
        );

        let s = Startup::new(
            FakeStore::default()
                .run(
                    StartupSource::RunHkcu,
                    "Discord",
                    r#""C:\D\Update.exe" --processStart Discord.exe"#,
                )
                .file(r"C:\D\Update.exe"),
        );
        s.set_enabled("run_hkcu:Discord", false).unwrap();
        let stored = s.store.get(StartupSource::RunHkcu, "Discord").unwrap();
        assert_eq!(stored.len(), 12);
        assert_eq!(stored[0], 3);
        assert!(!s.list().unwrap().entries[0].enabled);
        s.set_enabled("run_hkcu:Discord", true).unwrap();
        assert_eq!(
            s.store.get(StartupSource::RunHkcu, "Discord").unwrap(),
            vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        // A entrada continua lá: só o estado mudou.
        assert_eq!(s.store.run_values(StartupSource::RunHkcu).unwrap().len(), 1);
    }

    #[test]
    fn command_executable_resolution() {
        let env = |n: &str| match n {
            "ProgramFiles" => Some(r"C:\Program Files".to_string()),
            _ => None,
        };
        let cases = [
            (
                r#""C:\Program Files\App\app.exe" --min"#,
                r"C:\Program Files\App\app.exe",
            ),
            (
                r"C:\Program Files\App\app.exe --min",
                r"C:\Program Files\App\app.exe",
            ),
            (
                r"C:\Program Files\App\app.exe",
                r"C:\Program Files\App\app.exe",
            ),
            (
                r"%ProgramFiles%\Steam\steam.exe -silent",
                r"C:\Program Files\Steam\steam.exe",
            ),
            (
                r#""%ProgramFiles%\A B\x.EXE""#,
                r"C:\Program Files\A B\x.EXE",
            ),
            (r"C:\x\app.exe.config.exe /s", r"C:\x\app.exe.config.exe"),
            ("rundll32.exe shell32.dll,Control_RunDLL", "rundll32.exe"),
            (r"%Desconhecida%\a.exe", r"%Desconhecida%\a.exe"),
            ("  ", ""),
        ];
        for (cmd, want) in cases {
            let got = command_executable(cmd, &env).unwrap_or_default();
            assert_eq!(got, want, "{cmd}");
        }
    }

    #[test]
    fn missing_file_flagged_not_removed() {
        let s = Startup::new(
            FakeStore::default()
                .run(StartupSource::RunHkcu, "Velho", r"C:\sumiu\velho.exe /bg")
                .run(StartupSource::RunHkcu, "Ok", r"C:\ok\ok.exe")
                .file(r"C:\ok\ok.exe")
                .folder(StartupSource::StartupFolderUser, r"C:\S\Atalho.lnk")
                .folder(StartupSource::StartupFolderUser, r"C:\S\desktop.ini"),
        );
        let l = s.list().unwrap();
        assert!(!by_id(&l, "run_hkcu:Velho").file_exists);
        assert!(by_id(&l, "run_hkcu:Ok").file_exists);
        let lnk = by_id(&l, "folder_user:Atalho.lnk");
        assert_eq!(lnk.name, "Atalho");
        assert!(!lnk.file_exists);
        assert_eq!(l.entries.len(), 3, "desktop.ini não é entrada");
        // Pode ser desativada, e continua listada.
        s.set_enabled("run_hkcu:Velho", false).unwrap();
        let l = s.list().unwrap();
        assert!(!by_id(&l, "run_hkcu:Velho").enabled);
        assert_eq!(l.entries.len(), 3);
    }

    #[test]
    fn bare_names_are_found_in_system_dirs_and_path() {
        let mut store = FakeStore::default()
            .run(StartupSource::RunHkcu, "Sys", "rundll32 shell32.dll,X")
            .run(StartupSource::RunHkcu, "Path", "tool.exe -q")
            .run(StartupSource::RunHkcu, "Nada", "fantasma.exe")
            .file(r"C:\Windows\System32\rundll32.exe")
            .file(r"C:\bin\tool.exe");
        store.env.insert("SystemRoot", r"C:\Windows".into());
        store.env.insert("PATH", r"C:\outro;C:\bin".into());
        let l = Startup::new(store).list().unwrap();
        assert!(by_id(&l, "run_hkcu:Sys").file_exists);
        assert!(by_id(&l, "run_hkcu:Path").file_exists);
        assert!(!by_id(&l, "run_hkcu:Nada").file_exists);
    }

    #[test]
    fn hklm_entries_need_admin() {
        let s = Startup::new(
            FakeStore::default()
                .run(StartupSource::RunHkcu, "U", "u.exe")
                .run(StartupSource::RunHklm, "M", "m.exe")
                .run(StartupSource::RunHklm32, "M32", "m32.exe")
                .folder(StartupSource::StartupFolderUser, r"C:\u\a.lnk")
                .folder(StartupSource::StartupFolderCommon, r"C:\c\b.lnk")
                .approved_bytes(
                    StartupSource::RunHklm32,
                    "M32",
                    &[3, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8],
                ),
        );
        let l = s.list().unwrap();
        assert!(!by_id(&l, "run_hkcu:U").needs_admin);
        assert!(by_id(&l, "run_hklm:M").needs_admin);
        assert!(by_id(&l, "run_hklm32:M32").needs_admin);
        assert!(!by_id(&l, "run_hklm32:M32").enabled);
        assert!(!by_id(&l, "folder_user:a.lnk").needs_admin);
        assert!(by_id(&l, "folder_common:b.lnk").needs_admin);
    }

    #[test]
    fn sentinela_entry_has_no_toggle() {
        let s = Startup::new(FakeStore::default().run(
            StartupSource::RunHkcu,
            "Sentinela",
            r#""C:\Program Files\Sentinela\sentinela-gui.exe" --minimized"#,
        ));
        let l = s.list().unwrap();
        assert!(is_sentinela(&l.entries[0]));
        assert_eq!(
            s.set_enabled("run_hkcu:Sentinela", false).unwrap_err(),
            SENTINELA_NOTE
        );
        assert!(s.store.get(StartupSource::RunHkcu, "Sentinela").is_none());
    }

    #[test]
    fn unknown_ids_are_refused_and_nothing_is_written() {
        let s = Startup::new(FakeStore::default().run(StartupSource::RunHkcu, "A", "a.exe"));
        for id in ["run_hkcu:B", "outro:A", "semdoispontos", "folder_user:A"] {
            assert_eq!(
                s.set_enabled(id, false).unwrap_err(),
                format!("entrada de inicialização desconhecida: {id}")
            );
        }
        assert!(s.store.approved.borrow().is_empty());
    }

    #[test]
    fn one_unreadable_source_does_not_hide_the_others() {
        let mut store = FakeStore::default()
            .run(StartupSource::RunHkcu, "A", "a.exe")
            .run(StartupSource::RunHklm, "M", "m.exe");
        store.failing = Some(StartupSource::RunHklm);
        let l = Startup::new(store).list().unwrap();
        assert_eq!(l.entries.len(), 1);
        assert_eq!(
            l.warnings,
            vec!["Não foi possível ler Registro (todos os usuários): acesso negado"]
        );
    }

    #[test]
    fn expand_env_keeps_unknown_and_lone_percent() {
        let env = |n: &str| (n == "A").then(|| "x".to_string());
        assert_eq!(expand_env("%A%\\%B%\\50%", &env), "x\\%B%\\50%");
        assert_eq!(expand_env("%%A%", &env), "%x");
    }
}
