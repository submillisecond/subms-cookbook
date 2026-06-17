package com.submillisecond.recipes.tspromql;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.tspromql.Ast.AggOp;
import com.submillisecond.recipes.tspromql.Ast.BinOp;
import com.submillisecond.recipes.tspromql.Ast.Expr;
import com.submillisecond.recipes.tspromql.Ast.FuncKind;
import com.submillisecond.recipes.tspromql.Ast.GroupKind;
import com.submillisecond.recipes.tspromql.Ast.Grouping;
import com.submillisecond.recipes.tspromql.Ast.LabelMatcher;
import com.submillisecond.recipes.tspromql.Ast.MatchOp;
import com.submillisecond.recipes.tspromql.Ast.Selector;

/**
 * Hand-rolled lexer + precedence-climbing recursive-descent parser for the
 * PromQL subset. No parser-generator; the lexer is a char cursor and the
 * grammar is two precedence levels (additive over multiplicative) bottoming
 * out at selectors, functions, aggregations, and parenthesised groups. Mirrors
 * the Rust {@code parser} module.
 */
final class Parser {

    private enum TokType {
        IDENT,
        STR,
        NUM,
        DURATION,
        LPAREN,
        RPAREN,
        LBRACE,
        RBRACE,
        LBRACK,
        RBRACK,
        COMMA,
        PLUS,
        MINUS,
        STAR,
        SLASH,
        EQ_MATCH,
        NE_MATCH,
        EQ,
        NE
    }

    private record Tok(TokType type, String text) {}

    private final List<Tok> toks;
    private int pos;

    private Parser(List<Tok> toks) {
        this.toks = toks;
    }

    // ---------- lexer ----------

    private static List<Tok> tokenize(String src) {
        List<Tok> out = new ArrayList<>();
        int i = 0;
        int n = src.length();
        while (i < n) {
            char c = src.charAt(i);
            if (Character.isWhitespace(c)) {
                i++;
                continue;
            }
            switch (c) {
                case '(' -> { out.add(new Tok(TokType.LPAREN, "(")); i++; }
                case ')' -> { out.add(new Tok(TokType.RPAREN, ")")); i++; }
                case '{' -> { out.add(new Tok(TokType.LBRACE, "{")); i++; }
                case '}' -> { out.add(new Tok(TokType.RBRACE, "}")); i++; }
                case '[' -> { out.add(new Tok(TokType.LBRACK, "[")); i++; }
                case ']' -> { out.add(new Tok(TokType.RBRACK, "]")); i++; }
                case ',' -> { out.add(new Tok(TokType.COMMA, ",")); i++; }
                case '+' -> { out.add(new Tok(TokType.PLUS, "+")); i++; }
                case '-' -> { out.add(new Tok(TokType.MINUS, "-")); i++; }
                case '*' -> { out.add(new Tok(TokType.STAR, "*")); i++; }
                case '/' -> { out.add(new Tok(TokType.SLASH, "/")); i++; }
                case '"', '\'' -> i = lexString(src, i, c, out);
                case '=' -> {
                    if (i + 1 < n && src.charAt(i + 1) == '~') {
                        out.add(new Tok(TokType.EQ_MATCH, "=~"));
                        i += 2;
                    } else {
                        out.add(new Tok(TokType.EQ, "="));
                        i++;
                    }
                }
                case '!' -> {
                    if (i + 1 < n && src.charAt(i + 1) == '=') {
                        out.add(new Tok(TokType.NE, "!="));
                        i += 2;
                    } else if (i + 1 < n && src.charAt(i + 1) == '~') {
                        out.add(new Tok(TokType.NE_MATCH, "!~"));
                        i += 2;
                    } else {
                        throw TsPromQlException.parse("expected = or ~ after !");
                    }
                }
                default -> {
                    if (Character.isDigit(c)) {
                        i = lexNumber(src, i, out);
                    } else if (isIdentStart(c)) {
                        i = lexIdent(src, i, out);
                    } else {
                        throw TsPromQlException.parse("unexpected character '" + c + "'");
                    }
                }
            }
        }
        return out;
    }

    private static int lexString(String src, int i, char quote, List<Tok> out) {
        int start = i + 1;
        int j = start;
        while (j < src.length() && src.charAt(j) != quote) {
            j++;
        }
        if (j >= src.length()) {
            throw TsPromQlException.parse("unterminated string literal");
        }
        out.add(new Tok(TokType.STR, src.substring(start, j)));
        return j + 1;
    }

