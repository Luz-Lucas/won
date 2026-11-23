import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import UpdatesBlockedNotice from "./UpdatesBlockedNotice";

vi.mock("../api", () => ({
  updatesMode: vi.fn(),
  updatesUpdateNow: vi.fn(),
}));

describe("UpdatesBlockedNotice", () => {
  beforeEach(() => {
    vi.mocked(api.updatesMode).mockReset();
    vi.mocked(api.updatesUpdateNow).mockReset();
  });

  it("apps_page_warns_when_blocked", async () => {
    vi.mocked(api.updatesMode).mockResolvedValue("blocked");
    vi.mocked(api.updatesUpdateNow).mockResolvedValue({} as never);
    render(<UpdatesBlockedNotice />);
    expect(await screen.findByText(/Microsoft Store/)).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Liberar temporariamente" }));
    expect(api.updatesUpdateNow).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Liberar" }));
    expect(api.updatesUpdateNow).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByText(/Microsoft Store/)).toBeNull());
  });

  it("silent in the other modes and on error", async () => {
    vi.mocked(api.updatesMode).mockResolvedValue("paused");
    const { unmount } = render(<UpdatesBlockedNotice />);
    await waitFor(() => expect(api.updatesMode).toHaveBeenCalled());
    expect(screen.queryByRole("status")).toBeNull();
    unmount();
    vi.mocked(api.updatesMode).mockRejectedValue("sem backend");
    render(<UpdatesBlockedNotice />);
    await waitFor(() => expect(api.updatesMode).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("status")).toBeNull();
  });
});
