package com.submillisecond.recipes.health;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * Env/deploy provider: select env vars by explicit key + prefix/glob, redact
 * secrets, group them into a named ComponentHealth. Mirrors the Rust + Python
 * ports (incl. the cross-language fixture).
 */
public final class EnvSection {
    private static final String[] SECRET_NEEDLES = {"SECRET", "TOKEN", "KEY", "PASSWORD", "PASS", "CREDENTIAL"};

    private final String name;
    private final List<String> explicit = new ArrayList<>();
    private final List<String> prefixes = new ArrayList<>();
    private final List<String> globs = new ArrayList<>();
    private final List<Map.Entry<String, RedactionPolicy>> redactions = new ArrayList<>();
    private final List<Map.Entry<String, RedactionPolicy>> redactSubstrings = new ArrayList<>();
    private final Map<String, String> remap = new LinkedHashMap<>();
    private boolean stripPrefixInKey = false;
    private boolean lowercaseKeys = false;
    private HealthStatus status = HealthStatus.UP;
    private boolean includeEmpty = false;

    public EnvSection(String name) {
        this.name = name;
    }

    public String name() {
        return name;
    }

    public EnvSection key(String k) {
        explicit.add(k);
        return this;
    }

    public EnvSection keys(List<String> ks) {
        explicit.addAll(ks);
        return this;
    }

    public EnvSection prefix(String p) {
        prefixes.add(p);
        return this;
    }

    public EnvSection glob(String pattern) {
        globs.add(pattern);
        return this;
    }

    public EnvSection redact(String key, RedactionPolicy policy) {
        redactions.add(Map.entry(key, policy));
        return this;
    }

    public EnvSection redactSubstring(String needle, RedactionPolicy policy) {
        redactSubstrings.add(Map.entry(needle, policy));
        return this;
    }

    public EnvSection redactSecrets() {
        for (String n : SECRET_NEEDLES) {
            redactSubstrings.add(Map.entry(n, RedactionPolicy.MASK));
        }
        return this;
    }

    public EnvSection remap(String from, String to) {
        remap.put(from, to);
        return this;
    }

    public EnvSection stripPrefixInKey(boolean yes) {
        stripPrefixInKey = yes;
        return this;
    }

    public EnvSection lowercaseKeys(boolean yes) {
        lowercaseKeys = yes;
        return this;
    }

    public EnvSection status(HealthStatus s) {
        status = s;
        return this;
    }

    public EnvSection includeEmpty(boolean yes) {
        includeEmpty = yes;
        return this;
    }

    private boolean matches(String key) {
        if (explicit.contains(key)) {
            return true;
        }
        for (String p : prefixes) {
            if (key.startsWith(p)) {
                return true;
            }
        }
        for (String g : globs) {
            if (globMatch(g, key)) {
                return true;
            }
        }
        return false;
    }

    private String detailKey(String raw) {
        if (remap.containsKey(raw)) {
            return remap.get(raw);
        }
        String dk = raw;
        if (stripPrefixInKey) {
            String best = "";
            for (String p : prefixes) {
                if (raw.startsWith(p) && p.length() > best.length()) {
                    best = p;
                }
            }
            if (!best.isEmpty()) {
                dk = raw.substring(best.length());
            }
        }
        if (lowercaseKeys) {
            dk = dk.toLowerCase(java.util.Locale.ROOT);
        }
        return dk;
    }

    private RedactionPolicy policyFor(String raw, String detail) {
        for (Map.Entry<String, RedactionPolicy> e : redactions) {
            if (e.getKey().equals(raw) || e.getKey().equals(detail)) {
                return e.getValue();
            }
        }
        String rl = raw.toLowerCase(java.util.Locale.ROOT);
        String dl = detail.toLowerCase(java.util.Locale.ROOT);
        for (Map.Entry<String, RedactionPolicy> e : redactSubstrings) {
            String sl = e.getKey().toLowerCase(java.util.Locale.ROOT);
            if (rl.contains(sl) || dl.contains(sl)) {
                return e.getValue();
            }
        }
        return null;
    }

    public ComponentHealth render(EnvProvider env) {
        TreeSet<String> candidates = new TreeSet<>(explicit);
        for (String k : env.keys()) {
            if (matches(k)) {
                candidates.add(k);
            }
        }
        ComponentHealth c = new ComponentHealth(status);
        Map<String, Object> details = c.details();
        for (String raw : candidates) {
            String val = env.get(raw);
            if (val == null) {
                continue;
            }
            if (val.isEmpty() && !includeEmpty) {
                continue;
            }
            String dk = detailKey(raw);
            RedactionPolicy policy = policyFor(raw, dk);
            details.put(dk, policy != null ? policy.apply(val) : val);
        }
        return c;
    }

    public HealthIndicator intoIndicator(EnvProvider env) {
        return new HealthIndicator() {
            @Override
            public String name() {
                return name;
            }

            @Override
            public ComponentHealth check() {
                return render(env);
            }
        };
    }

    static boolean globMatch(String pattern, String s) {
        int star = pattern.indexOf('*');
        if (star < 0) {
            return pattern.equals(s);
        }
        String pre = pattern.substring(0, star);
        String post = pattern.substring(star + 1);
        return s.length() >= pre.length() + post.length() && s.startsWith(pre) && s.endsWith(post);
    }
}
