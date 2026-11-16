package e2;

import java.io.BufferedWriter;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;

/**
 * TEMPORARY (phase E.3). Read-only probes of the local player at four points of one client tick:
 * <ul>
 * <li>A: entry of {@code EntityPlayer.onLivingUpdate} (after EntityPlayerSP updated the sprint state);</li>
 * <li>B: entry of {@code EntityLivingBase.moveEntityWithHeading} (just before the movement is computed);</li>
 * <li>C: exit of {@code moveEntityWithHeading} (after the movement);</li>
 * <li>E: exit of {@code EntityPlayer.onLivingUpdate} (after setAIMoveSpeed).</li>
 * </ul>
 * Point D (end of {@code EntityPlayerSP.onUpdate}) is the regular state log. Nothing here writes to the game.
 * Obfuscated 1.8.9 names: pk.aM jumpMovementFactor, pr.bm landMovementFactor, pr.aZ moveStrafing,
 * pr.ba moveForward, wn.bF speedInAir, pr.bI() getAIMoveSpeed (overridden by wn.bI()).
 */
public final class Probe {
    private static BufferedWriter out;
    private static boolean ready;
    private static boolean failed;
    private static Field posX, posY, posZ, motX, motY, motZ, ground, jumpFactor, landFactor, strafe, forward, speedInAir;
    private static Method isSprinting, aiMoveSpeed;

    private Probe() {
    }

    static void init(String path) throws IOException {
        out = new BufferedWriter(new OutputStreamWriter(new FileOutputStream(path), StandardCharsets.UTF_8));
        out.write("# e2 probe log (temporary, phase E.3)\n");
        out.write("tick,script_tick,phase,point,sprinting,land_movement_factor,live_ai_move_speed,"
                + "jump_movement_factor,speed_in_air,move_forward,move_strafing,mx,my,mz,x,y,z,on_ground\n");
        out.flush();
    }

    static void flush() {
        try {
            if (out != null) {
                out.flush();
            }
        } catch (IOException ignored) {
            // The probe never disturbs the game.
        }
    }

    private static Field find(Class<?> from, String name) throws Exception {
        for (Class<?> c = from; c != null; c = c.getSuperclass()) {
            try {
                Field f = c.getDeclaredField(name);
                f.setAccessible(true);
                return f;
            } catch (NoSuchFieldException next) {
                // keep climbing
            }
        }
        throw new NoSuchFieldException(name);
    }

    private static void resolve(Class<?> player) throws Exception {
        posX = find(player, "s");
        posY = find(player, "t");
        posZ = find(player, "u");
        motX = find(player, "v");
        motY = find(player, "w");
        motZ = find(player, "x");
        ground = find(player, "C");
        jumpFactor = find(player, "aM");
        landFactor = find(player, "bm");
        strafe = find(player, "aZ");
        forward = find(player, "ba");
        speedInAir = find(player, "bF");
        isSprinting = player.getMethod("aw");
        aiMoveSpeed = player.getMethod("bI");
        ready = true;
    }

    /** Called at entry of {@code setAIMoveSpeed}: point {@code S}, the value written is in {@code land_movement_factor}. */
    public static void setter(Object self, float value) {
        if (failed || out == null || !ready || !"bew".equals(self.getClass().getName())) {
            return;
        }
        try {
            out.write(Hook.tickNow() + "," + Hook.scriptTick() + "," + Hook.phaseName() + ",S,"
                    + isSprinting.invoke(self) + "," + value + "," + aiMoveSpeed.invoke(self) + ","
                    + jumpFactor.getFloat(self) + "," + speedInAir.getFloat(self) + ","
                    + forward.getFloat(self) + "," + strafe.getFloat(self) + ","
                    + motX.getDouble(self) + "," + motY.getDouble(self) + "," + motZ.getDouble(self) + ","
                    + posX.getDouble(self) + "," + posY.getDouble(self) + "," + posZ.getDouble(self) + ","
                    + ground.getBoolean(self) + "\n");
        } catch (Throwable t) {
            failed = true;
            System.err.println("[e2-agent] probe disabled: " + t);
        }
    }

    /** Called by the injected bytecode with {@code this} and the probe point. */
    public static void at(Object self, String point) {
        if (failed || out == null || !"bew".equals(self.getClass().getName())) {
            return;
        }
        try {
            if (!ready) {
                resolve(self.getClass());
            }
            out.write(Hook.tickNow() + "," + Hook.scriptTick() + "," + Hook.phaseName() + "," + point + ","
                    + isSprinting.invoke(self) + "," + landFactor.getFloat(self) + "," + aiMoveSpeed.invoke(self) + ","
                    + jumpFactor.getFloat(self) + "," + speedInAir.getFloat(self) + ","
                    + forward.getFloat(self) + "," + strafe.getFloat(self) + ","
                    + motX.getDouble(self) + "," + motY.getDouble(self) + "," + motZ.getDouble(self) + ","
                    + posX.getDouble(self) + "," + posY.getDouble(self) + "," + posZ.getDouble(self) + ","
                    + ground.getBoolean(self) + "\n");
            if (point.equals("E") && Hook.tickNow() % 20 == 0) {
                out.flush();
            }
        } catch (Throwable t) {
            failed = true;
            System.err.println("[e2-agent] probe disabled: " + t);
        }
    }
}
