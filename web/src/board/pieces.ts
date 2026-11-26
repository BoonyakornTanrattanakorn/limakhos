import type { Color, PieceKind } from "../engine/types";

/**
 * Render a crisp, Lidraughts/Lichess-inspired turned-disc SVG piece
 * in a `0 0 100 100` coordinate space with deterministic gradient IDs.
 */
export function renderPieceSvg(color: Color, kind: PieceKind): string {
  const idSuffix = `${color}-${kind}`;
  const isLight = color === "light";
  const isKing = kind === "king";

  const gradId = `mk-grad-${idSuffix}`;
  const rimGradId = `mk-rim-${idSuffix}`;
  const crownGradId = `mk-crown-${idSuffix}`;

  // Palette tuned for crisp contrast across brown, blue, green, and walnut boards
  const stops = isLight
    ? {
        center: "#faf5ea",
        mid: "#e5d8bf",
        edge: "#cbb896",
        rimStroke: "#8f7b5d",
        rimBevelTop: "#fffdf7",
        rimBevelBottom: "#a89371",
        ringStroke: "#c9b898",
        ringHighlight: "rgba(255, 253, 245, 0.55)",
        specular: "rgba(255, 255, 255, 0.65)",
        underShadow: "rgba(92, 74, 48, 0.35)",
      }
    : {
        center: "#4a4c54",
        mid: "#26272c",
        edge: "#141518",
        rimStroke: "#5d606b",
        rimBevelTop: "#6d717e",
        rimBevelBottom: "#0a0b0d",
        ringStroke: "#3a3c44",
        ringHighlight: "rgba(255, 255, 255, 0.08)",
        specular: "rgba(255, 255, 255, 0.22)",
        underShadow: "rgba(0, 0, 0, 0.55)",
      };

  const kingDefs = isKing
    ? `
      <linearGradient id="${rimGradId}" x1="20%" y1="10%" x2="80%" y2="90%">
        <stop offset="0%" stop-color="#f2cd6b"/>
        <stop offset="50%" stop-color="#d9a441"/>
        <stop offset="100%" stop-color="#9e7220"/>
      </linearGradient>
      <linearGradient id="${crownGradId}" x1="25%" y1="15%" x2="75%" y2="90%">
        <stop offset="0%" stop-color="${isLight ? "#e5b246" : "#fae08c"}"/>
        <stop offset="55%" stop-color="${isLight ? "#d49a2a" : "#f0be4d"}"/>
        <stop offset="100%" stop-color="${isLight ? "#b57d18" : "#d69e2e"}"/>
      </linearGradient>`
    : "";

  const baseDisc = isKing
    ? `
      <!-- Subtle lower stacked disc lip for King -->
      <circle cx="50" cy="53" r="43.5" fill="${stops.underShadow}"/>
      <circle cx="50" cy="51.5" r="43.5" fill="${stops.rimBevelBottom}" stroke="${stops.rimStroke}" stroke-width="1.2"/>
      <!-- Upper turned disc -->
      <circle cx="50" cy="48.5" r="43.5" fill="url(#${gradId})" stroke="${stops.rimStroke}" stroke-width="2"/>
      <!-- Double-bevel golden accent ring -->
      <circle cx="50" cy="48.5" r="39.5" fill="none" stroke="${stops.ringStroke}" stroke-width="1.4" opacity="0.85"/>
      <circle cx="50" cy="48.5" r="35.5" fill="none" stroke="url(#${rimGradId})" stroke-width="2.6"/>
      <circle cx="50" cy="48.5" r="31.5" fill="none" stroke="${stops.ringHighlight}" stroke-width="1"/>
      <!-- Top-left specular arc -->
      <path d="M 20 33 A 34 34 0 0 1 65 18.5" fill="none" stroke="${stops.specular}" stroke-width="2.2" stroke-linecap="round"/>`
    : `
      <!-- Subtle lower rim edge thickness -->
      <circle cx="50" cy="52" r="43.5" fill="${stops.underShadow}"/>
      <!-- Main turned disc -->
      <circle cx="50" cy="50" r="43.5" fill="url(#${gradId})" stroke="${stops.rimStroke}" stroke-width="2"/>
      <!-- Inner rim highlight -->
      <circle cx="50" cy="50" r="40.8" fill="none" stroke="${stops.rimBevelTop}" stroke-width="0.9" opacity="0.35"/>
      <!-- 2 concentric lathe rings -->
      <circle cx="50" cy="50" r="33.5" fill="none" stroke="${stops.ringStroke}" stroke-width="1.7"/>
      <circle cx="50" cy="50" r="22.5" fill="none" stroke="${stops.ringStroke}" stroke-width="1.5"/>
      <circle cx="50" cy="50" r="32.2" fill="none" stroke="${stops.ringHighlight}" stroke-width="0.9"/>
      <!-- Top-left specular arc -->
      <path d="M 19.5 35 A 34 34 0 0 1 65 19.5" fill="none" stroke="${stops.specular}" stroke-width="2.4" stroke-linecap="round"/>`;

  const crownStroke = isLight ? "#8a6116" : "#3d2b07";
  const crownOverlay = isKing
    ? `
      <g transform="translate(0, -1.5)">
        <path
          d="M 33.5 59.5 L 29.5 40.5 L 40.5 48.5 L 50 34.5 L 59.5 48.5 L 70.5 40.5 L 66.5 59.5 Z"
          fill="url(#${crownGradId})"
          stroke="${crownStroke}"
          stroke-width="2"
          stroke-linejoin="round"
        />
        <path
          d="M 33.5 62.5 L 66.5 62.5 L 65.5 66.5 Q 50 68.5 34.5 66.5 Z"
          fill="url(#${crownGradId})"
          stroke="${crownStroke}"
          stroke-width="1.8"
          stroke-linejoin="round"
        />
        <circle cx="29.5" cy="38.5" r="2.3" fill="url(#${crownGradId})" stroke="${crownStroke}" stroke-width="1.5"/>
        <circle cx="50" cy="32" r="2.6" fill="url(#${crownGradId})" stroke="${crownStroke}" stroke-width="1.5"/>
        <circle cx="70.5" cy="38.5" r="2.3" fill="url(#${crownGradId})" stroke="${crownStroke}" stroke-width="1.5"/>
      </g>`
    : "";

  return `<svg class="piece-svg" viewBox="0 0 100 100" aria-hidden="true">
    <defs>
      <radialGradient id="${gradId}" cx="36%" cy="30%" r="68%" fx="34%" fy="28%">
        <stop offset="0%" stop-color="${stops.center}"/>
        <stop offset="58%" stop-color="${stops.mid}"/>
        <stop offset="100%" stop-color="${stops.edge}"/>
      </radialGradient>${kingDefs}
    </defs>
    ${baseDisc}
    ${crownOverlay}
  </svg>`;
}
