import { act, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { Measurement, NetPage, NetTarget, TweakItem } from "../api";
import Rede, { labelOf } from "./Rede";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  netPage: vi.fn(),
  netTargetAdd: vi.fn(),
  netTargetRemove: vi.fn(),
  netMeasure: vi.fn(),
  netTrace: vi.fn(),
  netBufferbloat: vi.fn(),
  netDnsBench: vi.fn(),
  netDnsPreview: vi.fn(),
  netDnsApply: vi.fn(),
  tweaksList: vi.fn(),
  tweaksApply: vi.fn(),
  tweaksUndo: vi.fn(),
  relaunchElevated: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const target = (host: string, port: number | null = null): NetTarget => ({
  name: "",
  host,
  port,
});

const netPage = (over: Partial<NetPage> = {}): NetPage => ({
  targets: [
    { name: "Roteador", host: "192.168.0.1", port: null, label: "192.168.0.1", user: false },
    { name: "", host: "jogo.example", port: 27015, label: "jogo.example:27015", user: true },
  ],
  adapters: [{ guid: "{A}", name: "Ethernet", wifi: false, has_ipv6: false, dns: [] }],
  wifi: null,
  wifi_notice: null,
  history: [],
  elevated: true,
  doh_known: ["1.1.1.1", "1.0.0.1", "8.8.8.8", "8.8.4.4"],
  ...over,
});

const measurement = (t: NetTarget, avg: number | null): Measurement => ({
  target: t,
  addr: t.host,
  sent: 20,
  received: avg === null ? 0 : 20,
  loss_pct: avg === null ? 100 : 0,
  min_ms: avg,
  avg_ms: avg,
  median_ms: avg,
  max_ms: avg,
  jitter_ms: avg === null ? null : 1.5,
  error: null,
});

const finish = async (result: unknown, job = 7, kind = "net") => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind, result } });
  });
};

const nicItem = (status: "applied" | "not_applied" = "not_applied"): TweakItem => ({
  tweak: {
    id: "nic-0a5c8bb6",
    title: "Adaptador Ethernet: sem economia de energia",
    description: "Impede o Windows de desligar o adaptador.",
    group: "rede",
    evidence: "situacional",
    effect: "reboot",
    needs_admin: true,
    laptop_warning: true,
    recommended: false,
    source: "fonte",
    actions: [],
    choice: null,
    choice_label: null,
    area: null,
  },
  view: {
    id: "nic-0a5c8bb6",
    status,
    blocked: null,
    changes: [{ description: "PnPCapabilities", current: "não existe", target: "24" }],
  },
  in_journal: status === "applied",
});

