import type { Color, MoveDto, PieceDto } from "../engine/types";
import {
  MAIN_DIAGONAL_SQUARES,
  displayCellToSquare,
  squareToDisplayCell,
  squareToSvgCenter,
} from "./coords";

export interface BoardArrow {
  from: number;
  to: number;
  path?: number[];
  color: string;
  opacity?: number;
  width?: number;
}

export interface BoardState {
  pieces: PieceDto[];
  legalMoves: MoveDto[];
  hasCapture: boolean;
  orientation: Color;
  showNumbers: boolean;
  showMainDiagonal: boolean;
  interactive: boolean;
  lastMove?: { from: number; to: number; path?: number[] } | null;
  arrows?: BoardArrow[];
}

export type OnMoveSelected = (move: MoveDto) => void;

/**
 * Purpose-built 8×8 Thai Draughts (Makhos 8) interactive board.
 * Supports TCA 2025 `1–32` numbering, step-by-step multi-jump disambiguation,
 * drag-and-drop + click play, and SVG analysis arrows.
 */
export class MakhosBoard {
  private root: HTMLElement;
  private gridEl: HTMLDivElement;
  private piecesLayer: HTMLDivElement;
  private svgLayer: SVGSVGElement;
  private state: BoardState;
  private onMove: OnMoveSelected;

  /** Current multi-step prefix selected by the user (e.g. `[5]` or `[5, 14]`). */
  private selectedPrefix: number[] = [];
  private dragging: {
    sq: number;
    el: HTMLDivElement;
    pointerId: number;
  } | null = null;

  constructor(container: HTMLElement, initialState: BoardState, onMove: OnMoveSelected) {
    this.root = container;
    this.state = initialState;
    this.onMove = onMove;

    this.root.classList.add("makhos-board-shell");
    this.root.innerHTML = "";

    const boardFrame = document.createElement("div");
    boardFrame.className = "makhos-board";

    this.gridEl = document.createElement("div");
    this.gridEl.className = "makhos-grid";

    this.piecesLayer = document.createElement("div");
    this.piecesLayer.className = "makhos-pieces";

    this.svgLayer = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    this.svgLayer.setAttribute("class", "makhos-arrows");
    this.svgLayer.setAttribute("viewBox", "0 0 800 800");

    boardFrame.append(this.gridEl, this.piecesLayer, this.svgLayer);
    this.root.appendChild(boardFrame);

    this.gridEl.addEventListener("pointerdown", (ev) => this.handleGridPointerDown(ev));
    window.addEventListener("pointermove", this.handlePointerMove);
    window.addEventListener("pointerup", this.handlePointerUp);

    this.render();
  }

  setState(next: Partial<BoardState>): void {
    const positionChanged = next.pieces !== undefined || next.legalMoves !== undefined;
    this.state = { ...this.state, ...next };
    if (positionChanged) {
      this.selectedPrefix = [];
    }
    this.render();
  }

  destroy(): void {
    window.removeEventListener("pointermove", this.handlePointerMove);
    window.removeEventListener("pointerup", this.handlePointerUp);
  }

  /** Moves that start with the currently selected prefix. */
  private matchingMoves(): MoveDto[] {
    if (this.selectedPrefix.length === 0) return [];
    return this.state.legalMoves.filter((m) => {
      if (m.path.length < this.selectedPrefix.length) return false;
      for (let i = 0; i < this.selectedPrefix.length; i++) {
        if (m.path[i] !== this.selectedPrefix[i]) return false;
      }
      return true;
    });
  }

  private handleGridPointerDown(ev: PointerEvent): void {
    if (!this.state.interactive) return;
    const sq = this.squareFromClientPoint(ev.clientX, ev.clientY);
    if (sq === null) {
      this.selectedPrefix = [];
      this.render();
      return;
    }

    // 1. If a piece is already selected, check if clicking `sq` advances or completes a legal move
    if (this.selectedPrefix.length > 0) {
      const candidates = this.matchingMoves();
      const stepIdx = this.selectedPrefix.length;

      // Exact final-destination match (only if unique among candidates)
      const exactFinal = candidates.filter((m) => m.to === sq);
      if (exactFinal.length === 1 && !candidates.some((m) => m.path[stepIdx] === sq && m.path.length > stepIdx + 1)) {
        this.commitMove(exactFinal[0]);
        return;
      }

      // Next intermediate jump step match
      const stepMatches = candidates.filter((m) => m.path[stepIdx] === sq);
      if (stepMatches.length === 1 && stepMatches[0].path.length === stepIdx + 1) {
        this.commitMove(stepMatches[0]);
        return;
      }
      if (stepMatches.length > 0) {
        this.selectedPrefix = [...this.selectedPrefix, sq];
        this.render();
        return;
      }
    }

    // 2. Otherwise, try selecting a movable piece on `sq`
    const movesFromSq = this.state.legalMoves.filter((m) => m.from === sq);
    if (movesFromSq.length > 0) {
      this.selectedPrefix = [sq];
      this.render();

      // Begin drag gesture if pointer started on the piece
      const pieceEl = this.piecesLayer.querySelector<HTMLDivElement>(`[data-square="${sq}"]`);
      if (pieceEl) {
        this.dragging = { sq, el: pieceEl, pointerId: ev.pointerId };
        pieceEl.classList.add("is-dragging");
        this.moveDraggedPiece(ev.clientX, ev.clientY);
      }
    } else {
      this.selectedPrefix = [];
      this.render();
    }
  }

