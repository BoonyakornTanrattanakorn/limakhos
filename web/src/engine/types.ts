export type Color = "light" | "dark";
export type PieceKind = "man" | "king";

export interface PieceDto {
  square: number;
  color: Color;
  kind: PieceKind;
}

export interface MoveDto {
  from: number;
  to: number;
  path: number[];
  captures: number[];
  promotes: boolean;
  short_notation: string;
  long_notation: string;
  notation: string;
}

export type GameOutcome =
  | { type: "ongoing" }
  | { type: "win"; winner: Color; reason: "no_pieces" | "no_legal_moves" | "resignation" | "timeout" }
  | { type: "draw"; reason: "threefold_repetition" | "sixteen_move_rule" };

export interface PositionSnapshot {
  fen: string;
  side_to_move: Color;
  king_only_plies: number;
  fullmove_number: number;
  hash: string;
  pieces: PieceDto[];
  has_capture: boolean;
  legal_moves: MoveDto[];
  outcome: GameOutcome;
}

export interface CoreMove {
  from: number;
  to: number;
  path: number[];
  captures: number[];
  promotes: boolean;
}

export interface AnalysisLine {
  multipv: number;
  score_cp: number;
  score_light_cp: number;
  mate_in: number | null;
  best_move: CoreMove;
  pv: CoreMove[];
  pv_notations: string[];
}

export interface AnalysisResult {
  depth: number;
  nodes: number;
  lines: AnalysisLine[];
}

export interface AnalysisProgress {
  ok: "progress";
  depth: number;
  nodes: number;
  lines?: AnalysisLine[];
}

export type WasmRequest =
  | { cmd: "inspect"; fen?: string; history_hashes?: string[] }
  | { cmd: "apply_move"; fen: string; move: string; history_hashes?: string[] }
  | {
      cmd: "analyze";
      fen: string;
      max_depth?: number;
      max_nodes?: number;
      max_time_ms?: number;
      multipv?: number;
      history_hashes?: string[];
      root_moves?: string[];
      resume?: boolean;
    }
  | { cmd: "ai_move"; fen: string; level: number; seed?: number; history_hashes?: string[] }
  | {
      cmd: "export_pdn";
      initial_fen?: string;
      moves: string[];
      event?: string;
      light_player?: string;
      dark_player?: string;
    }
  | { cmd: "import_pdn"; pdn: string }
  | { cmd: "clear_cache" };

export type WasmResponse =
  | ({ ok: "snapshot" } & PositionSnapshot)
  | { ok: "move_applied"; notation: string; applied: MoveDto; snapshot: PositionSnapshot }
  | ({ ok: "analysis" } & AnalysisResult)
  | { ok: "ai_move"; chosen: MoveDto | null; analysis: AnalysisResult }
  | { ok: "pdn"; pdn: string }
  | {
      ok: "imported_pdn";
      initial_fen: string;
      notations: string[];
      fens: string[];
      hashes: string[];
      outcome: GameOutcome;
    }
  | { ok: "ack" }
  | { ok: "error"; message: string };
