use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use sentinela_mods::{eject, inject_mod, recovery, registry};

#[derive(Parser)]
#[command(name = "sentinela-mods", about = "Motor de mods do Sentinela")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Lista os mods cadastrados e o que está carregado
    List,
    /// Carrega um mod nos processos permitidos
    Inject { name: String },
    /// Descarrega um mod
    Eject { name: String },
    /// Descarrega todos os mods
    EjectAll,
    /// Resolve símbolos públicos de um módulo do Windows (baixa o PDB oficial se preciso)
    Symbols {
        module: std::path::PathBuf,
        names: Vec<String>,
    },
    /// Liga o mod 5 (rótulos da barra) no Explorer, passando pelas travas do shell
    ShellLabels {
        dll: std::path::PathBuf,
        #[arg(long)]
        experimental: bool,
    },
    /// Liga o mod 4 (ícones pequenos da barra) no Explorer
    ShellIcons {
        dll: std::path::PathBuf,
        #[arg(long)]
        experimental: bool,
    },
    /// Fase 14, versão de sessão (não sobrevive a reboot): desliga a verificação de assinatura
    /// de temas só dentro do Explorer já aberto
    ShellThemeSession {
        dll: std::path::PathBuf,
        #[arg(long)]
        experimental: bool,
    },
    /// Mods 6/7: estilo da barra pela árvore XAML (método Windhawk). Desfazer: eject-all
    ShellXaml {
        dll: std::path::PathBuf,
        #[arg(long)]
        experimental: bool,
    },
    /// Grava o script de recuperação e mostra o caminho
    Recovery,
}

fn main() -> Result<()> {
    let dir = registry::data_dir();
    match Cli::parse().cmd {
        Cmd::List => {
            let reg = registry::load_registry(&dir)?;
            for m in &reg.mods {
                println!(
                    "{} ({}) -> {:?}",
                    m.name,
                    if m.enabled { "ativo" } else { "desativado" },
                    m.targets
                );
            }
            for i in registry::load_state(&dir) {
                println!("carregado: {} em {} (pid {})", i.mod_name, i.process, i.pid);
            }
        }
        Cmd::Inject { name } => {
            let reg = registry::load_registry(&dir)?;
            let Some(m) = reg.find(&name) else {
                bail!("mod {name} não cadastrado")
            };
            let done = inject_mod(&dir, &std::env::current_exe()?, m)?;
            println!("{} carregamento(s)", done.len());
        }
        Cmd::Eject { name } => println!("{} descarregado(s)", eject(&dir, Some(&name))?),
        Cmd::EjectAll => println!("{} descarregado(s)", eject(&dir, None)?),
        Cmd::Symbols { module, names } => {
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            for (name, rva) in sentinela_mods::symbols::resolve_module(&dir, &module, &refs)? {
                println!("{name} = 0x{rva:X}");
            }
        }
        Cmd::ShellLabels { dll, experimental } => {
            let build = sysinfo::System::kernel_version()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let done = sentinela_mods::enable_shell_labels(&dir, &std::env::current_exe()?, &dll, build, &[], experimental)?;
            println!("{} carregamento(s) no Explorer (build {build})", done.len());
        }
        Cmd::ShellIcons { dll, experimental } => {
            let build = sysinfo::System::kernel_version()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let done = sentinela_mods::enable_shell_icons(&dir, &std::env::current_exe()?, &dll, build, &[], experimental)?;
            println!("{} carregamento(s) no Explorer (build {build})", done.len());
        }
        Cmd::ShellThemeSession { dll, experimental } => {
            let build = sysinfo::System::kernel_version()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let done = sentinela_mods::enable_shell_theme_session(&dir, &std::env::current_exe()?, &dll, build, &[], experimental)?;
            println!("{} carregamento(s) no Explorer (build {build})", done.len());
        }
        Cmd::ShellXaml { dll, experimental } => {
            let build = sysinfo::System::kernel_version()
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            let done = sentinela_mods::enable_shell_xaml(&dir, &std::env::current_exe()?, &dll, build, &[], experimental)?;
            println!("{} carregamento(s) no Explorer (build {build})", done.len());
        }
        Cmd::Recovery => println!(
            "{}",
            recovery::write(&std::env::current_exe()?, &dir)?.display()
        ),
    }
    Ok(())
}
