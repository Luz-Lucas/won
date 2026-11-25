import { invoke } from "@tauri-apps/api/core";

export interface SourceInfo {
  name: string;
  url: string;
  fetched_at: string;
  count: number;
}

export interface DbStatus {
  dir: string;
  initialized: boolean;
  sources: SourceInfo[];
  warnings: string[];
}

export type ActivityKind = "clean" | "scan" | "quarantine" | "restore" | "update" | "tweak" | "apps";

export interface ActivityEntry {
  at: string;
  kind: ActivityKind;
  summary: string;
}

export interface SystemStatus {
  version: string;
  elevated: boolean;
  db: DbStatus;
  quarantine_count: number;
  activity: ActivityEntry[];
  tweaks_applied: number;
  tweaks_total: number;
  updates_mode: UpdateMode;
  /** Dias desde que o modo de atualização foi aplicado. */
  updates_days: number | null;
  /** Modo suspenso por "Atualizar agora". */
  updates_suspended: UpdateMode | null;
}

export const systemStatus = () => invoke<SystemStatus>("system_status");
export const relaunchElevated = () => invoke<void>("relaunch_elevated");
export const jobCancel = (id: number) => invoke<boolean>("job_cancel", { id });

export type JobKind =
  | "scan"
  | "clean"
  | "find_large"
  | "find_dupes"
  | "update"
  | "tweaks"
  | "apps"
  | "net";

/** Id do job deste tipo que segue rodando no backend, se houver. */
export const jobRunning = (kind: JobKind) =>
  invoke<number | null>("job_running", { kind });

export interface Candidate {
  path: string;
  size: number;
}

export interface CategorySummary {
  id: string;
  needs_admin: boolean;
  files: number;
  bytes: number;
  warnings: string[];
  skipped_reason: string | null;
  sample: Candidate[];
  deleted: number | null;
  freed: number | null;
  skipped_files: number;
}

export interface CleanSummary {
  categories: CategorySummary[];
  total_files: number;
  total_bytes: number;
  deleted: number;
  freed: number;
  elevated: boolean;
}

export const cleanPlan = (selected: string[]) =>
  invoke<CleanSummary>("clean_plan", { selected });
export const cleanApply = (selected: string[]) =>
  invoke<number>("clean_apply", { selected });

export type ScanTarget =
  | { kind: "quick" }
  | { kind: "full" }
  | { kind: "custom"; paths: string[] };

export type Severity = "info" | "risk" | "suspicious" | "malicious";

export interface Finding {
  detector: string;
  rule: string;
  severity: Severity;
  detail: string;
}

export interface ScanItem {
  path: string;
  verdict: Severity;
  findings: Finding[];
  /** SHA-256 em hexadecimal minúsculo. */
  sha256: string;
  /** Malware confirmado por hash: vem marcado para quarentena. */
  recommended: boolean;
  /** O "por quê" de cada achado, na mesma ordem de `findings`. */
  why: string[];
  /** Achado dentro de um compactado: a entrada. A quarentena move o compactado inteiro. */
  in_archive: string | null;
}

export interface WalkError {
  path: string;
  reason: string;
}

export interface ScanSummary {
  scanned: number;
  interrupted: boolean;
  malicious: number;
  suspicious: number;
  /** Nível Risco: contexto ruim, sem nada casar (vem desmarcado). */
  risk: number;
  items: ScanItem[];
  items_total: number;
  /** Achados tirados por estarem em "Ignorar sempre". */
  ignored: number;
  errors: WalkError[];
  errors_total: number;
  missing: string[];
  db_initialized: boolean;
  warnings: string[];
}

export interface QuarantineEntry {
  id: string;
  original_path: string;
  sha256: string;
  size: number;
  rule: string;
  quarantined_at: string;
  encoding?: string;
}

export type Guard =
  | { kind: "system_folder" }
  | { kind: "microsoft_signed"; signer: string }
  | { kind: "in_use"; pids: number[]; names: string[] };

export interface GuardView {
  path: string;
  guards: Guard[];
}

export interface QuarantineRequest {
  path: string;
  sha256: string;
  rule: string;
}

export type QuarantineDone = { path: string } & (
  | { status: "quarantined"; id: string }
  | { status: "in_use" }
  | { status: "failed"; reason: string }
);

