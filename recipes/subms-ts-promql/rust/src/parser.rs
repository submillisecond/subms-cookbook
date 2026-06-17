//! Hand-rolled lexer + recursive-descent parser for the PromQL subset. No
//! parser-generator, no regex crate: the tokeniser is a byte cursor and the
//! grammar is a precedence-climbing expression parser. The shape it accepts is
//! documented on [`crate`]; anything outside it surfaces as
//! [`TsPromQlError::Parse`].

use crate::TsPromQlError;

/// How a label matcher compares against a series' tag value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchOp {
    /// `label="v"` - exact equality.
    Eq,
    /// `label!="v"` - exact inequality.
    Ne,
    /// `label=~"re"` - pattern match (literal + `.*` only, anchored).
    Re,
    /// `label!~"re"` - negated pattern match.
    Nre,
}

/// One `name<op>"value"` label matcher inside a selector's `{...}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelMatcher {
    pub label: String,
    pub op: MatchOp,
    pub pattern: String,
}

impl LabelMatcher {
    /// Does `value` satisfy this matcher? `Re`/`Nre` go through the
    /// anchored simple-pattern matcher (literal text + `.*` wildcards only).
    pub fn matches(&self, value: Option<&str>) -> bool {
        match self.op {
            MatchOp::Eq => value == Some(self.pattern.as_str()),
            MatchOp::Ne => value != Some(self.pattern.as_str()),
            MatchOp::Re => value.is_some_and(|v| simple_re_match(&self.pattern, v)),
            MatchOp::Nre => !value.is_some_and(|v| simple_re_match(&self.pattern, v)),
        }
    }
}

/// Anchored simple-pattern match. The only metacharacter honoured is `.*`
/// (any run of characters); every other byte is a literal. `.` on its own is
/// a literal dot here, not "any char" - the subset is deliberately narrow and
/// documented as such. Matching is whole-string anchored, matching PromQL's
/// implicit `^...$` on `=~`.
pub fn simple_re_match(pattern: &str, value: &str) -> bool {
    // Split on the `.*` wildcard; each fragment must appear in order, the
    // first anchored at the start and the last anchored at the end.
    let parts: Vec<&str> = pattern.split(".*").collect();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !value[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if i == parts.len() - 1 {
            // last fragment is end-anchored
            if !value[pos..].ends_with(part) || value.len() - pos < part.len() {
                return false;
            }
            pos = value.len();
        } else {
            match value[pos..].find(part) {
                Some(off) => pos += off + part.len(),
                None => return false,
            }
        }
    }
    true
}

