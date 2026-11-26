import type { Color, GameOutcome, MoveDto, PieceDto } from "../engine/types";

export interface UserRecord {
  id: number;
  username: string;
  rating: number;
  rd: number;
  volatility: number;
  games_played: number;
  wins: number;
  losses: number;
  draws: number;
  created_at: number;
}

export interface AuthResponse {
  token: string;
  user: UserRecord;
}

export interface GameSummary {
  id: string;
  light_user_id: number | null;
  dark_user_id: number | null;
  light_name: string;
  dark_name: string;
  time_control: string;
  rated: boolean;
  result: string;
  reason: string;
  moves_count: number;
  light_rating_before: number | null;
  light_rating_after: number | null;
  dark_rating_before: number | null;
  dark_rating_after: number | null;
  created_at: number;
}

export interface GameDetail {
  summary: GameSummary;
  pdn: string;
}

export interface PlayerSeatDto {
  name: string;
  user_id: number | null;
  rating: number | null;
  connected: boolean;
}

export type RoomStatus = "waiting" | "active" | "finished";

export interface RoomSummary {
  id: string;
  time_control: string;
  initial_ms: number;
  increment_ms: number;
  rated: boolean;
  private: boolean;
  status: RoomStatus;
  light: PlayerSeatDto | null;
  dark: PlayerSeatDto | null;
  spectators: number;
  moves_count: number;
  created_at: number;
}

export interface CreateRoomResponse {
  room: RoomSummary;
  matched: boolean;
  assigned_color: Color | null;
  ws_path: string;
}

export interface RatingChangesDto {
  light_before: number;
  light_after: number;
  dark_before: number;
  dark_after: number;
}

export interface RoomStateSnapshot {
  room_id: string;
  status: RoomStatus;
  time_control: string;
  initial_ms: number;
  increment_ms: number;
  rated: boolean;
  private: boolean;
  light: PlayerSeatDto | null;
  dark: PlayerSeatDto | null;
  spectators: number;
  fen: string;
  side_to_move: Color;
  king_only_plies: number;
  fullmove_number: number;
  hash: string;
  pieces: PieceDto[];
  has_capture: boolean;
  legal_moves: MoveDto[];
  outcome: GameOutcome;
  notations: string[];
  last_move: { from: number; to: number; path: number[] } | null;
  clocks_ms: { light: number; dark: number };
  pdn: string;
  server_time_ms: number;
  rating_changes: RatingChangesDto | null;
}

export type ParticipantRole = "light" | "dark" | "spectator";

export type WsServerMessage =
  | {
      type: "welcome";
      room_id: string;
      your_role: ParticipantRole;
      your_name: string;
      state: RoomStateSnapshot;
    }
  | { type: "state"; state: RoomStateSnapshot }
  | { type: "error"; message: string }
  | { type: "pong"; server_time_ms: number };

export class OnlineApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export function resolveApiBase(configured?: string): string {
  const raw = (configured ?? import.meta.env?.VITE_API_BASE ?? "").trim();
  return raw.replace(/\/+$/, "");
}

export function resolveWsUrl(path: string, configuredBase?: string, loc?: Pick<Location, "protocol" | "host">): string {
  const base = resolveApiBase(configuredBase);
  const normalizedPath = path.startsWith("/") ? path : `/${path}`;
  if (base) {
    if (base.startsWith("https://")) return `wss://${base.slice("https://".length)}${normalizedPath}`;
    if (base.startsWith("http://")) return `ws://${base.slice("http://".length)}${normalizedPath}`;
    return `${base}${normalizedPath}`;
  }
  const locationObj = loc ?? window.location;
  const wsProto = locationObj.protocol === "https:" ? "wss:" : "ws:";
  return `${wsProto}//${locationObj.host}${normalizedPath}`;
}

export interface RoomConnection {
  sendMove: (moveNotation: string) => void;
  resign: () => void;
  sync: () => void;
  ping: () => void;
  close: () => void;
}

export interface RoomSocketHandlers {
  onWelcome: (msg: Extract<WsServerMessage, { type: "welcome" }>) => void;
  onState: (state: RoomStateSnapshot) => void;
  onError: (message: string) => void;
  onClose: (ev: CloseEvent) => void;
  onOpen?: () => void;
}

export class OnlineClient {
  private readonly apiBase: string;