export interface QuarantineOutcome {
  done: QuarantineDone[];
  not_ended: number[];
}

export interface ScanException {
  sha256: string;
  path: string;
  rule: string;
  added_at: string;
}

export interface ScheduleView {
  scheduled: boolean;
  auto_quarantine: boolean;
  offered: boolean;
}

export interface PauseState {
  paused: boolean;
  reason: "user" | "game" | null;
}

export const scanPause = (paused: boolean) => invoke<PauseState>("scan_pause", { paused });
export const scanPauseState = () => invoke<PauseState>("scan_pause_state");
/** Pedidos do menu "Verificar com o WON" do Explorador (esvazia a fila). */
export const scanRequestTake = () => invoke<string[]>("scan_request_take");

export interface ExplorerMenuView {
  enabled: boolean;
  /** Por que não dá para mudar (app de desenvolvimento ou elevado). */
  note: string | null;
}

export const explorerMenuGet = () => invoke<ExplorerMenuView>("explorer_menu_get");
export const explorerMenuSet = (on: boolean) =>
  invoke<ExplorerMenuView>("explorer_menu_set", { on });

export const scanGuard = (paths: string[]) => invoke<GuardView[]>("scan_guard", { paths });
export const scanQuarantine = (requests: QuarantineRequest[], endPids: number[]) =>
  invoke<QuarantineOutcome>("scan_quarantine", { requests, endPids });
export const quarantineDelete = (id: string) => invoke<string>("quarantine_delete", { id });
export const scanExceptionsList = () => invoke<ScanException[]>("scan_exceptions_list");
export const scanExceptionAdd = (sha256: string, path: string, rule: string) =>
  invoke<ScanException[]>("scan_exception_add", { sha256, path, rule });
export const scanExceptionRemove = (sha256: string) =>
  invoke<ScanException[]>("scan_exception_remove", { sha256 });
export const scanScheduleGet = () => invoke<ScheduleView>("scan_schedule_get");
export const scanScheduleSet = (on: boolean) => invoke<ScheduleView>("scan_schedule_set", { on });
export const scanScheduleDismiss = () => invoke<ScheduleView>("scan_schedule_dismiss");
export const scanAutoQuarantineSet = (on: boolean) =>
  invoke<ScheduleView>("scan_auto_quarantine_set", { on });

export const scanStart = (target: ScanTarget) =>
  invoke<number>("scan_start", { target });
export const quarantineList = () =>
  invoke<QuarantineEntry[]>("quarantine_list");
export const quarantineRestore = (id: string) =>
  invoke<string>("quarantine_restore", { id });

export interface UpdateSummary {
  hashes: number;
  rules_ok: number;
  rules_rejected: number;
  rules_count: number;
  warnings: string[];
}

export const signaturesUpdate = () => invoke<number>("signatures_update");
export const abuseKeySet = (key: string) =>
  invoke<void>("abuse_key_set", { key });
export const abuseKeyClear = () => invoke<void>("abuse_key_clear");
/** De onde viria a chave: salva, variável de ambiente ou nenhuma. Nunca a chave. */
export type KeySource = "stored" | "env" | "none";

export const abuseKeyPresent = () => invoke<KeySource>("abuse_key_present");

export interface LargeFile {
  path: string;
  size: number;
}

export interface DupGroup {
  size: number;
  sha256: string;
  paths: string[];
}

export interface FindResult<T> {
  items: T[];
  errors: WalkError[];
  errors_total: number;
  interrupted: boolean;
}

export const diskFindLarge = (path: string, top: number, minMb: number) =>
  invoke<number>("disk_find_large", { path, top, minMb });
export const diskFindDupes = (path: string, minKb: number) =>
  invoke<number>("disk_find_dupes", { path, minKb });

export type TweakGroup =
  | "jogos"
  | "energia"
  | "segundo_plano"
  | "privacidade"
  | "visual"
  | "personalizacao"
  | "avancado"
  | "atualizacoes"
  | "rede";
export type TweakEvidence = "comprovado" | "situacional" | "controverso";
export type TweakEffect = "immediate" | "explorer" | "relogon" | "reboot";
export type TweakStatus = "applied" | "not_applied" | "partial" | "unavailable";

