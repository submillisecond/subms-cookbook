# subms-ts-sql

A zero-dependency, hand-rolled SQL-subset parser and executor over the typed
`TsDataFrame` engine. It parses a `SELECT` against a named catalog of frames and
runs it by lowering each clause onto the cookbook's operator recipes - there is
no execution engine of its own. The SQL analogue of `subms-ts-promql`.

## Supported subset

```
SELECT <proj> FROM <table>
  [WHERE <predicate>]
  [GROUP BY <col, ...>]
  [ORDER BY <col> [ASC|DESC], ...]
  [LIMIT <n>]
```

- **Projection items:** `*`, a column, an arithmetic expression, `expr AS alias`,
  `CASE WHEN <pred> THEN <e> ELSE <e> END`, and the aggregates
  `SUM/AVG/MIN/MAX/COUNT(expr)` / `COUNT(*)`.
- **Expressions:** column refs, numeric / string literals, `+ - * /`,
  comparison (`= <> < <= > >=`), `AND`/`OR`/`NOT`, parentheses.
- **GROUP BY** over one or more typed key columns (string / int / bool).
- **ORDER BY** one or more keys, `ASC`/`DESC` (stable multi-key).
- Keywords are case-insensitive; identifiers keep their case; string literals
  are single-quoted (`''` escapes a quote); `--` starts a line comment.

## How it lowers

| Clause | Lowers onto |
|---|---|
| row-wise `SELECT` / `WHERE` / `ORDER BY` / `LIMIT` | `subms-ts-lazy` pipeline |
| `GROUP BY` + aggregates | `subms-ts-groupby` |
| every scalar / predicate / aggregate | `subms-ts-expr` IR |
| `TsDataFrame` / `TsArray` / `TsValue` core | `subms-ts` |

## Not in scope

`JOIN`, `HAVING`, subqueries, window functions, set operations, DDL/DML, and
arithmetic wrapped around an aggregate (`SUM(x) / 2`). Each is rejected with an
explicit typed error naming the gap, never silently ignored.

## Implementations

- [`rust/`](./rust/) - edition 2024, `std` only; crate `subms-ts-sql`,
  lib `subms_ts_sql`.
- [`java/`](./java/) - JDK 21, Maven; package
  `com.submillisecond.recipes.tssql`.

## Consumed by

Nothing - `subms-ts-sql` is a top-of-stack front-end (the position
`subms-ts-promql` holds). It consumes `subms-ts`, `subms-ts-expr`,
`subms-ts-lazy`, and `subms-ts-groupby`.

## Licence

MIT OR Apache-2.0.
