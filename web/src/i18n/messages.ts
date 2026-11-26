export type Locale = "th" | "en";

export interface Messages {
  appTitle: string;
  subtitle: string;
  modeAnalysis: string;
  modeAi: string;
  modeLocal: string;
  lightSide: string;
  darkSide: string;
  aiLevel: string;
  timeControl: string;
  unlimited: string;
  newGame: string;
  flipBoard: string;
  numbersToggle: string;
  mainDiagToggle: string;
  undo: string;
  resign: string;
  engineAnalysis: string;
  engineOn: string;
  engineOff: string;
  engineThinking: string;
  depth: string;
  nodes: string;
  nps: string;
  bestLines: string;
  mandatoryCapture: string;
  sixteenMoveCounter: string;
  turnLight: string;
  turnDark: string;
  movesHistory: string;
  fenPdn: string;
  loadFen: string;
  copyFen: string;
  importPdn: string;
  exportPdn: string;
  copied: string;
  outcomeWinNoPieces: string;
  outcomeWinNoMoves: string;
  outcomeWinResign: string;
  outcomeWinTimeout: string;
  outcomeDrawRepetition: string;
  outcomeDraw16Moves: string;
  winnerLight: string;
  winnerDark: string;
  drawLabel: string;
  playAs: string;
  noMovesYet: string;
}

export const MESSAGES: Record<Locale, Messages> = {
  en: {
    appTitle: "Limakhos",
    subtitle: "Makhos 8 (TCA 2025) • Local WASM Engine",
    modeAnalysis: "Analysis",
    modeAi: "Play vs AI",
    modeLocal: "Local 2P",
    lightSide: "Light (1–8, First)",
    darkSide: "Dark (25–32)",
    aiLevel: "AI Level",
    timeControl: "Time Control",
    unlimited: "Unlimited",
    newGame: "New Game",
    flipBoard: "Flip",
    numbersToggle: "1–32",
    mainDiagToggle: "4–29",
    undo: "Undo",
    resign: "Resign",
    engineAnalysis: "Local WASM Analysis",
    engineOn: "Engine ON",
    engineOff: "Engine OFF",
    engineThinking: "Analyzing...",
    depth: "Depth",
    nodes: "Nodes",
    nps: "NPS",
    bestLines: "Top Lines (Multi-PV)",
    mandatoryCapture: "Mandatory Capture!",
    sixteenMoveCounter: "16-Move Draw Rule",
    turnLight: "Light to move",
    turnDark: "Dark to move",
    movesHistory: "Moves (TCA Notation)",
    fenPdn: "FEN / PDN",
    loadFen: "Load FEN",
    copyFen: "Copy FEN",
    importPdn: "Import PDN",
    exportPdn: "Copy PDN",
    copied: "Copied!",
    outcomeWinNoPieces: "no pieces left",
    outcomeWinNoMoves: "no legal moves",
    outcomeWinResign: "resignation",
    outcomeWinTimeout: "time expired",
    outcomeDrawRepetition: "Draw by threefold repetition (Art. 13)",
    outcomeDraw16Moves: "Automatic draw by 16-move rule (Art. 14)",
    winnerLight: "Light wins",
    winnerDark: "Dark wins",
    drawLabel: "Draw",
    playAs: "Play as",
    noMovesYet: "No moves yet — Light moves first.",
  },
  th: {
    appTitle: "Limakhos",
    subtitle: "หมากฮอส 8 ตัว (กติกา พ.ศ. 2568) • วิเคราะห์ในเบราว์เซอร์",
    modeAnalysis: "กระดานวิเคราะห์",
    modeAi: "เล่นกับบอท AI",
    modeLocal: "เล่น 2 คน",
    lightSide: "ฝ่ายสีอ่อน (1–8, เดินก่อน)",
    darkSide: "ฝ่ายสีเข้ม (25–32)",
    aiLevel: "ระดับ AI",
    timeControl: "เวลาการแข่งขัน",
    unlimited: "ไม่จำกัดเวลา",
    newGame: "เริ่มเกมใหม่",
    flipBoard: "กลับกระดาน",
    numbersToggle: "เลข 1–32",
    mainDiagToggle: "แกน 4–29",
    undo: "ย้อนกลับ",
    resign: "ยอมแพ้",
    engineAnalysis: "วิเคราะห์ด้วยเอนจิน WASM",
    engineOn: "เปิดเอนจิน",
    engineOff: "ปิดเอนจิน",
    engineThinking: "กำลังวิเคราะห์...",
    depth: "ความลึก",
    nodes: "โหนด",
    nps: "โหนด/วิ",
    bestLines: "แนวทางที่ดีที่สุด (Multi-PV)",
    mandatoryCapture: "บังคับกิน!",
    sixteenMoveCounter: "กฎนับ 16 ตาเดิน",
    turnLight: "ตาเดินฝ่ายสีอ่อน",
    turnDark: "ตาเดินฝ่ายสีเข้ม",
    movesHistory: "บันทึกตาเดิน (กติกา 2568)",
    fenPdn: "นำเข้า / ส่งออก FEN & PDN",
    loadFen: "โหลด FEN",
    copyFen: "คัดลอก FEN",
    importPdn: "นำเข้า PDN",
    exportPdn: "คัดลอก PDN",
    copied: "คัดลอกแล้ว!",
    outcomeWinNoPieces: "หมากหมดกระดาน",
    outcomeWinNoMoves: "ไม่มีตาเดิน",
    outcomeWinResign: "ฝ่ายตรงข้ามยอมแพ้",
    outcomeWinTimeout: "หมดเวลา",
    outcomeDrawRepetition: "เสมอ — เดินซ้ำรูปเดิม 3 ครั้ง (ข้อ 13)",
    outcomeDraw16Moves: "เสมออัตโนมัติ — ครบ 16 ตาเดิน (ข้อ 14)",
    winnerLight: "ฝ่ายสีอ่อนชนะ",
    winnerDark: "ฝ่ายสีเข้มชนะ",
    drawLabel: "เสมอ",
    playAs: "เลือกฝ่าย",
    noMovesYet: "ยังไม่มีตาเดิน — ฝ่ายสีอ่อนเดินก่อน",
  },
};
