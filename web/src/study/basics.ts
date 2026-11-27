export interface BasicsLessonStep {
  id: string;
  titleTh: string;
  titleEn: string;
  explanationTh: string;
  explanationEn: string;
  promptTh: string;
  promptEn: string;
  hintTh: string;
  hintEn: string;
  fen: string;
  acceptedMoves: string[];
  demoArrow?: { from: number; to: number; path?: number[] };
}

export interface BasicsLesson {
  id: string;
  titleTh: string;
  titleEn: string;
  summaryTh: string;
  summaryEn: string;
  steps: BasicsLessonStep[];
}

export const BASICS_LESSONS: BasicsLesson[] = [
  {
    id: "board-and-movement",
    titleTh: "กระดานและการเดินเบี้ย",
    titleEn: "Board & Man Movement",
    summaryTh: "รู้จักกระดาน 32 ตาดำ เส้นทแยงหลัก 4–29 และการเดินเบี้ยทีละ 1 ช่องไปข้างหน้า",
    summaryEn: "Learn the 32 dark squares (1–32), the 4–29 main diagonal, and single-step forward Man moves.",
    steps: [
      {
        id: "basics-man-forward",
        titleTh: "การเดินเบี้ยไปข้างหน้า",
        titleEn: "Moving a Man Forward",
        explanationTh:
          "หมากฮอสไทยเล่นบนตาสีเข้ม 32 ช่อง (หมายเลข 1–32) เบี้ยธรรมดาเดินเฉียงไปข้างหน้าได้ครั้งละ 1 ช่อง และห้ามเดินถอยหลัง",
        explanationEn:
          "Thai Draughts is played on 32 dark squares (1–32). A regular Man moves 1 square diagonally forward and never backward.",
        promptTh: "เดินเบี้ยจากตา 6 ขึ้นหน้า 1 ช่อง (ไปตา 9 หรือ 10)",
        promptEn: "Move your Man on square 6 one step diagonally forward (to 9 or 10).",
        hintTh: "คลิกเบี้ยที่ตา 6 แล้วเลือกเดินขึ้นตา 9 หรือตา 10",
        hintEn: "Select the Man on 6 and step forward to square 9 or square 10.",
        fen: "W:W6:B28",
        acceptedMoves: ["6-9", "6-10"],
      },
      {
        id: "basics-main-diagonal",
        titleTh: "เส้นทแยงหลัก 4–29",
        titleEn: "The 4–29 Main Diagonal",
        explanationTh:
          "แนวทแยงที่ยาวที่สุดของกระดานคือเส้น 4–8–11–15–18–22–25–29 (8 ช่อง) ซึ่งเป็นจุดยุทธศาสตร์สำคัญตลอดทั้งเกม",
        explanationEn:
          "The longest diagonal spans 8 squares from 4 to 29 (4–8–11–15–18–22–25–29) and anchors control of the board.",
        promptTh: "เดินเบี้ยจากตา 11 เข้าสู่ตา 15 ตามแนวเส้นทแยงหลัก",
        promptEn: "Advance your Man from square 11 to square 15 along the main diagonal.",
        hintTh: "เดิน 11-15 เพื่อยึดตำแหน่งบนแนวทแยงหลัก 4–29",
        hintEn: "Play 11-15 to step along the 4–29 main diagonal.",
        fen: "W:W11:B32",
        acceptedMoves: ["11-15"],
        demoArrow: { from: 11, to: 15 },
      },
    ],
  },
  {
    id: "mandatory-capture",
    titleTh: "กฎบังคับกิน",
    titleEn: "Mandatory Capture",
    summaryTh: "เมื่อมีโอกาสกินต้องกินเสมอ และเบี้ยธรรมดากินได้เฉพาะข้างหน้าเท่านั้น",
    summaryEn: "Captures are compulsory when available, and in Thai Draughts Men capture forward only.",
    steps: [
      {
        id: "basics-mandatory-jump",
        titleTh: "บังคับกินเมื่อมีตาว่างด้านหลัง",
        titleEn: "Capture Is Compulsory",
        explanationTh:
          "เมื่อหมากคู่แข่งอยู่ติดในแนวทแยงหน้าและมีช่องว่างด้านหลังตรงข้าม ผู้เล่นต้องกินทันที ห้ามเดินหมากตัวอื่นแทน",
        explanationEn:
          "When an enemy piece sits diagonally ahead with an empty square behind it, capturing is mandatory—quiet moves are illegal.",
        promptTh: "ใช้เบี้ย 14 กินหมากฝ่ายสีเข้มที่ตา 18",
        promptEn: "Jump Dark's piece on 18 with your Man on square 14.",
        hintTh: "เดิน 14x23 (เบี้ยที่ตา 10 ไม่สามารถเดินหนีได้เพราะถูกบังคับกิน)",
        hintEn: "Play 14x23—moving the Man on 10 is illegal while a capture exists.",
        fen: "W:W10,14:B18,28",
        acceptedMoves: ["14x23"],
        demoArrow: { from: 14, to: 23 },
      },
      {
        id: "basics-forward-only-capture",
        titleTh: "เบี้ยธรรมดากินไปข้างหน้าเท่านั้น",
        titleEn: "Men Capture Forward Only",
        explanationTh:
          "กติกาหมากฮอสไทย (TCA 2025) ไม่อนุญาตให้เบี้ยธรรมดากินถอยหลัง เบี้ยที่ตา 14 จึงไม่ถูกบังคับกินหมากข้างหลังที่ตา 10",
        explanationEn:
          "Under TCA 2025 rules, a regular Man captures forward only and never backward, so the enemy piece behind on 10 cannot be jumped.",
        promptTh: "เดินเบี้ย 14 ไปข้างหน้า (ตา 17 หรือ 18) โดยไม่ต้องกินถอยหลัง",
        promptEn: "Advance your Man on 14 forward (to 17 or 18) since backward Man captures do not exist.",
        hintTh: "เบี้ย 14 กินถอยหลังไปตา 6 ไม่ได้ ให้เดินขึ้นหน้า 14-17 หรือ 14-18",
        hintEn: "14x6 is illegal for a Man; step forward with 14-17 or 14-18.",
        fen: "W:W14:B10,28",
        acceptedMoves: ["14-17", "14-18"],
      },
    ],
  },
  {
    id: "multi-jump",
    titleTh: "การกินหลายจังหวะ",
    titleEn: "Multi-Jump Captures",
    summaryTh: "เมื่อลงจอดแล้วยังมีหมากให้กินต่อได้ ต้องกระโดดกินต่อเนื่องในตาเดียวกัน",
    summaryEn: "If another jump is available from your landing square, you must keep jumping in the same turn.",
    steps: [
      {
        id: "basics-double-jump",
        titleTh: "กินต่อเนื่อง 2 จังหวะ",
        titleEn: "Double-Jump Sequence",
        explanationTh:
          "หลังกระโดดกินตัวแรกแล้ว หากตาที่ลงจอดยังมีหมากคู่แข่งให้กินต่อได้อีก ต้องกินต่อเนื่องให้จบสายในตาเดียว",
        explanationEn:
          "When another capture opens up from your landing square, you must continue jumping in the same turn until no capture remains.",
        promptTh: "ใช้เบี้ย 6 กวาดกินหมาก 2 ตัวรวด (6x15x24)",
        promptEn: "Sweep both Dark pieces in one turn from square 6 (6x15x24).",
        hintTh: "กระโดด 6 ข้าม 10 ลง 15 แล้วข้าม 19 ลง 24 ต่อทันที",
        hintEn: "Jump 6 over 10 to 15, then immediately jump 19 to 24.",
        fen: "W:W6:B10,19",
        acceptedMoves: ["6x15x24"],
        demoArrow: { from: 6, to: 24, path: [6, 15, 24] },
      },
    ],
  },
  {
    id: "no-majority",
    titleTh: "เลือกสายกินอิสระ (กติกาไทย)",
    titleEn: "No-Majority Rule",
    summaryTh: "กติกาไทยไม่บังคับกินจำนวนมากที่สุด ผู้เล่นเลือกสายกินที่คุ้มค่าที่สุดได้เอง",
    summaryEn: "TCA 2025 does not force the maximum capture—any legal capture is allowed, so choose wisely.",
    steps: [
      {
        id: "basics-no-majority-choice",
        titleTh: "เลือกสายกิน 2 ตัวแทนการกิน 1 ตัว",
        titleEn: "Choose the Stronger Capture Line",
        explanationTh:
          "ต่างจากหมากฮอสสากล กติกา TCA 2025 ไม่บังคับกินตัวมากที่สุด ทั้งสายกิน 1 ตัว (6x13) และสายกิน 2 ตัว (6x15x24) ต่างถูกกติกาทั้งคู่",
        explanationEn:
          "Unlike International Draughts, TCA 2025 has no majority rule: both the 1-piece jump (6x13) and the 2-piece jump (6x15x24) are legal.",
        promptTh: "เลือกสายกินที่ได้เปรียบกว่าเพื่อกิน 2 ตัวรวด (6x15x24)",
        promptEn: "Spot both legal captures and actively choose the 2-piece capture (6x15x24).",
        hintTh: "อย่าเผลอกิน 6x13 เพียงตัวเดียว ให้เลือกสาย 6x15x24",
        hintEn: "Skip the single jump 6x13 and play 6x15x24 instead.",
        fen: "W:W6:B9,10,19",
        acceptedMoves: ["6x15x24"],
        demoArrow: { from: 6, to: 24, path: [6, 15, 24] },
      },
    ],
  },
  {
    id: "promotion",
    titleTh: "การเข้าฮอส",
    titleEn: "Promotion to Flying King",
    summaryTh: "เบี้ยที่เดินหรือกินถึงแถวสุดท้ายของฝ่ายตรงข้ามจะเข้าฮอสและจบตาเดินทันที",
    summaryEn: "Reaching the far back rank crowns a Man into a Flying King and immediately ends the turn.",
    steps: [
      {
        id: "basics-promotion-crown",
        titleTh: "เลื่อนขั้นเป็นฮอสที่แถวหลัง",
        titleEn: "Crowning on the Back Rank",
        explanationTh:
          "เมื่อเบี้ยเดินเข้าถึงแถวหลังสุดของคู่แข่ง (ตา 29–32 สำหรับฝ่ายขาว) เบี้ยจะได้รับการเลื่อนขั้นเป็น \"ฮอส\" (Flying King)",
        explanationEn:
          "When a Man reaches the opponent's back rank (squares 29–32 for Light), it promotes into a Flying King.",
        promptTh: "เดินเบี้ย 27 เข้าสู่แถวหลัง (ตา 31 หรือ 32) เพื่อเข้าฮอส",
        promptEn: "Advance your Man from 27 onto the back rank (31 or 32) to crown a King.",
        hintTh: "เดิน 27-31 หรือ 27-32 เพื่อเข้าฮอส",
        hintEn: "Move 27-31 or 27-32 to promote.",
        fen: "W:W27:B4",
        acceptedMoves: ["27-31", "27-32"],
      },
      {
        id: "basics-promotion-ends-turn",
        titleTh: "เข้าฮอสแล้วจบตาเดินทันที",
        titleEn: "Promotion Ends the Turn",
        explanationTh:
          "ในหมากฮอสไทย เมื่อเบี้ยกระโดดกินจนถึงตาเข้าฮอส ตาเดินจะหยุดลงทันที แม้จะมีหมากคู่แข่งอยู่บนเส้นทแยงถัดไปก็ตาม",
        explanationEn:
          "In Thai Draughts, landing on the promotion square immediately ends your turn—the newly crowned King does not continue jumping.",
        promptTh: "กิน 26 เพื่อเข้าฮอสที่ตา 31 (ตาเดินจะจบลงบนตา 31 ทันที)",
        promptEn: "Capture 26 onto square 31 to promote (your turn stops immediately on 31).",
        hintTh: "เล่น 22x31 เพื่อเข้าฮอสและจบตาเดินบนตา 31",
        hintEn: "Play 22x31; promotion stops the turn even with Dark's piece on 27.",
        fen: "W:W22:B26,27",
        acceptedMoves: ["22x31"],
        demoArrow: { from: 22, to: 31 },
      },
    ],
  },
  {
    id: "flying-king",
    titleTh: "ฮอสบิน หยุดหลังตัวกิน และหักศอก 180°",
    titleEn: "Flying King, King Halt & 180° Reversal",
    summaryTh: "ฮอสบินไกลได้ทุกช่อง แต่เมื่อกินต้องหยุดติดหลังตัวที่กิน และหักศอกกลับ 180° ได้",
    summaryEn: "Flying Kings slide any distance, halt right behind captured pieces, and can reverse 180° mid-jump.",
    steps: [
      {
        id: "basics-king-halt",
        titleTh: "กฎฮอสหยุดหลังตัวกิน (King Halt)",
        titleEn: "Flying King & King Halt",
        explanationTh:
          "ฮอสเดินเฉียงได้ไกลสุดแนวทแยงทั้งหน้าและหลัง แต่เมื่อกินหมากคู่แข่งจะต้องหยุดลงบนช่องว่างติดด้านหลังตัวที่กินทันที (ตา 22)",
        explanationEn:
          "A Flying King slides any distance diagonally, but on a capture it must halt on the square immediately behind the jumped piece.",
        promptTh: "ใช้ฮอสตา 4 บินกิน 18 แล้วจอดที่ตา 22 เพื่อต่อกิน 26 ถึง 31",
        promptEn: "Fly your King from 4 over 18, halt on 22, and continue over 26 to 31.",
        hintTh: "ฮอส 4 กิน 18 ต้องหยุดที่ 22 แล้วหักมุมกิน 26 ลง 31 (4x22x31)",
        hintEn: "Play 4x22x31—halting on 22 aligns the second jump over 26.",
        fen: "W:WK4:B18,26",
        acceptedMoves: ["4x22x31"],
        demoArrow: { from: 4, to: 31, path: [4, 22, 31] },
      },
      {
        id: "basics-king-reversal",
        titleTh: "หยิบหมากออกทันทีและหักศอก 180°",
        titleEn: "Immediate Removal & 180° Reversal",
        explanationTh:
          "หมากที่ถูกกินจะถูกหยิบออกจากกระดานทันทีระหว่างจังหวะกิน ฮอสจึงสามารถหักศอกกลับ 180° ผ่านช่องที่เพิ่งว่างเพื่อกินต่อได้",
        explanationEn:
          "Captured pieces are removed immediately mid-sequence, allowing a Flying King to reverse 180° back through the newly cleared square.",
        promptTh: "กิน 11 ลงตา 8 ก่อน แล้วหักศอก 180° กวาด 18 และ 26 ถึง 31",
        promptEn: "Jump 11 to 8 first, then reverse 180° to sweep 18 and 26 to square 31.",
        hintTh: "เลือก 15x8x22x31 เพื่อกินครบทั้ง 3 ตัวในตาเดียว",
        hintEn: "Play 15x8x22x31 to capture all three Dark pieces.",
        fen: "W:WK15:B11,18,26",
        acceptedMoves: ["15x8x22x31"],
        demoArrow: { from: 15, to: 31, path: [15, 8, 22, 31] },
      },
    ],
  },
];
