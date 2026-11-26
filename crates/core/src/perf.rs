//! Monitor de desempenho: quanto CPU, RAM e GPU cada parte do WON (e os processos do Windows que
//! recebem a estilização) está usando agora, com média e pico desde o início da medição.

use serde::Serialize;
use std::collections::HashMap;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// (id, nome na tela, o que é).
pub const GROUPS: &[(&str, &str, &str)] = &[
    (
        "gui",
        "WON (esta janela)",
        "Interface do WON e o WebView2 dela.",
    ),
    (
        "desktop",
        "Barra, widgets e tiling",
        "sentinela-desktop: a barra do WON, os widgets, o tiling e os atalhos.",
    ),
    (
        "mods",
        "Injeção dos mods",
        "sentinela-mods: coloca e tira os mods nos processos do Windows.",
    ),
    (
        "explorer",
        "Explorer (barra nativa estilizada)",
        "Inclui o custo do próprio Windows; o do estilo é a diferença para a referência sem estilo.",
    ),
    (
        "start",
        "Menu Iniciar (com estilo)",
        "StartMenuExperienceHost: o estilo do Iniciar roda dentro dele.",
    ),
    (
        "search",
        "Pesquisa (com estilo)",
        "SearchHost: o estilo e o modo escuro da pesquisa rodam dentro dele.",
    ),
];

fn group_of(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    Some(match n.as_str() {
        "sentinela-gui.exe" | "sentinela.exe" => "gui",
        "sentinela-desktop.exe" => "desktop",
        "sentinela-mods.exe" => "mods",
        "explorer.exe" => "explorer",
        "startmenuexperiencehost.exe" => "start",
        "searchhost.exe" => "search",
        _ => return None,
    })
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct GroupStat {
    pub id: String,
    pub label: String,
    pub note: String,
    pub procs: usize,
    pub cpu: f64,
    pub cpu_avg: f64,
    pub cpu_peak: f64,
    pub ram_mb: f64,
    pub ram_avg_mb: f64,
    pub ram_peak_mb: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub groups: Vec<GroupStat>,
    /// CPU total da máquina, 0–100.
    pub total_cpu: f64,
    pub ram_used_mb: f64,
    pub ram_total_mb: f64,
    pub samples: u32,
}

#[derive(Default, Clone)]
struct Acc {
    cpu_sum: f64,
    ram_sum: f64,
    cpu_peak: f64,
    ram_peak: f64,
}

pub struct Sampler {
    sys: System,
    acc: HashMap<&'static str, Acc>,
    samples: u32,
    /// pid → grupo da última amostra (para atribuir a GPU).
    pub pid_group: HashMap<u32, &'static str>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        Sampler {
            sys: System::new(),
            acc: HashMap::new(),
            samples: 0,
            pid_group: HashMap::new(),
        }
    }

    /// Zera média e pico (começa uma medição nova).
    pub fn reset(&mut self) {
        self.acc.clear();
        self.samples = 0;
    }

    pub fn sample(&mut self) -> Snapshot {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let cores = self.sys.cpus().len().max(1) as f64;
        // Primeiro os processos conhecidos; depois os filhos (WebView2) pelo pai.
        let mut owner: HashMap<Pid, &'static str> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            if let Some(g) = group_of(&p.name().to_string_lossy()) {
                owner.insert(*pid, g);
            }
        }
        for _ in 0..3 {
            let mut add = Vec::new();
            for (pid, p) in self.sys.processes() {
                if owner.contains_key(pid) {
                    continue;
                }
                if let Some(g) = p.parent().and_then(|pp| owner.get(&pp)).copied()
                    && g == "gui"
                    && p.name()
                        .to_string_lossy()
                        .to_ascii_lowercase()
                        .contains("webview2")
                {
                    add.push((*pid, g));
                }
            }
            if add.is_empty() {
                break;
            }
            owner.extend(add);
        }
        let mut cpu: HashMap<&'static str, (f64, f64, usize)> = HashMap::new();
        self.pid_group.clear();
        for (pid, g) in &owner {
            if let Some(p) = self.sys.process(*pid) {
                let e = cpu.entry(g).or_default();
                e.0 += f64::from(p.cpu_usage()) / cores;
                e.1 += p.memory() as f64 / 1_048_576.0;
                e.2 += 1;
                self.pid_group.insert(pid.as_u32(), g);
            }
        }
        self.samples += 1;
        let n = f64::from(self.samples);
        let groups = GROUPS
            .iter()
            .map(|(id, label, note)| {
                let (c, r, k) = cpu.get(id).copied().unwrap_or_default();
                let a = self.acc.entry(id).or_default();
                a.cpu_sum += c;
                a.ram_sum += r;
                a.cpu_peak = a.cpu_peak.max(c);
                a.ram_peak = a.ram_peak.max(r);
                GroupStat {
                    id: (*id).into(),
                    label: (*label).into(),
                    note: (*note).into(),
                    procs: k,
                    cpu: round1(c),
                    cpu_avg: round1(a.cpu_sum / n),
                    cpu_peak: round1(a.cpu_peak),
                    ram_mb: r.round(),
                    ram_avg_mb: (a.ram_sum / n).round(),
                    ram_peak_mb: a.ram_peak.round(),
                }
            })
            .collect();
        Snapshot {
            groups,
            total_cpu: round1(f64::from(self.sys.global_cpu_usage())),
            ram_used_mb: (self.sys.used_memory() as f64 / 1_048_576.0).round(),
            ram_total_mb: (self.sys.total_memory() as f64 / 1_048_576.0).round(),
            samples: self.samples,
        }
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Lê `typeperf "\GPU Engine(*)\Utilization Percentage" -sc 1` (CSV: cabeçalho + valores) e soma
/// por pid. As instâncias se chamam `pid_1234_luid_..._engtype_3D`.
pub fn parse_gpu_csv(csv: &str) -> HashMap<u32, f64> {
    let mut lines = csv.lines().filter(|l| l.starts_with('"'));
    let (Some(head), Some(vals)) = (lines.next(), lines.next()) else {
        return HashMap::new();
    };
    let split = |l: &str| -> Vec<String> {
        l.trim()
            .trim_matches('"')
            .split("\",\"")
            .map(str::to_string)
            .collect()
    };
    let (h, v) = (split(head), split(vals));
    let mut out: HashMap<u32, f64> = HashMap::new();
    for (name, val) in h.iter().zip(v.iter()).skip(1) {
        let Some(rest) = name.split("GPU Engine(pid_").nth(1) else {
            continue;
        };
        let Some(pid) = rest.split('_').next().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if let Ok(x) = val.trim().parse::<f64>() {
            *out.entry(pid).or_default() += x;
        }
    }
    out
}

