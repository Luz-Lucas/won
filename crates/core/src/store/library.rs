//! Biblioteca local da Loja: baixar, aplicar e reverter.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::atalhos::{
    APP_ROLES, AppliedShortcuts, Backup, RevertedShortcuts, ShortcutsActive, classify,
};
use super::catalog::{Fetch, Item, Kind, MAX_PACK, Source, bundled_pack};
use super::sys::Sys;
use crate::tweaks::model::{Action, Hive, RegValue};
use crate::tweaks::{Snapshot, SystemBackend};

const MAX_FILE: u64 = 2 * 1024 * 1024;
const CURSORS_KEY: &str = "Control Panel\\Cursors";

const CURSOR_ROLES: &[&str] = &[
    "Arrow",
    "Help",
    "AppStarting",
    "Wait",
    "Crosshair",
    "IBeam",
    "NO",
    "SizeNS",
    "SizeWE",
    "SizeNWSE",
    "SizeNESW",
    "SizeAll",
    "UpArrow",
    "Hand",
];
const ICON_ROLES: &[&str] = &[
    "folder",
    "pc",
    "recycle_empty",
    "recycle_full",
    "user",
    "network",
    // unidades e tipos de arquivo (registro do usuário)
    "drive",
    "file_text",
    "file_zip",
    "file_script",
    // categorias de aplicativo (atalhos .lnk; ver `atalhos.rs`)
    "app_browser",
    "app_terminal",
    "app_code",
    "app_chat",
    "app_music",
    "app_video",
    "app_game",
    "app_tools",
    "app_office",
    "app_generic",
];

/// Papel -> programas (ProgIDs) cujo `DefaultIcon` o papel troca.
const FILE_ROLES: &[(&str, &[&str])] = &[
    ("file_text", &["txtfile"]),
    ("file_zip", &["CompressedFolder"]),
    ("file_script", &["batfile", "cmdfile"]),
];

