export type FormationId =
  | "three_in_a_row"
  | "e_poom"
  | "standard"
  | "guerrilla"
  | "starburst"
  | "flank_pinch";

export interface FormationMeta {
  id: FormationId;
  nameTh: string;
  nameEn: string;
  summaryTh: string;
  summaryEn: string;
}

export interface OpeningLine {
  id: string;
  formationId: FormationId;
  side: "light" | "dark";
  nameTh: string;
  nameEn: string;
  variationTh: string;
  variationEn: string;
  noteTh: string;
  noteEn: string;
  moves: string[];
}

export interface OpeningBookMove {
  notation: string;
  lineCount: number;
  lines: OpeningLine[];
}

export interface OpeningMatchResult {
  matchedFormation: FormationMeta | null;
  activeLine: OpeningLine | null;
  matchingLines: OpeningLine[];
  nextMoves: OpeningBookMove[];
}

export const FORMATIONS: FormationMeta[] = [
  {
    id: "three_in_a_row",
    nameTh: "สามตัวเรียง",
    nameEn: "Three-in-a-Row",
    summaryTh: "เรียงหมาก 3 ตัวในแนวทแยงต่อเนื่องเพื่อคุมแกนกลางและสร้างฐานต่อแลกที่แข็งแกร่ง",
    summaryEn: "Aligns three men along a straight diagonal for solid support and central counter-exchanges.",
  },
  {
    id: "e_poom",
    nameTh: "อีปุ้ม",
    nameEn: "E-Poom",
    summaryTh: "รูปตั้งรับและรุกแบบสามเหลี่ยมทึบ ปิดช่องโหว่แนวกลางเพื่อรอจังหวะสวนกลับ",
    summaryEn: "Compact triangular wedge sealing central gaps while preparing sharp counterattacks.",
  },
  {
    id: "standard",
    nameTh: "มาตรฐาน",
    nameEn: "Standard",
    summaryTh: "รูปเปิดสมดุลยอดนิยม เน้นยึดแนวเส้นทแยงหลัก 4–29 และพัฒนากำลังทั้งสองปีก",
    summaryEn: "Balanced classical setup contesting the 4–29 main diagonal and both flanks.",
  },
  {
    id: "guerrilla",
    nameTh: "กองโจร",
    nameEn: "Guerrilla",
    summaryTh: "รูปเปิดสายแอบซุ่มริมกระดาน หลีกเลี่ยงการปะทะตรงกลางแล้วจู่โจมจุดอ่อนด้านข้าง",
    summaryEn: "Asymmetric flank strategy sidestepping direct center clashes to ambush weak wings.",
  },
  {
    id: "starburst",
    nameTh: "ดาวกระจาย",
    nameEn: "Starburst",
    summaryTh: "กระจายตัวหมากเปิดเกมเร็ว สร้างเส้นทางแลกหมากและเจาะเข้าฮอสหลายทิศทาง",
    summaryEn: "Open, aggressive structure fanning pieces across wings for rapid tactical breakthroughs.",
  },
  {
    id: "flank_pinch",
    nameTh: "หนีบข้าง",
    nameEn: "Flank Pinch",
    summaryTh: "บีบทั้งสองปีกของคู่แข่ง (5–9 และ 8–12 / 25–21 และ 28–24) เพื่อบีบรัดพื้นที่กลางกระดาน",
    summaryEn: "Double-wing pincer squeezing the opponent's center from both outer edges.",
  },
];

