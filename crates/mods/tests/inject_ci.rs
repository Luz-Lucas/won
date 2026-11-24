//! Roda só no Windows (VM descartável do CI): injeta a DLL de teste em um processo filho,
//! confere o efeito, descarrega, e confere que o script de recuperação também desfaz.
#![cfg(windows)]

use sentinela_mods::{eject, inject_mod, recovery, registry};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn dll() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or(root.join("target"));
    ["debug", "release"]
        .iter()
        .map(|p| target.join(p).join("sentinela_mod_test.dll"))
        .find(|p| p.is_file())
}

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn marker() -> PathBuf {
    std::env::temp_dir().join("sentinela-mod-test.marker")
}

fn wait_marker(want: &str) -> bool {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if std::fs::read_to_string(marker()).is_ok_and(|t| t == want) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn spawn_target() -> std::process::Child {
    // `ping` fica vivo por ~1 minuto sem tocar em nada do sistema.
    Command::new("ping")
        .args(["-n", "60", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

fn mod_for(dll: PathBuf) -> registry::Mod {
    registry::Mod {
        name: "teste".into(),
        dll,
        targets: vec!["ping.exe".into()],
        enabled: true,
    }
}

#[test]
fn injeta_e_descarrega() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dll) = dll() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "DLL de teste não compilada: rode `cargo build -p sentinela-mod-test`"
        );
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let _ = std::fs::remove_file(marker());
    let mut child = spawn_target();
    std::thread::sleep(Duration::from_millis(500));

    let done = inject_mod(dir.path(), &mod_for(dll)).unwrap();
    assert!(
        done.iter().any(|i| i.pid == child.id()),
        "ping não recebeu o mod"
    );
    assert!(wait_marker("attached"));
    assert!(dir.path().join("recuperacao/desfazer-mods.cmd").is_file());

    assert!(eject(dir.path(), Some("teste")).unwrap() >= 1);
    assert!(wait_marker("detached"));
    assert!(registry::load_state(dir.path()).is_empty());
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn script_de_recuperacao_desfaz_tudo() {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dll) = dll() else {
        assert!(std::env::var_os("CI").is_none());
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let _ = std::fs::remove_file(marker());
    let mut child = spawn_target();
    std::thread::sleep(Duration::from_millis(500));
    inject_mod(dir.path(), &mod_for(dll)).unwrap();
    assert!(wait_marker("attached"));

    // O binário real faz o `eject-all`; o script o chama e arruma o resto.
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_sentinela-mods"));
    let script = recovery::write(&exe, dir.path()).unwrap();
    let text = std::fs::read_to_string(&script)
        .unwrap()
        .replace("pause\r\n", "");
    std::fs::write(&script, text).unwrap();
    std::fs::write(registry::registry_path(dir.path()), "").unwrap();
    let ok = Command::new("cmd")
        .arg("/c")
        .arg(&script)
        .env("SENTINELA_MODS_DIR", dir.path())
        .status()
        .unwrap();
    assert!(ok.success());
    assert!(wait_marker("detached"));
    assert!(!registry::state_path(dir.path()).exists());
    assert!(dir.path().join("mods.toml.desativado").is_file());
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn nao_toca_em_processo_protegido() {
    use sentinela_mods::policy::{Refusal, check_target};
    assert_eq!(
        check_target("explorer.exe", &["explorer.exe".into()]),
        Err(Refusal::Protected)
    );
}

/// Resolve `NtClose` pelo PDB oficial do ntdll e confere com a tabela de exportação do próprio
/// arquivo: os dois caminhos precisam dar o mesmo endereço.
#[test]
fn simbolos_batem_com_exportacao() {
    let ntdll = std::path::Path::new(r"C:\Windows\System32\ntdll.dll");
    let dir = tempfile::tempdir().unwrap();
    let got = match sentinela_mods::symbols::resolve_module(dir.path(), ntdll, &["NtClose"]) {
        Ok(m) => m,
        Err(e) if std::env::var_os("SENTINELA_REQUIRE_SYMBOLS").is_none() => {
            eprintln!("sem rede para símbolos, teste ignorado: {e:#}");
            return;
        }
        Err(e) => panic!("{e:#}"),
    };
    let bytes = std::fs::read(ntdll).unwrap();
    let pe = goblin::pe::PE::parse(&bytes).unwrap();
    let export = pe
        .exports
        .iter()
        .find(|e| e.name == Some("NtClose"))
        .unwrap()
        .rva as u32;
    assert_eq!(got["NtClose"], export);
    // O segundo uso vem do cache e continua batendo.
    let again = sentinela_mods::symbols::resolve_module(dir.path(), ntdll, &["NtClose"]).unwrap();
    assert_eq!(again, got);
    // Falha fechada: um nome inexistente derruba tudo.
    assert!(
        sentinela_mods::symbols::resolve_module(dir.path(), ntdll, &["NtClose", "NaoExiste_xyz"])
            .is_err()
    );
}