describe("Rede", () => {
  beforeEach(() => {
    vi.mocked(api.netPage).mockReset().mockResolvedValue(netPage());
    vi.mocked(api.tweaksList).mockReset().mockResolvedValue({
      tweaks: [],
      laptop: false,
      elevated: true,
      hkcu_allowed: true,
      journal_count: 0,
      restore_point_done: false,
    });
    for (const f of [
      api.netMeasure,
      api.netTrace,
      api.netBufferbloat,
      api.netDnsBench,
      api.netDnsPreview,
      api.netDnsApply,
      api.netTargetAdd,
      api.tweaksApply,
    ])
      vi.mocked(f).mockReset();
    listeners.clear();
  });

  it("shows the honesty text", async () => {
    render(<Rede />);
    expect(
      await screen.findByText(/A rota até o servidor do jogo é decidida pelo seu provedor/),
    ).toBeInTheDocument();
  });

  it("measure_runs_as_job_and_shows_results", async () => {
    vi.mocked(api.netMeasure).mockResolvedValue(7);
    render(<Rede />);
    await userEvent.click(await screen.findByRole("button", { name: "Medir" }));
    expect(api.netMeasure).toHaveBeenCalledWith(null);
    await act(async () => {
      listeners.get("job:progress")!({
        payload: { job: 7, phase: "net", index: 0, total: 2, current: "192.168.0.1" },
      });
    });
    expect(screen.getByText("Medindo 192.168.0.1 (1 de 2)…")).toBeInTheDocument();
    await finish({
      op: "measure",
      results: [
        measurement(target("192.168.0.1"), 0.7),
        measurement(target("jogo.example", 27015), null),
      ],
    });
    const row = screen.getByText("Roteador (192.168.0.1)").closest("tr")!;
    expect(within(row).getByText("0,7 ms")).toBeInTheDocument();
    expect(within(row).getByText("1,5 ms")).toBeInTheDocument();
    const game = screen.getByText("jogo.example:27015").closest("tr")!;
    expect(within(game).getByText("sem resposta")).toBeInTheDocument();
    expect(within(game).getByText("100%")).toBeInTheDocument();
  });

  it("adds a target", async () => {
    vi.mocked(api.netTargetAdd).mockResolvedValue(netPage());
    render(<Rede />);
    await userEvent.type(await screen.findByLabelText("Novo destino"), "x.example:443");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar destino" }));
    expect(api.netTargetAdd).toHaveBeenCalledWith("x.example:443");
  });

  it("trace_marks_delay_start", async () => {
    vi.mocked(api.netTrace).mockResolvedValue(7);
    render(<Rede />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Rastrear rota até jogo.example:27015" }),
    );
    expect(api.netTrace).toHaveBeenCalledWith("jogo.example:27015");
    await finish({
      op: "trace",
      trace: {
        target: target("jogo.example", 27015),
        addr: "203.0.113.9",
        hops: [
          { ttl: 1, addr: "192.168.0.1", rtts_ms: [1, 1.2, null], segment: "home" },
          { ttl: 2, addr: "100.64.0.1", rtts_ms: [9, 9, 9], segment: "isp" },
          { ttl: 3, addr: null, rtts_ms: [null, null, null], segment: "isp" },
          { ttl: 4, addr: "203.0.113.1", rtts_ms: [60, 61, 62], segment: "isp" },
        ],
        reached: true,
        delay_start: 4,
        error: null,
      },
    });
    const route = screen.getByLabelText("Rota");
    expect(within(route).getByText("Rota até jogo.example:27015")).toBeInTheDocument();
    const hop4 = within(route).getByText("203.0.113.1").closest("tr")!;
    expect(within(hop4).getByText(/o atraso começa aqui/)).toBeInTheDocument();
    expect(within(route).getByText("*")).toBeInTheDocument();
    expect(within(route).getAllByText("Sua casa")).toHaveLength(1);
    expect(within(route).getByText(/no provedor ou depois dele/)).toBeInTheDocument();
  });

  it("bufferbloat_shows_grade", async () => {
    vi.mocked(api.netBufferbloat).mockResolvedValue(7);
    render(<Rede />);
    await userEvent.click(await screen.findByRole("button", { name: "Testar bufferbloat" }));
    await finish({
      op: "bufferbloat",
      result: {
        idle: measurement(target("192.168.0.1"), 2),
        loaded: measurement(target("192.168.0.1"), 140),
        increase_ms: 138,
        grade: "high",
        download_mbps: 290.4,
        diagnosis: "Alto: ative o SQM/QoS no roteador.",
      },
    });
    const box = screen.getByLabelText("Resultado do bufferbloat");
    expect(within(box).getByText("Resultado: Alto")).toBeInTheDocument();
    expect(
      within(box).getByText("Parado: 2,0 ms · Ocupado: 140,0 ms · Download: 290 Mbps"),
    ).toBeInTheDocument();
    expect(within(box).getByText(/SQM\/QoS/)).toBeInTheDocument();
  });

  it("dns_apply_asks_confirmation_with_preview", async () => {
    vi.mocked(api.netDnsBench).mockResolvedValue(7);
    vi.mocked(api.netDnsPreview).mockResolvedValue([
      {
        adapter: "Ethernet",
        changes: [
          {
            description: "DNS do adaptador {A} (IPv4)",
            current: "automático (DHCP)",
            target: "8.8.8.8, 8.8.4.4",
          },
        ],
      },
    ]);
    vi.mocked(api.netDnsApply).mockResolvedValue([
      { tweak_id: "dns-a", ok: true, changed: true, message: "aplicada" },
    ]);
    render(<Rede />);
    expect(await screen.findByText(/não o ping dentro do jogo/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Medir DNS" }));
    await finish({
      op: "dns",
      report: {
        rows: [
          { name: "Google", addr: "8.8.8.8", median_ms: 5, answered: 9, sent: 9, current: false, resolver: "google" },
          { name: "Atual (10.0.0.1)", addr: "10.0.0.1", median_ms: 30, answered: 9, sent: 9, current: true, resolver: null },
        ],
        best: "google",
        keep_current: false,
        advice: "Google respondeu mais rápido.",
      },
    });
    await userEvent.click(screen.getByRole("button", { name: "Usar o mais rápido" }));
    const dialog = await screen.findByRole("dialog");
    expect(api.netDnsApply).not.toHaveBeenCalled();
    expect(
      within(dialog).getByText("DNS do adaptador {A} (IPv4): automático (DHCP) → 8.8.8.8, 8.8.4.4"),
    ).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));
    await waitFor(() => expect(api.netDnsApply).toHaveBeenCalledWith("google", false));
    expect(await screen.findByText("DNS aplicado. Desfaça pelo Histórico.")).toBeInTheDocument();
  });

  it("dns_apply_with_doh", async () => {
    vi.mocked(api.netDnsBench).mockResolvedValue(7);
    vi.mocked(api.netDnsPreview).mockResolvedValue([{ adapter: "Ethernet", changes: [] }]);
    vi.mocked(api.netDnsApply).mockResolvedValue([
      { tweak_id: "dns-a", ok: true, changed: true, message: "aplicada" },
    ]);
    render(<Rede />);
    await userEvent.click(await screen.findByRole("button", { name: "Medir DNS" }));
    await finish({
      op: "dns",
      report: {
        rows: [
          { name: "Google", addr: "8.8.8.8", median_ms: 5, answered: 9, sent: 9, current: false, resolver: "google" },
        ],
        best: "google",
        keep_current: false,
        advice: "Google respondeu mais rápido.",
      },
    });
    await userEvent.click(screen.getByLabelText("Criptografar (DNS sobre HTTPS)"));
    await userEvent.click(screen.getByRole("button", { name: "Usar o mais rápido" }));
    expect(api.netDnsPreview).toHaveBeenCalledWith("google", true);
    const dialog = await screen.findByRole("dialog", { name: "Usar o DNS Google criptografado?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));
    await waitFor(() => expect(api.netDnsApply).toHaveBeenCalledWith("google", true));
  });

  it("no DoH option for a server Windows cannot encrypt", async () => {
    vi.mocked(api.netDnsBench).mockResolvedValue(7);
    render(<Rede />);
    await userEvent.click(await screen.findByRole("button", { name: "Medir DNS" }));
    await finish({
      op: "dns",
      report: {
        rows: [
          { name: "AdGuard", addr: "94.140.14.14", median_ms: 5, answered: 9, sent: 9, current: false, resolver: "adguard" },
        ],
        best: "adguard",
        keep_current: false,
        advice: "AdGuard respondeu mais rápido.",
      },
    });
    expect(screen.getByRole("button", { name: "Usar o mais rápido" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Criptografar (DNS sobre HTTPS)")).toBeNull();
  });

  it("keeps the current DNS when it is the fastest", async () => {
    vi.mocked(api.netDnsBench).mockResolvedValue(7);
    render(<Rede />);
    await userEvent.click(await screen.findByRole("button", { name: "Medir DNS" }));
    await finish({
      op: "dns",
      report: {
        rows: [
          { name: "Atual (10.0.0.1)", addr: "10.0.0.1", median_ms: 1, answered: 9, sent: 9, current: true, resolver: null },
        ],
        best: "google",
        keep_current: true,
        advice: "O DNS que você usa já é o mais rápido. Não precisa mudar.",
      },
    });
    expect(screen.getByText(/já é o mais rápido/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Usar o mais rápido" })).toBeNull();
  });

  it("adapter_switch_asks_confirmation", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue({
      tweaks: [nicItem()],
      laptop: false,
      elevated: true,
      hkcu_allowed: true,
      journal_count: 0,
      restore_point_done: false,
    });
    vi.mocked(api.tweaksApply).mockResolvedValue(9);
    render(<Rede />);
    const sw = await screen.findByRole("switch", {
      name: "Adaptador Ethernet: sem economia de energia",
    });
    await userEvent.click(sw);
    expect(api.tweaksApply).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("PnPCapabilities: não existe → 24")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));
    expect(api.tweaksApply).toHaveBeenCalledWith(["nic-0a5c8bb6"]);
  });

  it("wifi_notice", async () => {
    vi.mocked(api.netPage).mockResolvedValue(
      netPage({
        adapters: [{ guid: "{W}", name: "Wi-Fi", wifi: true, has_ipv6: false, dns: [] }],
        wifi: { ssid: "Casa", signal_pct: 40, channel: 6 },
        wifi_notice: "Você está no Wi‑Fi \"Casa\" (sinal 40%). Pelo cabo a conexão é mais estável.",
      }),
    );
    render(<Rede />);
    expect(await screen.findByText(/Pelo cabo a conexão é mais estável/)).toBeInTheDocument();
    expect(screen.getByText('Rede "Casa", sinal 40%, canal 6 (2,4 GHz)')).toBeInTheDocument();
  });

  it("shows the history", async () => {
    vi.mocked(api.netPage).mockResolvedValue(
      netPage({
        history: [
          { at: "2026-11-23T12:00:00Z", kind: "dns", target: "DNS", summary: "mais rápido: Google (5,0 ms)" },
        ],
      }),
    );
    render(<Rede />);
    expect(await screen.findByText(/DNS · DNS: mais rápido: Google/)).toBeInTheDocument();
  });

  it("labels match the backend", () => {
    expect(labelOf(target("a.com"))).toBe("a.com");
    expect(labelOf(target("a.com", 80))).toBe("a.com:80");
    expect(labelOf(target("2001:db8::1", 80))).toBe("[2001:db8::1]:80");
  });
});