export const OPENING_BOOK: OpeningLine[] = [
  // 1. สามตัวเรียง (Three-in-a-Row) — Light & Dark
  {
    id: "tir-light-right",
    formationId: "three_in_a_row",
    side: "light",
    nameTh: "สามตัวเรียง (สายขวา 1-6-10)",
    nameEn: "Three-in-a-Row (Right 1-6-10)",
    variationTh: "สายหลักคุมจัตุรัส 10",
    variationEn: "Main line via 10",
    noteTh: "ฝ่ายสีอ่อนเปิด 6-10 แล้วเสริม 1-6 เรียง 1-6-10 ก่อนขยับปีกซ้าย 7-11 กดดันแกนกลาง",
    noteEn: "Light builds the 1-6-10 diagonal wall and reinforces the center with 7-11.",
    moves: ["6-10", "27-23", "1-6", "31-27", "7-11", "26-22"],
  },
  {
    id: "tir-light-center",
    formationId: "three_in_a_row",
    side: "light",
    nameTh: "สามตัวเรียง (สายกลาง 2-7-11)",
    nameEn: "Three-in-a-Row (Center 2-7-11)",
    variationTh: "ยึดเส้นทแยงหลัก",
    variationEn: "Main diagonal anchor",
    noteTh: "ยึดจุด 11 ด้วยรูปสามตัวเรียง 2-7-11 ทำให้ฝ่ายสีเข้มบุกเข้าเส้น 4-29 ได้ยาก",
    noteEn: "Forms 2-7-11 on the critical 11 outpost, restricting Dark along 4–29.",
    moves: ["7-11", "26-22", "2-7", "30-26", "6-10", "25-21"],
  },
  {
    id: "tir-dark-right",
    formationId: "three_in_a_row",
    side: "dark",
    nameTh: "สามตัวเรียง (ฝ่ายเดินหลัง 32-27-23)",
    nameEn: "Three-in-a-Row (Dark 32-27-23)",
    variationTh: "ต้านสายมาตรฐาน",
    variationEn: "Countering Standard",
    noteTh: "ฝ่ายสีเข้มเรียง 32-27-23 ตอบโต้การรุกที่จัตุรัส 11 พร้อมเตรียมแลกที่ 18",
    noteEn: "Dark sets up the 32-27-23 diagonal chain to challenge central advances.",
    moves: ["7-11", "27-23", "6-10", "32-27", "2-6", "26-22"],
  },
  {
    id: "tir-dark-left",
    formationId: "three_in_a_row",
    side: "dark",
    nameTh: "สามตัวเรียง (ฝ่ายเดินหลัง 30-26-23)",
    nameEn: "Three-in-a-Row (Dark 30-26-23)",
    variationTh: "สาย 30-26-23",
    variationEn: "30-26-23 Counter",
    noteTh: "ตอบรับ 6-10 ด้วย 27-23 แล้วขยับ 26-22 ตามด้วย 30-26 เรียงแนว 30-26-23 คุมทั้งแกนหลักและฝั่งขวา",
    noteEn: "Develops 27-23 and 26-22 then 30-26 to form the 30-26-23 diagonal chain.",
    moves: ["6-10", "27-23", "8-12", "26-22", "7-11", "30-26"],
  },

  // 2. อีปุ้ม (E-Poom) — Light & Dark
  {
    id: "epoom-light-classic",
    formationId: "e_poom",
    side: "light",
    nameTh: "อีปุ้ม (รูปหลักฝ่ายเดินก่อน)",
    nameEn: "E-Poom (Light Main Wedge)",
    variationTh: "โครงสร้างปิดแนวรับ",
    variationEn: "Solid wedge setup",
    noteTh: "ตั้งป้อม 6-10, 7-11, 5-9 และปิดท้ายด้วย 2-6 เพื่อความแน่นหนาในแดนกลาง",
    noteEn: "Builds a tight phalanx with 6-10, 7-11, 5-9, and 2-6 leaving zero gaps.",
    moves: ["6-10", "26-22", "7-11", "27-23", "5-9", "31-27", "2-6"],
  },
  {
    id: "epoom-light-left",
    formationId: "e_poom",
    side: "light",
    nameTh: "อีปุ้ม (สายซ้าย 8-11)",
    nameEn: "E-Poom (Left Support 8-11)",
    variationTh: "ปิดปีกซ้าย-กลาง",
    variationEn: "Left-center reinforcement",
    noteTh: "เริ่มด้วย 7-10 และ 8-11 คุมพื้นที่ 10-11 ควบคู่กับการหนุนจาก 3-7",
    noteEn: "Controls 10 and 11 early with 7-10 and 8-11 backed up by 3-7.",
    moves: ["7-10", "26-22", "8-11", "27-23", "3-7", "30-26", "6-9"],
  },
  {
    id: "epoom-dark-classic",
    formationId: "e_poom",
    side: "dark",
    nameTh: "อีปุ้ม (ฝ่ายเดินหลังสายตรง)",
    nameEn: "E-Poom (Dark Classic Defense)",
    variationTh: "ตั้งป้อม 27-23 / 26-22",
    variationEn: "27-23 & 26-22 Fortress",
    noteTh: "ฝ่ายสีเข้มตั้งรูปอีปุ้มแน่นหนาด้วย 27-23, 26-22, 28-24 และ 31-27",
    noteEn: "Dark anchors the center with 27-23, 26-22, 28-24, and 31-27.",
    moves: ["6-10", "27-23", "7-11", "26-22", "5-9", "28-24", "3-7", "31-27"],
  },
  {
    id: "epoom-dark-counter",
    formationId: "e_poom",
    side: "dark",
    nameTh: "อีปุ้ม (ฝ่ายเดินหลังสายหนีบ 25-22)",
    nameEn: "E-Poom (Dark 25-22 Variant)",
    variationTh: "คุมแกน 22-23",
    variationEn: "22-23 Central Clamp",
    noteTh: "ใช้ 26-23 และ 25-22 ประกบแนวกลาง รับมือการบุกเร็วของฝ่ายสีอ่อน",
    noteEn: "Pairs 26-23 with 25-22 to clamp Light's advance into squares 14–15.",
    moves: ["7-11", "26-23", "6-10", "25-22", "8-12", "30-26"],
  },

  // 3. มาตรฐาน (Standard) — Light & Dark
  {
    id: "std-light-main",
    formationId: "standard",
    side: "light",
    nameTh: "มาตรฐาน (สายหลัก 7-11)",
    nameEn: "Standard (7-11 Main Line)",
    variationTh: "ต่อสู้เส้นทแยงหลัก 4–29",
    variationEn: "4–29 Diagonal Battle",
    noteTh: "เปิด 7-11 เข้าเส้น 4-29 ทันที จากนั้นเสริม 3-7 และ 6-10 เพื่อชิงจังหวะกลางกระดาน",
    noteEn: "Claims square 11 on the main diagonal, followed by 3-7 and 6-10.",
    moves: ["7-11", "26-22", "3-7", "30-26", "6-10", "27-23"],
  },
  {
    id: "std-light-exchange",
    formationId: "standard",
    side: "light",
    nameTh: "มาตรฐาน (แลกกลาง 11-15)",
    nameEn: "Standard (Central Clash 11-15)",
    variationTh: "เปิดเกมแลกหมาก 11-15",
    variationEn: "11-15 Exchange Line",
    noteTh: "รุกคืบด้วย 11-15 กดดันจุดยุทธศาสตร์กลางกระดานหลังการพัฒนาพื้นฐาน",
    noteEn: "Pushes 11-15 early to contest central space after standard development.",
    moves: ["7-11", "26-22", "6-10", "27-23", "11-15", "22-18"],
  },
  {
    id: "std-dark-mirror",
    formationId: "standard",
    side: "dark",
    nameTh: "มาตรฐาน (ฝ่ายเดินหลัง 26-22)",
    nameEn: "Standard (Dark 26-22 Symmetry)",
    variationTh: "ตอบโต้สมมาตร",
    variationEn: "Symmetrical Response",
    noteTh: "ฝ่ายสีเข้มตอบ 26-22 ยึดจุด 22 บนเส้นทแยงหลักกลับคืนอย่างสมดุล",
    noteEn: "Dark counters symmetrically on square 22 along the 4–29 main diagonal.",
    moves: ["7-11", "26-22", "8-12", "25-21", "6-10", "31-26"],
  },
  {
    id: "std-dark-sharp",
    formationId: "standard",
    side: "dark",
    nameTh: "มาตรฐาน (ฝ่ายเดินหลังสายรุก 27-23)",
    nameEn: "Standard (Dark 27-23 Active)",
    variationTh: "กดดันจัตุรัส 18",
    variationEn: "Pressure on Square 18",
    noteTh: "ตอบ 6-10 ด้วย 27-23 และ 25-22 เปิดแนวรุกฝั่งขวาของฝ่ายสีเข้ม",
    noteEn: "Meets 6-10 with 27-23 and 25-22 for active counterplay on Dark's right.",
    moves: ["6-10", "27-23", "7-11", "25-22", "3-7", "32-27"],
  },

  // 4. กองโจร (Guerrilla) — Light & Dark
  {
    id: "guer-light-left",
    formationId: "guerrilla",
    side: "light",
    nameTh: "กองโจร (ปีกซ้าย 8-12)",
    nameEn: "Guerrilla (Left Wing 8-12)",
    variationTh: "แอบซุ่มขอบซ้าย 8-12",
    variationEn: "8-12 Edge Ambush",
    noteTh: "เดิน 8-12 ชิดริมซ้ายก่อนแล้วค่อยแทง 7-11 กับ 12-16 ตลบหลังฝ่ายตรงข้าม",
    noteEn: "Starts quietly on 8-12, hiding intentions before striking with 7-11 and 12-16.",
    moves: ["8-12", "26-22", "7-11", "27-23", "12-16", "31-27"],
  },
  {
    id: "guer-light-right",
    formationId: "guerrilla",
    side: "light",
    nameTh: "กองโจร (ปีกขวา 5-9)",
    nameEn: "Guerrilla (Right Wing 5-9)",
    variationTh: "แอบซุ่มขอบขวา 5-9",
    variationEn: "5-9 Edge Ambush",
    noteTh: "เปิดด้วย 5-9 อ้อมปีกขวา เลี่ยงการปะทะในจังหวะแรกแล้วตามด้วย 6-10",
    noteEn: "Avoids central confrontation on move 1 with 5-9, preparing a delayed 6-10.",
    moves: ["5-9", "27-23", "6-10", "26-22", "9-13", "30-26"],
  },
  {
    id: "guer-dark-right",
    formationId: "guerrilla",
    side: "dark",
    nameTh: "กองโจร (ฝ่ายเดินหลัง 25-21)",
    nameEn: "Guerrilla (Dark 25-21 Wing)",
    variationTh: "ซุ่มริมขวา 25-21",
    variationEn: "25-21 Flank Trap",
    noteTh: "ฝ่ายสีเข้มตอบ 25-21 ดึงเกมออกข้าง เพื่อล่อให้ฝ่ายสีอ่อนเปิดแนวรับ",
    noteEn: "Dark retreats to the edge with 25-21, tempting Light to overextend.",
    moves: ["7-11", "25-21", "6-10", "26-22", "5-9", "21-17"],
  },
  {
    id: "guer-dark-left",
    formationId: "guerrilla",
    side: "dark",
    nameTh: "กองโจร (ฝ่ายเดินหลัง 28-24)",
    nameEn: "Guerrilla (Dark 28-24 Wing)",
    variationTh: "ซุ่มริมซ้าย 28-24",
    variationEn: "28-24 Flank Trap",
    noteTh: "ใช้ 28-24 และ 24-20 เตรียมเจาะเข้าเส้นหลังทางฝั่งซ้ายของกระดาน",
    noteEn: "Uses 28-24 and 24-20 to infiltrate along the left edge.",
    moves: ["6-10", "28-24", "7-11", "27-23", "5-9", "24-20"],
  },

  // 5. ดาวกระจาย (Starburst) — Light & Dark
  {
    id: "star-light-open",
    formationId: "starburst",
    side: "light",
    nameTh: "ดาวกระจาย (เปิดกว้าง 6-9 / 8-11)",
    nameEn: "Starburst (Wide Spread 6-9 / 8-11)",
    variationTh: "แยกปีกซ้าย-ขวา",
    variationEn: "Split-Wing Tactical Spread",
    noteTh: "กระจายตัวหมาก 6-9, 8-11, 7-10 เปิดช่องแลกหมากหลายทิศทางและเปิดเกมดุเดือด",
    noteEn: "Spreads 6-9, 8-11, and 7-10 for multi-directional tactical breakthroughs.",
    moves: ["6-9", "27-23", "8-11", "26-22", "7-10", "25-21"],
  },
  {
    id: "star-light-fast",
    formationId: "starburst",
    side: "light",
    nameTh: "ดาวกระจาย (เร่งบุก 7-10 / 5-9)",
    nameEn: "Starburst (Rapid 7-10 / 5-9)",
    variationTh: "เจาะแนวกลางเร็ว",
    variationEn: "Fast Center Disruption",
    noteTh: "ใช้ 7-10 ร่วมกับ 5-9 และ 8-12 เพื่อกระจายแรงกดดันทั้งกระดาน",
    noteEn: "Combines 7-10 with 5-9 and 8-12 to stretch Dark's defensive front.",
    moves: ["7-10", "27-23", "5-9", "26-22", "8-12", "31-27"],
  },
  {
    id: "star-dark-split",
    formationId: "starburst",
    side: "dark",
    nameTh: "ดาวกระจาย (ฝ่ายเดินหลัง 27-24 / 25-22)",
    nameEn: "Starburst (Dark 27-24 / 25-22)",
    variationTh: "เปิดรับเชิงรุก",
    variationEn: "Active Split Counter",
    noteTh: "ฝ่ายสีเข้มกระจายตัว 27-24 และ 25-22 เพื่อสร้างเกมเปิดแลกตั้งแต่ต้นกระดาน",
    noteEn: "Dark splits 27-24 and 25-22 to trigger sharp open-board tactics.",
    moves: ["7-11", "27-24", "6-10", "25-22", "5-9", "26-23"],
  },
  {
    id: "star-dark-wing",
    formationId: "starburst",
    side: "dark",
    nameTh: "ดาวกระจาย (ฝ่ายเดินหลัง 26-23 / 28-24)",
    nameEn: "Starburst (Dark 26-23 / 28-24)",
    variationTh: "รุกกระจายปีก",
    variationEn: "Wing Spread Counter",
    noteTh: "แยกปีกซ้ายด้วย 28-24 พร้อม 26-23 เปิดทางให้ตัวหลัง 29-25 หนุนได้เร็ว",
    noteEn: "Opens the left wing with 28-24 while placing 26-23 for rapid 29-25 mobilization.",
    moves: ["6-10", "26-23", "7-11", "28-24", "8-12", "25-22"],
  },

  // 6. หนีบข้าง (Flank Pinch) — Light & Dark
  {
    id: "pinch-light-both",
    formationId: "flank_pinch",
    side: "light",
    nameTh: "หนีบข้าง (ปีกคู่ 8-12 & 5-9)",
    nameEn: "Flank Pinch (Light 8-12 & 5-9)",
    variationTh: "บีบสองข้างเข้ากลาง",
    variationEn: "Double-Edge Squeeze",
    noteTh: "ยึดทั้งริมซ้าย 8-12 และริมขวา 5-9 ก่อนบีบด้วย 6-10 และ 7-11 เพื่อล็อกกลางกระดาน",
    noteEn: "Secures both outer wings (8-12 and 5-9) before clamping the center with 6-10.",
    moves: ["8-12", "27-23", "5-9", "26-22", "6-10", "31-27", "7-11"],
  },
  {
    id: "pinch-light-rightfirst",
    formationId: "flank_pinch",
    side: "light",
    nameTh: "หนีบข้าง (เริ่มขวา 5-9 แล้ว 8-12)",
    nameEn: "Flank Pinch (5-9 then 8-12)",
    variationTh: "สลับจังหวะหนีบ",
    variationEn: "Right-First Pinch",
    noteTh: "เริ่มจาก 5-9 ตามด้วย 8-12 เพื่อรอดูแนวรับของฝ่ายสีเข้มก่อนเลือกลง 7-11",
    noteEn: "Plays 5-9 then 8-12 to read Dark's setup before committing 7-11.",
    moves: ["5-9", "26-22", "8-12", "27-23", "7-11", "30-26"],
  },
  {
    id: "pinch-dark-both",
    formationId: "flank_pinch",
    side: "dark",
    nameTh: "หนีบข้าง (ฝ่ายเดินหลัง 28-24 & 25-21)",
    nameEn: "Flank Pinch (Dark 28-24 & 25-21)",
    variationTh: "หนีบสองปีกฝ่ายสีเข้ม",
    variationEn: "Dark Double-Wing Pinch",
    noteTh: "ฝ่ายสีเข้มตั้งรูปหนีบข้างด้วย 28-24 และ 25-21 แล้วค่อยเติม 27-23",
    noteEn: "Dark clamps both edges with 28-24 and 25-21 before challenging with 27-23.",
    moves: ["7-11", "28-24", "6-10", "25-21", "8-12", "27-23"],
  },
  {
    id: "pinch-dark-center",
    formationId: "flank_pinch",
    side: "dark",
    nameTh: "หนีบข้าง (ฝ่ายเดินหลัง 25-21 แล้ว 28-24)",
    nameEn: "Flank Pinch (Dark 25-21 then 28-24)",
    variationTh: "หนีบสลับฝั่งรับ 6-10",
    variationEn: "25-21 then 28-24 vs 6-10",
    noteTh: "บีบพื้นที่ปีกขวาด้วย 25-21 แล้วปิดปีกซ้ายด้วย 28-24 เพื่อตัดทางขึ้นของฝ่ายสีอ่อน",
    noteEn: "Pinches both flanks against 6-10 to restrict Light's advancing men.",
    moves: ["6-10", "25-21", "7-11", "28-24", "5-9", "27-23"],
  },
];

