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
}

export const systemStatus = () => invoke<SystemStatus>("system_status");
export const relaunchElevated = () => invoke<void>("relaunch_elevated");
export const jobCancel = (id: number) => invoke<boolean>("job_cancel", { id });

export type JobKind = "scan" | "clean" | "find_large" | "find_dupes" | "update" | "tweaks" | "apps";

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

export type Severity = "info" | "suspicious" | "malicious";

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
  items: ScanItem[];
  items_total: number;
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
}

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
  | "visual";
export type TweakEvidence = "comprovado" | "situacional" | "controverso";
export type TweakEffect = "immediate" | "relogon" | "reboot";
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
  | { kind: "power_plan"; scheme: string };

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
export const tweaksUndo = (ids: string[]) =>
  invoke<number>("tweaks_undo", { ids });
export const tweaksUndoAll = () => invoke<number>("tweaks_undo_all");
export interface HistoryPage {
  entries: JournalEntry[];
  elevated: boolean;
  hkcu_allowed: boolean;
}

export const tweaksJournal = () => invoke<HistoryPage>("tweaks_journal");

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
