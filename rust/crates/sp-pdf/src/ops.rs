//! pdf.js's operator table and its EvaluatorPreprocessor.
//!
//! Ported from pdf.js `src/core/evaluator.js` (Mozilla Foundation, Apache
//! License 2.0): the same argument-count repair (surplus arguments carried
//! to the next operator, missing ones borrowed back), the same refusal past
//! 33 operands, and the same q/Q/cm bookkeeping done while reading.

use crate::lexer::{FormatError, Parser, Tok};
use crate::obj::Obj;

/// pdf.js `OPS`, the ones a content stream can produce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    SetLineWidth,
    SetLineCap,
    SetLineJoin,
    SetMiterLimit,
    SetDash,
    SetRenderingIntent,
    SetFlatness,
    SetGState,
    Save,
    Restore,
    Transform,
    MoveTo,
    LineTo,
    CurveTo,
    CurveTo2,
    CurveTo3,
    ClosePath,
    Rectangle,
    Stroke,
    CloseStroke,
    Fill,
    EoFill,
    FillStroke,
    EoFillStroke,
    CloseFillStroke,
    CloseEoFillStroke,
    EndPath,
    Clip,
    EoClip,
    BeginText,
    EndText,
    SetCharSpacing,
    SetWordSpacing,
    SetHScale,
    SetLeading,
    SetFont,
    SetTextRenderingMode,
    SetTextRise,
    MoveText,
    SetLeadingMoveText,
    SetTextMatrix,
    NextLine,
    ShowText,
    ShowSpacedText,
    NextLineShowText,
    NextLineSetSpacingShowText,
    SetCharWidth,
    SetCharWidthAndBounds,
    SetStrokeColorSpace,
    SetFillColorSpace,
    SetStrokeColor,
    SetStrokeColorN,
    SetFillColor,
    SetFillColorN,
    SetStrokeGray,
    SetFillGray,
    SetStrokeRgbColor,
    SetFillRgbColor,
    SetStrokeCmykColor,
    SetFillCmykColor,
    ShadingFill,
    BeginInlineImage,
    BeginImageData,
    EndInlineImage,
    PaintXObject,
    MarkPoint,
    MarkPointProps,
    BeginMarkedContent,
    BeginMarkedContentProps,
    EndMarkedContent,
    BeginCompat,
    EndCompat,
}

impl Op {
    /// The path-construction and path-painting range pdf.js tests with
    /// `fn >= OPS.moveTo && fn <= OPS.endPath`.
    pub fn is_path_op(self) -> bool {
        matches!(
            self,
            Op::MoveTo
                | Op::LineTo
                | Op::CurveTo
                | Op::CurveTo2
                | Op::CurveTo3
                | Op::ClosePath
                | Op::Rectangle
                | Op::Stroke
                | Op::CloseStroke
                | Op::Fill
                | Op::EoFill
                | Op::FillStroke
                | Op::EoFillStroke
                | Op::CloseFillStroke
                | Op::CloseEoFillStroke
                | Op::EndPath
        )
    }

