package com.submillisecond.recipes.ts;

/**
 * How a derived series relates to a target series in the same container. Lets
 * a planner walk the dep graph: "what would invalidating trades.aapl
 * invalidate?" - everything declaring it as a {@code DERIVED} / {@code AGGREGATE}
 * dep.
 */
public sealed interface TsDepKind permits TsDepKind.Builtin, TsDepKind.Custom {

    enum Builtin implements TsDepKind {
        DERIVED,
        COMPONENT,
        AGGREGATE,
        ASOF_JOIN_LEFT,
        ASOF_JOIN_RIGHT
    }

    record Custom(String name) implements TsDepKind {}

    TsDepKind DERIVED = Builtin.DERIVED;
    TsDepKind COMPONENT = Builtin.COMPONENT;
    TsDepKind AGGREGATE = Builtin.AGGREGATE;
    TsDepKind ASOF_JOIN_LEFT = Builtin.ASOF_JOIN_LEFT;
    TsDepKind ASOF_JOIN_RIGHT = Builtin.ASOF_JOIN_RIGHT;

    static TsDepKind custom(String name) {
        return new Custom(name);
    }
}
