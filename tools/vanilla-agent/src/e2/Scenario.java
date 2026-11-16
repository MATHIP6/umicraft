package e2;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

/**
 * A tick-driven input script.
 *
 * <pre>
 * # comment
 * settle 40          consecutive on-ground ticks before the script starts
 * exit_after 40      ticks after the last phase before the client exits (optional)
 * phase NAME TICKS KEY...   KEY: W A S D JUMP SNEAK SPRINT, or "-" for none
 * </pre>
 */
final class Scenario {
    static final Set<String> KEYS = new LinkedHashSet<String>(
            Arrays.asList("W", "A", "S", "D", "JUMP", "SNEAK", "SPRINT"));

    static final class Phase {
        final String name;
        final int ticks;
        final Set<String> keys;

        Phase(String name, int ticks, Set<String> keys) {
            this.name = name;
            this.ticks = ticks;
            this.keys = keys;
        }
    }

    int settle = 40;
    int exitAfter = -1;
    final List<Phase> phases = new ArrayList<Phase>();

    static Scenario parse(List<String> lines) {
        Scenario s = new Scenario();
        int n = 0;
        for (String raw : lines) {
            n++;
            String line = raw.trim();
            if (line.isEmpty() || line.startsWith("#")) {
                continue;
            }
            String[] t = line.split("\\s+");
            if (t[0].equals("settle") && t.length == 2) {
                s.settle = Integer.parseInt(t[1]);
            } else if (t[0].equals("exit_after") && t.length == 2) {
                s.exitAfter = Integer.parseInt(t[1]);
            } else if (t[0].equals("phase") && t.length >= 4) {
                int ticks = Integer.parseInt(t[2]);
                if (ticks <= 0) {
                    throw new IllegalArgumentException("line " + n + ": ticks must be positive");
                }
                Set<String> keys = new LinkedHashSet<String>();
                for (int i = 3; i < t.length; i++) {
                    if (t[i].equals("-")) {
                        continue;
                    }
                    if (!KEYS.contains(t[i])) {
                        throw new IllegalArgumentException("line " + n + ": unknown key " + t[i]);
                    }
                    keys.add(t[i]);
                }
                s.phases.add(new Phase(t[1], ticks, keys));
            } else {
                throw new IllegalArgumentException("line " + n + ": cannot parse: " + line);
            }
        }
        if (s.phases.isEmpty()) {
            throw new IllegalArgumentException("scenario has no phase");
        }
        return s;
    }

    static Scenario load(String path) throws IOException {
        return parse(Files.readAllLines(Paths.get(path), StandardCharsets.UTF_8));
    }

    int totalTicks() {
        int total = 0;
        for (Phase p : phases) {
            total += p.ticks;
        }
        return total;
    }

    /** The phase holding script tick {@code t}, or null when the script is over. */
    Phase at(int t) {
        int from = 0;
        for (Phase p : phases) {
            if (t < from + p.ticks) {
                return p;
            }
            from += p.ticks;
        }
        return null;
    }
}