export type RegValue =
  | { type: "dword"; data: number }
  | { type: "qword"; data: number }
  | { type: "string"; data: string };

export type TweakAction =
  | {
      kind: "registry";
      hive: "HKCU" | "HKLM";
      path: string;
      name: string;
      value: RegValue | null;
    }
  | {
      kind: "list_entry";
      hive: "HKCU" | "HKLM";
      path: string;
      name: string;
      key: string;
      value: string;
    }
  | { kind: "service"; name: string; start: "automatic" | "manual" | "disabled" }
  | { kind: "power_plan"; scheme: string }
  | { kind: "scheduled_task"; path: string; enabled: boolean }
  | { kind: "dns_servers"; interface: string; ipv6: boolean; servers: string[] };

export type TweakArea = "barra" | "iniciar" | "explorador" | "aparencia" | "sistema";

export interface Tweak {
  id: string;
  title: string;
  description: string;
  group: TweakGroup;
  evidence: TweakEvidence;
  effect: TweakEffect;
  needs_admin: boolean;
  laptop_warning: boolean;
  recommended: boolean;
  source: string;
  actions: TweakAction[];
  /** Grupo de opções mutuamente exclusivas (lista de opções). */
  choice?: string | null;
  /** Nome da lista de opções, quando há `choice`. */
  choice_label?: string | null;
  /** Aba da página Personalizar. */
  area?: TweakArea | null;
}

export interface TweakChange {
  description: string;
  current: string;
  target: string;
}

export interface TweakView {
  id: string;
  status: TweakStatus;
  blocked: string | null;
  changes: TweakChange[];
}

export interface TweakItem {
  tweak: Tweak;
  view: TweakView;
  in_journal: boolean;
}

export interface TweaksPage {
  tweaks: TweakItem[];
  laptop: boolean;
  elevated: boolean;
  hkcu_allowed: boolean;
  journal_count: number;
  restore_point_done: boolean;
}

export interface TweakItemResult {
  tweak_id: string;
  ok: boolean;
  /** Algo foi de fato escrito no sistema. */
  changed: boolean;
  message: string;
}

export interface ApplyReport {
  results: TweakItemResult[];
  effect: TweakEffect;
  restore_point: string | null;
  notice: string | null;
}

export interface UndoReport {
  results: TweakItemResult[];
  conflicts: string[];
  notice: string | null;
}

export type JournalSnapshot =
  | { kind: "registry"; value: RegValue | null }
  | { kind: "list_entry"; value: string | null }
  | { kind: "service"; start: "automatic" | "manual" | "disabled" }
  | { kind: "power_plan"; scheme: string }
  | { kind: "scheduled_task"; enabled: boolean }
  | { kind: "dns_servers"; servers: string[] }
  | { kind: "unavailable"; reason: string };

export interface JournalAction {
  action: TweakAction;
  before: JournalSnapshot;
  written: JournalSnapshot;
  done: boolean;
}

export interface JournalEntry {
  tweak_id: string;
  title: string;
  applied_at: string;
  actions: JournalAction[];
}

export const tweaksList = () => invoke<TweaksPage>("tweaks_list");
export const tweaksApply = (ids: string[]) =>
  invoke<number>("tweaks_apply", { ids });
export const tweaksApplyChoice = (id: string) =>
  invoke<number>("tweaks_apply_choice", { id });
/** Reinicia o Explorer (fecha as janelas de pastas abertas). */
export const restartExplorer = () => invoke<void>("restart_explorer");
export const tweaksUndo = (ids: string[]) =>
  invoke<number>("tweaks_undo", { ids });
export const tweaksUndoAll = () => invoke<number>("tweaks_undo_all");
export interface HistoryPage {
  entries: JournalEntry[];
  elevated: boolean;
  hkcu_allowed: boolean;
}

export const tweaksJournal = () => invoke<HistoryPage>("tweaks_journal");

export type StartupSource =
  | "run_hkcu"
  | "run_hklm"
  | "run_hklm32"
  | "startup_folder_user"
  | "startup_folder_common";

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  source: StartupSource;
  enabled: boolean;
  file_exists: boolean;
  needs_admin: boolean;
  /** A entrada do próprio WON (sem interruptor). */
  sentinela: boolean;
  /** Por que o interruptor está travado nesta janela. */
  locked: string | null;
}