/// Cross-series grouping for an aggregation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Grouping {
    /// No `by`/`without`: collapse everything into a single group.
    None,
    /// `by (a, b)` - group by exactly these labels.
    By(Vec<String>),
    /// `without (a, b)` - group by every label except these.
    Without(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggOp {
    Sum,
    Avg,
    Min,
    Max,
    Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FuncKind {
    Rate,
    Irate,
    Increase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Parsed query tree. Range vectors are only legal as the argument of a
/// range function, so they are not a top-level `Expr` variant - they ride
/// inside [`Expr::Func`] as the selector plus its `range_ns`.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A literal number (a PromQL scalar).
    Scalar(f64),
    /// Instant vector selector: metric name + matchers + offset.
    Selector(Selector),
    /// `f(selector[range])` - a range function over a range-vector selector.
    Func {
        kind: FuncKind,
        selector: Selector,
        range_ns: i64,
    },
    /// `op(inner) [by|without (...)]` - an aggregation over an inner expr.
    Agg {
        op: AggOp,
        grouping: Grouping,
        inner: Box<Expr>,
    },
    /// Binary op between two sub-expressions.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub metric: String,
    pub matchers: Vec<LabelMatcher>,
    pub offset_ns: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok<'a> {
    Ident(&'a str),
    Str(&'a str),
    Num(&'a str),
    Duration(&'a str),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    EqMatch, // =~
    NeMatch, // !~
    Eq,      // =
    Ne,      // !=
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn next_tok(&mut self) -> Result<Option<Tok<'a>>, TsPromQlError> {
        self.skip_ws();
        let Some(b) = self.peek_byte() else {
            return Ok(None);
        };
        match b {
            b'(' => self.single(Tok::LParen),
            b')' => self.single(Tok::RParen),
            b'{' => self.single(Tok::LBrace),
            b'}' => self.single(Tok::RBrace),
            b'[' => self.single(Tok::LBrack),
            b']' => self.single(Tok::RBrack),
            b',' => self.single(Tok::Comma),
            b'+' => self.single(Tok::Plus),
            b'-' => self.single(Tok::Minus),
            b'*' => self.single(Tok::Star),
            b'/' => self.single(Tok::Slash),
            b'"' | b'\'' => self.string(b),
            b'=' => {
                self.pos += 1;
                if self.peek_byte() == Some(b'~') {
                    self.pos += 1;
                    Ok(Some(Tok::EqMatch))
                } else {
                    Ok(Some(Tok::Eq))
                }
            }
            b'!' => {
                self.pos += 1;
                match self.peek_byte() {
                    Some(b'=') => {
                        self.pos += 1;
                        Ok(Some(Tok::Ne))
                    }
                    Some(b'~') => {
                        self.pos += 1;
                        Ok(Some(Tok::NeMatch))
                    }
                    _ => Err(TsPromQlError::Parse("expected = or ~ after !".into())),
                }
            }
            _ if b.is_ascii_digit() => self.number(),
            _ if is_ident_start(b) => self.ident_or_duration(),
            other => Err(TsPromQlError::Parse(format!(
                "unexpected character {:?}",
                other as char
            ))),
        }
    }

    fn single(&mut self, t: Tok<'a>) -> Result<Option<Tok<'a>>, TsPromQlError> {
        self.pos += 1;
        Ok(Some(t))
    }

    fn string(&mut self, quote: u8) -> Result<Option<Tok<'a>>, TsPromQlError> {
        let start = self.pos + 1;
        self.pos += 1;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != quote {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return Err(TsPromQlError::Parse("unterminated string literal".into()));
        }
        let s = &self.src[start..self.pos];
        self.pos += 1; // closing quote
        Ok(Some(Tok::Str(s)))
    }

    fn number(&mut self) -> Result<Option<Tok<'a>>, TsPromQlError> {
        let start = self.pos;
        let mut pure_int = true;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            if c.is_ascii_digit() {
                self.pos += 1;
            } else if c == b'.' || c == b'e' || c == b'E' {
                pure_int = false;
                self.pos += 1;
            } else if (c == b'+' || c == b'-')
                && matches!(self.bytes.get(self.pos - 1), Some(b'e') | Some(b'E'))
            {
                // exponent sign only
                self.pos += 1;
            } else {
                break;
            }
        }
        // A pure-integer run immediately followed by a duration unit is a
        // duration literal (5m, 90s, 1h30m), not a number. Pull in the rest of
        // the unit groups so 1h30m lexes as one token.
        if pure_int && matches!(self.peek_byte(), Some(b's' | b'm' | b'h' | b'd')) {
            while self.pos < self.bytes.len() {
                let c = self.bytes[self.pos];
                if c.is_ascii_digit() || matches!(c, b's' | b'm' | b'h' | b'd') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            return Ok(Some(Tok::Duration(&self.src[start..self.pos])));
        }
        Ok(Some(Tok::Num(&self.src[start..self.pos])))
    }

    fn ident_or_duration(&mut self) -> Result<Option<Tok<'a>>, TsPromQlError> {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_ident_cont(self.bytes[self.pos]) {
            self.pos += 1;
        }
        let word = &self.src[start..self.pos];
        // A bare duration token (5m, 30s) is only valid inside [ ] - the parser
        // handles it positionally; here we tag anything that looks like one.
        if is_duration_literal(word) {
            Ok(Some(Tok::Duration(word)))
        } else {
            Ok(Some(Tok::Ident(word)))
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b':'
}

/// A token is a duration literal when it is one or more `<digits><unit>`
/// groups, e.g. `5m`, `90s`, `1h30m`. Units: s, m, h, d.
fn is_duration_literal(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let bytes = word.as_bytes();
    let mut i = 0;
    let mut groups = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false; // a unit with no leading digits
        }
        match bytes.get(i) {
            Some(b's' | b'm' | b'h' | b'd') => i += 1,
            _ => return false,
        }
        groups += 1;
    }
    groups >= 1
}

/// Parse a duration literal to nanoseconds. `s`/`m`/`h`/`d` multipliers.
pub fn parse_duration_ns(word: &str) -> Result<i64, TsPromQlError> {
    let bytes = word.as_bytes();
    let mut i = 0;
    let mut total: i64 = 0;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let n: i64 = word[start..i]
            .parse()
            .map_err(|_| TsPromQlError::Parse(format!("bad duration number in {word:?}")))?;
        let unit = bytes
            .get(i)
            .ok_or_else(|| TsPromQlError::Parse(format!("duration {word:?} missing unit")))?;
        let mult: i64 = match unit {
            b's' => 1_000_000_000,
            b'm' => 60 * 1_000_000_000,
            b'h' => 3_600 * 1_000_000_000,
            b'd' => 86_400 * 1_000_000_000,
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "unknown duration unit {:?}",
                    *other as char
                )));
            }
        };
        total = total
            .checked_add(n.checked_mul(mult).ok_or_else(|| {
                TsPromQlError::Parse(format!("duration {word:?} overflows i64 nanos"))
            })?)
            .ok_or_else(|| {
                TsPromQlError::Parse(format!("duration {word:?} overflows i64 nanos"))
            })?;
        i += 1;
    }
    Ok(total)
}