/// GPU por grupo (soma dos pids de cada um). Só funciona no Windows (typeperf).
#[cfg(windows)]
pub fn gpu_by_pid() -> HashMap<u32, f64> {
    use std::os::windows::process::CommandExt;
    let Ok(o) = std::process::Command::new("typeperf")
        .args(["\\GPU Engine(*)\\Utilization Percentage", "-sc", "1"])
        .creation_flags(0x0800_0000)
        .output()
    else {
        return HashMap::new();
    };
    parse_gpu_csv(&String::from_utf8_lossy(&o.stdout))
}

#[cfg(not(windows))]
pub fn gpu_by_pid() -> HashMap<u32, f64> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_process_name() {
        assert_eq!(group_of("Sentinela-Desktop.exe"), Some("desktop"));
        assert_eq!(group_of("SearchHost.exe"), Some("search"));
        assert_eq!(group_of("notepad.exe"), None);
    }

    #[test]
    fn gpu_csv_is_summed_per_pid() {
        let csv = "\"(PDH-CSV 4.0)\",\"\\\\PC\\GPU Engine(pid_10_luid_0x0_phys_0_eng_0_engtype_3D)\\Utilization Percentage\",\"\\\\PC\\GPU Engine(pid_10_luid_0x0_phys_0_eng_1_engtype_Copy)\\Utilization Percentage\",\"\\\\PC\\GPU Engine(pid_20_luid_0x0_phys_0_eng_0_engtype_3D)\\Utilization Percentage\"\n\"11/26/2026 20:00:00.000\",\"1.5\",\"0.5\",\"3.0\"\n";
        let m = parse_gpu_csv(csv);
        assert_eq!(m.get(&10), Some(&2.0));
        assert_eq!(m.get(&20), Some(&3.0));
    }

    #[test]
    fn sampler_runs_and_reports_all_groups() {
        let mut s = Sampler::new();
        let a = s.sample();
        assert_eq!(a.groups.len(), GROUPS.len());
        assert_eq!(a.samples, 1);
        s.reset();
        assert_eq!(s.sample().samples, 1);
    }
}
