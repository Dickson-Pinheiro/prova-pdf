/**
 * prova-pdf — Complete TypeScript type definitions for the ExamSpec schema.
 *
 * Every field mirrors the Rust spec (serde rename_all = "camelCase").
 * These interfaces describe the JSON structure accepted by `generate_pdf()`.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Root
// ─────────────────────────────────────────────────────────────────────────────

export interface ExamSpec {
  metadata?: ExamMetadata;
  config?: PrintConfig;
  header?: InstitutionalHeader;
  sections: Section[];
  appendix?: Appendix;
}

export interface ExamMetadata {
  title?: string;
  author?: string;
  subject?: string;
  date?: string;
  keywords?: string[];
}

// ─────────────────────────────────────────────────────────────────────────────
// PrintConfig
// ─────────────────────────────────────────────────────────────────────────────

export interface PrintConfig {
  // Page geometry
  pageSize?: PageSize;
  margins?: Margins;
  /** 1 or 2 columns. Default 1. */
  columns?: 1 | 2;

  // Typography
  /** Base font size in pt. Default 12. */
  fontSize?: number;
  lineSpacing?: LineSpacing;
  /** Named font family from FontRegistry. Default "body". */
  fontFamily?: string;

  // Answer spaces
  /** Height of each discursive answer line in cm. Default 0.85. */
  discursiveLineHeight?: number;
  discursiveSpaceType?: DiscursiveSpaceType;

  // Economy/display flags
  economyMode?: boolean;
  breakAllQuestions?: boolean;
  imageGrayscale?: boolean;
  allBlack?: boolean;

  // Rendering flags
  showScore?: boolean;
  hideNumbering?: boolean;
  /** Show full institutional header with student fields. Default true. */
  headerFull?: boolean;

  // Multiple-choice layout
  /** Vertical gap between alternatives in cm. Default 0.3. */
  alternativeSpacingCm?: number;
  letterCase?: LetterCase;
  /** Remove coloured badges; show plain text alternatives. */
  removeColorAlternatives?: boolean;

  // Break behaviour flags
  breakEnunciation?: boolean;
  breakAlternatives?: boolean;
  forceChoicesWithStatement?: number;
  /** Textual question format: 0 = no answer lines, 1 = show lines (default). */
  textQuestionFormat?: number;

  // Visibility flags
  hideDisciplineName?: boolean;
  hideKnowledgeAreaName?: boolean;
  hideQuestionsReferences?: boolean;
  showQuestionBoard?: boolean;
}

export type PageSize = "A4" | "Ata" | CustomPageSize;

export interface CustomPageSize {
  widthMm: number;
  heightMm: number;
}

export interface Margins {
  /** Top margin in cm. Default 0.6. */
  top?: number;
  /** Bottom margin in cm. Default 0.6. */
  bottom?: number;
  /** Left margin in cm. Default 1.5. */
  left?: number;
  /** Right margin in cm. Default 1.5. */
  right?: number;
}

export type LineSpacing = "normal" | "oneAndHalf" | "twoAndHalf" | "threeAndHalf";

export type DiscursiveSpaceType = "lines" | "blank" | "noBorder";

export type LetterCase = "upper" | "lower";

// ─────────────────────────────────────────────────────────────────────────────
// Section
// ─────────────────────────────────────────────────────────────────────────────

export interface Section {
  title?: string;
  instructions?: InlineContent[];
  questions: Question[];
  category?: string;
  style?: Style;
  forcePageBreak?: boolean;
}

// ─────────────────────────────────────────────────────────────────────────────
// Question
// ─────────────────────────────────────────────────────────────────────────────

export type QuestionKind = "choice" | "textual" | "cloze" | "sum" | "essay" | "file";

export interface Question {
  // Identity
  number?: number;
  label?: string;

  // Content
  kind: QuestionKind;
  stem: InlineContent[];
  answer: AnswerSpace;

  // Supporting material
  baseTexts?: BaseText[];

  // Scoring
  points?: number;

  // Layout modifiers
  fullWidth?: boolean;
  draftLines?: number;
  draftLineHeight?: number;
  /** Whether to render the question number badge. Default true. */
  showNumber?: boolean;
  forcePageBreak?: boolean;

  // Style override
  style?: Style;
}

// ─────────────────────────────────────────────────────────────────────────────
// AnswerSpace — discriminated union via "type"
// ─────────────────────────────────────────────────────────────────────────────

