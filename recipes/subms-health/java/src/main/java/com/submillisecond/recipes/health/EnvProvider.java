package com.submillisecond.recipes.health;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/** Source of environment variables. Injectable so tests can freeze the env. */
public interface EnvProvider {
    String get(String key);

    List<String> keys();

    /** Reads the real process environment. */
    final class SystemEnv implements EnvProvider {
        @Override
        public String get(String key) {
            return System.getenv(key);
        }

        @Override
        public List<String> keys() {
            return new ArrayList<>(System.getenv().keySet());
        }
    }

    /** In-memory env for tests / frozen-at-boot snapshots. */
    final class MapEnv implements EnvProvider {
        private final TreeMap<String, String> vars = new TreeMap<>();

        public MapEnv with(String key, String value) {
            vars.put(key, value);
            return this;
        }

        @Override
        public String get(String key) {
            return vars.get(key);
        }

        @Override
        public List<String> keys() {
            return new ArrayList<>(vars.keySet());
        }

        Map<String, String> raw() {
            return vars;
        }
    }
}
