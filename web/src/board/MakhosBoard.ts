import type { Color, MoveDto, PieceDto } from "../engine/types";
import {
  MAIN_DIAGONAL_SQUARES,
  displayCellToSquare,
  squareToDisplayCell,
  squareToRowCol,
} from "./coords";
import { renderPieceSvg } from "./pieces";

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

interface LogicalCell {
  row: number;
  col: number;
}

interface UserCircle {
  cell: LogicalCell;
  color: string;
}

interface UserArrow {
  from: LogicalCell;
  to: LogicalCell;
  color: string;
}

/**
 * Purpose-built 8×8 Thai Draughts (Makhos 8) interactive board.
 * Supports TCA 2025 `1–32` numbering, step-by-step multi-jump disambiguation,
 * drag-and-drop + click play, Lichess right-click annotations, and SVG analysis arrows.
 */
export class MakhosBoard {
  private root: HTMLElement;
  private boardFrame: HTMLDivElement;
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

  /** User-drawn Lichess-style right-click square circles & arrows. */
  private userCircles: UserCircle[] = [];
  private userArrows: UserArrow[] = [];
  private rightDrawing: {
    from: LogicalCell;
    pointerId: number;
    downColor: string;
  } | null = null;

  constructor(container: HTMLElement, initialState: BoardState, onMove: OnMoveSelected) {
    this.root = container;
    this.state = initialState;
    this.onMove = onMove;

    this.root.classList.add("makhos-board-shell");
    this.root.innerHTML = "";

    this.boardFrame = document.createElement("div");
    this.boardFrame.className = "makhos-board";

    this.gridEl = document.createElement("div");
    this.gridEl.className = "makhos-grid";

    this.piecesLayer = document.createElement("div");
    this.piecesLayer.className = "makhos-pieces";

    this.svgLayer = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    this.svgLayer.setAttribute("class", "makhos-arrows");
    this.svgLayer.setAttribute("viewBox", "0 0 800 800");

    this.boardFrame.append(this.gridEl, this.piecesLayer, this.svgLayer);
    this.root.appendChild(this.boardFrame);

    this.gridEl.addEventListener("pointerdown", this.handleGridPointerDown);
    this.boardFrame.addEventListener("contextmenu", this.handleContextMenu);
    window.addEventListener("pointermove", this.handlePointerMove);
    window.addEventListener("pointerup", this.handlePointerUp);

    this.render();
  }

  setState(next: Partial<BoardState>): void {
    const positionChanged =
      (next.pieces !== undefined && next.pieces !== this.state.pieces) ||
      (next.legalMoves !== undefined && next.legalMoves !== this.state.legalMoves);
    this.state = { ...this.state, ...next };
    if (positionChanged) {
      this.selectedPrefix = [];
      if (this.dragging) {
        this.dragging.el.classList.remove("is-dragging");
        this.dragging = null;
      }
    }
    this.render();
  }

  /**
   * Update only the SVG arrows overlay without touching pieces, grid squares,
   * active selection, or an in-progress piece drag gesture.
   */
  setArrows(arrows: BoardArrow[]): void {
    this.state = { ...this.state, arrows };
    this.renderArrows();
  }

  destroy(): void {
    this.gridEl.removeEventListener("pointerdown", this.handleGridPointerDown);
    this.boardFrame.removeEventListener("contextmenu", this.handleContextMenu);
    window.removeEventListener("pointermove", this.handlePointerMove);
    window.removeEventListener("pointerup", this.handlePointerUp);
  }

  private handleContextMenu = (ev: MouseEvent): void => {
    ev.preventDefault();
  };

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

  private annotationColor(ev: PointerEvent | MouseEvent): string {
    if (ev.ctrlKey || (ev.shiftKey && ev.altKey)) return "#e68f00";
    if (ev.shiftKey) return "#882020";
    if (ev.altKey) return "#003088";
    return "#15781B";
  }

  private hasModifierKey(ev: PointerEvent | MouseEvent): boolean {
    return ev.ctrlKey || ev.shiftKey || ev.altKey;
  }