export interface StartupPage {
  items: StartupItem[];
  warnings: string[];
  elevated: boolean;
}

export const startupList = () => invoke<StartupPage>("startup_list");
/** Liga ou desliga e devolve a lista atualizada. */
export const startupSetEnabled = (id: string, enabled: boolean) =>
  invoke<StartupPage>("startup_set_enabled", { id, enabled });

export type GameSource = "steam" | "epic" | "riot" | "manual";

export interface GameItem {
  /** Id do ajuste gerado (`gpu-…`). */
  id: string;
  name: string;
  source: GameSource;
  exe: string;
  exe_exists: boolean;
  /** "Alto desempenho" gravado agora. */
  enabled: boolean;
  in_journal: boolean;
  /** Ligado fora do WON (Configurações → Gráficos). */
  external: boolean;
  blocked: string | null;
  /** Prioridade de rede (QoS, DSCP 46). */
  qos: GameSwitch;
}

export interface GameSwitch {
  /** Id do ajuste gerado (`qos-…`). */
  id: string;
  enabled: boolean;
  in_journal: boolean;
  external: boolean;
  blocked: string | null;
}

export interface GamesPage {
  games: GameItem[];
  /** No Histórico, mas o jogo não é mais encontrado. */
  missing: { id: string; title: string; kind: "gpu" | "qos" }[];
  gpus: number;
  warnings: string[];
  elevated: boolean;
}

export const gamesList = () => invoke<GamesPage>("games_list");
export const gamesGpuSet = (id: string, enabled: boolean) =>
  invoke<GamesPage>("games_gpu_set", { id, enabled });
/** Prioridade de rede (QoS); exige administrador. */
export const gamesQosSet = (id: string, enabled: boolean) =>
  invoke<GamesPage>("games_qos_set", { id, enabled });
export const gamesAdd = (path: string) => invoke<GamesPage>("games_add", { path });
export const gamesRemove = (path: string) =>
  invoke<GamesPage>("games_remove", { path });

export type UpdateMode = "default" | "security" | "paused" | "blocked";

export interface UpdateModeState {
  mode: UpdateMode;
  /** Quando o modo foi aplicado (RFC 3339). */
  since: string | null;
  /** AAAA-MM-DD do fim da pausa. */
  paused_until: string | null;
  /** O que o Windows desfez sozinho ("Parcial"). */
  reverted: string[];
}

export interface DefenderState {
  last_attempt: number | null;
  last_ok: number | null;
  last_error: string | null;
}

export interface UpdatesPage {
  state: UpdateModeState;
  days: number | null;
  elevated: boolean;
  /** Ex.: "Windows 11 25H2". */
  version: string | null;
  /** Modo suspenso por "Atualizar agora". */
  suspended: { mode: UpdateMode; until: string | null } | null;
  defender: DefenderState;
}

export const updatesStatus = () => invoke<UpdatesPage>("updates_status");
/** Só o modo atual (rápido, pelo diário). */
export const updatesMode = () => invoke<UpdateMode>("updates_mode");
export const updatesPreview = (mode: UpdateMode, until: string | null) =>
  invoke<TweakChange[]>("updates_preview", { mode, until });
export const updatesSetMode = (mode: UpdateMode, until: string | null) =>
  invoke<UpdatesPage>("updates_set_mode", { mode, until });
/** Volta ao Padrão, guarda o modo e abre o Windows Update. */
export const updatesUpdateNow = () => invoke<UpdatesPage>("updates_update_now");
export const updatesResume = () => invoke<UpdatesPage>("updates_resume");

export interface SettingsView {
  start_with_windows: boolean;
  close_to_tray: boolean;
  elevated: boolean;
  /** Por que a inicialização com o Windows não pôde ser alterada, se for o caso. */
  note: string | null;
}

export const settingsGet = () => invoke<SettingsView>("settings_get");
export const settingsSet = (startWithWindows: boolean, closeToTray: boolean) =>
  invoke<SettingsView>("settings_set", { startWithWindows, closeToTray });

