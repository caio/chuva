use std::ops::Range;

// XXX The only reason iterators are made to implement Copy
//     here is because askama's Template proc macro generates
//     code that moves the iterator
#[derive(Clone, Copy)]
pub struct Lexer<'a> {
    src: Tokenizer<'a>,
    merge_state: Option<MergeState>,
    stash: Option<CopyToken>,
}

#[derive(Debug, PartialEq)]
pub enum Expr {
    Showers { range: Range<usize>, gaps: usize },
    Rain(Range<usize>),
    Dry(Range<usize>),
}

impl<'a> Lexer<'a> {
    pub fn new(slot: usize, src: &'a [f32]) -> Self {
        Self::from_tokenizer(Tokenizer::new(slot, src))
    }

    fn from_tokenizer(mut src: Tokenizer<'a>) -> Self {
        // This is done so that if the first token is dry
        // it doesn't get merged into a shower
        let mut stash = None;
        let mut merge_state = None;
        if let Some(next) = src.next() {
            if next.is_dry() {
                stash = Some(next.into());
            } else {
                merge_state = Some(MergeState::new(next));
            }
        }

        Self {
            src,
            stash,
            merge_state,
        }
    }

    // Merges tiny Dry gaps into big rain
    fn next(&mut self) -> Option<Expr> {
        if let Some(tok) = self.stash.take() {
            return Some(tok.into());
        }

        for tok in &mut self.src {
            // dry and long: emit
            if tok.is_dry() && tok.len() > 1 {
                if let Some(merge_state) = self.merge_state.take() {
                    self.stash = Some(tok.into());
                    return Some(merge_state.into_expr());
                } else {
                    return Some(tok.into());
                }
            }

            if let Some(merge_state) = &mut self.merge_state {
                merge_state.merge(tok);
            } else {
                self.merge_state = Some(MergeState::new(tok));
            }
        }
        assert!(self.stash.is_none());

        if let Some(dry) = self
            .merge_state
            .as_mut()
            .and_then(|state| state.undo_last_dry_merge())
        {
            self.stash = Some(dry);
            let state = self.merge_state.take().unwrap();
            return Some(state.into_expr());
        }

        assert!(self.stash.is_none());
        self.merge_state.take().map(|s| s.into_expr())
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Expr;

    fn next(&mut self) -> Option<Self::Item> {
        Self::next(self)
    }
}

#[derive(Debug, PartialEq, Clone)]
enum Token {
    Rain(Range<usize>),
    Dry(Range<usize>),
}

impl Token {
    fn len(&self) -> usize {
        match self {
            Token::Rain(range) => range.len(),
            Token::Dry(range) => range.len(),
        }
    }

    fn into_range(self) -> Range<usize> {
        match self {
            Token::Rain(range) => range,
            Token::Dry(range) => range,
        }
    }

    #[inline]
    fn is_dry(&self) -> bool {
        matches!(self, Token::Dry(_))
    }
}

// boolean itertools::chunk_by, but worse
#[derive(Clone, Copy)]
struct Tokenizer<'a> {
    pos: usize,
    preds: &'a [f32],
}

impl<'a> Tokenizer<'a> {
    // Takes `pos` as an offset to the slice instead of
    // just a slice so that the output ranges all refer
    // to the beginning of the prediction
    //
    // This way the code that transforms these into human
    // readable time has to dance less
    fn new(pos: usize, preds: &'a [f32]) -> Self {
        Self { pos, preds }
    }

    fn next(&mut self) -> Option<Token> {
        if self.pos >= self.preds.len() {
            return None;
        }

        // XXX could have the loop before the branches so it
        //     reads more concisely

        // Rain
        if self.preds[self.pos] > 0f32 {
            if let Some((end, _)) = self
                .preds
                .iter()
                .enumerate()
                .skip(self.pos + 1)
                .find(|x| x.1 == &0f32)
            {
                let start = self.pos;
                self.pos = end;
                return Some(Token::Rain(start..end));
            } else {
                let start = self.pos;
                let end = self.preds.len();
                self.pos = end;
                return Some(Token::Rain(start..end));
            }
        }

        // Dry
        // Same as above, but the position condition
        // is reversed
        if let Some((end, _)) = self
            .preds
            .iter()
            .enumerate()
            .skip(self.pos + 1)
            .find(|x| x.1 > &0f32)
        {
            let start = self.pos;
            self.pos = end;
            Some(Token::Dry(start..end))
        } else {
            let start = self.pos;
            let end = self.preds.len();
            self.pos = end;
            Some(Token::Dry(start..end))
        }
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        Self::next(self)
    }
}

#[derive(Clone, Copy)]
enum CopyToken {
    Rain((usize, usize)),
    Dry((usize, usize)),
}

impl From<Token> for CopyToken {
    fn from(value: Token) -> Self {
        match value {
            Token::Rain(range) => Self::Rain((range.start, range.end)),
            Token::Dry(range) => Self::Dry((range.start, range.end)),
        }
    }
}