export type AnswerSpace =
  | ChoiceAnswer
  | TextualAnswer
  | ClozeAnswer
  | SumAnswer
  | EssayAnswer
  | FileAnswer;

export interface ChoiceAnswer {
  type: "choice";
  alternatives: Alternative[];
  layout?: AlternativeLayout;
}

export interface TextualAnswer {
  type: "textual";
  lineCount?: number;
  blankHeightCm?: number;
  lineHeightCm?: number;
}

export interface ClozeAnswer {
  type: "cloze";
  wordBank: InlineContent[][];
  shuffleDisplay?: boolean;
}

export interface SumAnswer {
  type: "sum";
  items: SumItem[];
  /** Whether to show the sum box. Default true. */
  showSumBox?: boolean;
}

export interface EssayAnswer {
  type: "essay";
  lineCount?: number;
  heightCm?: number;
}

export interface FileAnswer {
  type: "file";
  label?: string;
}

export type AlternativeLayout = "vertical" | "horizontal";

export interface Alternative {
  label: string;
  content: InlineContent[];
}

export interface SumItem {
  value: number;
  content: InlineContent[];
}

// ─────────────────────────────────────────────────────────────────────────────
// BaseText
// ─────────────────────────────────────────────────────────────────────────────

export type BaseTextPosition =
  | "beforeQuestion"
  | "afterQuestion"
  | "leftOfQuestion"
  | "rightOfQuestion"
  | "sectionTop"
  | "examTop"
  | "examBottom";

export interface BaseText {
  content: InlineContent[];
  position: BaseTextPosition;
  title?: string;
  attribution?: string;
  style?: Style;
}

// ─────────────────────────────────────────────────────────────────────────────
// InstitutionalHeader
// ─────────────────────────────────────────────────────────────────────────────

export interface InstitutionalHeader {
  institution?: string;
  title?: string;
  subject?: string;
  year?: string;
  logoKey?: string;
  studentFields?: StudentField[];
  runningHeader?: RunningHeader;
  runningFooter?: RunningHeader;
  instructions?: InlineContent[];
}

export interface StudentField {
  label: string;
  widthCm?: number;
}

export interface RunningHeader {
  left?: string;
  center?: string;
  right?: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// InlineContent — discriminated union via "type"
// ─────────────────────────────────────────────────────────────────────────────

export type InlineContent =
  | InlineText
  | InlineMath
  | InlineImage
  | InlineSub
  | InlineSup
  | InlineBlank;

export interface InlineText {
  type: "text";
  value: string;
  style?: Style;
}

export interface InlineMath {
  type: "math";
  latex: string;
  /** true = display (centred, full width), false = inline. Default false. */
  display?: boolean;
}

export interface InlineImage {
  type: "image";
  /** Key registered via add_image(). */
  key: string;
  widthCm?: number;
  heightCm?: number;
  caption?: string;
}

export interface InlineSub {
  type: "sub";
  content: InlineContent[];
}

export interface InlineSup {
  type: "sup";
  content: InlineContent[];
}

export interface InlineBlank {
  type: "blank";
  /** Width of the blank in cm. Default 3.5. */
  widthCm?: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Style
// ─────────────────────────────────────────────────────────────────────────────

export type FontWeight = "normal" | "bold";
export type FontStyle = "normal" | "italic";
export type TextAlign = "left" | "center" | "right" | "justified";

/** Partial style — undefined fields cascade from context. */
export interface Style {
  fontSize?: number;
  fontWeight?: FontWeight;
  fontStyle?: FontStyle;
  fontFamily?: string;
  color?: string;
  backgroundColor?: string;
  underline?: boolean;
  textAlign?: TextAlign;
}

// ─────────────────────────────────────────────────────────────────────────────
// Appendix
// ─────────────────────────────────────────────────────────────────────────────

export interface Appendix {
  title?: string;
  content: AppendixItem[];
}

export type AppendixItem = AppendixBlock | AppendixFormulaSheet | AppendixPageBreak;

export interface AppendixBlock {
  type: "block";
  content: InlineContent[];
  title?: string;
  style?: Style;
}

export interface AppendixFormulaSheet {
  type: "formulaSheet";
  title?: string;
  formulas: FormulaEntry[];
}

export interface AppendixPageBreak {
  type: "pageBreak";
}

export interface FormulaEntry {
  label?: string;
  latex: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// FontRulesInput
// ─────────────────────────────────────────────────────────────────────────────

export interface FontRulesInput {
  body?: string;
  heading?: string;
  question?: string;
  math?: string;
}