/** Avisa o backend que a interface carregou (ele mostra a janela). */
export const windowReady = () => invoke<void>("window_ready");
/** Sai do app depois que o usuário confirmou (fila de aplicativos rodando). */
export const quitApp = () => invoke<void>("quit_app");

export type AppCategory =
  | "navegadores"
  | "comunicacao"
  | "jogos"
  | "ferramentas_jogos"
  | "multimidia"
  | "utilitarios"
  | "desenvolvimento";
export type AppState = "not_installed" | "installed" | "update_available";

export interface CatalogApp {
  id: string;
  name: string;
  description: string;
  category: AppCategory;
  winget: string;
  link: string;
  foss: boolean;
}

export interface AppItem {
  app: CatalogApp;
  /** null: winget indisponível ou consulta falhou. */
  state: AppState | null;
}

export interface Availability {
  available: boolean;
  version: string | null;
  reason: string | null;
}

export interface AppsPage {
  availability: Availability;
  apps: AppItem[];
  categories: { id: AppCategory; label: string }[];
}

export interface AppOutcome {
  /** Id do catálogo (não o do winget). */
  id: string;
  ok: boolean;
  changed: boolean;
  message: string;
}

export const STORE_APP_INSTALLER_URL = "ms-windows-store://pdp/?ProductId=9NBLGGH4NNS1";

export const appsPage = () => invoke<AppsPage>("apps_page");
export const appsPageRefresh = () => invoke<AppsPage>("apps_page_refresh");
export const appsInstall = (ids: string[]) => invoke<number>("apps_install", { ids });
export const appsUpgrade = (ids: string[]) => invoke<number>("apps_upgrade", { ids });

export interface NetTarget {
  name: string;
  host: string;
  port: number | null;
}

export interface NetTargetItem extends NetTarget {
  /** `host`, `host:porta` ou `[IPv6]:porta`. */
  label: string;
  /** Acrescentado pelo usuário (pode remover). */
  user: boolean;
}

export interface Measurement {
  target: NetTarget;
  addr: string;
  sent: number;
  received: number;
  loss_pct: number;
  min_ms: number | null;
  avg_ms: number | null;
  median_ms: number | null;
  max_ms: number | null;
  jitter_ms: number | null;
  error: string | null;
}

export interface Hop {
  ttl: number;
  addr: string | null;
  rtts_ms: (number | null)[];
  segment: "home" | "isp";
}

export interface Trace {
  target: NetTarget;
  addr: string;
  hops: Hop[];
  reached: boolean;
  /** TTL do salto em que o atraso começa. */
  delay_start: number | null;
  error: string | null;
}

export type BloatGrade = "good" | "moderate" | "high";

export interface BufferbloatResult {
  idle: Measurement;
  loaded: Measurement;
  increase_ms: number | null;
  grade: BloatGrade | null;
  download_mbps: number | null;
  diagnosis: string;
}

export interface DnsRow {
  name: string;
  addr: string;
  median_ms: number | null;
  answered: number;
  sent: number;
  current: boolean;
  resolver: string | null;
}

export interface DnsReport {
  rows: DnsRow[];
  /** Id do DNS público mais rápido. */
  best: string | null;
  keep_current: boolean;
  advice: string;
}

export type NetResult =
  | { op: "measure"; results: Measurement[] }
  | { op: "trace"; trace: Trace }
  | { op: "bufferbloat"; result: BufferbloatResult }
  | { op: "dns"; report: DnsReport }
  | { op: "compare"; comparison: Comparison };

export interface NetAdapter {
  guid: string;
  name: string;
  wifi: boolean;
  has_ipv6: boolean;
  dns: string[];
}

export interface WifiStatus {
  ssid: string;
  signal_pct: number;
  channel: number | null;
}

export interface NetHistoryEntry {
  at: string;
  kind: "latency" | "trace" | "bufferbloat" | "dns" | "route";
  target: string;
  summary: string;
}

export interface NetPage {
  targets: NetTargetItem[];
  adapters: NetAdapter[];
  wifi: WifiStatus | null;
  wifi_notice: string | null;
  history: NetHistoryEntry[];
  elevated: boolean;
  /** Servidores que o Windows sabe criptografar (DNS sobre HTTPS). */
  doh_known: string[];
}

export interface DnsPreview {
  adapter: string;
  changes: TweakChange[];
}