  private handlePointerMove = (ev: PointerEvent): void => {
    if (!this.dragging || ev.pointerId !== this.dragging.pointerId) return;
    this.moveDraggedPiece(ev.clientX, ev.clientY);
  };

  private handlePointerUp = (ev: PointerEvent): void => {
    if (!this.dragging || ev.pointerId !== this.dragging.pointerId) return;
    const { sq, el } = this.dragging;
    el.classList.remove("is-dragging");
    el.style.transform = "";
    this.dragging = null;

    const dropSq = this.squareFromClientPoint(ev.clientX, ev.clientY);
    if (dropSq === null || dropSq === sq) {
      this.render();
      return;
    }

    const candidates = this.matchingMoves();
    const stepIdx = this.selectedPrefix.length;
    const finalMatches = candidates.filter((m) => m.to === dropSq);
    if (finalMatches.length === 1) {
      this.commitMove(finalMatches[0]);
      return;
    }

    const stepMatches = candidates.filter((m) => m.path[stepIdx] === dropSq);
    if (stepMatches.length === 1 && stepMatches[0].path.length === stepIdx + 1) {
      this.commitMove(stepMatches[0]);
      return;
    }
    if (stepMatches.length > 0) {
      this.selectedPrefix = [...this.selectedPrefix, dropSq];
      this.render();
      return;
    }
    this.render();
  };

  private moveDraggedPiece(clientX: number, clientY: number): void {
    if (!this.dragging) return;
    const rect = this.gridEl.getBoundingClientRect();
    const cellW = rect.width / 8;
    const cellH = rect.height / 8;
    const x = clientX - rect.left - cellW / 2;
    const y = clientY - rect.top - cellH / 2;
    this.dragging.el.style.left = `${(x / rect.width) * 100}%`;
    this.dragging.el.style.top = `${(y / rect.height) * 100}%`;
  }

  private squareFromClientPoint(clientX: number, clientY: number): number | null {
    const rect = this.gridEl.getBoundingClientRect();
    if (
      clientX < rect.left ||
      clientX >= rect.right ||
      clientY < rect.top ||
      clientY >= rect.bottom
    ) {
      return null;
    }
    const displayCol = Math.floor(((clientX - rect.left) / rect.width) * 8);
    const displayRow = Math.floor(((clientY - rect.top) / rect.height) * 8);
    return displayCellToSquare(displayRow, displayCol, this.state.orientation);
  }

  private commitMove(move: MoveDto): void {
    this.selectedPrefix = [];
    this.onMove(move);
  }

  private render(): void {
    this.renderGrid();
    this.renderPieces();
    this.renderArrows();
  }

  private renderGrid(): void {
    const candidates = this.matchingMoves();
    const stepIdx = this.selectedPrefix.length;

    const movableFrom = new Set(this.state.legalMoves.map((m) => m.from));
    const nextStepTargets = new Set<number>();
    const finalTargets = new Set<number>();
    for (const m of candidates) {
      if (m.path[stepIdx] !== undefined) nextStepTargets.add(m.path[stepIdx]);
      finalTargets.add(m.to);
    }

    const lastSquares = new Set<number>();
    if (this.state.lastMove) {
      lastSquares.add(this.state.lastMove.from);
      lastSquares.add(this.state.lastMove.to);
    }

    const selectedSq = this.selectedPrefix[this.selectedPrefix.length - 1] ?? null;

    this.gridEl.innerHTML = "";
    for (let displayRow = 0; displayRow < 8; displayRow++) {
      for (let displayCol = 0; displayCol < 8; displayCol++) {
        const sq = displayCellToSquare(displayRow, displayCol, this.state.orientation);
        const cell = document.createElement("div");
        const isDark = sq !== null;
        cell.className = `makhos-square ${isDark ? "is-dark" : "is-light"}`;

        if (sq !== null) {
          cell.dataset.square = String(sq);

          if (this.state.showMainDiagonal && MAIN_DIAGONAL_SQUARES.has(sq)) {
            cell.classList.add("is-main-diag");
          }
          if (lastSquares.has(sq)) {
            cell.classList.add("is-last-move");
          }
          if (sq === selectedSq || this.selectedPrefix.includes(sq)) {
            cell.classList.add("is-selected");
          }
          if (this.state.interactive && this.state.hasCapture && movableFrom.has(sq)) {
            cell.classList.add("has-mandatory-capture");
          }

          if (this.state.showNumbers) {
            const num = document.createElement("span");
            num.className = "sq-num";
            num.textContent = String(sq);
            cell.appendChild(num);
          }

          if (nextStepTargets.has(sq) || finalTargets.has(sq)) {
            const dot = document.createElement("span");
            dot.className = finalTargets.has(sq) ? "move-dot is-final" : "move-dot is-step";
            cell.appendChild(dot);
          }
        }

        this.gridEl.appendChild(cell);
      }
    }
  }

