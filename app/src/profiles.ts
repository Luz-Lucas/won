/** Perfis de otimização do usuário, guardados no perfil do WebView desta conta. */
export interface UserProfile {
  id: string;
  name: string;
  /** Ids de ajustes do catálogo. */
  tweaks: string[];
}

const KEY = "sentinela.perfis.v1";
export const MAX_PROFILES = 20;
export const MAX_NAME = 40;

const valid = (v: unknown): v is UserProfile => {
  if (typeof v !== "object" || v === null) return false;
  const p = v as Record<string, unknown>;
  return (
    typeof p.id === "string" &&
    typeof p.name === "string" &&
    p.name.trim() !== "" &&
    Array.isArray(p.tweaks) &&
    p.tweaks.every((t) => typeof t === "string")
  );
};

/** Lista lida do disco; dado corrompido ou sem armazenamento vira lista vazia. */
export function loadProfiles(): UserProfile[] {
  try {
    const raw = window.localStorage.getItem(KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed) ? parsed.filter(valid).slice(0, MAX_PROFILES) : [];
  } catch {
    return [];
  }
}

export function storeProfiles(list: UserProfile[]): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(list.slice(0, MAX_PROFILES)));
  } catch {
    /* sem armazenamento: o perfil vale só até fechar a janela */
  }
}

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`;

/** Salva com o nome; um nome que já existe (sem diferenciar maiúsculas) é atualizado. */
export function upsertProfile(
  list: UserProfile[],
  name: string,
  tweaks: string[],
): { list: UserProfile[]; profile: UserProfile } {
  const clean = name.trim().slice(0, MAX_NAME);
  const existing = list.find((p) => p.name.toLowerCase() === clean.toLowerCase());
  const profile: UserProfile = { id: existing?.id ?? newId(), name: clean, tweaks: [...tweaks] };
  return {
    list: existing ? list.map((p) => (p.id === existing.id ? profile : p)) : [...list, profile],
    profile,
  };
}