export const netPage = () => invoke<NetPage>("net_page");
export const netTargetAdd = (spec: string) => invoke<NetPage>("net_target_add", { spec });
export const netTargetRemove = (label: string) =>
  invoke<NetPage>("net_target_remove", { label });
/** Mede todos os destinos (`label` null) ou só um. */
export const netMeasure = (label: string | null) => invoke<number>("net_measure", { label });
export const netTrace = (label: string) => invoke<number>("net_trace", { label });
export const netBufferbloat = () => invoke<number>("net_bufferbloat");
export const netDnsBench = () => invoke<number>("net_dns_bench");
export const netDnsPreview = (resolver: string, doh: boolean) =>
  invoke<DnsPreview[]>("net_dns_preview", { resolver, doh });
export const netDnsApply = (resolver: string, doh: boolean) =>
  invoke<TweakItemResult[]>("net_dns_apply", { resolver, doh });

export type RouteKind = "warp" | "wireguard" | "paid";

export interface NetRoute {
  /** `warp`, `wg:{nome}` ou `paid:{id}`. */
  id: string;
  name: string;
  kind: RouteKind;
  /** Ligada agora (null: não deu para saber). */
  on: boolean | null;
  /** O WON liga e desliga (serviços pagos: não). */
  controllable: boolean;
  note: string | null;
}

export interface TunnelInfo {
  name: string;
  address: string | null;
  endpoint: string | null;
}

export interface GameRouteItem {
  key: string;
  name: string;
  route: string | null;
}

export interface RoutesPage {
  routes: NetRoute[];
  tunnels: TunnelInfo[];
  games: GameRouteItem[];
  wifi_hold: boolean;
  elevated: boolean;
}

export interface CompareRow {
  /** `normal` ou o id da rota. */
  id: string;
  name: string;
  measurement: Measurement;
}

export interface Comparison {
  target: NetTarget;
  rows: CompareRow[];
  best: string | null;
  advice: string;
  errors: string[];
  interrupted: boolean;
}

export const netRoutesPage = () => invoke<RoutesPage>("net_routes_page");
export const netTunnelImport = (name: string, path: string) =>
  invoke<RoutesPage>("net_tunnel_import", { name, path });
export const netWarpTunnelCreate = (name?: string) =>
  invoke<RoutesPage>("net_warp_tunnel_create", { name });
export const netWarpRegister = () => invoke<RoutesPage>("net_warp_register");
export const netTunnelRemove = (name: string) =>
  invoke<RoutesPage>("net_tunnel_remove", { name });
export const netRouteSet = (id: string, on: boolean) =>
  invoke<RoutesPage>("net_route_set", { id, on });
export const netGameRouteSet = (key: string, route: string | null) =>
  invoke<RoutesPage>("net_game_route_set", { key, route });
export const netWifiHoldSet = (on: boolean) => invoke<RoutesPage>("net_wifi_hold_set", { on });
/** Compara a conexão atual com as rotas `ids` até o destino `label` (job "net"). */
export const netCompare = (label: string, ids: string[]) =>
  invoke<number>("net_compare", { label, ids });

// --- Wallpaper Engine e temas (fase 5) ---

export type WallpaperPauseMode = "off" | "pause" | "stop";

export interface WallpaperStatus {
  installed: boolean;
  running: boolean;
  build: string | null;
  pause_mode: WallpaperPauseMode;
}

export interface WallpaperItem {
  id: string;
  dir: string;
  title: string;
  kind: string | null;
  preview: string | null;
  /** Miniatura em data URL; `null` para GIF ou sem miniatura. */
  thumb: string | null;
  active: boolean;
  /** Índices dos monitores em que está em uso. */
  active_on: number[];
}

export interface WallpaperMonitor {
  /** Índice do `-monitor` do Wallpaper Engine (começa em 0). */
  index: number;
  name: string;
  width: number;
  height: number;
  primary: boolean;
  current: string | null;
}

export const wallpaperStatus = () => invoke<WallpaperStatus>("wallpaper_status");
export const wallpaperGallery = () => invoke<WallpaperItem[]>("wallpaper_gallery");
export const wallpaperMonitors = () => invoke<WallpaperMonitor[]>("wallpaper_monitors");
/** `monitor` `null` aplica em todos os monitores. */
export const wallpaperOpen = (id: string, monitor: number | null = null) =>
  invoke<void>("wallpaper_open", { id, monitor });
