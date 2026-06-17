package com.submillisecond.recipes.ts;

import java.util.Optional;

public record TsDep(long seriesId, TsDepKind kind, Optional<String> note) {

    public static TsDep of(long seriesId, TsDepKind kind) {
        return new TsDep(seriesId, kind, Optional.empty());
    }

    public TsDep withNote(String note) {
        return new TsDep(seriesId, kind, Optional.of(note));
    }
}
