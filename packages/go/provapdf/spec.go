// Package provapdf — typed ExamSpec and all subtypes.
//
// Every field mirrors the Rust spec (serde rename_all = "camelCase").
// Optional fields use pointers; slices default to nil/empty.
package provapdf

// ─────────────────────────────────────────────────────────────────────────────
// Root
// ─────────────────────────────────────────────────────────────────────────────

// ExamSpec is the root document passed to GeneratePDF.
type ExamSpec struct {
	Metadata ExamMetadata        `json:"metadata,omitempty"`
	Config   PrintConfig         `json:"config,omitempty"`
	Header   InstitutionalHeader `json:"header,omitempty"`
	Sections []Section           `json:"sections"`
	Appendix *Appendix           `json:"appendix,omitempty"`
}

// ExamMetadata holds optional document-level metadata.
type ExamMetadata struct {
	Title    *string  `json:"title,omitempty"`
	Author   *string  `json:"author,omitempty"`
	Subject  *string  `json:"subject,omitempty"`
	Date     *string  `json:"date,omitempty"`
	Keywords []string `json:"keywords,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// PrintConfig
// ─────────────────────────────────────────────────────────────────────────────

// PrintConfig controls page geometry, typography, and rendering flags.
type PrintConfig struct {
	// Page geometry
	PageSize *PageSize `json:"pageSize,omitempty"`
	Margins  *Margins  `json:"margins,omitempty"`
	Columns  *uint8    `json:"columns,omitempty"` // 1 or 2

	// Typography
	FontSize    *float64     `json:"fontSize,omitempty"`    // pt, default 12
	LineSpacing *LineSpacing `json:"lineSpacing,omitempty"` // default "normal"
	FontFamily  *string      `json:"fontFamily,omitempty"`  // default "body"

	// Answer spaces
	DiscursiveLineHeight *float64             `json:"discursiveLineHeight,omitempty"` // cm, default 0.85
	DiscursiveSpaceType  *DiscursiveSpaceType `json:"discursiveSpaceType,omitempty"` // default "lines"

	// Economy/display flags
	EconomyMode       *bool `json:"economyMode,omitempty"`
	BreakAllQuestions *bool `json:"breakAllQuestions,omitempty"`
	ImageGrayscale    *bool `json:"imageGrayscale,omitempty"`
	AllBlack          *bool `json:"allBlack,omitempty"`

	// Rendering flags
	ShowScore     *bool `json:"showScore,omitempty"`
	HideNumbering *bool `json:"hideNumbering,omitempty"`
	HeaderFull    *bool `json:"headerFull,omitempty"` // default true

	// Multiple-choice layout
	AlternativeSpacingCm    *float64    `json:"alternativeSpacingCm,omitempty"`    // cm, default 0.3
	LetterCase              *LetterCase `json:"letterCase,omitempty"`              // default "upper"
	RemoveColorAlternatives *bool       `json:"removeColorAlternatives,omitempty"`

	// Break behaviour flags
	BreakEnunciation          *bool  `json:"breakEnunciation,omitempty"`
	BreakAlternatives         *bool  `json:"breakAlternatives,omitempty"`
	ForceChoicesWithStatement *uint8 `json:"forceChoicesWithStatement,omitempty"`
	TextQuestionFormat        *uint8 `json:"textQuestionFormat,omitempty"` // default 1

	// Visibility flags
	HideDisciplineName      *bool `json:"hideDisciplineName,omitempty"`
	HideKnowledgeAreaName   *bool `json:"hideKnowledgeAreaName,omitempty"`
	HideQuestionsReferences *bool `json:"hideQuestionsReferences,omitempty"`
	ShowQuestionBoard       *bool `json:"showQuestionBoard,omitempty"`
}

// PageSize is either a named preset ("A4", "Ata") or custom dimensions.
// For presets, use PageSizeA4 or PageSizeAta.
// For custom sizes, use PageSizeCustom.
type PageSize struct {
	preset *string
	custom *PageSizeCustomDims
}

type PageSizeCustomDims struct {
	WidthMm  float64 `json:"widthMm"`
	HeightMm float64 `json:"heightMm"`
}

// PageSizeA4 returns the A4 preset (210×297mm).
func PageSizeA4() *PageSize { s := "A4"; return &PageSize{preset: &s} }

// PageSizeAta returns the ATA preset (200×266mm).
func PageSizeAta() *PageSize { s := "Ata"; return &PageSize{preset: &s} }

// PageSizeCustom returns a custom page size in millimetres.
func PageSizeCustom(widthMm, heightMm float64) *PageSize {
	return &PageSize{custom: &PageSizeCustomDims{WidthMm: widthMm, HeightMm: heightMm}}
}

func (p PageSize) MarshalJSON() ([]byte, error) {
	if p.custom != nil {
		return jsonMarshal(p.custom)
	}
	if p.preset != nil {
		return jsonMarshal(*p.preset)
	}
	return jsonMarshal("A4")
}

// Margins in centimetres.
type Margins struct {
	Top    *float64 `json:"top,omitempty"`    // default 0.6
	Bottom *float64 `json:"bottom,omitempty"` // default 0.6
	Left   *float64 `json:"left,omitempty"`   // default 1.5
	Right  *float64 `json:"right,omitempty"`  // default 1.5
}

// LineSpacing controls the inter-baseline multiplier.
type LineSpacing = string

const (
	LineSpacingNormal       LineSpacing = "normal"
	LineSpacingOneAndHalf   LineSpacing = "oneAndHalf"
	LineSpacingTwoAndHalf   LineSpacing = "twoAndHalf"
	LineSpacingThreeAndHalf LineSpacing = "threeAndHalf"
)

// DiscursiveSpaceType controls how textual answer space is rendered.
type DiscursiveSpaceType = string

const (
	DiscursiveSpaceLines    DiscursiveSpaceType = "lines"
	DiscursiveSpaceBlank    DiscursiveSpaceType = "blank"
	DiscursiveSpaceNoBorder DiscursiveSpaceType = "noBorder"
)

// LetterCase controls casing of auto-generated choice labels.
type LetterCase = string

const (
	LetterCaseUpper LetterCase = "upper"
	LetterCaseLower LetterCase = "lower"
)

// ─────────────────────────────────────────────────────────────────────────────
// Section
// ─────────────────────────────────────────────────────────────────────────────

// Section is a labeled group of questions.
type Section struct {
	Title          *string         `json:"title,omitempty"`
	Instructions   []InlineContent `json:"instructions,omitempty"`
	Questions      []Question      `json:"questions"`
	Category       *string         `json:"category,omitempty"`
	Style          *Style          `json:"style,omitempty"`
	ForcePageBreak *bool           `json:"forcePageBreak,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// Question
// ─────────────────────────────────────────────────────────────────────────────

// QuestionKind identifies the type of answer expected.
type QuestionKind = string

const (
	QuestionKindChoice  QuestionKind = "choice"
	QuestionKindTextual QuestionKind = "textual"
	QuestionKindCloze   QuestionKind = "cloze"
	QuestionKindSum     QuestionKind = "sum"
	QuestionKindEssay   QuestionKind = "essay"
	QuestionKindFile    QuestionKind = "file"
)

// Question is a single exam question.
type Question struct {
	// Identity
	Number *uint32 `json:"number,omitempty"`
	Label  *string `json:"label,omitempty"`

	// Content
	Kind   QuestionKind    `json:"kind"`
	Stem   []InlineContent `json:"stem"`
	Answer AnswerSpace     `json:"answer"`

	// Supporting material
	BaseTexts []BaseText `json:"baseTexts,omitempty"`

	// Scoring
	Points *float64 `json:"points,omitempty"`

	// Layout modifiers
	FullWidth      *bool    `json:"fullWidth,omitempty"`
	DraftLines     *uint32  `json:"draftLines,omitempty"`
	DraftLineHeight *float64 `json:"draftLineHeight,omitempty"`
	ShowNumber     *bool    `json:"showNumber,omitempty"` // default true
	ForcePageBreak *bool    `json:"forcePageBreak,omitempty"`

	// Style override
	Style *Style `json:"style,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// AnswerSpace — discriminated union via "type" field
// ─────────────────────────────────────────────────────────────────────────────

// AnswerSpace is serialised with a "type" discriminator.
// Use the constructor functions: NewChoiceAnswer, NewTextualAnswer, etc.
type AnswerSpace struct {
	Type string `json:"type"`

	// Choice fields
	Alternatives []Alternative      `json:"alternatives,omitempty"`
	Layout       *AlternativeLayout `json:"layout,omitempty"`

	// Textual fields
	LineCount     *uint32  `json:"lineCount,omitempty"`
	BlankHeightCm *float64 `json:"blankHeightCm,omitempty"`
	LineHeightCm  *float64 `json:"lineHeightCm,omitempty"`

	// Cloze fields
	WordBank       [][]InlineContent `json:"wordBank,omitempty"`
	ShuffleDisplay *bool             `json:"shuffleDisplay,omitempty"`

	// Sum fields
	Items      []SumItem `json:"items,omitempty"`
	ShowSumBox *bool     `json:"showSumBox,omitempty"`

	// Essay fields  (reuses LineCount and adds HeightCm)
	HeightCm *float64 `json:"heightCm,omitempty"`

	// File fields
	FileLabel *string `json:"label,omitempty"`
}

// AlternativeLayout controls arrangement of choice options.
type AlternativeLayout = string

const (
	AlternativeLayoutVertical   AlternativeLayout = "vertical"
	AlternativeLayoutHorizontal AlternativeLayout = "horizontal"
)

// Alternative is a single choice option (A, B, C, …).
type Alternative struct {
	Label   string          `json:"label"`
	Content []InlineContent `json:"content"`
}

// SumItem is one selectable item in a sum question.
type SumItem struct {
	Value   uint32          `json:"value"` // 1, 2, 4, 8, 16, 32, 64…
	Content []InlineContent `json:"content"`
}

// ─────────────────────────────────────────────────────────────────────────────
// BaseText
// ─────────────────────────────────────────────────────────────────────────────

// BaseTextPosition determines where a BaseText block is rendered.
type BaseTextPosition = string

const (
	BaseTextBeforeQuestion  BaseTextPosition = "beforeQuestion"
	BaseTextAfterQuestion   BaseTextPosition = "afterQuestion"
	BaseTextLeftOfQuestion  BaseTextPosition = "leftOfQuestion"
	BaseTextRightOfQuestion BaseTextPosition = "rightOfQuestion"
	BaseTextSectionTop      BaseTextPosition = "sectionTop"
	BaseTextExamTop         BaseTextPosition = "examTop"
	BaseTextExamBottom      BaseTextPosition = "examBottom"
)

// BaseText is a supporting text, figure, or quotation.
type BaseText struct {
	Content     []InlineContent  `json:"content"`
	Position    BaseTextPosition `json:"position"`
	Title       *string          `json:"title,omitempty"`
	Attribution *string          `json:"attribution,omitempty"`
	Style       *Style           `json:"style,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// InstitutionalHeader
// ─────────────────────────────────────────────────────────────────────────────

// InstitutionalHeader is rendered at the top of page 1.
type InstitutionalHeader struct {
	Institution   *string         `json:"institution,omitempty"`
	Title         *string         `json:"title,omitempty"`
	Subject       *string         `json:"subject,omitempty"`
	Year          *string         `json:"year,omitempty"`
	LogoKey       *string         `json:"logoKey,omitempty"`
	StudentFields []StudentField  `json:"studentFields,omitempty"`
	RunningHeader *RunningHeader  `json:"runningHeader,omitempty"`
	RunningFooter *RunningHeader  `json:"runningFooter,omitempty"`
	Instructions  []InlineContent `json:"instructions,omitempty"`
}

// StudentField is a labelled blank line for the student to fill in.
type StudentField struct {
	Label   string   `json:"label"`
	WidthCm *float64 `json:"widthCm,omitempty"`
}

// RunningHeader defines text for page headers/footers with {page}/{pages} tokens.
type RunningHeader struct {
	Left   *string `json:"left,omitempty"`
	Center *string `json:"center,omitempty"`
	Right  *string `json:"right,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// InlineContent — discriminated union via "type" field
// ─────────────────────────────────────────────────────────────────────────────

// InlineContent represents any inline element within stems, alternatives, etc.
// Use the constructor functions: TextContent, MathContent, etc.
type InlineContent struct {
	Type string `json:"type"`

	// Text fields
	Value *string `json:"value,omitempty"`
	Style *Style  `json:"style,omitempty"`

	// Math fields
	Latex   *string `json:"latex,omitempty"`
	Display *bool   `json:"display,omitempty"`

	// Image fields
	Key      *string  `json:"key,omitempty"`
	WidthCm  *float64 `json:"widthCm,omitempty"`
	HeightCm *float64 `json:"heightCm,omitempty"`
	Caption  *string  `json:"caption,omitempty"`

	// Sub/Sup fields
	Content []InlineContent `json:"content,omitempty"`
}

// TextContent creates an InlineContent of type "text".
func TextContent(value string) InlineContent {
	return InlineContent{Type: "text", Value: &value}
}

// StyledTextContent creates an InlineContent of type "text" with style.
func StyledTextContent(value string, style Style) InlineContent {
	return InlineContent{Type: "text", Value: &value, Style: &style}
}

// MathContent creates an InlineContent of type "math".
func MathContent(latex string, display bool) InlineContent {
	return InlineContent{Type: "math", Latex: &latex, Display: &display}
}

// ImageContent creates an InlineContent of type "image".
func ImageContent(key string) InlineContent {
	return InlineContent{Type: "image", Key: &key}
}

// SubContent creates an InlineContent of type "sub" (subscript).
func SubContent(children []InlineContent) InlineContent {
	return InlineContent{Type: "sub", Content: children}
}

// SupContent creates an InlineContent of type "sup" (superscript).
func SupContent(children []InlineContent) InlineContent {
	return InlineContent{Type: "sup", Content: children}
}

// BlankContent creates an InlineContent of type "blank" (cloze fill-in).
func BlankContent(widthCm *float64) InlineContent {
	return InlineContent{Type: "blank", WidthCm: widthCm}
}

// ─────────────────────────────────────────────────────────────────────────────
// Style
// ─────────────────────────────────────────────────────────────────────────────

// FontWeight controls font boldness.
type FontWeight = string

const (
	FontWeightNormal FontWeight = "normal"
	FontWeightBold   FontWeight = "bold"
)

// FontStyle controls italic rendering.
type FontStyleValue = string

const (
	FontStyleNormal FontStyleValue = "normal"
	FontStyleItalic FontStyleValue = "italic"
)

// TextAlign controls horizontal text alignment.
type TextAlign = string

const (
	TextAlignLeft      TextAlign = "left"
	TextAlignCenter    TextAlign = "center"
	TextAlignRight     TextAlign = "right"
	TextAlignJustified TextAlign = "justified"
)

// Style is a partial style override — nil fields cascade from context.
type Style struct {
	FontSize        *float64        `json:"fontSize,omitempty"`
	FontWeight      *FontWeight     `json:"fontWeight,omitempty"`
	FontStyle       *FontStyleValue `json:"fontStyle,omitempty"`
	FontFamily      *string         `json:"fontFamily,omitempty"`
	Color           *string         `json:"color,omitempty"`
	BackgroundColor *string         `json:"backgroundColor,omitempty"`
	Underline       *bool           `json:"underline,omitempty"`
	TextAlign       *TextAlign      `json:"textAlign,omitempty"`
}

// ─────────────────────────────────────────────────────────────────────────────
// Appendix
// ─────────────────────────────────────────────────────────────────────────────

// Appendix is rendered after all sections.
type Appendix struct {
	Title   *string        `json:"title,omitempty"`
	Content []AppendixItem `json:"content"`
}

// AppendixItem is a discriminated union via "type".
type AppendixItem struct {
	Type string `json:"type"` // "block", "formulaSheet", or "pageBreak"

	// Block fields
	Content     []InlineContent `json:"content,omitempty"`
	BlockTitle  *string         `json:"title,omitempty"`
	BlockStyle  *Style          `json:"style,omitempty"`

	// FormulaSheet fields
	Formulas []FormulaEntry `json:"formulas,omitempty"`
}

// FormulaEntry is a single formula in a formula sheet.
type FormulaEntry struct {
	Label *string `json:"label,omitempty"`
	Latex string  `json:"latex"`
}

// ─────────────────────────────────────────────────────────────────────────────
// FontRulesInput
// ─────────────────────────────────────────────────────────────────────────────

// FontRulesInput is already defined in provapdf.go as FontRules.
// Re-exported here for documentation: overrides font-role → family-name.
//
//	type FontRules struct {
//	    Body     string `json:"body,omitempty"`
//	    Heading  string `json:"heading,omitempty"`
//	    Question string `json:"question,omitempty"`
//	    Math     string `json:"math,omitempty"`
//	}