  private handleGridPointerDown = (ev: PointerEvent): void => {
    // Right-click starts a square mark or arrow annotation
    if (ev.button === 2) {
      const cell = this.logicalCellFromClientPoint(ev.clientX, ev.clientY);
      if (!cell) return;
      this.rightDrawing = {
        from: cell,
        pointerId: ev.pointerId,
        downColor: this.annotationColor(ev),
      };
      return;
    }

    // Only left-click interacts with pieces and clears user annotations
    if (ev.button !== 0) return;

    if (this.userCircles.length > 0 || this.userArrows.length > 0) {
      this.userCircles = [];
      this.userArrows = [];
      if (!this.state.interactive) {
        this.renderArrows();
        return;
      }
    }

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
      if (
        exactFinal.length === 1 &&
        !candidates.some((m) => m.path[stepIdx] === sq && m.path.length > stepIdx + 1)
      ) {
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
  };

  private handlePointerMove = (ev: PointerEvent): void => {
    if (!this.dragging || ev.pointerId !== this.dragging.pointerId) return;
    this.moveDraggedPiece(ev.clientX, ev.clientY);
  };

  private handlePointerUp = (ev: PointerEvent): void => {
    if (this.rightDrawing && ev.pointerId === this.rightDrawing.pointerId) {
      const { from, downColor } = this.rightDrawing;
      this.rightDrawing = null;

      const to = this.logicalCellFromClientPoint(ev.clientX, ev.clientY);
      if (!to) return;

      const color = this.hasModifierKey(ev) ? this.annotationColor(ev) : downColor;
      if (from.row === to.row && from.col === to.col) {
        this.toggleUserCircle(from, color);
      } else {
        this.toggleUserArrow(from, to, color);
      }
      this.renderArrows();
      return;
    }

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

  private toggleUserCircle(cell: LogicalCell, color: string): void {
    const idx = this.userCircles.findIndex(
      (c) => c.cell.row === cell.row && c.cell.col === cell.col,
    );
    if (idx === -1) {
      this.userCircles.push({ cell, color });
    } else if (this.userCircles[idx].color === color) {
      this.userCircles.splice(idx, 1);
    } else {
      this.userCircles[idx].color = color;
    }
  }

  private toggleUserArrow(from: LogicalCell, to: LogicalCell, color: string): void {
    const idx = this.userArrows.findIndex(
      (a) =>
        a.from.row === from.row &&
        a.from.col === from.col &&
        a.to.row === to.row &&
        a.to.col === to.col,
    );
    if (idx === -1) {
      this.userArrows.push({ from, to, color });
    } else if (this.userArrows[idx].color === color) {
      this.userArrows.splice(idx, 1);
    } else {
      this.userArrows[idx].color = color;
    }
  }

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

  private logicalCellFromClientPoint(clientX: number, clientY: number): LogicalCell | null {
    const rect = this.gridEl.getBoundingClientRect();
    if (
      clientX < rect.left ||
      clientX >= rect.right ||
      clientY < rect.top ||
      clientY >= rect.bottom
    ) {
      return null;
    }
    const displayCol = Math.min(7, Math.max(0, Math.floor(((clientX - rect.left) / rect.width) * 8)));
    const displayRow = Math.min(7, Math.max(0, Math.floor(((clientY - rect.top) / rect.height) * 8)));
    const row = this.state.orientation === "light" ? 7 - displayRow : displayRow;
    const col = this.state.orientation === "light" ? displayCol : 7 - displayCol;
    return { row, col };
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
    this.boardFrame.classList.toggle("is-interactive", this.state.interactive);
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
          if (this.state.interactive && movableFrom.has(sq)) {
            cell.classList.add("is-movable-square");
            if (this.state.hasCapture) {
              cell.classList.add("has-mandatory-capture");
            }
          }

          if (this.state.showNumbers) {
            const num = document.createElement("span");
            num.className = "sq-num";
            num.textContent = String(sq);
            cell.appendChild(num);
          }

          if (nextStepTargets.has(sq) || finalTargets.has(sq)) {
            cell.classList.add("is-legal-dest");
            cell.dataset.legalDest = "true";
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
      el.innerHTML = renderPieceSvg(p.color, p.kind);

      this.piecesLayer.appendChild(el);
    }
  }

  private cellCenterSvg(cell: LogicalCell): { x: number; y: number } {
    const displayRow = this.state.orientation === "light" ? 7 - cell.row : cell.row;
    const displayCol = this.state.orientation === "light" ? cell.col : 7 - cell.col;
    return {
      x: displayCol * 100 + 50,
      y: displayRow * 100 + 50,
    };
  }

  private renderArrows(): void {
    type SvgArrowSpec = {
      points: { x: number; y: number }[];
      color: string;
      opacity: number;
      width: number;
    };

    const arrows: SvgArrowSpec[] = [];

    for (const arrow of this.state.arrows ?? []) {
      const sqPoints = arrow.path && arrow.path.length >= 2 ? arrow.path : [arrow.from, arrow.to];
      if (sqPoints.length < 2) continue;
      if (sqPoints.length === 2 && sqPoints[0] === sqPoints[1]) continue;
      arrows.push({
        points: sqPoints.map((sq) => this.cellCenterSvg(squareToRowCol(sq))),
        color: arrow.color,
        opacity: arrow.opacity ?? 0.78,
        width: arrow.width ?? 13,
      });
    }

    // Preview selected multi-jump path
    if (this.selectedPrefix.length >= 2) {
      arrows.push({
        points: this.selectedPrefix.map((sq) => this.cellCenterSvg(squareToRowCol(sq))),
        color: "#e68f00",
        opacity: 0.9,
        width: 11,
      });
    }

    // User right-click drawn arrows
    for (const ua of this.userArrows) {
      arrows.push({
        points: [this.cellCenterSvg(ua.from), this.cellCenterSvg(ua.to)],
        color: ua.color,
        opacity: 0.72,
        width: 13,
      });
    }

    this.svgLayer.innerHTML = "";
    if (arrows.length === 0 && this.userCircles.length === 0) return;

    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    this.svgLayer.appendChild(defs);

    // Render user-drawn square circles beneath arrows
    for (const uc of this.userCircles) {
      const center = this.cellCenterSvg(uc.cell);
      const ring = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      ring.setAttribute("cx", String(center.x));
      ring.setAttribute("cy", String(center.y));
      ring.setAttribute("r", "44");
      ring.setAttribute("fill", "none");
      ring.setAttribute("stroke", uc.color);
      ring.setAttribute("stroke-width", "6.5");
      ring.setAttribute("opacity", "0.78");
      this.svgLayer.appendChild(ring);
    }

    // Render arrows with crisp userSpaceOnUse markers & group opacity
    arrows.forEach((arrow, idx) => {
      const pts = arrow.points;
      if (pts.length < 2) return;

      const w = arrow.width;
      const headLen = Math.round(w * 2.15);
      const headWidth = Math.round(w * 2.5);
      const refX = headLen * 0.2;

      const markerId = `mk-arrow-head-${idx}`;
      const marker = document.createElementNS("http://www.w3.org/2000/svg", "marker");
      marker.setAttribute("id", markerId);
      marker.setAttribute("markerUnits", "userSpaceOnUse");
      marker.setAttribute("markerWidth", String(headLen));
      marker.setAttribute("markerHeight", String(headWidth));
      marker.setAttribute("refX", String(refX));
      marker.setAttribute("refY", String(headWidth / 2));
      marker.setAttribute("orient", "auto");

      const head = document.createElementNS("http://www.w3.org/2000/svg", "polygon");
      head.setAttribute("points", `0 0, ${headLen} ${headWidth / 2}, 0 ${headWidth}`);
      head.setAttribute("fill", arrow.color);
      marker.appendChild(head);
      defs.appendChild(marker);

      const first = pts[0];
      const second = pts[1];
      const dStartX = second.x - first.x;
      const dStartY = second.y - first.y;
      const startLen = Math.hypot(dStartX, dStartY) || 1;
      const padStart = Math.min(14, startLen * 0.25);

      const last = pts[pts.length - 1];
      const prev = pts[pts.length - 2];
      const dEndX = last.x - prev.x;
      const dEndY = last.y - prev.y;
      const endLen = Math.hypot(dEndX, dEndY) || 1;
      const tipOffset = 14;
      const padEnd = Math.min(tipOffset + (headLen - refX), endLen * 0.45);

      const adjusted: { x: number; y: number }[] = pts.map((p, pIdx) => {
        if (pIdx === 0) {
          return {
            x: first.x + (dStartX / startLen) * padStart,
            y: first.y + (dStartY / startLen) * padStart,
          };
        }
        if (pIdx === pts.length - 1) {
          return {
            x: last.x - (dEndX / endLen) * padEnd,
            y: last.y - (dEndY / endLen) * padEnd,
          };
        }
        return p;
      });

      const d = adjusted
        .map((p, pIdx) => `${pIdx === 0 ? "M" : "L"} ${p.x.toFixed(1)} ${p.y.toFixed(1)}`)
        .join(" ");

      const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
      group.setAttribute("opacity", String(arrow.opacity));

      const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", d);
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", arrow.color);
      path.setAttribute("stroke-width", String(w));
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
      path.setAttribute("marker-end", `url(#${markerId})`);

      group.appendChild(path);
      this.svgLayer.appendChild(group);
    });
  }
}
