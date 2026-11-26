import { describe, expect, it } from "vitest";
import { resolveApiBase, resolveWsUrl } from "./client";

describe("online client URL resolution", () => {
  it("resolves same-origin and cross-origin API bases", () => {
    expect(resolveApiBase("")).toBe("");
    expect(resolveApiBase("https://api.limakhos.org/")).toBe("https://api.limakhos.org");
  });

  it("maps HTTP/HTTPS API bases to WS/WSS URLs", () => {
    expect(resolveWsUrl("/ws/room/abc123", "https://api.limakhos.org")).toBe(
      "wss://api.limakhos.org/ws/room/abc123",
    );
    expect(resolveWsUrl("ws/room/abc123", "http://127.0.0.1:8080/")).toBe(
      "ws://127.0.0.1:8080/ws/room/abc123",
    );
  });

  it("derives WebSocket URL from browser location when VITE_API_BASE is unset", () => {
    expect(
      resolveWsUrl("/ws/room/xyz", "", { protocol: "https:", host: "play.limakhos.org" }),
    ).toBe("wss://play.limakhos.org/ws/room/xyz");
    expect(
      resolveWsUrl("/ws/room/xyz", "", { protocol: "http:", host: "localhost:5173" }),
    ).toBe("ws://localhost:5173/ws/room/xyz");
  });
});
