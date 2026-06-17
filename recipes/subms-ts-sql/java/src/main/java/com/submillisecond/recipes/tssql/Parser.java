package com.submillisecond.recipes.tssql;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.tssql.Ast.AggFunc;
import com.submillisecond.recipes.tssql.Ast.Aggregate;
import com.submillisecond.recipes.tssql.Ast.And;
import com.submillisecond.recipes.tssql.Ast.Arith;
import com.submillisecond.recipes.tssql.Ast.ArithOp;
import com.submillisecond.recipes.tssql.Ast.Case;
import com.submillisecond.recipes.tssql.Ast.CmpOp;
import com.submillisecond.recipes.tssql.Ast.Column;
import com.submillisecond.recipes.tssql.Ast.Compare;
import com.submillisecond.recipes.tssql.Ast.ExprItem;
import com.submillisecond.recipes.tssql.Ast.IntLit;
import com.submillisecond.recipes.tssql.Ast.Literal;
import com.submillisecond.recipes.tssql.Ast.Not;
import com.submillisecond.recipes.tssql.Ast.NumLit;
import com.submillisecond.recipes.tssql.Ast.Or;
import com.submillisecond.recipes.tssql.Ast.OrderKey;
import com.submillisecond.recipes.tssql.Ast.SelectItem;
import com.submillisecond.recipes.tssql.Ast.SqlExpr;
import com.submillisecond.recipes.tssql.Ast.SqlLiteral;
import com.submillisecond.recipes.tssql.Ast.Star;
import com.submillisecond.recipes.tssql.Ast.StrLit;
import com.submillisecond.recipes.tssql.Ast.TsSqlStmt;

/**
 * Hand-rolled lexer + recursive-descent parser for the SQL subset. No
 * parser-generator, no regex: the tokeniser is a char cursor and the grammar is
 * a precedence-climbing expression parser inside a fixed clause skeleton
 * ({@code SELECT ... FROM ... [WHERE] [GROUP BY] [ORDER BY] [LIMIT]}).
 *
 * <p>Keywords are matched case-insensitively; identifiers keep their case.
 * String literals are single-quoted with {@code ''} as the embedded-quote
 * escape. {@code --} starts a line comment to the newline. Anything
 * structurally outside the subset is a {@link TsSqlException.Kind#PARSE}; an
 * out-of-scope clause is a {@link TsSqlException.Kind#UNSUPPORTED} naming it.
 * Mirrors the Rust {@code parser} module.
 */
final class Parser {

