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

export type ActivityKind = "clean" | "scan" | "quarantine" | "restore" | "update";

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
}

export const systemStatus = () => invoke<SystemStatus>("system_status");
export const relaunchElevated = () => invoke<void>("relaunch_elevated");
export const jobCancel = (id: number) => invoke<boolean>("job_cancel", { id });

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