    private static int lexNumber(String src, int i, List<Tok> out) {
        int start = i;
        int n = src.length();
        boolean pureInt = true;
        while (i < n) {
            char c = src.charAt(i);
            if (Character.isDigit(c)) {
                i++;
            } else if (c == '.' || c == 'e' || c == 'E') {
                pureInt = false;
                i++;
            } else if ((c == '+' || c == '-') && i > start
                    && (src.charAt(i - 1) == 'e' || src.charAt(i - 1) == 'E')) {
                i++;
            } else {
                break;
            }
        }
        if (pureInt && i < n && isDurationUnit(src.charAt(i))) {
            while (i < n && (Character.isDigit(src.charAt(i)) || isDurationUnit(src.charAt(i)))) {
                i++;
            }
            out.add(new Tok(TokType.DURATION, src.substring(start, i)));
            return i;
        }
        out.add(new Tok(TokType.NUM, src.substring(start, i)));
        return i;
    }

    private static int lexIdent(String src, int i, List<Tok> out) {
        int start = i;
        while (i < src.length() && isIdentCont(src.charAt(i))) {
            i++;
        }
        out.add(new Tok(TokType.IDENT, src.substring(start, i)));
        return i;
    }

    private static boolean isIdentStart(char c) {
        return Character.isLetter(c) || c == '_' || c == ':';
    }

    private static boolean isIdentCont(char c) {
        return Character.isLetterOrDigit(c) || c == '_' || c == ':';
    }

    private static boolean isDurationUnit(char c) {
        return c == 's' || c == 'm' || c == 'h' || c == 'd';
    }

    /** Parse a duration literal (5m, 90s, 1h30m) to nanoseconds. */
    static long parseDurationNs(String word) {
        long total = 0;
        int i = 0;
        int n = word.length();
        while (i < n) {
            int start = i;
            while (i < n && Character.isDigit(word.charAt(i))) {
                i++;
            }
            if (i == start) {
                throw TsPromQlException.parse("duration " + word + " missing number");
            }
            if (i >= n) {
                throw TsPromQlException.parse("duration " + word + " missing unit");
            }
            long value = Long.parseLong(word.substring(start, i));
            long mult = switch (word.charAt(i)) {
                case 's' -> 1_000_000_000L;
                case 'm' -> 60L * 1_000_000_000L;
                case 'h' -> 3_600L * 1_000_000_000L;
                case 'd' -> 86_400L * 1_000_000_000L;
                default -> throw TsPromQlException.parse("unknown duration unit '" + word.charAt(i) + "'");
            };
            total = Math.addExact(total, Math.multiplyExact(value, mult));
            i++;
        }
        return total;
    }

    // ---------- parser ----------

    static Expr parse(String src) {
        List<Tok> toks = tokenize(src);
        if (toks.isEmpty()) {
            throw TsPromQlException.parse("empty query");
        }
        Parser p = new Parser(toks);
        Expr e = p.parseExpr();
        if (p.pos != p.toks.size()) {
            throw TsPromQlException.parse("trailing tokens after expression");
        }
        return e;
    }

    private Tok peek() {
        return pos < toks.size() ? toks.get(pos) : null;
    }

    private Tok bump() {
        Tok t = peek();
        if (t != null) {
            pos++;
        }
        return t;
    }

    private void expect(TokType want) {
        Tok t = bump();
        if (t == null || t.type() != want) {
            throw TsPromQlException.parse("expected " + want + ", found " + (t == null ? "end" : t.type()));
        }
    }

    private boolean at(TokType type) {
        Tok t = peek();
        return t != null && t.type() == type;
    }

    private boolean atIdent(String word) {
        Tok t = peek();
        return t != null && t.type() == TokType.IDENT && t.text().equals(word);
    }

    // expr := term (('+' | '-') term)*
    private Expr parseExpr() {
        Expr lhs = parseTerm();
        while (true) {
            BinOp op;
            if (at(TokType.PLUS)) {
                op = BinOp.ADD;
            } else if (at(TokType.MINUS)) {
                op = BinOp.SUB;
            } else {
                break;
            }
            bump();
            lhs = new Ast.Binary(op, lhs, parseTerm());
        }
        return lhs;
    }

    // term := factor (('*' | '/') factor)*
    private Expr parseTerm() {
        Expr lhs = parseFactor();
        while (true) {
            BinOp op;
            if (at(TokType.STAR)) {
                op = BinOp.MUL;
            } else if (at(TokType.SLASH)) {
                op = BinOp.DIV;
            } else {
                break;
            }
            bump();
            lhs = new Ast.Binary(op, lhs, parseFactor());
        }
        return lhs;
    }

    // factor := number | '(' expr ')' | aggregation | function | selector
    private Expr parseFactor() {
        Tok t = peek();
        if (t == null) {
            throw TsPromQlException.parse("unexpected end of query");
        }
        return switch (t.type()) {
            case NUM -> {
                bump();
                yield new Ast.Scalar(Double.parseDouble(t.text()));
            }
            case LPAREN -> {
                bump();
                Expr e = parseExpr();
                expect(TokType.RPAREN);
                yield e;
            }
            case IDENT -> parseIdentLead(t.text());
            default -> throw TsPromQlException.parse("unexpected token " + t.type() + " in expression");
        };
    }