    /// pdf.js's OPS name, which is what the oracle dump writes for a paint op.
    pub fn js_name(self) -> &'static str {
        match self {
            Op::Stroke => "stroke",
            Op::CloseStroke => "closeStroke",
            Op::Fill => "fill",
            Op::EoFill => "eoFill",
            Op::FillStroke => "fillStroke",
            Op::EoFillStroke => "eoFillStroke",
            Op::CloseFillStroke => "closeFillStroke",
            Op::CloseEoFillStroke => "closeEOFillStroke",
            Op::EndPath => "endPath",
            _ => "other",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OpSpec {
    pub id: Op,
    pub num_args: usize,
    pub variable_args: bool,
}

const fn s(id: Op, num_args: usize, variable_args: bool) -> Option<OpSpec> {
    Some(OpSpec {
        id,
        num_args,
        variable_args,
    })
}

/// pdf.js `EvaluatorPreprocessor.opMap`, entries with no spec included: they
/// are "known" to the lexer (so `BMC`'s prefix `BM` stops a token) while the
/// preprocessor skips them as unknown commands.
pub static OP_MAP: [(&str, Option<OpSpec>); 83] = [
    ("w", s(Op::SetLineWidth, 1, false)),
    ("J", s(Op::SetLineCap, 1, false)),
    ("j", s(Op::SetLineJoin, 1, false)),
    ("M", s(Op::SetMiterLimit, 1, false)),
    ("d", s(Op::SetDash, 2, false)),
    ("ri", s(Op::SetRenderingIntent, 1, false)),
    ("i", s(Op::SetFlatness, 1, false)),
    ("gs", s(Op::SetGState, 1, false)),
    ("q", s(Op::Save, 0, false)),
    ("Q", s(Op::Restore, 0, false)),
    ("cm", s(Op::Transform, 6, false)),
    ("m", s(Op::MoveTo, 2, false)),
    ("l", s(Op::LineTo, 2, false)),
    ("c", s(Op::CurveTo, 6, false)),
    ("v", s(Op::CurveTo2, 4, false)),
    ("y", s(Op::CurveTo3, 4, false)),
    ("h", s(Op::ClosePath, 0, false)),
    ("re", s(Op::Rectangle, 4, false)),
    ("S", s(Op::Stroke, 0, false)),
    ("s", s(Op::CloseStroke, 0, false)),
    ("f", s(Op::Fill, 0, false)),
    ("F", s(Op::Fill, 0, false)),
    ("f*", s(Op::EoFill, 0, false)),
    ("B", s(Op::FillStroke, 0, false)),
    ("B*", s(Op::EoFillStroke, 0, false)),
    ("b", s(Op::CloseFillStroke, 0, false)),
    ("b*", s(Op::CloseEoFillStroke, 0, false)),
    ("n", s(Op::EndPath, 0, false)),
    ("W", s(Op::Clip, 0, false)),
    ("W*", s(Op::EoClip, 0, false)),
    ("BT", s(Op::BeginText, 0, false)),
    ("ET", s(Op::EndText, 0, false)),
    ("Tc", s(Op::SetCharSpacing, 1, false)),
    ("Tw", s(Op::SetWordSpacing, 1, false)),
    ("Tz", s(Op::SetHScale, 1, false)),
    ("TL", s(Op::SetLeading, 1, false)),
    ("Tf", s(Op::SetFont, 2, false)),
    ("Tr", s(Op::SetTextRenderingMode, 1, false)),
    ("Ts", s(Op::SetTextRise, 1, false)),
    ("Td", s(Op::MoveText, 2, false)),
    ("TD", s(Op::SetLeadingMoveText, 2, false)),
    ("Tm", s(Op::SetTextMatrix, 6, false)),
    ("T*", s(Op::NextLine, 0, false)),
    ("Tj", s(Op::ShowText, 1, false)),
    ("TJ", s(Op::ShowSpacedText, 1, false)),
    ("'", s(Op::NextLineShowText, 1, false)),
    ("\"", s(Op::NextLineSetSpacingShowText, 3, false)),
    ("d0", s(Op::SetCharWidth, 2, false)),
    ("d1", s(Op::SetCharWidthAndBounds, 6, false)),
    ("CS", s(Op::SetStrokeColorSpace, 1, false)),
    ("cs", s(Op::SetFillColorSpace, 1, false)),
    ("SC", s(Op::SetStrokeColor, 4, true)),
    ("SCN", s(Op::SetStrokeColorN, 33, true)),
    ("sc", s(Op::SetFillColor, 4, true)),
    ("scn", s(Op::SetFillColorN, 33, true)),
    ("G", s(Op::SetStrokeGray, 1, false)),
    ("g", s(Op::SetFillGray, 1, false)),
    ("RG", s(Op::SetStrokeRgbColor, 3, false)),
    ("rg", s(Op::SetFillRgbColor, 3, false)),
    ("K", s(Op::SetStrokeCmykColor, 4, false)),
    ("k", s(Op::SetFillCmykColor, 4, false)),
    ("sh", s(Op::ShadingFill, 1, false)),
    ("BI", s(Op::BeginInlineImage, 0, false)),
    ("ID", s(Op::BeginImageData, 0, false)),
    ("EI", s(Op::EndInlineImage, 1, false)),
    ("Do", s(Op::PaintXObject, 1, false)),
    ("MP", s(Op::MarkPoint, 1, false)),
    ("DP", s(Op::MarkPointProps, 2, false)),
    ("BMC", s(Op::BeginMarkedContent, 1, false)),
    ("BDC", s(Op::BeginMarkedContentProps, 2, false)),
    ("EMC", s(Op::EndMarkedContent, 0, false)),
    ("BX", s(Op::BeginCompat, 0, false)),
    ("EX", s(Op::EndCompat, 0, false)),
    ("BM", None),
    ("BD", None),
    ("true", None),
    ("fa", None),
    ("fal", None),
    ("fals", None),
    ("false", None),
    ("nu", None),
    ("nul", None),
    ("null", None),
];

/// One operator with its operands.
pub struct Operation {
    pub op: Op,
    pub args: Vec<Obj>,
}

/// What the preprocessor tells its caller about state it tracks itself.
pub trait StateHooks {
    fn save(&mut self);
    fn restore(&mut self);
    fn transform(&mut self, args: &[Obj]);
}

const MAX_INVALID_PATH_OPS: usize = 10;

pub struct Preprocessor<'a> {
    pub parser: Parser<'a>,
    non_processed_args: Vec<Obj>,
    is_path_op: bool,
    num_invalid_path_ops: usize,
}

impl<'a> Preprocessor<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, FormatError> {
        Ok(Self {
            parser: Parser::new(data)?,
            non_processed_args: Vec::new(),
            is_path_op: false,
            num_invalid_path_ops: 0,
        })
    }

    /// `read(operation)`. `Ok(None)` at the end of the stream; `Err` where
    /// pdf.js throws, which ends the stream for its callers.
    pub fn read(&mut self, hooks: &mut dyn StateHooks) -> Result<Option<Operation>, FormatError> {
        let mut args: Vec<Obj> = Vec::new();
        loop {
            let obj = self.parser.get_obj()?;
            match obj {
                Tok::Eof => return Ok(None),
                Tok::Obj(Obj::Cmd(c)) => {
                    let spec = match OP_MAP.iter().find(|(k, _)| *k == &*c) {
                        Some((_, Some(spec))) => *spec,
                        _ => continue, // Unknown command, skipped with its args kept
                    };
                    let fn_ = spec.id;
                    let num_args = spec.num_args;
                    let mut args_length = args.len();
                    if !self.is_path_op {
                        self.num_invalid_path_ops = 0;
                    }
                    self.is_path_op = fn_.is_path_op();
                    if !spec.variable_args {
                        if args_length != num_args {
                            while args_length > num_args {
                                self.non_processed_args.push(args.remove(0));
                                args_length -= 1;
                            }
                            while args_length < num_args && !self.non_processed_args.is_empty() {
                                let a = self.non_processed_args.pop().unwrap_or(Obj::Null);
                                args.insert(0, a);
                                args_length += 1;
                            }
                        }
                        if args_length < num_args {
                            if self.is_path_op {
                                self.num_invalid_path_ops += 1;
                                if self.num_invalid_path_ops > MAX_INVALID_PATH_OPS {
                                    return Err(FormatError);
                                }
                            }
                            args.clear();
                            continue;
                        }
                    }
                    match fn_ {
                        Op::Save => hooks.save(),
                        Op::Restore => hooks.restore(),
                        Op::Transform => hooks.transform(&args),
                        _ => {}
                    }
                    return Ok(Some(Operation { op: fn_, args }));
                }
                Tok::Obj(Obj::Null) => {}
                Tok::Obj(o) => {
                    args.push(o);
                    if args.len() > 33 {
                        return Err(FormatError);
                    }
                }
            }
        }
    }
}
