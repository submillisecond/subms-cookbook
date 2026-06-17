//! Hand-rolled lexer + recursive-descent parser for the SQL subset. No
//! parser-generator, no regex: the tokeniser is a byte cursor and the grammar
//! is a precedence-climbing expression parser inside a fixed clause skeleton
//! (`SELECT ... FROM ... [WHERE] [GROUP BY] [ORDER BY] [LIMIT]`).
//!
//! Keywords are matched case-insensitively; identifiers keep their case.
//! String literals are single-quoted with `''` as the embedded-quote escape.
//! `--` starts a line comment that runs to the newline. Anything structurally
//! outside the subset is a [`TsSqlError::Parse`]; an explicitly out-of-scope
//! clause (`JOIN`, `HAVING`, ...) is a [`TsSqlError::Unsupported`] naming it.

use crate::TsSqlError;
use crate::ast::{AggFunc, ArithOp, CmpOp, OrderKey, SelectItem, SqlExpr, SqlLiteral, TsSqlStmt};

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// A bare identifier or keyword. Keyword classification is case-insensitive
    /// and happens in the parser, so the lexer keeps the original spelling.
    Word(String),
    Num(String),
    Str(String),
    LParen,
    RParen,
    Comma,
    Star,
    Plus,
    Minus,
    Slash,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            // `--` line comment runs to end-of-line (or end-of-input).
            if self.pos + 1 < self.bytes.len()
                && self.bytes[self.pos] == b'-'
                && self.bytes[self.pos + 1] == b'-'
            {
                self.pos += 2;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn next_tok(&mut self) -> Result<Option<Tok>, TsSqlError> {
        self.skip_trivia();
        let Some(&b) = self.bytes.get(self.pos) else {
            return Ok(None);
        };
        match b {
            b'(' => self.single(Tok::LParen),
            b')' => self.single(Tok::RParen),
            b',' => self.single(Tok::Comma),
            b'*' => self.single(Tok::Star),
            b'+' => self.single(Tok::Plus),
            b'-' => self.single(Tok::Minus),
            b'/' => self.single(Tok::Slash),
            b'=' => self.single(Tok::Eq),
            b'\'' => self.string(),
            b'<' => {
                self.pos += 1;
                match self.bytes.get(self.pos) {
                    Some(b'=') => {
                        self.pos += 1;
                        Ok(Some(Tok::Le))
                    }
                    Some(b'>') => {
                        self.pos += 1;
                        Ok(Some(Tok::Ne))
                    }
                    _ => Ok(Some(Tok::Lt)),
                }
            }
            b'>' => {
                self.pos += 1;
                if self.bytes.get(self.pos) == Some(&b'=') {
                    self.pos += 1;
                    Ok(Some(Tok::Ge))
                } else {
                    Ok(Some(Tok::Gt))
                }
            }
            b'!' => {
                self.pos += 1;
                if self.bytes.get(self.pos) == Some(&b'=') {
                    self.pos += 1;
                    Ok(Some(Tok::Ne))
                } else {
                    Err(TsSqlError::parse("expected '=' after '!'"))
                }
            }
            _ if b.is_ascii_digit() => self.number(),
            _ if is_ident_start(b) => self.word(),
            other => Err(TsSqlError::parse(format!(
                "unexpected character {:?}",
                other as char
            ))),
        }
    }

    fn single(&mut self, t: Tok) -> Result<Option<Tok>, TsSqlError> {
        self.pos += 1;
        Ok(Some(t))
    }

    fn string(&mut self) -> Result<Option<Tok>, TsSqlError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        loop {
            match self.bytes.get(self.pos) {
                None => return Err(TsSqlError::parse("unterminated string literal")),
                Some(&b'\'') => {
                    // `''` is an embedded single quote; a lone `'` closes.
                    if self.bytes.get(self.pos + 1) == Some(&b'\'') {
                        out.push('\'');
                        self.pos += 2;
                    } else {
                        self.pos += 1;
                        return Ok(Some(Tok::Str(out)));
                    }
                }
                Some(&c) => {
                    out.push(c as char);
                    self.pos += 1;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Option<Tok>, TsSqlError> {
        let start = self.pos;
        while self.pos < self.bytes.len() {
            let c = self.bytes[self.pos];
            // A '+'/'-' is part of the number only as an exponent sign.
            let exponent_sign = (c == b'+' || c == b'-')
                && matches!(self.bytes.get(self.pos - 1), Some(b'e') | Some(b'E'));
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || exponent_sign {
                self.pos += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| TsSqlError::parse("invalid number bytes"))?;
        Ok(Some(Tok::Num(s.to_string())))
    }

    fn word(&mut self) -> Result<Option<Tok>, TsSqlError> {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_ident_cont(self.bytes[self.pos]) {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| TsSqlError::parse("invalid identifier bytes"))?;
        Ok(Some(Tok::Word(s.to_string())))
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn tokenize(src: &str) -> Result<Vec<Tok>, TsSqlError> {
    let mut lex = Lexer::new(src);
    let mut out = Vec::new();
    while let Some(t) = lex.next_tok()? {
        out.push(t);
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Is the next token the keyword `kw` (case-insensitive)? Does not consume.
    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(kw))
    }

    /// Consume the next token if it is keyword `kw`. Returns whether it fired.
    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.peek_kw(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, kw: &str) -> Result<(), TsSqlError> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(TsSqlError::parse(format!(
                "expected keyword {kw}, found {:?}",
                self.peek()
            )))
        }
    }

    fn expect(&mut self, want: &Tok) -> Result<(), TsSqlError> {
        match self.bump() {
            Some(ref t) if t == want => Ok(()),
            other => Err(TsSqlError::parse(format!(
                "expected {want:?}, found {other:?}"
            ))),
        }
    }

    /// An identifier (not a reserved keyword). Used for table / column names.
    fn ident(&mut self) -> Result<String, TsSqlError> {
        match self.bump() {
            Some(Tok::Word(w)) if !is_reserved(&w) => Ok(w),
            Some(Tok::Word(w)) => Err(TsSqlError::parse(format!(
                "reserved keyword {w} used where an identifier was expected"
            ))),
            other => Err(TsSqlError::parse(format!(
                "expected identifier, found {other:?}"
            ))),
        }
    }

    fn parse_statement(&mut self) -> Result<TsSqlStmt, TsSqlError> {
        self.expect_kw("SELECT")?;
        let projection = self.parse_projection()?;
        self.expect_kw("FROM")?;
        let table = self.ident()?;
        self.reject_unsupported_join()?;

        let filter = if self.eat_kw("WHERE") {
            Some(self.parse_predicate()?)
        } else {
            None
        };

        let group_by = if self.eat_kw("GROUP") {
            self.expect_kw("BY")?;
            self.parse_column_list()?
        } else {
            Vec::new()
        };

        if self.peek_kw("HAVING") {
            return Err(TsSqlError::unsupported("HAVING"));
        }

        let order_by = if self.eat_kw("ORDER") {
            self.expect_kw("BY")?;
            self.parse_order_keys()?
        } else {
            Vec::new()
        };

        let limit = if self.eat_kw("LIMIT") {
            Some(self.parse_limit()?)
        } else {
            None
        };

        if self.pos != self.toks.len() {
            return Err(TsSqlError::parse(format!(
                "trailing tokens after statement: {:?}",
                &self.toks[self.pos..]
            )));
        }
        Ok(TsSqlStmt {
            projection,
            table,
            filter,
            group_by,
            order_by,
            limit,
        })
    }

    // JOIN / subquery in the FROM position is out of scope; name it explicitly
    // rather than failing with a generic trailing-token error.
    fn reject_unsupported_join(&mut self) -> Result<(), TsSqlError> {
        for kw in ["JOIN", "INNER", "LEFT", "RIGHT", "FULL", "CROSS"] {
            if self.peek_kw(kw) {
                return Err(TsSqlError::unsupported(kw));
            }
        }
        Ok(())
    }

    fn parse_projection(&mut self) -> Result<Vec<SelectItem>, TsSqlError> {
        let mut items = Vec::new();
        loop {
            if matches!(self.peek(), Some(Tok::Star)) {
                self.bump();
                items.push(SelectItem::Star);
            } else {
                let expr = self.parse_expr()?;
                let alias = if self.eat_kw("AS") {
                    self.ident()?
                } else {
                    derive_alias(&expr, items.len())
                };
                items.push(SelectItem::Expr { expr, alias });
            }
            if matches!(self.peek(), Some(Tok::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(items)
    }

    fn parse_column_list(&mut self) -> Result<Vec<String>, TsSqlError> {
        let mut cols = Vec::new();
        loop {
            cols.push(self.ident()?);
            if matches!(self.peek(), Some(Tok::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(cols)
    }

    fn parse_order_keys(&mut self) -> Result<Vec<OrderKey>, TsSqlError> {
        let mut keys = Vec::new();
        loop {
            let column = self.ident()?;
            let ascending = if self.eat_kw("DESC") {
                false
            } else {
                // ASC is the default; consume it when present.
                self.eat_kw("ASC");
                true
            };
            keys.push(OrderKey { column, ascending });
            if matches!(self.peek(), Some(Tok::Comma)) {
                self.bump();
            } else {
                break;
            }
        }
        Ok(keys)
    }

    fn parse_limit(&mut self) -> Result<usize, TsSqlError> {
        match self.bump() {
            Some(Tok::Num(n)) => n.parse::<usize>().map_err(|_| {
                TsSqlError::parse(format!("LIMIT expects a non-negative integer, got {n}"))
            }),
            other => Err(TsSqlError::parse(format!(
                "expected an integer after LIMIT, found {other:?}"
            ))),
        }
    }

    // ---------- predicate grammar (boolean operators over comparisons) ----------

    /// predicate := or_pred
    fn parse_predicate(&mut self) -> Result<SqlExpr, TsSqlError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<SqlExpr, TsSqlError> {
        let mut lhs = self.parse_and()?;
        while self.eat_kw("OR") {
            let rhs = self.parse_and()?;
            lhs = SqlExpr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<SqlExpr, TsSqlError> {
        let mut lhs = self.parse_not()?;
        while self.eat_kw("AND") {
            let rhs = self.parse_not()?;
            lhs = SqlExpr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_not(&mut self) -> Result<SqlExpr, TsSqlError> {
        if self.eat_kw("NOT") {
            let inner = self.parse_not()?;
            Ok(SqlExpr::Not(Box::new(inner)))
        } else {
            self.parse_comparison()
        }
    }

    /// comparison := expr (cmp_op expr)?  |  '(' predicate ')'
    fn parse_comparison(&mut self) -> Result<SqlExpr, TsSqlError> {
        // A parenthesised predicate is only a predicate when it actually holds a
        // boolean - but `(a + b)` is also a parenthesised scalar. We let the
        // scalar parser handle the parens and re-check for a trailing cmp op.
        let lhs = self.parse_expr()?;
        let op = match self.peek() {
            Some(Tok::Eq) => CmpOp::Eq,
            Some(Tok::Ne) => CmpOp::Ne,
            Some(Tok::Lt) => CmpOp::Lt,
            Some(Tok::Le) => CmpOp::Le,
            Some(Tok::Gt) => CmpOp::Gt,
            Some(Tok::Ge) => CmpOp::Ge,
            _ => return Ok(lhs), // a bare boolean column / parenthesised predicate
        };
        self.bump();
        let rhs = self.parse_expr()?;
        Ok(SqlExpr::Compare(op, Box::new(lhs), Box::new(rhs)))
    }

    // ---------- scalar expression grammar ----------

    /// expr := term (('+' | '-') term)*
    fn parse_expr(&mut self) -> Result<SqlExpr, TsSqlError> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => ArithOp::Add,
                Some(Tok::Minus) => ArithOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_term()?;
            lhs = SqlExpr::Arith(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// term := factor (('*' | '/') factor)*
    fn parse_term(&mut self) -> Result<SqlExpr, TsSqlError> {
        let mut lhs = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => ArithOp::Mul,
                Some(Tok::Slash) => ArithOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_factor()?;
            lhs = SqlExpr::Arith(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// factor := number | string | CASE | aggregate | column | '(' expr ')'
    fn parse_factor(&mut self) -> Result<SqlExpr, TsSqlError> {
        match self.peek() {
            Some(Tok::Num(_)) => {
                let Some(Tok::Num(n)) = self.bump() else {
                    unreachable!()
                };
                Ok(SqlExpr::Literal(parse_number_literal(&n)?))
            }
            Some(Tok::Str(_)) => {
                let Some(Tok::Str(s)) = self.bump() else {
                    unreachable!()
                };
                Ok(SqlExpr::Literal(SqlLiteral::Str(s)))
            }
            Some(Tok::Minus) => {
                // Unary minus: fold a literal sign in, else `0 - x`.
                self.bump();
                let inner = self.parse_factor()?;
                Ok(negate(inner))
            }
            Some(Tok::LParen) => {
                self.bump();
                let inner = self.parse_predicate()?;
                self.expect(&Tok::RParen)?;
                Ok(inner)
            }
            Some(Tok::Word(w)) if w.eq_ignore_ascii_case("CASE") => self.parse_case(),
            Some(Tok::Word(w)) => {
                let word = w.clone();
                if let Some(func) = agg_func(&word) {
                    self.bump();
                    self.parse_aggregate(func)
                } else if is_reserved(&word) {
                    Err(TsSqlError::parse(format!(
                        "reserved keyword {word} in expression position"
                    )))
                } else {
                    self.bump();
                    Ok(SqlExpr::Column(word))
                }
            }
            other => Err(TsSqlError::parse(format!(
                "unexpected token {other:?} where an expression was expected"
            ))),
        }
    }

    fn parse_case(&mut self) -> Result<SqlExpr, TsSqlError> {
        self.expect_kw("CASE")?;
        self.expect_kw("WHEN")?;
        let when = self.parse_predicate()?;
        self.expect_kw("THEN")?;
        let then = self.parse_expr()?;
        self.expect_kw("ELSE")?;
        let otherwise = self.parse_expr()?;
        self.expect_kw("END")?;
        Ok(SqlExpr::Case {
            when: Box::new(when),
            then: Box::new(then),
            otherwise: Box::new(otherwise),
        })
    }

    fn parse_aggregate(&mut self, func: AggFunc) -> Result<SqlExpr, TsSqlError> {
        self.expect(&Tok::LParen)?;
        // COUNT(*) is the one star-argument aggregate.
        if matches!(self.peek(), Some(Tok::Star)) {
            self.bump();
            self.expect(&Tok::RParen)?;
            if func != AggFunc::Count {
                return Err(TsSqlError::parse(
                    "only COUNT supports a * argument".to_string(),
                ));
            }
            return Ok(SqlExpr::Aggregate { func, arg: None });
        }
        let arg = self.parse_expr()?;
        if arg.contains_aggregate() {
            return Err(TsSqlError::parse(
                "an aggregate may not nest inside another aggregate".to_string(),
            ));
        }
        self.expect(&Tok::RParen)?;
        Ok(SqlExpr::Aggregate {
            func,
            arg: Some(Box::new(arg)),
        })
    }
}

fn negate(inner: SqlExpr) -> SqlExpr {
    match inner {
        SqlExpr::Literal(SqlLiteral::Int(n)) => SqlExpr::Literal(SqlLiteral::Int(-n)),
        SqlExpr::Literal(SqlLiteral::Num(n)) => SqlExpr::Literal(SqlLiteral::Num(-n)),
        other => SqlExpr::Arith(
            ArithOp::Sub,
            Box::new(SqlExpr::Literal(SqlLiteral::Int(0))),
            Box::new(other),
        ),
    }
}

fn parse_number_literal(raw: &str) -> Result<SqlLiteral, TsSqlError> {
    let is_int = !raw.contains('.') && !raw.contains('e') && !raw.contains('E');
    if is_int {
        if let Ok(n) = raw.parse::<i64>() {
            return Ok(SqlLiteral::Int(n));
        }
    }
    raw.parse::<f64>()
        .map(SqlLiteral::Num)
        .map_err(|_| TsSqlError::parse(format!("bad numeric literal {raw}")))
}

// A projected expression with no AS clause gets a stable derived name: a bare
// column keeps its own name; anything computed gets `col_<index>` so the output
// schema is deterministic and addressable.
fn derive_alias(expr: &SqlExpr, index: usize) -> String {
    match expr {
        SqlExpr::Column(name) => name.clone(),
        _ => format!("col_{index}"),
    }
}

fn agg_func(word: &str) -> Option<AggFunc> {
    match word.to_ascii_uppercase().as_str() {
        "SUM" => Some(AggFunc::Sum),
        "AVG" => Some(AggFunc::Avg),
        "MIN" => Some(AggFunc::Min),
        "MAX" => Some(AggFunc::Max),
        "COUNT" => Some(AggFunc::Count),
        _ => None,
    }
}

// Reserved words that may never be an identifier or a bare column reference.
// Aggregate-function names are NOT reserved (they are only special when
// immediately followed by `(`), matching common SQL leniency.
fn is_reserved(word: &str) -> bool {
    matches!(
        word.to_ascii_uppercase().as_str(),
        "SELECT"
            | "FROM"
            | "WHERE"
            | "GROUP"
            | "BY"
            | "ORDER"
            | "LIMIT"
            | "AS"
            | "AND"
            | "OR"
            | "NOT"
            | "CASE"
            | "WHEN"
            | "THEN"
            | "ELSE"
            | "END"
            | "ASC"
            | "DESC"
            | "HAVING"
            | "JOIN"
    )
}

/// Parse a SQL string into a [`TsSqlStmt`]. The whole string must be a single
/// well-formed `SELECT`; trailing tokens are a parse error.
pub fn parse(sql: &str) -> Result<TsSqlStmt, TsSqlError> {
    let toks = tokenize(sql)?;
    if toks.is_empty() {
        return Err(TsSqlError::parse("empty query"));
    }
    let mut p = Parser { toks, pos: 0 };
    p.parse_statement()
}
