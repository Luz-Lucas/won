//! Backend real do Windows: registro, serviços (só inicialização), plano de energia,
//! ponto de restauração, detecção de notebook e SID do usuário.
//!
//! Dividido em submódulos (`registry`, `service`, `power`) para manter cada arquivo curto.

mod power;
mod registry;
mod service;
mod tasks;

pub use tasks::tasks_in_folder;

use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Restore::{
    BEGIN_SYSTEM_CHANGE, END_SYSTEM_CHANGE, MODIFY_SETTINGS, RESTOREPOINTINFOW, SRSetRestorePointW,
    STATEMGRSTATUS,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::backend::{Snapshot, SystemBackend, reg_key};
use super::catalog::keys_to_remove;
use super::model::{Action, RegValue};
use crate::clean::platform::is_elevated;

/// Backend que fala com o Windows de verdade.
pub struct WindowsBackend {
    elevated: bool,
    hkcu_allowed: bool,
}

impl WindowsBackend {
    pub fn new(hkcu_allowed: bool) -> Self {
        Self {
            elevated: is_elevated(),
            hkcu_allowed,
        }
    }
}

/// Valor do item `key` dentro de `k=v;k=v;`.
fn list_entry_get(list: &str, key: &str) -> Option<String> {
    list.split(';')
        .find_map(|item| item.split_once('=').filter(|(k, _)| *k == key))
        .map(|(_, v)| v.to_string())
}

/// Troca, acrescenta (`Some`) ou remove (`None`) só o item `key`; os outros
/// ficam como estavam, na mesma ordem. O resultado termina em `;` (ou é vazio).
fn list_entry_set(list: &str, key: &str, value: Option<&str>) -> String {
    let mut items: Vec<String> = Vec::new();
    let mut done = false;
    for item in list.split(';').filter(|i| !i.is_empty()) {
        let is_target = item.split_once('=').is_some_and(|(k, _)| k == key);
        if !is_target {
            items.push(item.to_string());
        } else if let (Some(v), false) = (value, done) {
            items.push(format!("{key}={v}"));
            done = true;
        }
    }
    if let (Some(v), false) = (value, done) {
        items.push(format!("{key}={v}"));
    }
    items.iter().map(|i| format!("{i};")).collect()
}

fn read_list_string(
    hive: super::model::Hive,
    path: &str,
    name: &str,
) -> Result<Option<String>, String> {
    match registry::read_value(hive, path, name)? {
        None => Ok(None),
        Some(RegValue::String(s)) => Ok(Some(s)),
        Some(_) => Err(registry::UNSUPPORTED.to_string()),
    }
}

impl SystemBackend for WindowsBackend {
    fn read(&self, action: &Action) -> Result<Snapshot, String> {
        match action {
            Action::Registry {
                hive, path, name, ..
            } => Ok(Snapshot::Registry {
                value: registry::read_value(*hive, path, name)?,
            }),
            Action::ListEntry {
                hive,
                path,
                name,
                key,
                ..
            } => Ok(Snapshot::ListEntry {
                value: read_list_string(*hive, path, name)?.and_then(|s| list_entry_get(&s, key)),
            }),
            Action::Service { name, .. } => Ok(match service::read_start(name)? {
                Some(start) => Snapshot::Service { start },
                None => Snapshot::Unavailable {
                    reason: "serviço não existe".to_string(),
                },
            }),
            Action::PowerPlan { scheme } => {
                if !power::scheme_exists(scheme) {
                    return Ok(Snapshot::Unavailable {
                        reason: "plano de energia não existe nesta máquina".to_string(),
                    });
                }
                Ok(Snapshot::PowerPlan {
                    scheme: power::active_scheme()?,
                })
            }
            Action::ScheduledTask { path, .. } => Ok(Snapshot::ScheduledTask {
                enabled: tasks::read_enabled(path)?,
            }),
        }
    }

    fn write(&self, action: &Action, target: &Snapshot) -> Result<(), String> {
        match (action, target) {
            (_, Snapshot::Unavailable { .. }) => {
                Err("estado indisponível não pode ser escrito".to_string())
            }
            (
                Action::Registry {
                    hive, path, name, ..
                },
                Snapshot::Registry { value },
            ) => registry::write_value(*hive, path, name, value.as_ref()),
            (
                Action::ListEntry {
                    hive,
                    path,
                    name,
                    key,
                    ..
                },
                Snapshot::ListEntry { value },
            ) => {
                let current = read_list_string(*hive, path, name)?;
                let updated =
                    list_entry_set(current.as_deref().unwrap_or(""), key, value.as_deref());
                let new_value = (!updated.is_empty()).then_some(RegValue::String(updated));
                registry::write_value(*hive, path, name, new_value.as_ref())
            }
            (Action::Service { name, .. }, Snapshot::Service { start }) => {
                service::write_start(name, *start)
            }
            (Action::PowerPlan { .. }, Snapshot::PowerPlan { scheme }) => power::set_active(scheme),
            (Action::ScheduledTask { path, .. }, Snapshot::ScheduledTask { enabled }) => {
                tasks::set_enabled(path, *enabled)
            }
            _ => Err("estado incompatível com a ação".to_string()),
        }
    }

    fn elevated(&self) -> bool {
        self.elevated
    }

    fn hkcu_allowed(&self) -> bool {
        self.hkcu_allowed
    }

    fn scheme_name(&self, guid: &str) -> Option<String> {
        power::friendly_name(guid)
    }

    fn key_exists(&self, action: &Action) -> Result<bool, String> {
        match reg_key(action) {
            Some((hive, path)) => registry::key_exists(hive, path),
            None => Ok(true),
        }
    }

    fn delete_key_if_empty(
        &self,
        action: &Action,
        created_from: Option<&str>,
    ) -> Result<(), String> {
        let Some((hive, path)) = reg_key(action) else {
            return Ok(());
        };
        // Da folha para cima, só dentro da raiz permitida; para na primeira chave não vazia.
        for p in keys_to_remove(hive, path, created_from) {
            if !registry::delete_key_if_empty(hive, &p)? {
                break;
            }
        }
        Ok(())
    }
}

/// Cria um ponto de restauração (BEGIN + END). Exige administrador.
pub fn create_restore_point(description: &str) -> Result<(), String> {
    if !is_elevated() {
        return Err("criar ponto de restauração exige administrador".to_string());
    }
    let mut info = RESTOREPOINTINFOW {
        dwEventType: BEGIN_SYSTEM_CHANGE,
        dwRestorePtType: MODIFY_SETTINGS,
        llSequenceNumber: 0,
        szDescription: [0; 256],
    };
    // Deixa sempre o último u16 como NUL.
    let mut desc = [0u16; 256];
    for (slot, unit) in desc[..255].iter_mut().zip(description.encode_utf16()) {
        *slot = unit;
    }
    info.szDescription = desc;
    let mut status = STATEMGRSTATUS {
        nStatus: 0,
        llSequenceNumber: 0,
    };
    // SAFETY: `info` e `status` são estruturas válidas, inicializadas e vivem durante a chamada.
    let ok = unsafe { SRSetRestorePointW(&info, &mut status) };
    if ok == 0 {
        return Err(match status.nStatus {
            1058 => "Proteção do Sistema desligada".to_string(),
            code => registry::win_err("não foi possível criar o ponto de restauração", code),
        });
    }
    info.dwEventType = END_SYSTEM_CHANGE;
    info.llSequenceNumber = status.llSequenceNumber;
    // SAFETY: mesma garantia da chamada anterior; o número de sequência veio do BEGIN.
    let ok = unsafe { SRSetRestorePointW(&info, &mut status) };
    if ok == 0 {
        return Err(registry::win_err(
            "não foi possível concluir o ponto de restauração",
            status.nStatus,
        ));
    }
    Ok(())
}

/// O aparelho tem bateria? (`BatteryFlag` 128 = sem bateria, 255 = desconhecido.)
pub fn is_laptop() -> bool {
    let mut st = SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    // SAFETY: `st` é um ponteiro de saída válido.
    if unsafe { GetSystemPowerStatus(&mut st) } == 0 {
        return false;
    }
    st.BatteryFlag != 128 && st.BatteryFlag != 255
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        // SAFETY: handle aberto por OpenProcessToken, fechado só aqui.
        unsafe { CloseHandle(self.0) };
    }
}

