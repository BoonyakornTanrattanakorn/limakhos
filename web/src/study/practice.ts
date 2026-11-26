export type PracticeModuleId = "2kings" | "3kings" | "breakthrough" | "diagonal";

export interface PracticeDrill {
  id: string;
  titleTh: string;
  titleEn: string;
  goalTh: string;
  goalEn: string;
  fen: string;
  humanColor: "light" | "dark";
  aiLevel: number;
}

export interface PracticeModule {
  id: PracticeModuleId;
  titleTh: string;
  titleEn: string;
  descriptionTh: string;
  descriptionEn: string;
  drills: PracticeDrill[];
}

export const PRACTICE_MODULES: PracticeModule[] = [
  {
    id: "2kings",
    titleTh: "2 ฮอส ไล่ 1 ฮอส",
    titleEn: "2 Kings vs. 1 King",
    descriptionTh: "ฝึกประสานฮอส 2 ตัวต้อนฮอสเดี่ยวเข้ามุมหรือขอบกระดานให้ได้ภายในกฎ 16 ตาเดิน (32 Ply)",
    descriptionEn: "Coordinate two flying Kings to trap or capture a lone King before the 16-move draw limit.",
    drills: [
      {
        id: "2k-diag",
        titleTh: "บทที่ 1: บีบจากเส้นทแยงหลัก 4–29",
        titleEn: "Drill 1: Main Diagonal Cut-Off",
        goalTh: "ชนะภายใน 16 ตาเดิน (ใช้ 2 ฮอสตัดทางหนีจากเส้นทแยงหลัก)",
        goalEn: "Win before the 16-move clock expires by cutting off escape diagonals.",
        fen: "W:WK8,K11:BK22",
        humanColor: "light",
        aiLevel: 7,
      },
      {
        id: "2k-corner",
        titleTh: "บทที่ 2: ไล่ต้อนข้ามกระดาน (1,2 vs 32)",
        titleEn: "Drill 2: Cross-Board Corner Chase",
        goalTh: "ประสานฮอส 1 และ 2 ไล่ฮอส 32 เข้ามุมอับหรือบังคับกิน",
        goalEn: "Drive the cornered King on 32 into a decisive capture net.",
        fen: "W:WK1,K2:BK32",
        humanColor: "light",
        aiLevel: 7,
      },
      {
        id: "2k-edge",
        titleTh: "บทที่ 3: ตัดปีก 4-15 vs 24",
        titleEn: "Drill 3: Edge Containment",
        goalTh: "จำกัดพื้นที่ฮอสสีเข้มบนตา 24 และปิดเกมให้เฉียบคม",
        goalEn: "Contain Dark's King near the edge and force a winning tactic.",
        fen: "W:WK4,K15:BK24",
        humanColor: "light",
        aiLevel: 7,
      },
    ],
  },
  {
    id: "3kings",
    titleTh: "3 ฮอส ไล่ 1 ฮอส",
    titleEn: "3 Kings vs. 1 King",
    descriptionTh: "ฝึกวางตาข่ายฮอส 3 ตัวเพื่อปิดทางบินทั้งหมดของฮอสเดี่ยวอย่างรวดเร็วและไม่ติดเสมอ 16 ตา",
    descriptionEn: "Build a three-King net to seal every diagonal and finish cleanly within 16 moves.",
    drills: [
      {
        id: "3k-net",
        titleTh: "บทที่ 1: ตาข่ายสามเหลี่ยม (1,2,3 vs 29)",
        titleEn: "Drill 1: Triangular Net",
        goalTh: "นำฮอส 3 ตัวรุกคืบคุมเส้น 4–29 แล้วต้อนกินฮอส 29",
        goalEn: "Advance the three Kings in formation to trap the King on 29.",
        fen: "W:WK1,K2,K3:BK29",
        humanColor: "light",
        aiLevel: 7,
      },
      {
        id: "3k-flank",
        titleTh: "บทที่ 2: ล้อมจากแนวซ้าย (4,12,20 vs 25)",
        titleEn: "Drill 2: Flank Encirclement",
        goalTh: "บีบพื้นที่จากด้านซ้ายและชนะฮอสบนตา 25",
        goalEn: "Sweep inward from the left flank to trap the King on 25.",
        fen: "W:WK4,K12,K20:BK25",
        humanColor: "light",
        aiLevel: 7,
      },
      {
        id: "3k-center",
        titleTh: "บทที่ 3: ล้อมฮอสกลางกระดาน (5,6,7 vs 18)",
        titleEn: "Drill 3: Central King Capture",
        goalTh: "ปิดสี่ทิศทางรอบตา 18 เพื่อบังคับกินหรืออับตาเดิน",
        goalEn: "Surround the central King on 18 and force capture or stalemate.",
        fen: "W:WK5,K6,K7:BK18",
        humanColor: "light",
        aiLevel: 7,
      },
    ],
  },
  {
    id: "breakthrough",
    titleTh: "ต่อเข้าฮอส (Sacrificial Breakthroughs)",
    titleEn: "Sacrificial Breakthroughs",
    descriptionTh: "ฝึกเสียสละเบี้ยเพื่อเจาะแนวรับของคู่แข่งและฝ่าเข้าไปขึ้นฮอสในแดนหลัง",
    descriptionEn: "Sacrifice a man to shatter defensive pawn walls and break through to promotion.",
    drills: [
      {
        id: "brk-1",
        titleTh: "บทที่ 1: สละเบี้ย 17-22 เจาะเข้าฮอส 29",
        titleEn: "Drill 1: 17-22 Breakthrough to 29",
        goalTh: "ต่อเบี้ย 17-22 แล้วกวาดเข้าฮอสตา 29 เพื่อชนะเกม",
        goalEn: "Play 17-22 to force a promotion sweep to square 29 and convert the win.",
        fen: "W:W13,17,18:B25,26",
        humanColor: "light",
        aiLevel: 6,
      },
      {
        id: "brk-2",
        titleTh: "บทที่ 2: สละเบี้ย 10-15 กวาดเข้าฮอส 31",
        titleEn: "Drill 2: 10-15 Triple Sweep to 31",
        goalTh: "หาจังหวะต่อเบี้ยล่อกินแล้วกวาดเข้าฮอสตา 31",
        goalEn: "Set up the decoy sacrifice and sweep all three defenders.",
        fen: "W:W6,10,11:B18,19,26",
        humanColor: "light",
        aiLevel: 6,
      },
      {
        id: "brk-3",
        titleTh: "บทที่ 3: ฝ่าแนวรับ 3 ต่อ 2 (14,15,19 vs 22,26)",
        titleEn: "Drill 3: 3-vs-2 Promotion Race",
        goalTh: "ใช้ความได้เปรียบตัวหมากฝ่าเข้าฮอสและปราบเบี้ยฝ่ายสีเข้ม",
        goalEn: "Break through the two-man blockade to crown a King and win.",
        fen: "W:W14,15,19:B22,26",
        humanColor: "light",
        aiLevel: 6,
      },
    ],
  },
  {
    id: "diagonal",
    titleTh: "ควบคุมเส้นทแยงหลัก 4–29",
    titleEn: "Main Diagonal Control (4–29)",
    descriptionTh: "ฝึกยึดครองและใช้ประโยชน์จากเส้นทแยงหลัก 4–8–11–15–18–22–25–29 ในช่วงท้ายกระดาน",
    descriptionEn: "Master seizing, contesting, and converting control of the 4–29 main diagonal.",
    drills: [
      {
        id: "diag-1",
        titleTh: "บทที่ 1: ยึดเส้นหลักตัดกำลังฝ่ายสีเข้ม",
        titleEn: "Drill 1: Seize the 4–29 Spine",
        goalTh: "เข้ายึดเส้นทแยงหลัก 4–29 และแปรรูปเป็นชัยชนะ",
        goalEn: "Claim the 4–29 main diagonal to restrict Dark's King and push your Man.",
        fen: "W:W10,K1:BK22,21",
        humanColor: "light",
        aiLevel: 6,
      },
      {
        id: "diag-2",
        titleTh: "บทที่ 2: ชิงจังหวะ 1 ฮอส + 1 เบี้ยบนเส้นหลัก",
        titleEn: "Drill 2: King + Man Diagonal Pin",
        goalTh: "ใช้ฮอสบนเส้น 4–29 คุ้มกันเบี้ยขึ้นฮอส",
        goalEn: "Use the main diagonal to escort your advancing Man to promotion.",
        fen: "W:W19,K4:BK25,28",
        humanColor: "light",
        aiLevel: 6,
      },
      {
        id: "diag-3",
        titleTh: "บทที่ 3: หนีบมุม 4 และเส้นทแยงหลัก",
        titleEn: "Drill 3: Corner & Diagonal Squeeze",
        goalTh: "ประสานฮอสคุมตา 11–15 เพื่อล็อกฮอสสีเข้ม",
        goalEn: "Anchor on 11–15 to dominate the board and win the endgame.",
        fen: "W:WK1,14:BK29,21",
        humanColor: "light",
        aiLevel: 6,
      },
    ],
  },
];