export const wallpaperPauseModeSet = (mode: WallpaperPauseMode) =>
  invoke<WallpaperStatus>("wallpaper_pause_mode_set", { mode });

export interface ThemeInfo {
  name: string;
  description: string;
  toml: string;
  builtin: boolean;
  /** Tudo do tema já está assim agora (lido de volta do sistema). */
  applied?: boolean;
}

export interface ThemeRecord {
  seq: number;
  theme: string;
  applied_at: string;
  tweak_ids: string[];
  wallpaper: { previous: string | null; applied: string } | null;
}

export interface ThemesPage {
  themes: ThemeInfo[];
  history: ThemeRecord[];
}

export interface ThemePreviewItem {
  kind: "tweak" | "accent" | "wallpaper";
  label: string;
  change: boolean;
  detail: string;
}

export interface ThemePreview {
  items: ThemePreviewItem[];
  warnings: string[];
}

export interface ThemeApplied {
  results: { tweak_id: string; ok: boolean; changed: boolean; message: string }[];
  effect: TweakEffect;
  warnings: string[];
  record: ThemeRecord | null;
}

export interface ThemeUndone {
  results: { tweak_id: string; ok: boolean; changed: boolean; message: string }[];
  conflicts: string[];
  warnings: string[];
  remaining: ThemeRecord | null;
}

export interface ImportedTheme {
  name: string;
  description: string;
  toml: string;
  warnings: string[];
  disabled: string[];
}

export const themeList = () => invoke<ThemesPage>("theme_list");
export const themePreview = (toml: string) => invoke<ThemePreview>("theme_preview", { toml });
export const themeApply = (toml: string) => invoke<ThemeApplied>("theme_apply", { toml });
export const themeUndo = () => invoke<ThemeUndone>("theme_undo");
export const themeExport = (name: string, path: string) =>
  invoke<void>("theme_export", { name, path });
export const themeImport = (path: string) => invoke<ImportedTheme>("theme_import", { path });

export type LojaKind = "cursores" | "icones" | "temas";

export interface LojaItem {
  id: string;
  kind: LojaKind;
  name: string;
  author: string;
  license: string;
  description: string;
  tags: string[];
  /** PNG em `data:`. */
  preview: string;
  bytes: number;
  remote: boolean;
  /** Temas: o theme.toml. */
  toml: string | null;
  /** Temas: cursor e ícones que acompanham. */
  cursor: string | null;
  icons: string | null;
  installed: boolean;
  applied: boolean;
}

export interface LojaPage {
  items: LojaItem[];
  online: boolean;
  online_error: string | null;
  /** Pacote cujos ícones estão nos atalhos (.lnk) agora. */
  shortcuts_active: string | null;
}

export interface LojaShortcutsApplied {
  id: string;
  changed: number;
  skipped: number;
  failed: string[];
  restart_explorer: boolean;
}

export interface LojaShortcutsReverted {
  id: string;
  restored: number;
  failed: string[];
  restart_explorer: boolean;
}

export interface LojaApplied {
  id: string;
  restart_explorer: boolean;
}

export interface LojaReverted extends LojaApplied {
  failed: string[];
}

export const lojaList = (online: boolean) => invoke<LojaPage>("loja_list", { online });
export const lojaInstall = (id: string, online: boolean) =>
  invoke<void>("loja_install", { id, online });
export const lojaRemove = (id: string, online: boolean) =>
  invoke<void>("loja_remove", { id, online });
export const lojaApply = (id: string, online: boolean) =>
  invoke<LojaApplied>("loja_apply", { id, online });
export const lojaRevert = (kind: LojaKind) => invoke<LojaReverted>("loja_revert", { kind });
export const lojaApplyShortcuts = (id: string, online: boolean) =>
  invoke<LojaShortcutsApplied>("loja_apply_shortcuts", { id, online });
export const lojaRevertShortcuts = () =>
  invoke<LojaShortcutsReverted>("loja_revert_shortcuts");

export interface TerminalProfile {
  shell: string;
  path: string;
  active: boolean;
}