const CLSID_PC: &str = "{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
const CLSID_BIN: &str = "{645FF040-5081-101B-9F08-00AA002F954E}";
const CLSID_USER: &str = "{59031a47-3f72-44a7-89c5-5595fe6b30ee}";
const CLSID_NET: &str = "{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Prev {
    path: String,
    name: String,
    value: Option<RegValue>,
    key_existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Active {
    id: String,
    previous: Vec<Prev>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryState {
    cursores: Option<Active>,
    icones: Option<Active>,
    /// Atalhos (.lnk) com ícone trocado e a cópia dos originais.
    #[serde(default)]
    atalhos: Option<ShortcutsActive>,
}

impl LibraryState {
    pub fn active(&self, kind: Kind) -> Option<&str> {
        self.slot(kind).as_ref().map(|a| a.id.as_str())
    }
    /// Pacote cujos ícones estão nos atalhos agora.
    pub fn active_shortcuts(&self) -> Option<&str> {
        self.atalhos.as_ref().map(|a| a.id.as_str())
    }
    fn slot(&self, kind: Kind) -> &Option<Active> {
        match kind {
            Kind::Icones => &self.icones,
            _ => &self.cursores,
        }
    }
    fn slot_mut(&mut self, kind: Kind) -> &mut Option<Active> {
        match kind {
            Kind::Icones => &mut self.icones,
            _ => &mut self.cursores,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub id: String,
    /// Os ícones só aparecem de vez depois de reiniciar o Explorer.
    pub restart_explorer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reverted {
    pub id: String,
    pub restart_explorer: bool,
    /// Valores que não deu para devolver.
    pub failed: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    tipo: Kind,
    papeis: BTreeMap<String, String>,
}

pub struct Library {
    dir: PathBuf,
}

fn safe_file(name: &str, kind: Kind) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let ext_ok = match kind {
        Kind::Cursores => ext == "cur" || ext == "ani",
        Kind::Icones => ext == "ico",
        Kind::Temas => false,
    };
    ext_ok
        && !stem.is_empty()
        && stem.len() <= 40
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn magic_ok(data: &[u8], file: &str) -> bool {
    if file.ends_with(".cur") {
        data.starts_with(&[0, 0, 2, 0])
    } else if file.ends_with(".ico") {
        data.starts_with(&[0, 0, 1, 0])
    } else {
        data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"ACON"
    }
}

fn reg(path: &str, name: &str) -> Action {
    Action::Registry {
        hive: Hive::Hkcu,
        path: path.into(),
        name: name.into(),
        value: None,
    }
}

impl Library {
    pub fn new(dir: PathBuf) -> Library {
        Library { dir }
    }

    /// `%LOCALAPPDATA%\Sentinela\loja`.
    pub fn system() -> Option<Library> {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(Library::new(base.join("Sentinela").join("loja")))
    }

    fn pack_dir(&self, kind: Kind, id: &str) -> PathBuf {
        self.dir.join(kind.dir()).join(id)
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("estado.json")
    }

    pub fn state(&self) -> LibraryState {
        std::fs::read(self.state_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, s: &LibraryState) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("pasta da Loja: {e}"))?;
        let tmp = self.dir.join("estado.json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("não foi possível salvar o estado: {e}"))?;
        std::fs::rename(&tmp, self.state_path()).map_err(|e| format!("estado: {e}"))
    }

    pub fn installed(&self, kind: Kind, id: &str) -> bool {
        match kind {
            Kind::Temas => true,
            _ => self.pack_dir(kind, id).join("manifest.json").is_file(),
        }
    }

    /// Baixa (ou copia do programa) o pacote para a biblioteca.
    pub fn install(&self, item: &Item, fetch: &dyn Fetch) -> Result<(), String> {
        if item.kind == Kind::Temas {
            return Ok(());
        }
        let bytes = match &item.source {
            Source::Bundled(p) => bundled_pack(p).ok_or("pacote embutido não encontrado")?,
            Source::Remote(url) => fetch.get(url, MAX_PACK)?,
            Source::None => return Err("este item não tem pacote".into()),
        };
        let sum = hex::encode(Sha256::digest(&bytes));
        if !item.sha256.is_empty() && !sum.eq_ignore_ascii_case(&item.sha256) {
            return Err("o pacote não confere com o catálogo (SHA-256); nada foi instalado".into());
        }
        self.extract(item.kind, &item.id, &bytes)
    }

    fn extract(&self, kind: Kind, id: &str, bytes: &[u8]) -> Result<(), String> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("pacote inválido: {e}"))?;
        let manifest: Manifest = {
            let mut f = zip
                .by_name("manifest.json")
                .map_err(|_| "pacote sem manifest.json")?;
            if f.size() > 16 * 1024 {
                return Err("manifest.json grande demais".into());
            }
            let mut b = Vec::new();
            f.by_ref()
                .take(16 * 1024)
                .read_to_end(&mut b)
                .map_err(|e| e.to_string())?;
            serde_json::from_slice(&b).map_err(|e| format!("manifest.json inválido: {e}"))?
        };
        if manifest.tipo != kind {
            return Err("o tipo do pacote não bate com o do item".into());
        }
        let allowed = if kind == Kind::Cursores {
            CURSOR_ROLES
        } else {
            ICON_ROLES
        };
        if manifest.papeis.is_empty() || manifest.papeis.len() > allowed.len() {
            return Err("manifest com papéis demais ou nenhum".into());
        }
        let tmp = self.dir.join(kind.dir()).join(format!("{id}.novo"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| format!("pasta da Loja: {e}"))?;
        let result = (|| {
            for (role, file) in &manifest.papeis {
                if !allowed.contains(&role.as_str()) || !safe_file(file, kind) {
                    return Err(format!("entrada recusada: {role} -> {file}"));
                }
                let mut f = zip
                    .by_name(file)
                    .map_err(|_| format!("faltou {file} no pacote"))?;
                if f.size() > MAX_FILE {
                    return Err(format!("{file} grande demais"));
                }
                let mut data = Vec::new();
                f.by_ref()
                    .take(MAX_FILE)
                    .read_to_end(&mut data)
                    .map_err(|e| e.to_string())?;
                if !magic_ok(&data, file) {
                    return Err(format!("{file} não é um arquivo do formato esperado"));
                }
                std::fs::write(tmp.join(file), data).map_err(|e| format!("gravar {file}: {e}"))?;
            }
            std::fs::write(
                tmp.join("manifest.json"),
                serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        })();
        if let Err(e) = result {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        let dest = self.pack_dir(kind, id);
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&tmp, &dest).map_err(|e| format!("instalar o pacote: {e}"))
    }

    /// Tira o pacote do disco. Recusa se ele estiver aplicado.
    pub fn uninstall(&self, item: &Item) -> Result<(), String> {
        if self.state().active(item.kind) == Some(item.id.as_str()) {
            return Err("Reverta o item antes de removê-lo.".into());
        }
        if item.kind == Kind::Temas {
            return Ok(());
        }
        std::fs::remove_dir_all(self.pack_dir(item.kind, &item.id)).or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(format!("não foi possível remover: {e}"))
            }
        })
    }

    fn roles(&self, kind: Kind, id: &str) -> Result<(PathBuf, BTreeMap<String, String>), String> {
        let dir = self.pack_dir(kind, id);
        let b = std::fs::read(dir.join("manifest.json"))
            .map_err(|_| "Baixe o item antes de aplicar.")?;
        let m: Manifest = serde_json::from_slice(&b).map_err(|e| format!("manifest.json: {e}"))?;
        let allowed = if kind == Kind::Cursores {
            CURSOR_ROLES
        } else {
            ICON_ROLES
        };
        for (r, f) in &m.papeis {
            if !allowed.contains(&r.as_str()) || !safe_file(f, kind) {
                return Err("pacote instalado com entrada inválida; baixe de novo".into());
            }
        }
        Ok((dir, m.papeis))
    }

    /// (chave, valor, dado) que o item grava no HKCU.
    fn writes(
        kind: Kind,
        name: &str,
        dir: &Path,
        roles: &BTreeMap<String, String>,
    ) -> Vec<(String, String, RegValue)> {
        let file = |r: &str| {
            roles
                .get(r)
                .map(|f| dir.join(f).to_string_lossy().into_owned())
        };
        let s = |v: String| RegValue::String(v);
        let mut out = Vec::new();
        if kind == Kind::Cursores {
            out.push((
                CURSORS_KEY.to_string(),
                String::new(),
                s(format!("WON - {name}")),
            ));
            for r in CURSOR_ROLES {
                if let Some(f) = file(r) {
                    out.push((CURSORS_KEY.into(), (*r).into(), s(f)));
                }
            }
            out.push((
                CURSORS_KEY.into(),
                "Scheme Source".into(),
                RegValue::Dword(1),
            ));
            return out;
        }
        let key = |clsid: &str| format!("Software\\Classes\\CLSID\\{clsid}\\DefaultIcon");
        if let Some(f) = file("folder") {
            out.push((
                "Software\\Classes\\Directory\\DefaultIcon".into(),
                String::new(),
                s(f),
            ));
        }
        for (role, clsid) in [
            ("pc", CLSID_PC),
            ("user", CLSID_USER),
            ("network", CLSID_NET),
        ] {
            if let Some(f) = file(role) {
                out.push((key(clsid), String::new(), s(f)));
            }
        }
        if let Some(f) = file("drive") {
            out.push((
                "Software\\Classes\\Drive\\DefaultIcon".into(),
                String::new(),
                s(f),
            ));
        }
        for (role, progs) in FILE_ROLES {
            if let Some(f) = file(role) {
                for prog in *progs {
                    out.push((
                        format!("Software\\Classes\\{prog}\\DefaultIcon"),
                        String::new(),
                        s(f.clone()),
                    ));
                }
            }
        }
        let empty = file("recycle_empty");
        let full = file("recycle_full");
        if let Some(e) = &empty {
            out.push((key(CLSID_BIN), String::new(), s(e.clone())));
            out.push((key(CLSID_BIN), "empty".into(), s(e.clone())));
        }
        if let Some(f) = &full {
            out.push((key(CLSID_BIN), "full".into(), s(f.clone())));
        }
        out
    }

    fn write_value(
        backend: &dyn SystemBackend,
        path: &str,
        name: &str,
        value: Option<RegValue>,
    ) -> Result<(), String> {
        backend.write(&reg(path, name), &Snapshot::Registry { value })
    }

    /// Aponta o Windows para o pacote baixado. Guarda o que havia antes (só na
    /// primeira aplicação do tipo: trocar de pacote não perde o original).
    pub fn apply(
        &self,
        item: &Item,
        backend: &dyn SystemBackend,
        sys: &dyn Sys,
    ) -> Result<Applied, String> {
        if item.kind == Kind::Temas {
            return Err("Temas são aplicados pelo motor de temas.".into());
        }
        if !backend.hkcu_allowed() {
            return Err(
                "Esta janela está em outra conta do Windows; abra o WON na sua conta.".into(),
            );
        }
        let (dir, roles) = self.roles(item.kind, &item.id)?;
        let writes = Self::writes(item.kind, &item.name, &dir, &roles);
        let mut state = self.state();
        // Guarda o valor original de tudo o que este pacote grava e ainda não está guardado
        // (pacotes diferentes trazem papéis diferentes: trocar de um pacote de 6 ícones para
        // um de 19 não pode perder o original dos 13 novos).
        let mut previous = state
            .slot(item.kind)
            .as_ref()
            .map(|a| a.previous.clone())
            .unwrap_or_default();
        for (path, name, _) in &writes {
            if previous.iter().any(|p| &p.path == path && &p.name == name) {
                continue;
            }
            let a = reg(path, name);
            let value = match backend.read(&a)? {
                Snapshot::Registry { value } => value,
                _ => None,
            };
            previous.push(Prev {
                path: path.clone(),
                name: name.clone(),
                value,
                key_existed: backend.key_exists(&a)?,
            });
        }
        for (done, (path, name, value)) in writes.iter().enumerate() {
            if let Err(e) = Self::write_value(backend, path, name, Some(value.clone())) {
                // Desfaz o que já foi escrito para não deixar meio aplicado.
                for (path, name, _) in writes.iter().take(done) {
                    if let Some(p) = previous.iter().find(|p| &p.path == path && &p.name == name) {
                        let _ = Self::write_value(backend, &p.path, &p.name, p.value.clone());
                    }
                }
                return Err(format!("não foi possível aplicar: {e}"));
            }
        }
        // Valores que o pacote anterior tinha e este não tem voltam ao original.
        for p in &previous {
            if !writes
                .iter()
                .any(|(pa, na, _)| pa == &p.path && na == &p.name)
            {
                let _ = Self::write_value(backend, &p.path, &p.name, p.value.clone());
            }
        }
        *state.slot_mut(item.kind) = Some(Active {
            id: item.id.clone(),
            previous,
        });
        self.save(&state)?;
        self.refresh(item.kind, sys);
        Ok(Applied {
            id: item.id.clone(),
            restart_explorer: item.kind == Kind::Icones,
        })
    }

    fn refresh(&self, kind: Kind, sys: &dyn Sys) {
        if kind == Kind::Cursores {
            sys.refresh_cursors();
        } else {
            sys.refresh_icons();
        }
    }

    /// Devolve exatamente o que havia antes da primeira aplicação do tipo.
    pub fn revert(
        &self,
        kind: Kind,
        backend: &dyn SystemBackend,
        sys: &dyn Sys,
    ) -> Result<Reverted, String> {
        let mut state = self.state();
        let Some(active) = state.slot(kind).clone() else {
            return Err("A Loja não aplicou nada deste tipo.".into());
        };
        if !backend.hkcu_allowed() {
            return Err(
                "Esta janela está em outra conta do Windows; abra o WON na sua conta.".into(),
            );
        }
        let mut failed = Vec::new();
        for p in &active.previous {
            if let Err(e) = Self::write_value(backend, &p.path, &p.name, p.value.clone()) {
                failed.push(format!("{}: {e}", p.name));
            }
        }
        for p in active.previous.iter().filter(|p| !p.key_existed) {
            sys.delete_key(&p.path);
        }
        if failed.is_empty() {
            *state.slot_mut(kind) = None;
            self.save(&state)?;
        }
        self.refresh(kind, sys);
        Ok(Reverted {
            id: active.id,
            restart_explorer: kind == Kind::Icones,
            failed,
        })
    }

    fn shortcuts_backup_dir(&self) -> PathBuf {
        self.dir.join("originais").join("atalhos")
    }

    /// Troca o ícone dos atalhos (Área de Trabalho e Menu Iniciar do usuário) pelos glifos de
    /// aplicativo do pacote `id`. Guarda uma cópia de cada `.lnk` antes de mexer.
    pub fn apply_shortcuts(&self, id: &str, sys: &dyn Sys) -> Result<AppliedShortcuts, String> {
        let (dir, roles) = self.roles(Kind::Icones, id)?;
        if !roles.keys().any(|r| APP_ROLES.contains(&r.as_str())) {
            return Err("Este pacote não tem ícones de aplicativos.".into());
        }
        let mut state = self.state();
        let mut items = state.atalhos.take().map(|a| a.items).unwrap_or_default();
        let bdir = self.shortcuts_backup_dir();
        std::fs::create_dir_all(&bdir).map_err(|e| format!("pasta dos originais: {e}"))?;
        let (mut changed, mut skipped, mut failed) = (0usize, 0usize, Vec::new());
        for sc in sys.list_shortcuts() {
            let name = sc
                .path
                .file_stem()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Some(role) = classify(&name, &sc.target) else {
                skipped += 1;
                continue;
            };
            let Some(file) = roles.get(role).or_else(|| roles.get("app_generic")) else {
                skipped += 1;
                continue;
            };
            let key = sc.path.to_string_lossy().into_owned();
            if !items.iter().any(|b| b.path == key) {
                let tag = hex::encode(&Sha256::digest(key.as_bytes())[..8]);
                let copy = bdir.join(format!("{tag}.lnk"));
                if let Err(e) = std::fs::copy(&sc.path, &copy) {
                    failed.push(format!("{name}: sem cópia do original ({e})"));
                    continue;
                }
                items.push(Backup {
                    path: key.clone(),
                    backup: copy.to_string_lossy().into_owned(),
                });
            }
            match sys.set_shortcut_icon(&sc.path, &dir.join(file)) {
                Ok(()) => changed += 1,
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
        state.atalhos = Some(ShortcutsActive {
            id: id.to_string(),
            items,
        });
        self.save(&state)?;
        sys.refresh_icons();
        Ok(AppliedShortcuts {
            id: id.to_string(),
            changed,
            skipped,
            failed,
            restart_explorer: true,
        })
    }

    /// Devolve os `.lnk` originais (cópia byte a byte).
    pub fn revert_shortcuts(&self, sys: &dyn Sys) -> Result<RevertedShortcuts, String> {
        let mut state = self.state();
        let Some(active) = state.atalhos.take() else {
            return Err("A Loja não mexeu em nenhum atalho.".into());
        };
        let (mut restored, mut failed, mut left) = (0usize, Vec::new(), Vec::new());
        for b in active.items {
            let target = PathBuf::from(&b.path);
            if !target.parent().is_some_and(|p| p.is_dir()) {
                let _ = std::fs::remove_file(&b.backup); // a pasta sumiu: nada a devolver
                continue;
            }
            match std::fs::copy(&b.backup, &target) {
                Ok(_) => {
                    restored += 1;
                    let _ = std::fs::remove_file(&b.backup);
                }
                Err(e) => {
                    failed.push(format!("{}: {e}", b.path));
                    left.push(b);
                }
            }
        }
        if !left.is_empty() {
            state.atalhos = Some(ShortcutsActive {
                id: active.id.clone(),
                items: left,
            });
        }
        self.save(&state)?;
        sys.refresh_icons();
        Ok(RevertedShortcuts {
            id: active.id,
            restored,
            failed,
            restart_explorer: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::catalog::{Catalog, HttpFetch};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        vals: Mutex<HashMap<(String, String), RegValue>>,
        keys: Mutex<Vec<String>>,
    }
    impl SystemBackend for Fake {
        fn read(&self, a: &Action) -> Result<Snapshot, String> {
            let Action::Registry { path, name, .. } = a else {
                unreachable!()
            };
            Ok(Snapshot::Registry {
                value: self
                    .vals
                    .lock()
                    .unwrap()
                    .get(&(path.clone(), name.clone()))
                    .cloned(),
            })
        }
        fn write(&self, a: &Action, t: &Snapshot) -> Result<(), String> {
            let (Action::Registry { path, name, .. }, Snapshot::Registry { value }) = (a, t) else {
                unreachable!()
            };
            let k = (path.clone(), name.clone());
            let mut v = self.vals.lock().unwrap();
            match value {
                Some(x) => {
                    v.insert(k, x.clone());
                    self.keys.lock().unwrap().push(path.clone());
                }
                None => {
                    v.remove(&k);
                }
            }
            Ok(())
        }
        fn elevated(&self) -> bool {
            false
        }
        fn hkcu_allowed(&self) -> bool {
            true
        }
        fn key_exists(&self, a: &Action) -> Result<bool, String> {
            let Action::Registry { path, .. } = a else {
                unreachable!()
            };
            Ok(self.keys.lock().unwrap().contains(path))
        }
    }

    #[derive(Default)]
    struct FakeSys(Mutex<Vec<String>>);
    impl Sys for FakeSys {
        fn list_shortcuts(&self) -> Vec<crate::store::atalhos::Shortcut> {
            Vec::new()
        }
        fn set_shortcut_icon(&self, _: &Path, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn refresh_cursors(&self) {
            self.0.lock().unwrap().push("cursores".into());
        }
        fn refresh_icons(&self) {
            self.0.lock().unwrap().push("icones".into());
        }
        fn delete_key(&self, p: &str) {
            self.0.lock().unwrap().push(format!("del {p}"));
        }
    }

    fn lib() -> (tempfile::TempDir, Library) {
        let d = tempfile::tempdir().unwrap();
        let l = Library::new(d.path().to_path_buf());
        (d, l)
    }

    #[test]
    fn every_bundled_pack_installs_and_checks_out() {
        let (_d, l) = lib();
        let c = Catalog::bundled().unwrap();
        for i in c.items.iter().filter(|i| i.kind != Kind::Temas) {
            assert!(!l.installed(i.kind, &i.id));
            l.install(i, &HttpFetch)
                .unwrap_or_else(|e| panic!("{}: {e}", i.id));
            assert!(l.installed(i.kind, &i.id));
        }
    }

    #[test]
    fn cursor_apply_then_revert_restores_the_original() {
        let (_d, l) = lib();
        let c = Catalog::bundled().unwrap();
        let item = c.get("cursor-noite").unwrap();
        let b = Fake::default();
        let sys = FakeSys::default();
        b.vals.lock().unwrap().insert(
            (CURSORS_KEY.into(), "Arrow".into()),
            RegValue::String("C:\\antigo.cur".into()),
        );
        b.keys.lock().unwrap().push(CURSORS_KEY.into());
        assert!(l.apply(item, &b, &sys).is_err(), "sem baixar não aplica");
        l.install(item, &HttpFetch).unwrap();
        l.apply(item, &b, &sys).unwrap();
        let arrow = b.vals.lock().unwrap()[&(CURSORS_KEY.to_string(), "Arrow".to_string())].clone();
        assert!(matches!(&arrow, RegValue::String(s) if s.ends_with("Arrow.cur")));
        assert_eq!(l.state().active(Kind::Cursores), Some("cursor-noite"));
        assert!(l.uninstall(item).is_err(), "aplicado não se remove");

        // Trocar de pacote não perde o original.
        let outro = c.get("cursor-claro").unwrap();
        l.install(outro, &HttpFetch).unwrap();
        l.apply(outro, &b, &sys).unwrap();

        let r = l.revert(Kind::Cursores, &b, &sys).unwrap();
        assert!(r.failed.is_empty());
        let v = b.vals.lock().unwrap();
        assert_eq!(
            v[&(CURSORS_KEY.to_string(), "Arrow".to_string())],
            RegValue::String("C:\\antigo.cur".into())
        );
        assert!(!v.contains_key(&(CURSORS_KEY.to_string(), "Hand".to_string())));
        drop(v);
        assert_eq!(l.state().active(Kind::Cursores), None);
        assert!(l.revert(Kind::Cursores, &b, &FakeSys::default()).is_err());
        l.uninstall(item).unwrap();
    }

    #[test]
    fn icon_apply_sets_shell_keys_and_revert_deletes_created_ones() {
        let (_d, l) = lib();
        let c = Catalog::bundled().unwrap();
        let item = c.get("icones-flat").unwrap();
        let b = Fake::default();
        let sys = FakeSys::default();
        l.install(item, &HttpFetch).unwrap();
        let r = l.apply(item, &b, &sys).unwrap();
        assert!(r.restart_explorer);
        let v = b.vals.lock().unwrap();
        let bin = format!("Software\\Classes\\CLSID\\{CLSID_BIN}\\DefaultIcon");
        assert!(v.contains_key(&(bin.clone(), "full".to_string())));
        assert!(v.contains_key(&(
            "Software\\Classes\\Directory\\DefaultIcon".to_string(),
            String::new()
        )));
        drop(v);
        // As chaves não existiam antes: a reversão as apaga.
        b.keys.lock().unwrap().clear();
        l.revert(Kind::Icones, &b, &sys).unwrap();
        assert!(b.vals.lock().unwrap().is_empty());
        assert!(
            sys.0
                .lock()
                .unwrap()
                .iter()
                .any(|s| s == &format!("del {bin}"))
        );
    }

    #[test]
    fn hostile_pack_is_refused() {
        let (_d, l) = lib();
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default();
            use std::io::Write;
            z.start_file("manifest.json", o).unwrap();
            z.write_all(br#"{"tipo":"cursores","papeis":{"Arrow":"..\\evil.exe"}}"#)
                .unwrap();
            z.finish().unwrap();
        }
        assert!(l.extract(Kind::Cursores, "x", &buf).is_err());
        assert!(!l.installed(Kind::Cursores, "x"));
    }

    #[test]
    fn tampered_pack_fails_the_checksum() {
        struct Bad;
        impl Fetch for Bad {
            fn get(&self, _: &str, _: u64) -> Result<Vec<u8>, String> {
                Ok(b"nada a ver".to_vec())
            }
        }
        let (_d, l) = lib();
        let mut item = Catalog::bundled()
            .unwrap()
            .get("cursor-noite")
            .unwrap()
            .clone();
        item.source = Source::Remote("https://x".into());
        assert!(l.install(&item, &Bad).unwrap_err().contains("SHA-256"));
    }

    /// Pacote com todos os 20 papéis (ícones de 6 bytes só com o cabeçalho certo).
    fn full_pack(l: &Library, id: &str) {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut z = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let o = zip::write::SimpleFileOptions::default();
            let mut papeis = BTreeMap::new();
            for r in ICON_ROLES {
                z.start_file(format!("{r}.ico"), o).unwrap();
                z.write_all(&[0, 0, 1, 0, 0, 0]).unwrap();
                papeis.insert((*r).to_string(), format!("{r}.ico"));
            }
            z.start_file("manifest.json", o).unwrap();
            z.write_all(
                serde_json::json!({"tipo": "icones", "papeis": papeis})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
            z.finish().unwrap();
        }
        l.extract(Kind::Icones, id, &buf).unwrap();
    }

    #[test]
    fn full_pack_sets_drive_and_file_type_keys_and_revert_restores_everything() {
        let (_d, l) = lib();
        let b = Fake::default();
        let sys = FakeSys::default();
        let item = Catalog::bundled()
            .unwrap()
            .get("icones-flat")
            .unwrap()
            .clone();
        let txt = "Software\\Classes\\txtfile\\DefaultIcon".to_string();
        let drive = "Software\\Classes\\Drive\\DefaultIcon".to_string();
        // O usuário já tinha um ícone de .txt próprio.
        b.vals.lock().unwrap().insert(
            (txt.clone(), String::new()),
            RegValue::String("C:\\meu.ico".into()),
        );
        b.keys.lock().unwrap().push(txt.clone());

        // 1º um pacote antigo (6 papéis), depois um completo: o original dos papéis novos
        // tem que ser guardado mesmo assim.
        l.install(&item, &HttpFetch).unwrap();
        l.apply(&item, &b, &sys).unwrap();
        full_pack(&l, "completo");
        let full = Item {
            id: "completo".into(),
            name: "Completo".into(),
            ..item.clone()
        };
        l.apply(&full, &b, &sys).unwrap();
        {
            let v = b.vals.lock().unwrap();
            assert!(
                matches!(&v[&(txt.clone(), String::new())], RegValue::String(x) if x.ends_with("file_text.ico"))
            );
            assert!(v.contains_key(&(drive.clone(), String::new())));
            assert!(v.contains_key(&(
                "Software\\Classes\\batfile\\DefaultIcon".to_string(),
                String::new()
            )));
        }
        b.keys.lock().unwrap().retain(|k| k == &txt);
        l.revert(Kind::Icones, &b, &sys).unwrap();
        let v = b.vals.lock().unwrap();
        assert_eq!(
            v[&(txt, String::new())],
            RegValue::String("C:\\meu.ico".into())
        );
        assert!(!v.contains_key(&(drive, String::new())));
    }

    #[derive(Default)]
    struct LnkSys {
        lnks: Vec<crate::store::atalhos::Shortcut>,
        set: Mutex<Vec<(PathBuf, PathBuf)>>,
    }
    impl Sys for LnkSys {
        fn refresh_cursors(&self) {}
        fn refresh_icons(&self) {}
        fn delete_key(&self, _: &str) {}
        fn list_shortcuts(&self) -> Vec<crate::store::atalhos::Shortcut> {
            self.lnks.clone()
        }
        fn set_shortcut_icon(&self, lnk: &Path, icon: &Path) -> Result<(), String> {
            // imita o Windows: reescreve o arquivo do atalho
            std::fs::write(lnk, b"MODIFICADO").map_err(|e| e.to_string())?;
            self.set.lock().unwrap().push((lnk.into(), icon.into()));
            Ok(())
        }
    }

    #[test]
    fn shortcuts_get_category_icons_and_revert_returns_the_original_bytes() {
        use crate::store::atalhos::Shortcut;
        let (d, l) = lib();
        full_pack(&l, "completo");
        let desk = d.path().join("desk");
        std::fs::create_dir_all(&desk).unwrap();
        let mk = |n: &str, t: &str| {
            let p = desk.join(format!("{n}.lnk"));
            std::fs::write(&p, format!("ORIGINAL-{n}")).unwrap();
            Shortcut {
                path: p,
                target: t.into(),
            }
        };
        let sys = LnkSys {
            lnks: vec![
                mk("Google Chrome", r"C:\x\chrome.exe"),
                mk("Discord", r"C:\x\Update.exe"),
                mk("Downloads", r"C:\Users\a\Downloads"),
            ],
            ..Default::default()
        };
        let r = l.apply_shortcuts("completo", &sys).unwrap();
        assert_eq!((r.changed, r.skipped), (2, 1));
        assert_eq!(l.state().active_shortcuts(), Some("completo"));
        let set = sys.set.lock().unwrap().clone();
        assert!(set[0].1.ends_with("app_browser.ico"));
        assert!(set[1].1.ends_with("app_chat.ico"));
        assert_eq!(
            std::fs::read(desk.join("Downloads.lnk")).unwrap(),
            b"ORIGINAL-Downloads"
        );

        // Aplicar de novo (outro pacote) não troca o backup do original pelo arquivo já alterado.
        l.apply_shortcuts("completo", &sys).unwrap();
        let rv = l.revert_shortcuts(&sys).unwrap();
        assert_eq!((rv.restored, rv.failed.len()), (2, 0));
        assert_eq!(
            std::fs::read(desk.join("Google Chrome.lnk")).unwrap(),
            b"ORIGINAL-Google Chrome"
        );
        assert_eq!(
            std::fs::read(desk.join("Discord.lnk")).unwrap(),
            b"ORIGINAL-Discord"
        );
        assert_eq!(l.state().active_shortcuts(), None);
        assert!(l.revert_shortcuts(&sys).is_err());
    }

    #[test]
    fn shortcuts_need_a_pack_with_app_icons() {
        let (_d, l) = lib();
        let c = Catalog::bundled().unwrap();
        let item = c.get("icones-flat").unwrap();
        l.install(item, &HttpFetch).unwrap();
        let err = l
            .apply_shortcuts("icones-flat", &LnkSys::default())
            .unwrap_err();
        assert!(err.contains("aplicativos"));
    }

    /// Lê os arquivos do repositório (`catalog/loja/`) no lugar da internet.
    struct RepoFetch;
    impl Fetch for RepoFetch {
        fn get(&self, url: &str, _: u64) -> Result<Vec<u8>, String> {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog/loja");
            let rel = if url == crate::store::catalog::REMOTE_URL {
                "remoto.json".to_string()
            } else {
                format!("remoto/{}", url.rsplit('/').next().unwrap())
            };
            std::fs::read(root.join(rel)).map_err(|e| e.to_string())
        }
    }

    #[test]
    fn every_online_pack_in_the_repo_checks_out_and_installs() {
        let (_d, l) = lib();
        let c = Catalog::bundled()
            .unwrap()
            .with_remote(&RepoFetch, crate::store::catalog::REMOTE_URL)
            .unwrap();
        let remotos: Vec<_> = c.items.iter().filter(|i| i.remote).collect();
        assert!(
            remotos.len() >= 21,
            "catálogo online sem os itens esperados"
        );
        for i in remotos {
            l.install(i, &RepoFetch)
                .unwrap_or_else(|e| panic!("{}: {e}", i.id));
            assert!(
                !i.author.is_empty() && !i.license.is_empty(),
                "{} sem autor/licença",
                i.id
            );
            // pacotes com a tag "atalhos" têm de trazer ícone para todas as categorias de app
            if i.tags.iter().any(|t| t == "atalhos") {
                let (_, roles) = l.roles(i.kind, &i.id).unwrap();
                for r in APP_ROLES {
                    assert!(roles.contains_key(*r), "{} sem {r}", i.id);
                }
            }
        }
    }
}
