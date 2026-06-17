package com.submillisecond.recipes.ts;

/**
 * Declared preferred wire format. Lets a container pick a codec at serialize
 * time without inspecting the value shape; also records what a series loaded
 * as when materialising from disk.
 */
public sealed interface TsFormat permits TsFormat.Builtin, TsFormat.Custom {

    String codecName();

    enum Builtin implements TsFormat {
        JSON("json"),
        CBOR("cbor"),
        GORILLA("gorilla"),
        YAML("yaml"),
        GZIP_JSON("gzip+json"),
        GZIP_CBOR("gzip+cbor"),
        GZIP_GORILLA("gzip+gorilla");

        private final String codecName;

        Builtin(String codecName) {
            this.codecName = codecName;
        }

        @Override
        public String codecName() {
            return codecName;
        }
    }

    record Custom(String name) implements TsFormat {
        @Override
        public String codecName() {
            return name;
        }
    }

    TsFormat JSON = Builtin.JSON;
    TsFormat CBOR = Builtin.CBOR;
    TsFormat GORILLA = Builtin.GORILLA;
    TsFormat YAML = Builtin.YAML;
    TsFormat GZIP_JSON = Builtin.GZIP_JSON;
    TsFormat GZIP_CBOR = Builtin.GZIP_CBOR;
    TsFormat GZIP_GORILLA = Builtin.GZIP_GORILLA;

    static TsFormat custom(String name) {
        return new Custom(name);
    }
}