/// Tokenise the whole input up front. Queries are short, so a single pass into
/// a vec is simpler than a streaming cursor and lets the parser peek freely.
fn tokenize(src: &str) -> Result<Vec<Tok<'_>>, TsPromQlError> {
    let mut lex = Lexer::new(src);
    let mut out = Vec::new();
    while let Some(t) = lex.next_tok()? {
        out.push(t);
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<Tok<'a>>,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Tok<'a>> {
        self.toks.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<Tok<'a>> {
        let t = self.toks.get(self.pos).copied();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, want: Tok<'a>) -> Result<(), TsPromQlError> {
        match self.bump() {
            Some(t) if t == want => Ok(()),
            other => Err(TsPromQlError::Parse(format!(
                "expected {want:?}, found {other:?}"
            ))),
        }
    }

    /// expr := term (('+' | '-') term)*
    fn parse_expr(&mut self) -> Result<Expr, TsPromQlError> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_term()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    /// term := factor (('*' | '/') factor)*
    fn parse_term(&mut self) -> Result<Expr, TsPromQlError> {
        let mut lhs = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_factor()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    /// factor := number | '(' expr ')' | aggregation | function | selector
    fn parse_factor(&mut self) -> Result<Expr, TsPromQlError> {
        match self.peek() {
            Some(Tok::Num(n)) => {
                self.bump();
                let v: f64 = n
                    .parse()
                    .map_err(|_| TsPromQlError::Parse(format!("bad number {n:?}")))?;
                Ok(Expr::Scalar(v))
            }
            Some(Tok::LParen) => {
                self.bump();
                let e = self.parse_expr()?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(word)) => self.parse_ident_lead(word),
            Some(Tok::Duration(word)) => {
                // A duration token in expression position is only meaningful as
                // a metric name collision (e.g. a metric literally named "1m");
                // reject rather than guess.
                Err(TsPromQlError::Parse(format!(
                    "unexpected duration token {word:?} in expression position"
                )))
            }
            other => Err(TsPromQlError::Parse(format!(
                "unexpected token {other:?} where an expression was expected"
            ))),
        }
    }

    /// An identifier in factor position is an aggregation keyword, a function
    /// name, or a metric selector.
    fn parse_ident_lead(&mut self, word: &str) -> Result<Expr, TsPromQlError> {
        if let Some(op) = agg_op(word) {
            self.bump();
            return self.parse_aggregation(op);
        }
        if let Some(kind) = func_kind(word) {
            self.bump();
            return self.parse_function(kind);
        }
        // plain metric selector
        let sel = self.parse_selector()?;
        Ok(Expr::Selector(sel))
    }

    /// aggregation := OP grouping? '(' expr ')'  |  OP '(' expr ')' grouping?
    fn parse_aggregation(&mut self, op: AggOp) -> Result<Expr, TsPromQlError> {
        // PromQL allows the grouping clause before or after the parenthesised
        // body. Accept both placements.
        let mut grouping = self.try_parse_grouping()?;
        self.expect(Tok::LParen)?;
        let inner = self.parse_expr()?;
        self.expect(Tok::RParen)?;
        if grouping == Grouping::None {
            grouping = self.try_parse_grouping()?;
        }
        Ok(Expr::Agg {
            op,
            grouping,
            inner: Box::new(inner),
        })
    }

    fn try_parse_grouping(&mut self) -> Result<Grouping, TsPromQlError> {
        let kw = match self.peek() {
            Some(Tok::Ident("by")) => true,
            Some(Tok::Ident("without")) => false,
            _ => return Ok(Grouping::None),
        };
        self.bump();
        self.expect(Tok::LParen)?;
        let mut labels = Vec::new();
        loop {
            match self.bump() {
                Some(Tok::Ident(l)) => labels.push(l.to_string()),
                Some(Tok::RParen) => break,
                other => {
                    return Err(TsPromQlError::Parse(format!(
                        "expected label or ) in grouping, found {other:?}"
                    )));
                }
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.bump();
                }
                Some(Tok::RParen) => {
                    self.bump();
                    break;
                }
                other => {
                    return Err(TsPromQlError::Parse(format!(
                        "expected , or ) in grouping, found {other:?}"
                    )));
                }
            }
        }
        Ok(if kw {
            Grouping::By(labels)
        } else {
            Grouping::Without(labels)
        })
    }

    /// function := KIND '(' selector '[' duration ']' ')'
    fn parse_function(&mut self, kind: FuncKind) -> Result<Expr, TsPromQlError> {
        self.expect(Tok::LParen)?;
        let mut selector = self.parse_selector()?;
        self.expect(Tok::LBrack)?;
        let range_ns = match self.bump() {
            Some(Tok::Duration(d)) => parse_duration_ns(d)?,
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "expected range duration in [ ], found {other:?}"
                )));
            }
        };
        self.expect(Tok::RBrack)?;
        // An offset can trail the range vector too.
        if let Some(off) = self.try_parse_offset()? {
            selector.offset_ns = off;
        }
        self.expect(Tok::RParen)?;
        Ok(Expr::Func {
            kind,
            selector,
            range_ns,
        })
    }

    /// selector := IDENT ('{' matcher (',' matcher)* '}')? ('offset' duration)?
    fn parse_selector(&mut self) -> Result<Selector, TsPromQlError> {
        let metric = match self.bump() {
            Some(Tok::Ident(m)) => m.to_string(),
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "expected metric name, found {other:?}"
                )));
            }
        };
        let mut matchers = Vec::new();
        if self.peek() == Some(Tok::LBrace) {
            self.bump();
            loop {
                if self.peek() == Some(Tok::RBrace) {
                    self.bump();
                    break;
                }
                matchers.push(self.parse_matcher()?);
                match self.peek() {
                    Some(Tok::Comma) => {
                        self.bump();
                    }
                    Some(Tok::RBrace) => {
                        self.bump();
                        break;
                    }
                    other => {
                        return Err(TsPromQlError::Parse(format!(
                            "expected , or }} in matcher list, found {other:?}"
                        )));
                    }
                }
            }
        }
        let offset_ns = self.try_parse_offset()?.unwrap_or(0);
        Ok(Selector {
            metric,
            matchers,
            offset_ns,
        })
    }

    fn parse_matcher(&mut self) -> Result<LabelMatcher, TsPromQlError> {
        let label = match self.bump() {
            Some(Tok::Ident(l)) => l.to_string(),
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "expected label name, found {other:?}"
                )));
            }
        };
        let op = match self.bump() {
            Some(Tok::Eq) => MatchOp::Eq,
            Some(Tok::Ne) => MatchOp::Ne,
            Some(Tok::EqMatch) => MatchOp::Re,
            Some(Tok::NeMatch) => MatchOp::Nre,
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "expected match operator, found {other:?}"
                )));
            }
        };
        let pattern = match self.bump() {
            Some(Tok::Str(s)) => s.to_string(),
            other => {
                return Err(TsPromQlError::Parse(format!(
                    "expected quoted matcher value, found {other:?}"
                )));
            }
        };
        Ok(LabelMatcher { label, op, pattern })
    }

    fn try_parse_offset(&mut self) -> Result<Option<i64>, TsPromQlError> {
        if self.peek() == Some(Tok::Ident("offset")) {
            self.bump();
            match self.bump() {
                Some(Tok::Duration(d)) => Ok(Some(parse_duration_ns(d)?)),
                other => Err(TsPromQlError::Parse(format!(
                    "expected duration after offset, found {other:?}"
                ))),
            }
        } else {
            Ok(None)
        }
    }
}

fn agg_op(word: &str) -> Option<AggOp> {
    match word {
        "sum" => Some(AggOp::Sum),
        "avg" => Some(AggOp::Avg),
        "min" => Some(AggOp::Min),
        "max" => Some(AggOp::Max),
        "count" => Some(AggOp::Count),
        _ => None,
    }
}

fn func_kind(word: &str) -> Option<FuncKind> {
    match word {
        "rate" => Some(FuncKind::Rate),
        "irate" => Some(FuncKind::Irate),
        "increase" => Some(FuncKind::Increase),
        _ => None,
    }
}

/// Parse a full query string into an [`Expr`]. Trailing tokens after a
/// complete expression are a parse error (the whole string must be consumed).
pub fn parse(src: &str) -> Result<Expr, TsPromQlError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(TsPromQlError::Parse("empty query".into()));
    }
    let mut p = Parser { toks, pos: 0 };
    let expr = p.parse_expr()?;
    if p.pos != p.toks.len() {
        return Err(TsPromQlError::Parse(format!(
            "trailing tokens after expression: {:?}",
            &p.toks[p.pos..]
        )));
    }
    Ok(expr)
}
