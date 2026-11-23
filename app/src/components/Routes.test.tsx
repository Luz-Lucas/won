import { act, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { Measurement, NetRoute, NetTargetItem, RoutesPage } from "../api";
import Routes from "./Routes";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  netRoutesPage: vi.fn(),
  netTunnelImport: vi.fn(),
  netTunnelRemove: vi.fn(),
  netRouteSet: vi.fn(),
  netGameRouteSet: vi.fn(),
  netWifiHoldSet: vi.fn(),
  netCompare: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
}));

const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const warp: NetRoute = {
  id: "warp",
  name: "Cloudflare WARP",
  kind: "warp",
  on: false,
  controllable: true,
  note: null,
};

const page = (over: Partial<RoutesPage> = {}): RoutesPage => ({
  routes: [warp],
  tunnels: [],
  games: [{ key: "d:\\jogos\\cs2.exe", name: "CS2", route: null }],
  wifi_hold: false,
  elevated: false,
  ...over,
});

const targets: NetTargetItem[] = [
  { name: "", host: "jogo.example", port: 27015, label: "jogo.example:27015", user: true },
];

const m = (median: number | null): Measurement => ({
  target: { name: "", host: "jogo.example", port: 27015 },
  addr: "203.0.113.9",
  sent: 30,
  received: 30,
  loss_pct: 0,
  min_ms: median,
  avg_ms: median,
  median_ms: median,
  max_ms: median,
  jitter_ms: 1,
  error: null,
});

describe("Routes", () => {
  beforeEach(() => {
    vi.mocked(api.netRoutesPage).mockReset().mockResolvedValue(page());
    for (const f of [
      api.netTunnelImport,
      api.netRouteSet,
      api.netGameRouteSet,
      api.netWifiHoldSet,
      api.netCompare,
    ])
      vi.mocked(f).mockReset();
    openDialog.mockReset();
    listeners.clear();
  });

  it("routes_list_and_compare", async () => {
    vi.mocked(api.netCompare).mockResolvedValue(7);
    render(<Routes targets={targets} />);
    expect(await screen.findByRole("switch", { name: "Cloudflare WARP" })).not.toBeChecked();
    const compare = screen.getByRole("button", { name: "Comparar rotas" });
    expect(compare).toBeDisabled();
    await userEvent.click(
      within(screen.getByRole("group", { name: "Rotas para comparar" })).getByLabelText(
        "Cloudflare WARP",
      ),
    );
    await userEvent.click(compare);
    expect(api.netCompare).toHaveBeenCalledWith("jogo.example:27015", ["warp"]);
    await act(async () => {
      listeners.get("job:done")!({
        payload: {
          job: 7,
          kind: "net",
          result: {
            op: "compare",
            comparison: {
              target: { name: "", host: "jogo.example", port: 27015 },
              rows: [
                { id: "normal", name: "Conexão atual", measurement: m(60) },
                { id: "warp", name: "Cloudflare WARP", measurement: m(40) },
              ],
              best: "warp",
              advice: "Cloudflare WARP foi mais rápido.",
              errors: [],
              interrupted: false,
            },
          },
        },
      });
    });
    const table = screen.getByLabelText("Comparação de rotas");
    const row = within(table).getByText(/recomendada/).closest("tr")!;
    expect(row).toHaveTextContent("Cloudflare WARP");
    expect(row).toHaveTextContent("40,0 ms");
    expect(within(table).getByText("Cloudflare WARP foi mais rápido.")).toBeInTheDocument();
  });

  it("tunnel_warning_before_enabling", async () => {
    vi.mocked(api.netRouteSet).mockResolvedValue(page({ routes: [{ ...warp, on: true }] }));
    render(<Routes targets={targets} />);
    await userEvent.click(await screen.findByRole("switch", { name: "Cloudflare WARP" }));
    expect(api.netRouteSet).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog", { name: "Ligar Cloudflare WARP?" });
    expect(dialog).toHaveTextContent("anti-cheats bloqueiam VPN");
    await userEvent.click(within(dialog).getByRole("button", { name: "Ligar" }));
    expect(api.netRouteSet).toHaveBeenCalledWith("warp", true);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Cloudflare WARP" })).toBeChecked(),
    );
    // Desligar não pede confirmação.
    vi.mocked(api.netRouteSet).mockResolvedValue(page());
    await userEvent.click(screen.getByRole("switch", { name: "Cloudflare WARP" }));
    expect(api.netRouteSet).toHaveBeenLastCalledWith("warp", false);
  });

  it("per_game_route_select", async () => {
    vi.mocked(api.netGameRouteSet).mockResolvedValue(
      page({ games: [{ key: "d:\\jogos\\cs2.exe", name: "CS2", route: "warp" }] }),
    );
    render(<Routes targets={targets} />);
    const select = await screen.findByRole("combobox", { name: "Rota de CS2" });
    await userEvent.selectOptions(select, "warp");
    expect(api.netGameRouteSet).toHaveBeenCalledWith("d:\\jogos\\cs2.exe", "warp");
    await waitFor(() => expect(select).toHaveValue("warp"));
    vi.mocked(api.netGameRouteSet).mockResolvedValue(page());
    await userEvent.selectOptions(select, "");
    expect(api.netGameRouteSet).toHaveBeenLastCalledWith("d:\\jogos\\cs2.exe", null);
  });

  it("wifi_hold_setting", async () => {
    vi.mocked(api.netWifiHoldSet).mockResolvedValue(page({ wifi_hold: true }));
    render(<Routes targets={targets} />);
    const box = await screen.findByLabelText(
      "Segurar a varredura do Wi‑Fi enquanto um jogo estiver aberto",
    );
    await userEvent.click(box);
    expect(api.netWifiHoldSet).toHaveBeenCalledWith(true);
    await waitFor(() => expect(box).toBeChecked());
  });

  it("imports a tunnel with a name", async () => {
    vi.mocked(api.netTunnelImport).mockResolvedValue(page());
    openDialog.mockResolvedValue("C:\\Users\\x\\vps.conf");
    render(<Routes targets={targets} />);
    await userEvent.click(await screen.findByRole("button", { name: "Importar .conf…" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Dê um nome ao túnel");
    expect(openDialog).not.toHaveBeenCalled();
    await userEvent.type(screen.getByLabelText("Nome do túnel"), "vps-sp");
    await userEvent.click(screen.getByRole("button", { name: "Importar .conf…" }));
    expect(api.netTunnelImport).toHaveBeenCalledWith("vps-sp", "C:\\Users\\x\\vps.conf");
  });

  it("shows when nothing is installed", async () => {
    vi.mocked(api.netRoutesPage).mockResolvedValue(page({ routes: [] }));
    render(<Routes targets={targets} />);
    expect(await screen.findByText(/Nenhuma alternativa instalada/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Comparar rotas" })).toBeNull();
  });
});
