package com.submillisecond.recipes.ts;

import java.util.List;

/** A named subset of a panel's slots (e.g. {@code price} = open/high/low/close). */
public record TsPanelGroup(String name, List<String> seriesNames) {

    public TsPanelGroup {
        seriesNames = List.copyOf(seriesNames);
    }
}