export function matchOpeningHistory(playedNotations: string[]): OpeningMatchResult {
  const matchingLines = OPENING_BOOK.filter((line) => {
    if (playedNotations.length > line.moves.length) return false;
    for (let i = 0; i < playedNotations.length; i++) {
      if (line.moves[i] !== playedNotations[i]) return false;
    }
    return true;
  });

  const nextMap = new Map<string, OpeningLine[]>();
  for (const line of matchingLines) {
    const nextMove = line.moves[playedNotations.length];
    if (!nextMove) continue;
    const list = nextMap.get(nextMove) ?? [];
    list.push(line);
    nextMap.set(nextMove, list);
  }

  const nextMoves: OpeningBookMove[] = Array.from(nextMap.entries())
    .map(([notation, lines]) => ({
      notation,
      lineCount: lines.length,
      lines,
    }))
    .sort((a, b) => b.lineCount - a.lineCount || a.notation.localeCompare(b.notation));

  let matchedFormation: FormationMeta | null = null;
  let activeLine: OpeningLine | null = null;

  if (playedNotations.length > 0 && matchingLines.length > 0) {
    activeLine =
      matchingLines.find((l) => l.moves.length === playedNotations.length) ?? matchingLines[0];
    const formationId = activeLine.formationId;
    const allSameFormation = matchingLines.every((l) => l.formationId === formationId);
    if (allSameFormation || playedNotations.length >= 3) {
      matchedFormation = FORMATIONS.find((f) => f.id === formationId) ?? null;
    }
  }

  return {
    matchedFormation,
    activeLine: playedNotations.length > 0 ? activeLine : null,
    matchingLines,
    nextMoves,
  };
}
