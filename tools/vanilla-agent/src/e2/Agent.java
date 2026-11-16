package e2;

import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.security.ProtectionDomain;

import jdk.internal.org.objectweb.asm.ClassReader;
import jdk.internal.org.objectweb.asm.ClassVisitor;
import jdk.internal.org.objectweb.asm.ClassWriter;
import jdk.internal.org.objectweb.asm.MethodVisitor;
import jdk.internal.org.objectweb.asm.Opcodes;

/**
 * {@code -javaagent:e2-agent.jar=scenario=FILE,log=FILE}
 *
 * Adds two static calls to {@code EntityPlayerSP.onUpdate} (obfuscated {@code bew.t_()V} in 1.8.9) when
 * the class is loaded. The client jar on disk is not modified; the change lives in memory only.
 */
public final class Agent {
    private Agent() {
    }

    public static void premain(String args, Instrumentation inst) throws Exception {
        String scenario = null;
        String log = null;
        if (args != null) {
            for (String kv : args.split(",")) {
                int i = kv.indexOf('=');
                if (i > 0 && kv.substring(0, i).equals("scenario")) {
                    scenario = kv.substring(i + 1);
                } else if (i > 0 && kv.substring(0, i).equals("log")) {
                    log = kv.substring(i + 1);
                }
            }
        }
        if (scenario == null || log == null) {
            throw new IllegalArgumentException("usage: -javaagent:e2-agent.jar=scenario=FILE,log=FILE");
        }
        Hook.init(Scenario.load(scenario), log);
        // TEMPORARY (phase E.3): read-only probes at four points of the simulation, enabled by
        // probe=FILE. They only read fields and write a CSV.
        String probe = null;
        if (args != null) {
            for (String kv : args.split(",")) {
                if (kv.startsWith("probe=")) {
                    probe = kv.substring(6);
                }
            }
        }
        final boolean probing = probe != null;
        if (probing) {
            Probe.init(probe);
        }
        inst.addTransformer(new ClassFileTransformer() {
            @Override
            public byte[] transform(ClassLoader loader, String name, Class<?> cls, ProtectionDomain pd, byte[] bytes) {
                try {
                    if ("bew".equals(name)) {
                        return instrument(bytes);
                    }
                    if (probing && "wn".equals(name)) {
                        // EntityPlayer.onLivingUpdate: A at entry, E at exit.
                        return probeMethod(bytes, "m", "()V", "A", "E");
                    }
                    if (probing && "pr".equals(name)) {
                        // EntityLivingBase.moveEntityWithHeading: B at entry, C at exit.
                        return probeSetter(probeMethod(bytes, "g", "(FF)V", "B", "C"));
                    }
                    return null;
                } catch (Throwable t) {
                    System.err.println("[e2-agent] cannot instrument " + name + ": " + t);
                    return null;
                }
            }
        });
        System.err.println("[e2-agent] armed: " + scenario + " -> " + log + (probing ? " probe=" + probe : ""));
    }

    /** TEMPORARY: logs the argument of {@code setAIMoveSpeed} (pr.k(F)V) with {@code Probe.setter(this, value)}. */
    static byte[] probeSetter(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_MAXS);
        reader.accept(new ClassVisitor(Opcodes.ASM5, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String sig, String[] ex) {
                MethodVisitor mv = super.visitMethod(access, n, d, sig, ex);
                if (!n.equals("k") || !d.equals("(F)V")) {
                    return mv;
                }
                return new MethodVisitor(Opcodes.ASM5, mv) {
                    @Override
                    public void visitCode() {
                        super.visitCode();
                        super.visitVarInsn(Opcodes.ALOAD, 0);
                        super.visitVarInsn(Opcodes.FLOAD, 1);
                        super.visitMethodInsn(Opcodes.INVOKESTATIC, "e2/Probe", "setter", "(Ljava/lang/Object;F)V", false);
                    }
                };
            }
        }, 0);
        return writer.toByteArray();
    }

    /** Adds {@code Probe.at(this, entry)} at method entry and {@code Probe.at(this, exit)} before each return. */
    static byte[] probeMethod(byte[] bytes, final String method, final String desc, final String entry, final String exit) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_MAXS);
        final boolean[] patched = {false};
        reader.accept(new ClassVisitor(Opcodes.ASM5, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String sig, String[] ex) {
                MethodVisitor mv = super.visitMethod(access, n, d, sig, ex);
                if (!n.equals(method) || !d.equals(desc)) {
                    return mv;
                }
                patched[0] = true;
                return new MethodVisitor(Opcodes.ASM5, mv) {
                    private void call(String point) {
                        super.visitVarInsn(Opcodes.ALOAD, 0);
                        super.visitLdcInsn(point);
                        super.visitMethodInsn(Opcodes.INVOKESTATIC, "e2/Probe", "at", "(Ljava/lang/Object;Ljava/lang/String;)V", false);
                    }

                    @Override
                    public void visitCode() {
                        super.visitCode();
                        call(entry);
                    }

                    @Override
                    public void visitInsn(int op) {
                        if (op == Opcodes.RETURN) {
                            call(exit);
                        }
                        super.visitInsn(op);
                    }
                };
            }
        }, 0);
        if (!patched[0]) {
            throw new IllegalStateException(method + desc + " not found");
        }
        return writer.toByteArray();
    }

    static byte[] instrument(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_MAXS);
        final boolean[] patched = {false};
        reader.accept(new ClassVisitor(Opcodes.ASM5, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String desc, String sig, String[] ex) {
                MethodVisitor mv = super.visitMethod(access, n, desc, sig, ex);
                if (!n.equals("t_") || !desc.equals("()V")) {
                    return mv;
                }
                patched[0] = true;
                return new MethodVisitor(Opcodes.ASM5, mv) {
                    @Override
                    public void visitCode() {
                        super.visitCode();
                        super.visitVarInsn(Opcodes.ALOAD, 0);
                        super.visitMethodInsn(Opcodes.INVOKESTATIC, "e2/Hook", "before", "(Ljava/lang/Object;)V", false);
                    }

                    @Override
                    public void visitInsn(int op) {
                        if (op == Opcodes.RETURN) {
                            super.visitVarInsn(Opcodes.ALOAD, 0);
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "e2/Hook", "after", "(Ljava/lang/Object;)V", false);
                        }
                        super.visitInsn(op);
                    }
                };
            }
        }, 0);
        if (!patched[0]) {
            throw new IllegalStateException("bew.t_()V not found: not the 1.8.9 client?");
        }
        return writer.toByteArray();
    }
}