    private Expr parseIdentLead(String word) {
        AggOp agg = aggOp(word);
        if (agg != null) {
            bump();
            return parseAggregation(agg);
        }
        FuncKind fk = funcKind(word);
        if (fk != null) {
            bump();
            return parseFunction(fk);
        }
        return new Ast.SelectorExpr(parseSelector());
    }

    private Expr parseAggregation(AggOp op) {
        Grouping grouping = tryParseGrouping();
        expect(TokType.LPAREN);
        Expr inner = parseExpr();
        expect(TokType.RPAREN);
        if (grouping.kind() == GroupKind.NONE) {
            grouping = tryParseGrouping();
        }
        return new Ast.Agg(op, grouping, inner);
    }

    private Grouping tryParseGrouping() {
        boolean by;
        if (atIdent("by")) {
            by = true;
        } else if (atIdent("without")) {
            by = false;
        } else {
            return Grouping.NONE;
        }
        bump();
        expect(TokType.LPAREN);
        List<String> labels = new ArrayList<>();
        while (true) {
            Tok t = bump();
            if (t == null) {
                throw TsPromQlException.parse("unterminated grouping");
            }
            if (t.type() == TokType.RPAREN) {
                break;
            }
            if (t.type() != TokType.IDENT) {
                throw TsPromQlException.parse("expected label in grouping, found " + t.type());
            }
            labels.add(t.text());
            if (at(TokType.COMMA)) {
                bump();
            } else if (at(TokType.RPAREN)) {
                bump();
                break;
            } else {
                throw TsPromQlException.parse("expected , or ) in grouping");
            }
        }
        return new Grouping(by ? GroupKind.BY : GroupKind.WITHOUT, labels);
    }

    private Expr parseFunction(FuncKind kind) {
        expect(TokType.LPAREN);
        Selector base = parseSelector();
        expect(TokType.LBRACK);
        Tok d = bump();
        if (d == null || d.type() != TokType.DURATION) {
            throw TsPromQlException.parse("expected range duration in [ ]");
        }
        long rangeNs = parseDurationNs(d.text());
        expect(TokType.RBRACK);
        long offset = tryParseOffset();
        if (offset != 0) {
            base = new Selector(base.metric(), base.matchers(), offset);
        }
        expect(TokType.RPAREN);
        return new Ast.Func(kind, base, rangeNs);
    }

    private Selector parseSelector() {
        Tok m = bump();
        if (m == null || m.type() != TokType.IDENT) {
            throw TsPromQlException.parse("expected metric name");
        }
        List<LabelMatcher> matchers = new ArrayList<>();
        if (at(TokType.LBRACE)) {
            bump();
            while (true) {
                if (at(TokType.RBRACE)) {
                    bump();
                    break;
                }
                matchers.add(parseMatcher());
                if (at(TokType.COMMA)) {
                    bump();
                } else if (at(TokType.RBRACE)) {
                    bump();
                    break;
                } else {
                    throw TsPromQlException.parse("expected , or } in matcher list");
                }
            }
        }
        long offset = tryParseOffset();
        return new Selector(m.text(), matchers, offset);
    }

    private LabelMatcher parseMatcher() {
        Tok label = bump();
        if (label == null || label.type() != TokType.IDENT) {
            throw TsPromQlException.parse("expected label name");
        }
        Tok opTok = bump();
        MatchOp op;
        if (opTok == null) {
            throw TsPromQlException.parse("expected match operator");
        }
        op = switch (opTok.type()) {
            case EQ -> MatchOp.EQ;
            case NE -> MatchOp.NE;
            case EQ_MATCH -> MatchOp.RE;
            case NE_MATCH -> MatchOp.NRE;
            default -> throw TsPromQlException.parse("expected match operator, found " + opTok.type());
        };
        Tok val = bump();
        if (val == null || val.type() != TokType.STR) {
            throw TsPromQlException.parse("expected quoted matcher value");
        }
        return new LabelMatcher(label.text(), op, val.text());
    }

    private long tryParseOffset() {
        if (atIdent("offset")) {
            bump();
            Tok d = bump();
            if (d == null || d.type() != TokType.DURATION) {
                throw TsPromQlException.parse("expected duration after offset");
            }
            return parseDurationNs(d.text());
        }
        return 0;
    }

    private static AggOp aggOp(String word) {
        return switch (word) {
            case "sum" -> AggOp.SUM;
            case "avg" -> AggOp.AVG;
            case "min" -> AggOp.MIN;
            case "max" -> AggOp.MAX;
            case "count" -> AggOp.COUNT;
            default -> null;
        };
    }

    private static FuncKind funcKind(String word) {
        return switch (word) {
            case "rate" -> FuncKind.RATE;
            case "irate" -> FuncKind.IRATE;
            case "increase" -> FuncKind.INCREASE;
            default -> null;
        };
    }
}