#[derive(Clone, Copy)]
struct MergeState {
    start: usize,
    end: usize,
    num_gaps: usize,
    last_was_dry: bool,
}

impl MergeState {
    fn new(tok: Token) -> Self {
        let num_gaps = if tok.is_dry() { 1 } else { 0 };
        let range = tok.into_range();
        Self {
            start: range.start,
            end: range.end,
            num_gaps,
            last_was_dry: false,
        }
    }

    fn merge(&mut self, tok: Token) {
        self.last_was_dry = false;
        if tok.is_dry() {
            self.num_gaps += 1;
            self.last_was_dry = true;
        }
        self.end = tok.into_range().end;
    }

    fn undo_last_dry_merge(&mut self) -> Option<CopyToken> {
        if self.last_was_dry {
            self.num_gaps -= 1;
            self.last_was_dry = false;
            let old_end = self.end;
            self.end -= 1;
            Some(CopyToken::Dry((self.end, old_end)))
        } else {
            None
        }
    }

    fn into_expr(self) -> Expr {
        let range = self.start..self.end;
        if range.len() == 1 {
            if self.num_gaps == 1 {
                Expr::Dry(range)
            } else {
                Expr::Rain(range)
            }
        } else if self.num_gaps == 0 {
            Expr::Rain(range)
        } else {
            Expr::Showers {
                range,
                gaps: self.num_gaps,
            }
        }
    }
}
impl From<CopyToken> for Expr {
    fn from(tok: CopyToken) -> Self {
        match tok {
            CopyToken::Rain((start, end)) => Expr::Rain(start..end),
            CopyToken::Dry((start, end)) => Expr::Dry(start..end),
        }
    }
}

impl From<Token> for Expr {
    fn from(tok: Token) -> Self {
        match tok {
            Token::Rain(range) => Expr::Rain(range),
            Token::Dry(range) => Expr::Dry(range),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Expr, Lexer, Token, Tokenizer};
    use chuva::Prediction;

    fn iter_tokens(pos: usize, preds: &[f32]) -> impl Iterator<Item = Token> {
        Tokenizer::new(pos, preds)
    }

    fn interpret(pos: usize, data: &[f32]) -> impl Iterator<Item = Expr> {
        Lexer::from_tokenizer(Tokenizer::new(pos, data))
    }

    // shape: ▃▄▄▆▆▅▁          ▁▄▅▄▂
    const SAMPLE: Prediction<'static> = &[
        0.48, 0.84, 1.92, 4.32, 5.52, 2.76, 0.12, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.12, 1.56, 3.24, 1.92, 0.24, 0.0, 0.0, 0.0,
    ];

    #[test]
    fn tokenization_works() {
        let spans = iter_tokens(0, SAMPLE).collect::<Vec<_>>();

        assert_eq!(
            vec![
                Token::Rain(0..7),
                Token::Dry(7..17),
                Token::Rain(17..22),
                Token::Dry(22..25)
            ],
            spans
        );
    }

    #[test]
    fn out_of_bounds_offset_yields_none() {
        assert_eq!(
            None,
            iter_tokens(25, SAMPLE).next(),
            "out of bounds should yield None"
        );
    }

    #[test]
    fn singles() {
        assert_eq!(Some(Expr::Dry(0..1)), interpret(0, &[0.0]).next());
        assert_eq!(Some(Expr::Rain(0..1)), interpret(0, &[1.0]).next());
    }

    #[test]
    fn doesnt_merge_first_dry_token() {
        let mut iter = interpret(0, &[0.0, 1.2]);
        assert_eq!(Some(Expr::Dry(0..1)), iter.next());
        assert_eq!(Some(Expr::Rain(1..2)), iter.next());
        assert_eq!(None, iter.next());
    }

    #[test]
    fn doesnt_merge_last_single_dry() {
        let output = interpret(0, &[1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0]).collect::<Vec<_>>();
        assert_eq!(
            vec![
                Expr::Rain(0..1),
                Expr::Dry(1..3),
                Expr::Rain(3..6),
                Expr::Dry(6..7)
            ],
            output
        );
    }

    // shape:     ▄▄▁ ▁▁ ▁▁▁
    // I'd like some less noisy output for this one
    // i.e.: don't consider very brief dry spans as dry
    const SHOWERS: Prediction<'static> = &[
        0.0, 0.0, 0.0, 0.0, 0.72, 1.20, 0.12, 0.0, 0.12, 0.12, 0.0, 0.12, 0.12, 0.12, 0.0, 0.0,
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ];

    #[test]
    fn merges_tiny_gaps() {
        let output = interpret(0, SHOWERS).collect::<Vec<_>>();
        assert_eq!(
            vec![
                Expr::Dry(0..4),
                Expr::Showers {
                    range: 4..14,
                    gaps: 2
                },
                Expr::Dry(14..25),
            ],
            output
        );
    }
}
