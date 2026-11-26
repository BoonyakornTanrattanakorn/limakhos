export type Locale = "th" | "en";

export interface Messages {
  appTitle: string;
  subtitle: string;
  modeAnalysis: string;
  modeAi: string;
  modeLocal: string;
  modeOnline: string;
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
  onlineAccount: string;
  guestName: string;
  username: string;
  password: string;
  login: string;
  register: string;
  logout: string;
  onlineLobby: string;
  quickMatch: string;
  createPrivate: string;
  joinById: string;
  roomIdPlaceholder: string;
  joinRoom: string;
  watchRoom: string;
  leaveRoom: string;
  copyInvite: string;
  openRooms: string;
  noOpenRooms: string;
  refresh: string;
  leaderboard: string;
  recentGames: string;
  noGamesYet: string;
  analyzeGame: string;
  waitingOpponent: string;
  spectating: string;
  spectators: string;
  onlineOfflineError: string;
  disconnected: string;
  ratingLabel: string;
  openingExplorer: string;
  openingBookNext: string;
  outOfBook: string;
  openingLinesCount: string;
  openingStartHint: string;
  openingFirstMover: string;
  openingSecondMover: string;
  modePuzzles: string;
  modePractice: string;
  puzzleTitle: string;
  puzzleRating: string;
  puzzleSolvedCount: string;
  puzzleHint: string;
  puzzleShowSolution: string;
  puzzleRetry: string;
  puzzleNext: string;
  puzzlePrompt: string;
  puzzleWrong: string;
  puzzleSolved: string;
  puzzleRevealed: string;
  practiceTitle: string;
  practiceCompleted: string;
  practiceRetry: string;
  practiceHint: string;
  practiceWinMsg: string;
  practiceFailMsg: string;
}

