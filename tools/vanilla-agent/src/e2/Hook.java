package e2;

import java.io.BufferedWriter;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Runtime side of the agent. {@link #before} runs at the start of
 * {@code EntityPlayerSP.onUpdate}, {@link #after} at its end (after the
 * physics step, as the packet the same tick carries). It only writes the state
 * of the key bindings the scenario asks for and reads the player; it never
 * writes position, motion or rotation.
 *
 * Member names are the 1.8.9 obfuscated names; {@link #resolve} fails loudly if
 * they are not those of the jar being run.
 */
public final class Hook {
    private static Scenario scenario;
    private static BufferedWriter log;
    private static boolean ready;
    private static boolean failed;

    private static Field posX, posY, posZ, motX, motY, motZ, yaw, pitch, ground;
    private static Method isSneaking, isSprinting;
    private static Method getMinecraft;
    private static Field gameSettings;
    private static final Map<String, Object> bindings = new LinkedHashMap<String, Object>();
    private static Field pressedField;

    private static int tick = -1;
    private static int groundRun;
    private static int startTick = -1;
    private static int endTick = -1;
    private static String appliedKeys = "";

    // Accessors for the temporary E.3 probes (read-only).
    static int tickNow() {
        return tick;
    }

    static int scriptTick() {
        return startTick < 0 ? -1 : tick - startTick;
    }

    static String phaseName() {
        int st = scriptTick();
        if (st < 0) {
            return "pre";
        }
        Scenario.Phase phase = scenario.at(st);
        return phase == null ? "end" : phase.name;
    }

    static void init(Scenario s, String logPath) throws IOException {
        scenario = s;
        log = new BufferedWriter(new OutputStreamWriter(new FileOutputStream(logPath), StandardCharsets.UTF_8));
        log.write("# e2 agent log; java " + System.getProperty("java.version") + "; scenario phases="
                + s.phases.size() + " ticks=" + s.totalTicks() + " settle=" + s.settle + "\n");
        log.write("tick,script_tick,phase,x,y,z,mx,my,mz,yaw,pitch,on_ground,sprinting,sneaking,"
                + "keys_applied,keys_pressed_WASDJSC\n");
        log.flush();
    }

    private static Field field(Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }

    private static void resolve(Object player) throws Exception {
        Class<?> entity = player.getClass();
        while (entity != null && !entity.getName().equals("pk")) {
            entity = entity.getSuperclass();
        }
        if (entity == null) {
            throw new IllegalStateException("class pk (Entity) not found above " + player.getClass());
        }
        posX = field(entity, "s");
        posY = field(entity, "t");
        posZ = field(entity, "u");
        motX = field(entity, "v");
        motY = field(entity, "w");
        motZ = field(entity, "x");
        yaw = field(entity, "y");
        pitch = field(entity, "z");
        ground = field(entity, "C");
        isSneaking = entity.getMethod("av");
        isSprinting = entity.getMethod("aw");
        ClassLoader loader = player.getClass().getClassLoader();
        Class<?> mc = Class.forName("ave", false, loader);
        for (Method m : mc.getDeclaredMethods()) {
            if (java.lang.reflect.Modifier.isStatic(m.getModifiers()) && m.getParameterTypes().length == 0
                    && m.getReturnType() == mc) {
                getMinecraft = m;
                break;
            }
        }
        if (getMinecraft == null) {
            throw new IllegalStateException("Minecraft.getMinecraft not found");
        }
        Class<?> settings = Class.forName("avh", false, loader);
        for (Field f : mc.getDeclaredFields()) {
            if (f.getType() == settings) {
                f.setAccessible(true);
                gameSettings = f;
            }
        }
        Class<?> kb = Class.forName("avb", false, loader);
        Field description = field(kb, "d"); // KeyBinding.keyDescription
        pressedField = field(kb, "h");
        Object options = gameSettings.get(getMinecraft.invoke(null));
        String[][] names = {{"W", "key.forward"}, {"A", "key.left"}, {"S", "key.back"}, {"D", "key.right"},
                {"JUMP", "key.jump"}, {"SNEAK", "key.sneak"}, {"SPRINT", "key.sprint"}};
        for (String[] n : names) {
            for (Field f : settings.getDeclaredFields()) {
                if (f.getType() == kb) {
                    f.setAccessible(true);
                    Object b = f.get(options);
                    if (n[1].equals(description.get(b))) {
                        bindings.put(n[0], b);
                    }
                }
            }
        }
        if (bindings.size() != names.length) {
            throw new IllegalStateException("key bindings found: " + bindings.keySet());
        }
        ready = true;
    }

    public static void before(Object player) {
        if (failed) {
            return;
        }
        try {
            if (!ready) {
                resolve(player);
            }
            tick++;
            boolean onGround = ground.getBoolean(player);
            groundRun = onGround ? groundRun + 1 : 0;
            if (startTick < 0 && groundRun >= scenario.settle) {
                startTick = tick;
            }
            Scenario.Phase phase = startTick < 0 ? null : scenario.at(tick - startTick);
            if (startTick >= 0 && phase == null && endTick < 0) {
                endTick = tick;
            }
            StringBuilder applied = new StringBuilder();
            for (Map.Entry<String, Object> e : bindings.entrySet()) {
                boolean down = phase != null && phase.keys.contains(e.getKey());
                if (down) {
                    applied.append(applied.length() == 0 ? "" : "+").append(e.getKey());
                }
                // While the script runs every key is forced to the scripted state, so a stray
                // key cannot leak in; before and after it the keys are left to the player.
                if (startTick >= 0 && endTick < 0) {
                    pressedField.setBoolean(e.getValue(), down);
                }
            }
            appliedKeys = applied.toString();
        } catch (Throwable t) {
            failed = true;
            System.err.println("[e2-agent] disabled: " + t);
            t.printStackTrace();
        }
    }

    public static void after(Object player) {
        if (failed || !ready) {
            return;
        }
        try {
            int st = startTick < 0 ? -1 : tick - startTick;
            Scenario.Phase phase = st < 0 ? null : scenario.at(st);
            StringBuilder pressed = new StringBuilder();
            for (Object b : bindings.values()) {
                pressed.append(pressedField.getBoolean(b) ? '1' : '0');
            }
            log.write(tick + "," + st + "," + (phase == null ? (st < 0 ? "pre" : "end") : phase.name) + ","
                    + posX.getDouble(player) + "," + posY.getDouble(player) + "," + posZ.getDouble(player) + ","
                    + motX.getDouble(player) + "," + motY.getDouble(player) + "," + motZ.getDouble(player) + ","
                    + yaw.getFloat(player) + "," + pitch.getFloat(player) + ","
                    + ground.getBoolean(player) + "," + isSprinting.invoke(player) + ","
                    + isSneaking.invoke(player) + "," + (appliedKeys.isEmpty() ? "-" : appliedKeys) + ","
                    + pressed + "\n");
            if (tick % 20 == 0) {
                log.flush();
            }
            if (endTick >= 0 && scenario.exitAfter >= 0 && tick - endTick >= scenario.exitAfter) {
                log.flush();
                log.close();
                Probe.flush();
                Runtime.getRuntime().halt(0);
            }
        } catch (Throwable t) {
            failed = true;
            System.err.println("[e2-agent] disabled: " + t);
        }
    }
}
