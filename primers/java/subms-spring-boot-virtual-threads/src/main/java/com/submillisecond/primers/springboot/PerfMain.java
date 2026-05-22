package com.submillisecond.primers.springboot;

import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Drive the subms harness against an embedded Spring Boot 4 server. The
 * recipe boots Tomcat twice on different ports: once with
 * {@code spring.threads.virtual.enabled=true}, once with platform threads
 * capped at {@code server.tomcat.threads.max=200}. Both stages issue
 * {@code entries} concurrent requests against {@code /route} and record
 * end-to-end latency.
 *
 * <p>Params come from stdin by default; passing a file path as the first
 * argument loads {@code key=value} lines from that file instead. Useful on
 * Windows where some shells mangle pipe-based stdin.
 */
public final class PerfMain {
    public static void main(String[] args) throws IOException {
        SubMsBenchParams params;
        if (args.length > 0) {
            params = SubMsBenchParams.fromMap(parseKv(Path.of(args[0])));
        } else {
            params = SubMsBenchParams.fromStdin();
        }

        // Spring Boot writes its banner + startup logs to System.out and
        // Tomcat layers on more chatter. We need a clean JSON-only stdout,
        // so capture FD 1 directly and route Spring's noise to stderr for
        // the duration of the bench. Restore after the recipe completes.
        PrintStream realOut = new PrintStream(new FileOutputStream(FileDescriptor.out), true);
        PrintStream originalOut = System.out;
        System.setOut(System.err);
        try {
            SubMsPerfHarness h = SubMsBench.runBench(new SpringBootRecipe(), params);
            h.writeJson(realOut);
        } finally {
            System.setOut(originalOut);
        }
    }

    private static Map<String, String> parseKv(Path path) throws IOException {
        Map<String, String> m = new LinkedHashMap<>();
        for (String line : Files.readAllLines(path)) {
            String t = line.trim();
            if (t.isEmpty() || t.startsWith("#")) continue;
            int eq = t.indexOf('=');
            if (eq < 0) continue;
            m.put(t.substring(0, eq).trim(), t.substring(eq + 1).trim());
        }
        return m;
    }
}