export const MESSAGES: Record<Locale, Messages> = {
  en: {
    appTitle: "Limakhos",
    subtitle: "Makhos 8 (TCA 2025) • Local WASM & Self-Hosted Play",
    modeAnalysis: "Analysis",
    modeAi: "Play vs AI",
    modeLocal: "Local 2P",
    modeOnline: "Online",
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
    onlineAccount: "Account / Guest",
    guestName: "Guest Name",
    username: "Username",
    password: "Password",
    login: "Sign In",
    register: "Register",
    logout: "Sign Out",
    onlineLobby: "Multiplayer Lobby",
    quickMatch: "Quick Matchmaking",
    createPrivate: "Create Private Room",
    joinById: "Join / Watch by Room ID",
    roomIdPlaceholder: "Room ID (e.g. a1b2c3d4)",
    joinRoom: "Join",
    watchRoom: "Watch",
    leaveRoom: "Leave Room",
    copyInvite: "Copy Invite Link",
    openRooms: "Open Rooms",
    noOpenRooms: "No open rooms yet — create one or queue a match!",
    refresh: "Refresh",
    leaderboard: "Glicko-2 Leaderboard",
    recentGames: "Recent Games",
    noGamesYet: "No archived games yet.",
    analyzeGame: "Analyze",
    waitingOpponent: "Waiting for opponent...",
    spectating: "Spectating",
    spectators: "Spectators",
    onlineOfflineError: "Online server unreachable. Local Analysis, AI, and 2P modes continue working 100% offline.",
    disconnected: "offline",
    ratingLabel: "Rating",
    openingExplorer: "Opening Explorer (เปิดหมาก)",
    openingBookNext: "Book Moves",
    outOfBook: "Out of opening book — continue with local WASM analysis.",
    openingLinesCount: "lines",
    openingStartHint: "Pick a formation or play a book move to explore canonical Thai openings:",
    openingFirstMover: "Light (1st)",
    openingSecondMover: "Dark (2nd)",
    modePuzzles: "Puzzles",
    modePractice: "Practice",
    puzzleTitle: "Kon Makhos Puzzles (กลหมากฮอส)",
    puzzleRating: "Puzzle Rating",
    puzzleSolvedCount: "Solved",
    puzzleHint: "Hint",
    puzzleShowSolution: "Show Solution",
    puzzleRetry: "Retry",
    puzzleNext: "Next Puzzle",
    puzzlePrompt: "Find the winning tactic on the board.",
    puzzleWrong: "Not the best move — try again!",
    puzzleSolved: "Solved! Excellent tactic.",
    puzzleRevealed: "Solution revealed. Try the next puzzle!",
    practiceTitle: "Interactive Practice",
    practiceCompleted: "Completed",
    practiceRetry: "Restart Drill",
    practiceHint: "Engine Hint",
    practiceWinMsg: "Drill mastered! You converted the winning position.",
    practiceFailMsg: "Drill missed (draw/loss) — restart and try again!",
  },
  th: {
    appTitle: "Limakhos",
    subtitle: "หมากฮอส 8 ตัว (กติกา พ.ศ. 2568) • วิเคราะห์ในเบราว์เซอร์ & ออนไลน์",
    modeAnalysis: "กระดานวิเคราะห์",
    modeAi: "เล่นกับบอท AI",
    modeLocal: "เล่น 2 คน",
    modeOnline: "เล่นออนไลน์",
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
    onlineAccount: "บัญชีผู้เล่น / ผู้เยี่ยมชม",
    guestName: "ชื่อผู้เล่น (Guest)",
    username: "ชื่อผู้ใช้",
    password: "รหัสผ่าน",
    login: "เข้าสู่ระบบ",
    register: "สมัครสมาชิก",
    logout: "ออกจากระบบ",
    onlineLobby: "ห้องเล่นออนไลน์",
    quickMatch: "จับคู่ด่วน (Matchmaking)",
    createPrivate: "สร้างห้องส่วนตัว",
    joinById: "เข้าห้องด้วยรหัสห้อง",
    roomIdPlaceholder: "รหัสห้อง (เช่น a1b2c3d4)",
    joinRoom: "เข้าร่วม",
    watchRoom: "เข้าชม",
    leaveRoom: "ออกจากห้อง",
    copyInvite: "คัดลอกลิงก์ชวนเพื่อน",
    openRooms: "ห้องที่เปิดอยู่",
    noOpenRooms: "ยังไม่มีห้องเปิด — กดสร้างห้องหรือจับคู่ด่วนได้เลย",
    refresh: "รีเฟรช",
    leaderboard: "อันดับคะแนน Glicko-2",
    recentGames: "กระดานล่าสุด",
    noGamesYet: "ยังไม่มีประวัติการแข่งขัน",
    analyzeGame: "วิเคราะห์",
    waitingOpponent: "กำลังรอผู้เล่นอีกฝ่าย...",
    spectating: "กำลังรับชม",
    spectators: "ผู้ชม",
    onlineOfflineError: "ไม่สามารถเชื่อมต่อเซิร์ฟเวอร์ออนไลน์ได้ (โหมดวิเคราะห์, AI และเล่น 2 คนยังใช้งานแบบออฟไลน์ได้ตามปกติ)",
    disconnected: "หลุดการเชื่อมต่อ",
    ratingLabel: "เรตติ้ง",
    openingExplorer: "คลังเปิดหมาก (Opening Explorer)",
    openingBookNext: "ตาเดินในตำรา",
    outOfBook: "ออกนอกตำราเปิดหมากแล้ว — วิเคราะห์ต่อด้วยเอนจิน WASM ได้ทันที",
    openingLinesCount: "สาย",
    openingStartHint: "เลือกทรงหมากมาตรฐานหรือคลิกตาเดินในตำราเพื่อศึกษาแนวการเปิดหมาก:",
    openingFirstMover: "ฝ่ายเดินก่อน (1–8)",
    openingSecondMover: "ฝ่ายเดินหลัง (25–32)",
    modePuzzles: "กลหมากฮอส",
    modePractice: "ฝึกซ้อม",
    puzzleTitle: "ปริศนากลหมากฮอส (Kon Makhos)",
    puzzleRating: "เรตติ้งกลหมาก",
    puzzleSolvedCount: "ผ่านแล้ว",
    puzzleHint: "คำใบ้",
    puzzleShowSolution: "ดูเฉลย",
    puzzleRetry: "ลองใหม่",
    puzzleNext: "ข้อถัดไป",
    puzzlePrompt: "เดินตาที่ดีที่สุดเพื่อชิงความได้เปรียบหรือปิดเกม",
    puzzleWrong: "ยังไม่ถูกต้อง — ลองหาตาที่ดีกว่านี้!",
    puzzleSolved: "ยอดเยี่ยม! แก้กลหมากฮอสสำเร็จ",
    puzzleRevealed: "แสดงเฉลยแล้ว — ลองฝึกข้อถัดไปได้เลย",
    practiceTitle: "บทฝึกซ้อมเชิงโต้ตอบ (Practice)",
    practiceCompleted: "ผ่านแล้ว",
    practiceRetry: "เริ่มด่านใหม่",
    practiceHint: "ขอคำแนะนำ AI",
    practiceWinMsg: "ผ่านด่านสำเร็จ! ปิดเกมได้อย่างแม่นยำ",
    practiceFailMsg: "ยังไม่ผ่านด่าน (เสมอ/แพ้) — กดเริ่มใหม่เพื่อลองอีกครั้ง",
  },
};