    private enum TokType {
        WORD,
        NUM,
        STR,
        LPAREN,
        RPAREN,
        COMMA,
        STAR,
        PLUS,
        MINUS,
        SLASH,
        EQ,
        NE,
        LT,
        LE,
        GT,
        GE
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
            // `--` line comment runs to end-of-line.
            if (c == '-' && i + 1 < n && src.charAt(i + 1) == '-') {
                i += 2;
                while (i < n && src.charAt(i) != '\n') {
                    i++;
                }
                continue;
            }
            switch (c) {
                case '(' -> { out.add(new Tok(TokType.LPAREN, "(")); i++; }
                case ')' -> { out.add(new Tok(TokType.RPAREN, ")")); i++; }
                case ',' -> { out.add(new Tok(TokType.COMMA, ",")); i++; }
                case '*' -> { out.add(new Tok(TokType.STAR, "*")); i++; }
                case '+' -> { out.add(new Tok(TokType.PLUS, "+")); i++; }
                case '-' -> { out.add(new Tok(TokType.MINUS, "-")); i++; }
                case '/' -> { out.add(new Tok(TokType.SLASH, "/")); i++; }
                case '=' -> { out.add(new Tok(TokType.EQ, "=")); i++; }
                case '\'' -> i = lexString(src, i, out);
                case '<' -> {
                    if (i + 1 < n && src.charAt(i + 1) == '=') {
                        out.add(new Tok(TokType.LE, "<="));
                        i += 2;
                    } else if (i + 1 < n && src.charAt(i + 1) == '>') {
                        out.add(new Tok(TokType.NE, "<>"));
                        i += 2;
                    } else {
                        out.add(new Tok(TokType.LT, "<"));
                        i++;
                    }
                }
                case '>' -> {
                    if (i + 1 < n && src.charAt(i + 1) == '=') {
                        out.add(new Tok(TokType.GE, ">="));
                        i += 2;
                    } else {
                        out.add(new Tok(TokType.GT, ">"));
                        i++;
                    }
                }
                case '!' -> {
                    if (i + 1 < n && src.charAt(i + 1) == '=') {
                        out.add(new Tok(TokType.NE, "!="));
                        i += 2;
                    } else {
                        throw TsSqlException.parse("expected '=' after '!'");
                    }
                }
                default -> {
                    if (Character.isDigit(c)) {
                        i = lexNumber(src, i, out);
                    } else if (isIdentStart(c)) {
                        i = lexWord(src, i, out);
                    } else {
                        throw TsSqlException.parse("unexpected character '" + c + "'");
                    }
                }
            }
        }
        return out;
    }

    private static int lexString(String src, int i, List<Tok> out) {
        int n = src.length();
        i++; // opening quote
        StringBuilder sb = new StringBuilder();
        while (i < n) {
            char c = src.charAt(i);
            if (c == '\'') {
                if (i + 1 < n && src.charAt(i + 1) == '\'') {
                    sb.append('\'');
                    i += 2;
                } else {
                    out.add(new Tok(TokType.STR, sb.toString()));
                    return i + 1;
                }
            } else {
                sb.append(c);
                i++;
            }
        }
        throw TsSqlException.parse("unterminated string literal");
    }

    private static int lexNumber(String src, int i, List<Tok> out) {
        int n = src.length();
        int start = i;
        while (i < n) {
            char c = src.charAt(i);
            boolean exponentSign = (c == '+' || c == '-')
                    && i > start
                    && (src.charAt(i - 1) == 'e' || src.charAt(i - 1) == 'E');
            if (Character.isDigit(c) || c == '.' || c == 'e' || c == 'E' || exponentSign) {
                i++;
            } else {
                break;
            }
        }
        out.add(new Tok(TokType.NUM, src.substring(start, i)));
        return i;
    }

    private static int lexWord(String src, int i, List<Tok> out) {
        int n = src.length();
        int start = i;
        while (i < n && isIdentCont(src.charAt(i))) {
            i++;
        }
        out.add(new Tok(TokType.WORD, src.substring(start, i)));
        return i;
    }

    private static boolean isIdentStart(char c) {
        return Character.isLetter(c) || c == '_';
    }

    private static boolean isIdentCont(char c) {
        return Character.isLetterOrDigit(c) || c == '_';
    }

    // ---------- parser plumbing ----------

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

    private boolean peekKw(String kw) {
        Tok t = peek();
        return t != null && t.type() == TokType.WORD && t.text().equalsIgnoreCase(kw);
    }

    private boolean eatKw(String kw) {
        if (peekKw(kw)) {
            pos++;
            return true;
        }
        return false;
    }

    private void expectKw(String kw) {
        if (!eatKw(kw)) {
            throw TsSqlException.parse("expected keyword " + kw + ", found " + describe(peek()));
        }
    }

    private void expect(TokType want) {
        Tok t = bump();
        if (t == null || t.type() != want) {
            throw TsSqlException.parse("expected " + want + ", found " + describe(t));
        }
    }

    private static String describe(Tok t) {
        return t == null ? "end-of-input" : (t.type() + " " + t.text());
    }

    private String ident() {
        Tok t = bump();
        if (t == null || t.type() != TokType.WORD) {
            throw TsSqlException.parse("expected identifier, found " + describe(t));
        }
        if (isReserved(t.text())) {
            throw TsSqlException.parse(
                    "reserved keyword " + t.text() + " used where an identifier was expected");
        }
        return t.text();
    }

    // ---------- statement ----------

    private TsSqlStmt parseStatement() {
        expectKw("SELECT");
        List<SelectItem> projection = parseProjection();
        expectKw("FROM");
        String table = ident();
        rejectUnsupportedJoin();

        SqlExpr filter = eatKw("WHERE") ? parsePredicate() : null;

        List<String> groupBy = new ArrayList<>();
        if (eatKw("GROUP")) {
            expectKw("BY");
            groupBy = parseColumnList();
        }

        if (peekKw("HAVING")) {
            throw TsSqlException.unsupported("HAVING");
        }

        List<OrderKey> orderBy = new ArrayList<>();
        if (eatKw("ORDER")) {
            expectKw("BY");
            orderBy = parseOrderKeys();
        }

        Optional<Integer> limit = Optional.empty();
        if (eatKw("LIMIT")) {
            limit = Optional.of(parseLimit());
        }

        if (pos != toks.size()) {
            throw TsSqlException.parse("trailing tokens after statement: " + describe(peek()));
        }
        return new TsSqlStmt(projection, table, filter, groupBy, orderBy, limit);
    }

    private void rejectUnsupportedJoin() {
        for (String kw : List.of("JOIN", "INNER", "LEFT", "RIGHT", "FULL", "CROSS")) {
            if (peekKw(kw)) {
                throw TsSqlException.unsupported(kw);
            }
        }
    }

    private List<SelectItem> parseProjection() {
        List<SelectItem> items = new ArrayList<>();
        while (true) {
            Tok t = peek();
            if (t != null && t.type() == TokType.STAR) {
                bump();
                items.add(new Star());
            } else {
                SqlExpr expr = parseExpr();
                String alias = eatKw("AS") ? ident() : deriveAlias(expr, items.size());
                items.add(new ExprItem(expr, alias));
            }
            if (peek() != null && peek().type() == TokType.COMMA) {
                bump();
            } else {
                break;
            }
        }
        return items;
    }

    private List<String> parseColumnList() {
        List<String> cols = new ArrayList<>();
        while (true) {
            cols.add(ident());
            if (peek() != null && peek().type() == TokType.COMMA) {
                bump();
            } else {
                break;
            }
        }
        return cols;
    }

    private List<OrderKey> parseOrderKeys() {
        List<OrderKey> keys = new ArrayList<>();
        while (true) {
            String column = ident();
            boolean ascending;
            if (eatKw("DESC")) {
                ascending = false;
            } else {
                eatKw("ASC"); // optional default
                ascending = true;
            }
            keys.add(new OrderKey(column, ascending));
            if (peek() != null && peek().type() == TokType.COMMA) {
                bump();
            } else {
                break;
            }
        }
        return keys;
    }

    private int parseLimit() {
        Tok t = bump();
        if (t == null || t.type() != TokType.NUM) {
            throw TsSqlException.parse("expected an integer after LIMIT, found " + describe(t));
        }
        try {
            int n = Integer.parseInt(t.text());
            if (n < 0) {
                throw new NumberFormatException();
            }
            return n;
        } catch (NumberFormatException e) {
            throw TsSqlException.parse("LIMIT expects a non-negative integer, got " + t.text());
        }
    }

    // ---------- predicate grammar ----------

    private SqlExpr parsePredicate() {
        return parseOr();
    }

    private SqlExpr parseOr() {
        SqlExpr lhs = parseAnd();
        while (eatKw("OR")) {
            lhs = new Or(lhs, parseAnd());
        }
        return lhs;
    }

    private SqlExpr parseAnd() {
        SqlExpr lhs = parseNot();
        while (eatKw("AND")) {
            lhs = new And(lhs, parseNot());
        }
        return lhs;
    }

    private SqlExpr parseNot() {
        if (eatKw("NOT")) {
            return new Not(parseNot());
        }
        return parseComparison();
    }

    private SqlExpr parseComparison() {
        SqlExpr lhs = parseExpr();
        Tok t = peek();
        CmpOp op = t == null ? null : switch (t.type()) {
            case EQ -> CmpOp.EQ;
            case NE -> CmpOp.NE;
            case LT -> CmpOp.LT;
            case LE -> CmpOp.LE;
            case GT -> CmpOp.GT;
            case GE -> CmpOp.GE;
            default -> null;
        };
        if (op == null) {
            return lhs;
        }
        bump();
        return new Compare(op, lhs, parseExpr());
    }

    // ---------- scalar expression grammar ----------

    private SqlExpr parseExpr() {
        SqlExpr lhs = parseTerm();
        while (true) {
            Tok t = peek();
            ArithOp op = t == null ? null : switch (t.type()) {
                case PLUS -> ArithOp.ADD;
                case MINUS -> ArithOp.SUB;
                default -> null;
            };
            if (op == null) {
                break;
            }
            bump();
            lhs = new Arith(op, lhs, parseTerm());
        }
        return lhs;
    }

    private SqlExpr parseTerm() {
        SqlExpr lhs = parseFactor();
        while (true) {
            Tok t = peek();
            ArithOp op = t == null ? null : switch (t.type()) {
                case STAR -> ArithOp.MUL;
                case SLASH -> ArithOp.DIV;
                default -> null;
            };
            if (op == null) {
                break;
            }
            bump();
            lhs = new Arith(op, lhs, parseFactor());
        }
        return lhs;
    }

    private SqlExpr parseFactor() {
        Tok t = peek();
        if (t == null) {
            throw TsSqlException.parse("unexpected end-of-input where an expression was expected");
        }
        switch (t.type()) {
            case NUM -> {
                bump();
                return new Literal(parseNumberLiteral(t.text()));
            }
            case STR -> {
                bump();
                return new Literal(new StrLit(t.text()));
            }
            case MINUS -> {
                bump();
                return negate(parseFactor());
            }
            case LPAREN -> {
                bump();
                SqlExpr inner = parsePredicate();
                expect(TokType.RPAREN);
                return inner;
            }
            case WORD -> {
                if (t.text().equalsIgnoreCase("CASE")) {
                    return parseCase();
                }
                AggFunc func = aggFunc(t.text());
                if (func != null && nextIsLParen()) {
                    bump();
                    return parseAggregate(func);
                }
                if (isReserved(t.text())) {
                    throw TsSqlException.parse(
                            "reserved keyword " + t.text() + " in expression position");
                }
                bump();
                return new Column(t.text());
            }
            default -> throw TsSqlException.parse(
                    "unexpected token " + describe(t) + " where an expression was expected");
        }
    }

    private boolean nextIsLParen() {
        return pos + 1 < toks.size() && toks.get(pos + 1).type() == TokType.LPAREN;
    }

    private SqlExpr parseCase() {
        expectKw("CASE");
        expectKw("WHEN");
        SqlExpr when = parsePredicate();
        expectKw("THEN");
        SqlExpr then = parseExpr();
        expectKw("ELSE");
        SqlExpr otherwise = parseExpr();
        expectKw("END");
        return new Case(when, then, otherwise);
    }

    private SqlExpr parseAggregate(AggFunc func) {
        expect(TokType.LPAREN);
        if (peek() != null && peek().type() == TokType.STAR) {
            bump();
            expect(TokType.RPAREN);
            if (func != AggFunc.COUNT) {
                throw TsSqlException.parse("only COUNT supports a * argument");
            }
            return new Aggregate(func, Optional.empty());
        }
        SqlExpr arg = parseExpr();
        if (arg.containsAggregate()) {
            throw TsSqlException.parse("an aggregate may not nest inside another aggregate");
        }
        expect(TokType.RPAREN);
        return new Aggregate(func, Optional.of(arg));
    }

    // ---------- helpers ----------

    private static SqlExpr negate(SqlExpr inner) {
        if (inner instanceof Literal lit) {
            if (lit.value() instanceof IntLit n) {
                return new Literal(new IntLit(-n.value()));
            }
            if (lit.value() instanceof NumLit n) {
                return new Literal(new NumLit(-n.value()));
            }
        }
        return new Arith(ArithOp.SUB, new Literal(new IntLit(0)), inner);
    }

    private static SqlLiteral parseNumberLiteral(String raw) {
        boolean isInt = raw.indexOf('.') < 0 && raw.indexOf('e') < 0 && raw.indexOf('E') < 0;
        if (isInt) {
            try {
                return new IntLit(Long.parseLong(raw));
            } catch (NumberFormatException ignored) {
                // fall through to double parsing
            }
        }
        try {
            return new NumLit(Double.parseDouble(raw));
        } catch (NumberFormatException e) {
            throw TsSqlException.parse("bad numeric literal " + raw);
        }
    }

    private static String deriveAlias(SqlExpr expr, int index) {
        if (expr instanceof Column c) {
            return c.name();
        }
        return "col_" + index;
    }

    private static AggFunc aggFunc(String word) {
        return switch (word.toUpperCase()) {
            case "SUM" -> AggFunc.SUM;
            case "AVG" -> AggFunc.AVG;
            case "MIN" -> AggFunc.MIN;
            case "MAX" -> AggFunc.MAX;
            case "COUNT" -> AggFunc.COUNT;
            default -> null;
        };
    }

    private static boolean isReserved(String word) {
        return switch (word.toUpperCase()) {
            case "SELECT", "FROM", "WHERE", "GROUP", "BY", "ORDER", "LIMIT", "AS", "AND", "OR",
                    "NOT", "CASE", "WHEN", "THEN", "ELSE", "END", "ASC", "DESC", "HAVING", "JOIN" ->
                true;
            default -> false;
        };
    }

    /**
     * Parse a SQL string into a {@link TsSqlStmt}. The whole string must be a
     * single well-formed SELECT; trailing tokens are a parse error.
     */
    static TsSqlStmt parse(String sql) {
        List<Tok> toks = tokenize(sql);
        if (toks.isEmpty()) {
            throw TsSqlException.parse("empty query");
        }
        return new Parser(toks).parseStatement();
    }
}