/// SID do token do processo, no formato `S-1-5-21-…`.
pub fn current_user_sid() -> Option<String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: pseudo-handle do processo atual; `token` é ponteiro de saída válido.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return None;
    }
    let token = HandleGuard(token);
    let mut needed = 0u32;
    // SAFETY: buffer nulo com tamanho 0 só pergunta o tamanho necessário.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed) };
    if needed == 0 {
        return None;
    }
    // u64 garante o alinhamento de TOKEN_USER.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    // SAFETY: `buf` tem ao menos `needed` bytes.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buf.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return None;
    }
    // SAFETY: a chamada preencheu o início do buffer com um TOKEN_USER; o SID aponta para dentro do mesmo buffer.
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut wstr: *mut u16 = null_mut();
    // SAFETY: `sid` é válido enquanto `buf` viver; `wstr` recebe memória do sistema.
    if unsafe { ConvertSidToStringSidW(sid, &mut wstr) } == 0 || wstr.is_null() {
        return None;
    }
    // SAFETY: a string retornada é NUL-terminada; o comprimento é medido até o NUL.
    let text = unsafe {
        let mut len = 0usize;
        while *wstr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(wstr, len))
    };
    // SAFETY: documentado: a string de ConvertSidToStringSidW é liberada com LocalFree.
    unsafe { LocalFree(wstr.cast()) };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::model::Hive;
    use std::ptr::null;

    #[test]
    fn list_entry_set_replaces_only_that_item() {
        assert_eq!(list_entry_set("A=1;B=2;", "B", Some("3")), "A=1;B=3;");
    }

    #[test]
    fn list_entry_set_appends() {
        assert_eq!(list_entry_set("", "A", Some("1")), "A=1;");
    }

    #[test]
    fn list_entry_remove_keeps_others() {
        assert_eq!(list_entry_set("A=1;B=2;C=3;", "B", None), "A=1;C=3;");
    }

    #[test]
    fn list_entry_remove_last_item_gives_empty() {
        assert_eq!(list_entry_set("A=1;", "A", None), "");
        assert_eq!(list_entry_get("A=1;B=2;", "B").as_deref(), Some("2"));
        assert_eq!(list_entry_get("A=1;", "B"), None);
    }

    #[test]
    fn sid_format_starts_with_s_1() {
        let sid = current_user_sid().expect("sid");
        assert!(sid.starts_with("S-1-"), "{sid}");
    }

    // --- Testes de sistema: rodar à mão com `--ignored`. Só tocam em HKCU\Software\Sentinela-test-{pid}. ---

    static REG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // Serializa os testes de registro: todos usam a mesma chave `-{pid}`.
    struct TestKey(
        String,
        #[allow(dead_code)] std::sync::MutexGuard<'static, ()>,
    );

    impl TestKey {
        fn new() -> Self {
            let guard = REG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            Self(
                format!("Software\\Sentinela-test-{}", std::process::id()),
                guard,
            )
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyW};
            let w = registry::wide(&self.0);
            // SAFETY: `w` é NUL-terminado; só apaga a chave de teste criada por este processo.
            unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, w.as_ptr()) };
        }
    }

    fn reg(path: &str, name: &str, value: Option<RegValue>) -> Action {
        Action::Registry {
            hive: Hive::Hkcu,
            path: path.to_string(),
            name: name.to_string(),
            value,
        }
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn registry_roundtrip_dword_string_and_delete() {
        let k = TestKey::new();
        let be = WindowsBackend::new(true);
        for v in [
            RegValue::Dword(7),
            RegValue::Qword(1 << 40),
            RegValue::String("olá mundo".to_string()),
        ] {
            let a = reg(&k.0, "v", Some(v.clone()));
            be.write(
                &a,
                &Snapshot::Registry {
                    value: Some(v.clone()),
                },
            )
            .unwrap();
            assert_eq!(be.read(&a).unwrap(), Snapshot::Registry { value: Some(v) });
        }
        let a = reg(&k.0, "v", None);
        be.write(&a, &Snapshot::Registry { value: None }).unwrap();
        assert_eq!(be.read(&a).unwrap(), Snapshot::Registry { value: None });
        // apagar de novo (valor e chave já ausentes) é Ok
        be.write(&a, &Snapshot::Registry { value: None }).unwrap();
        let ausente = reg("Software\\Sentinela-test-inexistente-xyz", "v", None);
        be.write(&ausente, &Snapshot::Registry { value: None })
            .unwrap();
        assert_eq!(
            be.read(&ausente).unwrap(),
            Snapshot::Registry { value: None }
        );
        assert!(
            be.write(&a, &Snapshot::Unavailable { reason: "x".into() })
                .is_err()
        );
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn default_value_name_empty_reads_writes_and_deletes() {
        let k = TestKey::new();
        let be = WindowsBackend::new(true);
        let a = reg(&k.0, "", Some(RegValue::String("padrão".into())));
        // ausente no início
        assert_eq!(be.read(&a).unwrap(), Snapshot::Registry { value: None });
        let want = Snapshot::Registry {
            value: Some(RegValue::String("padrão".into())),
        };
        be.write(&a, &want).unwrap();
        assert_eq!(be.read(&a).unwrap(), want);
        // um valor nomeado ao lado não se confunde com o padrão
        let named = reg(&k.0, "x", Some(RegValue::Dword(1)));
        assert_eq!(be.read(&named).unwrap(), Snapshot::Registry { value: None });
        // apagar o padrão
        be.write(&a, &Snapshot::Registry { value: None }).unwrap();
        assert_eq!(be.read(&a).unwrap(), Snapshot::Registry { value: None });
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn key_exists_and_delete_if_empty_respect_contents() {
        let k = TestKey::new();
        let be = WindowsBackend::new(true);
        let sub = format!("{}\\a\\b", k.0);
        let leaf = reg(&sub, "v", Some(RegValue::Dword(1)));
        assert!(!be.key_exists(&leaf).unwrap());
        be.write(
            &leaf,
            &Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        )
        .unwrap();
        assert!(be.key_exists(&leaf).unwrap());
        // com valor: não apaga
        be.delete_key_if_empty(&leaf, None).unwrap();
        assert!(be.key_exists(&leaf).unwrap());
        // sem valor: apaga a folha e os pais vazios, mas nunca a raiz de teste
        be.write(&leaf, &Snapshot::Registry { value: None })
            .unwrap();
        let top = format!("{}\\a", k.0);
        be.delete_key_if_empty(&leaf, Some(&top)).unwrap();
        assert!(!be.key_exists(&leaf).unwrap());
        let a = reg(&format!("{}\\a", k.0), "v", None);
        assert!(!be.key_exists(&a).unwrap());
        let root = reg(&k.0, "v", None);
        assert!(
            be.key_exists(&root).unwrap(),
            "a raiz permitida não é apagada"
        );
        // chave com subchave não é apagada
        let child = reg(&format!("{}\\c\\d", k.0), "v", Some(RegValue::Dword(1)));
        be.write(
            &child,
            &Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        )
        .unwrap();
        let parent = reg(&format!("{}\\c", k.0), "v", None);
        be.delete_key_if_empty(&parent, None).unwrap();
        assert!(be.key_exists(&parent).unwrap());
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn delete_key_stops_at_created_from_and_keeps_preexisting_parent() {
        let k = TestKey::new();
        let be = WindowsBackend::new(true);
        let parent_path = format!("{}\\p", k.0);
        let child_path = format!("{}\\q", parent_path);
        // O pai já existia (vazio): cria e apaga o valor, a chave fica.
        let parent = reg(&parent_path, "v", Some(RegValue::Dword(1)));
        be.write(
            &parent,
            &Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        )
        .unwrap();
        be.write(&parent, &Snapshot::Registry { value: None })
            .unwrap();
        assert!(be.key_exists(&parent).unwrap());
        // O motor cria só o filho.
        let child = reg(&child_path, "v", Some(RegValue::Dword(1)));
        be.write(
            &child,
            &Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        )
        .unwrap();
        be.write(&child, &Snapshot::Registry { value: None })
            .unwrap();
        be.delete_key_if_empty(&child, Some(&child_path)).unwrap();
        assert!(!be.key_exists(&child).unwrap(), "o filho criado sumiu");
        assert!(be.key_exists(&parent).unwrap(), "o pai que já existia fica");
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn wrong_type_reads_as_error_for_binary() {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_BINARY, RegCloseKey, RegCreateKeyExW,
            RegSetValueExW,
        };
        let k = TestKey::new();
        let wpath = registry::wide(&k.0);
        let wname = registry::wide("bin");
        let mut hk = null_mut();
        // SAFETY: ponteiros válidos e NUL-terminados; fecha a chave logo depois.
        unsafe {
            assert_eq!(
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    wpath.as_ptr(),
                    0,
                    null(),
                    0,
                    KEY_SET_VALUE,
                    null(),
                    &mut hk,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                RegSetValueExW(hk, wname.as_ptr(), 0, REG_BINARY, [1u8, 2, 3].as_ptr(), 3),
                0
            );
            RegCloseKey(hk);
        }
        let be = WindowsBackend::new(true);
        let a = reg(&k.0, "bin", Some(RegValue::Dword(1)));
        assert_eq!(be.read(&a).unwrap_err(), "tipo de valor não suportado");
        // nunca escreve por cima de tipo desconhecido
        assert!(
            be.write(
                &a,
                &Snapshot::Registry {
                    value: Some(RegValue::Dword(1))
                }
            )
            .is_err()
        );
        assert!(be.write(&a, &Snapshot::Registry { value: None }).is_err());
    }

    #[test]
    #[ignore = "toca no registro (só HKCU\\Software\\Sentinela-test-pid)"]
    fn list_entry_roundtrip_preserves_other_items() {
        let k = TestKey::new();
        let be = WindowsBackend::new(true);
        let raw = reg(&k.0, "lista", Some(RegValue::String("A=1;B=2;".into())));
        be.write(
            &raw,
            &Snapshot::Registry {
                value: Some(RegValue::String("A=1;B=2;".into())),
            },
        )
        .unwrap();
        let entry = |v: &str| Action::ListEntry {
            hive: Hive::Hkcu,
            path: k.0.clone(),
            name: "lista".to_string(),
            key: "B".to_string(),
            value: v.to_string(),
        };
        let a = entry("3");
        assert_eq!(
            be.read(&a).unwrap(),
            Snapshot::ListEntry {
                value: Some("2".into())
            }
        );
        be.write(
            &a,
            &Snapshot::ListEntry {
                value: Some("3".into()),
            },
        )
        .unwrap();
        assert_eq!(
            be.read(&raw).unwrap(),
            Snapshot::Registry {
                value: Some(RegValue::String("A=1;B=3;".into()))
            }
        );
        be.write(&a, &Snapshot::ListEntry { value: None }).unwrap();
        assert_eq!(
            be.read(&raw).unwrap(),
            Snapshot::Registry {
                value: Some(RegValue::String("A=1;".into()))
            }
        );
        // sobrando só o item removido: o valor some
        let only = Action::ListEntry {
            hive: Hive::Hkcu,
            path: k.0.clone(),
            name: "lista".to_string(),
            value: "1".to_string(),
            key: "A".to_string(),
        };
        be.write(&only, &Snapshot::ListEntry { value: None })
            .unwrap();
        assert_eq!(be.read(&raw).unwrap(), Snapshot::Registry { value: None });
        // valor inexistente + Some(v) cria "key=v;"
        be.write(
            &only,
            &Snapshot::ListEntry {
                value: Some("9".into()),
            },
        )
        .unwrap();
        assert_eq!(
            be.read(&raw).unwrap(),
            Snapshot::Registry {
                value: Some(RegValue::String("A=9;".into()))
            }
        );
    }

    #[test]
    #[ignore = "consulta o plano de energia (somente leitura)"]
    fn power_plan_reads_active_scheme() {
        let be = WindowsBackend::new(true);
        let active = power::active_scheme().unwrap();
        assert_eq!(active.len(), 36, "{active}");
        let a = Action::PowerPlan {
            scheme: active.clone(),
        };
        assert_eq!(be.read(&a).unwrap(), Snapshot::PowerPlan { scheme: active });
        let bogus = Action::PowerPlan {
            scheme: "00000000-0000-0000-0000-000000000001".to_string(),
        };
        assert_eq!(
            be.read(&bogus).unwrap(),
            Snapshot::Unavailable {
                reason: "plano de energia não existe nesta máquina".to_string()
            }
        );
    }
}