  constructor(apiBase?: string) {
    this.apiBase = resolveApiBase(apiBase);
  }

  private async request<T>(path: string, init: RequestInit = {}, token?: string | null): Promise<T> {
    const headers = new Headers(init.headers);
    if (init.body && !headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }
    if (token) {
      headers.set("Authorization", `Bearer ${token}`);
    }

    const resp = await fetch(`${this.apiBase}${path}`, { ...init, headers });
    const text = await resp.text();
    let parsed: Record<string, unknown> | null = null;
    try {
      parsed = text ? (JSON.parse(text) as Record<string, unknown>) : null;
    } catch {
      parsed = null;
    }

    if (!resp.ok) {
      const msg =
        (parsed && typeof parsed.error === "string" && parsed.error) ||
        `HTTP ${resp.status}`;
      throw new OnlineApiError(resp.status, msg);
    }
    return parsed as T;
  }

  health(): Promise<{ status: string; version: string }> {
    return this.request("/api/health");
  }

  register(username: string, password: string): Promise<AuthResponse> {
    return this.request("/api/auth/register", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    });
  }

  login(username: string, password: string): Promise<AuthResponse> {
    return this.request("/api/auth/login", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    });
  }

  me(token: string): Promise<{ user: UserRecord }> {
    return this.request("/api/auth/me", {}, token);
  }

  async leaderboard(limit = 30): Promise<UserRecord[]> {
    const res = await this.request<{ players: UserRecord[] }>(`/api/leaderboard?limit=${limit}`);
    return res.players;
  }

  async recentGames(limit = 20): Promise<GameSummary[]> {
    const res = await this.request<{ games: GameSummary[] }>(`/api/games?limit=${limit}`);
    return res.games;
  }

  gamePdn(id: string): Promise<GameDetail> {
    return this.request(`/api/games/${encodeURIComponent(id)}/pdn`);
  }

  async listRooms(): Promise<RoomSummary[]> {
    const res = await this.request<{ rooms: RoomSummary[] }>("/api/rooms");
    return res.rooms;
  }

  createRoom(
    opts: {
      time_control: string;
      mode: "private" | "matchmake";
      rated?: boolean;
      color?: "light" | "dark" | "random";
      guest_name?: string;
    },
    token?: string | null,
  ): Promise<CreateRoomResponse> {
    return this.request(
      "/api/rooms",
      {
        method: "POST",
        body: JSON.stringify({
          time_control: opts.time_control,
          mode: opts.mode,
          private: opts.mode === "private",
          rated: opts.rated,
          color: opts.color,
          guest_name: opts.guest_name,
        }),
      },
      token,
    );
  }

  connectRoom(
    roomId: string,
    opts: { token?: string | null; guestName?: string; role?: "player" | "spectator" },
    handlers: RoomSocketHandlers,
  ): RoomConnection {
    const params = new URLSearchParams();
    if (opts.token) params.set("token", opts.token);
    if (opts.guestName) params.set("name", opts.guestName);
    if (opts.role) params.set("role", opts.role);
    const query = params.toString();
    const url = resolveWsUrl(`/ws/room/${encodeURIComponent(roomId)}${query ? `?${query}` : ""}`, this.apiBase);

    const ws = new WebSocket(url);
    ws.addEventListener("open", () => {
      handlers.onOpen?.();
    });
    ws.addEventListener("message", (ev) => {
      try {
        const msg = JSON.parse(String(ev.data)) as WsServerMessage;
        if (msg.type === "welcome") {
          handlers.onWelcome(msg);
        } else if (msg.type === "state") {
          handlers.onState(msg.state);
        } else if (msg.type === "error") {
          handlers.onError(msg.message);
        }
      } catch {
        handlers.onError("Invalid server message");
      }
    });
    ws.addEventListener("close", (ev) => {
      handlers.onClose(ev);
    });
    ws.addEventListener("error", () => {
      handlers.onError("WebSocket connection error");
    });

    const sendJson = (payload: Record<string, unknown>) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(payload));
      }
    };

    return {
      sendMove: (moveNotation: string) => sendJson({ type: "move", move: moveNotation }),
      resign: () => sendJson({ type: "resign" }),
      sync: () => sendJson({ type: "sync" }),
      ping: () => sendJson({ type: "ping" }),
      close: () => ws.close(),
    };
  }
}