  private renderPieces(): void {
    const movableFrom = new Set(this.state.legalMoves.map((m) => m.from));
    this.piecesLayer.innerHTML = "";

    for (const p of this.state.pieces) {
      const { displayRow, displayCol } = squareToDisplayCell(p.square, this.state.orientation);
      const el = document.createElement("div");
      el.className = `makhos-piece is-${p.color} is-${p.kind}`;
      if (this.state.interactive && movableFrom.has(p.square)) {
        el.classList.add("is-movable");
      }
      el.dataset.square = String(p.square);
      el.style.left = `${displayCol * 12.5}%`;
      el.style.top = `${displayRow * 12.5}%`;

      if (p.kind === "king") {
        const crown = document.createElement("span");
        crown.className = "king-crown";
        crown.textContent = "★";
        el.appendChild(crown);
      }

      this.piecesLayer.appendChild(el);
    }
  }

  private renderArrows(): void {
    const arrows: BoardArrow[] = [...(this.state.arrows ?? [])];

    // Preview selected multi-jump path
    if (this.selectedPrefix.length >= 2) {
      for (let i = 0; i < this.selectedPrefix.length - 1; i++) {
        arrows.push({
          from: this.selectedPrefix[i],
          to: this.selectedPrefix[i + 1],
          color: "#f59e0b",
          opacity: 0.9,
          width: 10,
        });
      }
    }

    this.svgLayer.innerHTML = "";
    if (arrows.length === 0) return;

    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    this.svgLayer.appendChild(defs);

    arrows.forEach((arrow, idx) => {
      const markerId = `arrow-head-${idx}`;
      const marker = document.createElementNS("http://www.w3.org/2000/svg", "marker");
      marker.setAttribute("id", markerId);
      marker.setAttribute("markerWidth", "4");
      marker.setAttribute("markerHeight", "4");
      marker.setAttribute("refX", "2.5");
      marker.setAttribute("refY", "2");
      marker.setAttribute("orient", "auto");

      const head = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
      head.setAttribute("points", "0 0, 4 2, 0 4");
      head.setAttribute("fill", arrow.color);
      marker.appendChild(head);
      defs.appendChild(marker);

      const points =
        arrow.path && arrow.path.length >= 2 ? arrow.path : [arrow.from, arrow.to];

      for (let s = 0; s < points.length - 1; s++) {
        const a = squareToSvgCenter(points[s], this.state.orientation);
        const b = squareToSvgCenter(points[s + 1], this.state.orientation);
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const len = Math.hypot(dx, dy) || 1;
        const padEnd = s === points.length - 2 ? 22 : 6;
        const padStart = s === 0 ? 12 : 6;
        const x1 = a.x + (dx / len) * padStart;
        const y1 = a.y + (dy / len) * padStart;
        const x2 = b.x - (dx / len) * padEnd;
        const y2 = b.y - (dy / len) * padEnd;

        const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
        line.setAttribute("x1", String(x1));
        line.setAttribute("y1", String(y1));
        line.setAttribute("x2", String(x2));
        line.setAttribute("y2", String(y2));
        line.setAttribute("stroke", arrow.color);
        line.setAttribute("stroke-width", String(arrow.width ?? 14));
        line.setAttribute("stroke-linecap", "round");
        line.setAttribute("opacity", String(arrow.opacity ?? 0.78));
        if (s === points.length - 2) {
          line.setAttribute("marker-end", `url(#${markerId})`);
        }
        this.svgLayer.appendChild(line);
      }
    });
  }
}