export interface TerminalFont {
  name: string;
  label: string;
}

export interface TerminalStatus {
  installed: boolean;
  version: string | null;
  update_available: boolean;
  profiles: TerminalProfile[];
  windows_terminal: boolean;
  fonts: TerminalFont[];
  can_undo: boolean;
}

export interface TerminalRequest {
  theme: string | null;
  extras: string[];
  font: string | null;
  scheme_from_wallpaper: boolean;
}

export interface TerminalChange {
  label: string;
  path: string;
  /** [adicionada, linha] */
  lines: [boolean, string][];
  is_new: boolean;
}

export interface TerminalPlan {
  changes: TerminalChange[];
  warnings: string[];
  disabled: string[];
}

export interface TerminalApplied {
  changed: number;
  errors: string[];
  warnings: string[];
}

export interface TerminalHistoryItem {
  seq: number;
  label: string;
  path: string;
  applied_at: string;
}

export interface TerminalUndo {
  undone: TerminalHistoryItem | null;
  changed_after: string | null;
}

export interface TerminalTiming {
  median_ms: number;
  runs: number;
  slow: boolean;
}

export const terminalStatus = () => invoke<TerminalStatus>("terminal_status");
export const terminalInstallOmp = (upgrade: boolean) =>
  invoke<string>("terminal_install_omp", { upgrade });
export const terminalInstallFont = (name: string) =>
  invoke<void>("terminal_install_font", { name });
export const terminalThemes = () => invoke<string[]>("terminal_themes");
export const terminalThemePreview = (name: string) =>
  invoke<string>("terminal_theme_preview", { name });
export const terminalTiming = (name: string) =>
  invoke<TerminalTiming>("terminal_timing", { name });
export const terminalPlan = (req: TerminalRequest) =>
  invoke<TerminalPlan>("terminal_plan", { req });
export const terminalApply = (req: TerminalRequest) =>
  invoke<TerminalApplied>("terminal_apply", { req });
export const terminalDisablePrompt = () => invoke<number>("terminal_disable_prompt");
export const terminalHistory = () => invoke<TerminalHistoryItem[]>("terminal_history");
export const terminalUndo = (force: boolean) =>
  invoke<TerminalUndo>("terminal_undo", { force });

export interface DesktopStatus {
  installed: boolean;
  running: boolean;
  config_path: string;
  config_exists: boolean;
  errors: string[];
  hotkeys: number;
  rules: number;
}

export const desktopStatus = () => invoke<DesktopStatus>("desktop_status");
export const desktopStart = () => invoke<void>("desktop_start");
export const desktopStop = () => invoke<void>("desktop_stop");
export const desktopConfigRead = () => invoke<string>("desktop_config_read");
export const desktopConfigCheck = (text: string) =>
  invoke<string[]>("desktop_config_check", { text });
export const desktopConfigWrite = (text: string) =>
  invoke<void>("desktop_config_write", { text });

export interface AppUpdate {
  current: string;
  latest: string;
  available: boolean;
  url: string;
}

export const appUpdateCheck = () => invoke<AppUpdate>("app_update_check");

export interface GameSession {
  previous_scheme: string;
  started_at: string;
}

export const gameSessionStatus = () => invoke<GameSession | null>("game_session_status");
export const gameSessionStart = () => invoke<GameSession>("game_session_start");
export const gameSessionEnd = () => invoke<void>("game_session_end");

export interface ModLoaded {
  mod_name: string;
  pid: number;
  process: string;
  dll: string;
}

export interface ModItem {
  name: string;
  dll: string;
  targets: string[];
  enabled: boolean;
  dll_exists: boolean;
  loaded: ModLoaded[];
}

export interface ModsStatus {
  installed: boolean;
  mods: ModItem[];
  anticheat_running: boolean;
  registry_path: string;
  recovery_path: string | null;
  registry_error: string | null;
}

export const modsStatus = () => invoke<ModsStatus>("mods_status");
export const modsInject = (name: string) => invoke<number>("mods_inject", { name });
export const modsEject = (name: string) => invoke<number>("mods_eject", { name });
export const modsEjectAll = () => invoke<number>("mods_eject_all");
export const modsRecoveryWrite = () => invoke<string>("mods_recovery_write");
