package com.submillisecond.recipes.lsm;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Regression: the background flush worker could die of an unchecked throwable
 * with a frozen memtable still queued, and every producer then parked forever on
 * a queue nothing would drain - a silently hung JVM, no error, no timeout.
 *
 * <p>The fault reproduced here is the one observed in the wild: a classpath
 * missing {@code subms-bloom-filter}, so {@code SSTable.write} raises
 * {@link NoClassDefFoundError} on the worker thread. The tree is driven through
 * a class loader that resolves the recipe's own classes but refuses the bloom
 * filter, which is the only way to fault the worker through its real code path.
 *
 * <p>Every case runs the writer on its own thread and joins with a timeout, so a
 * regression fails the suite instead of hanging CI.
 */
final class FlushWorkerDeathTest {

    private static final long DEADLINE_MS = 30_000;

    @TempDir
    Path tempRoot;

    private static final class BloomlessLoader extends URLClassLoader {

        BloomlessLoader(URL[] urls, ClassLoader parent) {
            super(urls, parent);
        }

        @Override
        protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
            if (name.startsWith("com.submillisecond.recipes.bloom.")) {
                throw new ClassNotFoundException(name);
            }
            if (!name.startsWith("com.submillisecond.recipes.lsm.")) {
                return super.loadClass(name, resolve);
            }
            // Child-first for the recipe itself: the parent can load these too,
            // and delegating would hand back classes whose bloom filter resolves.
            synchronized (getClassLoadingLock(name)) {
                Class<?> c = findLoadedClass(name);
                if (c == null) {
                    c = findClass(name);
                }
                if (resolve) {
                    resolveClass(c);
                }
                return c;
            }
        }
    }

    @Test
    void writerFailsInsteadOfHangingWhenTheFlushWorkerDies() throws Exception {
        AtomicReference<Throwable> failure = new AtomicReference<>();
        runDriver("worker-death", (treeClass, tree) -> {
            failure.set(driveUntilFailure(treeClass, tree));
            closeQuietly(treeClass, tree);
        });

        Throwable t = failure.get();
        assertNotNull(t, "the missing bloom filter never surfaced as an error");
        assertInstanceOf(IOException.class, t, "writer failure surfaced as " + t);
        assertTrue(
            t.getMessage().contains("flush worker stopped"),
            "expected the dead worker to be named, got: " + t.getMessage());
        assertTrue(
            hasCause(t, NoClassDefFoundError.class),
            "expected the unchecked throwable that killed the worker as the cause");
    }

    @Test
    void readsStillServeAfterTheFlushWorkerDies() throws Exception {
        AtomicReference<Object> hit = new AtomicReference<>();
        runDriver("worker-death-reads", (treeClass, tree) -> {
            assertNotNull(driveUntilFailure(treeClass, tree));
            Method get = treeClass.getMethod("get", String.class);
            hit.set(get.invoke(tree, "key0"));
            closeQuietly(treeClass, tree);
        });

        // Optional<String>, from a different loader only for the tree itself.
        assertNotNull(hit.get(), "the read path died with the write path");
        assertTrue(hit.get().toString().contains("v"), "the first key stopped being readable");
    }

    @Test
    void aHealthyTreeIsUnaffected() throws Exception {
        Path dir = Files.createDirectories(tempRoot.resolve("healthy"));
        try (LsmTree tree = new LsmTree(dir, 64)) {
            for (int i = 0; i < 64; i++) {
                tree.put("key" + i, "v".repeat(32));
                tree.flush();
            }
            assertNull(driveUntilFailure(LsmTree.class, tree), "a live worker must not report failure");
            assertTrue(tree.sstableCount() > 0);
        }
    }

    private interface Driver {
        void run(Class<?> treeClass, Object tree) throws Exception;
    }

    /** Builds a bloom-less tree, hands it to {@code driver} on its own thread, and enforces the deadline. */
    private void runDriver(String label, Driver driver) throws Exception {
        Path dir = Files.createDirectories(tempRoot.resolve(label));
        URL classes = LsmTree.class.getProtectionDomain().getCodeSource().getLocation();
        assertNotNull(classes, "cannot locate the recipe classes to reload");

        AtomicReference<Throwable> unexpected = new AtomicReference<>();
        try (BloomlessLoader loader =
                new BloomlessLoader(new URL[] {classes}, LsmTree.class.getClassLoader())) {
            Class<?> treeClass = loader.loadClass("com.submillisecond.recipes.lsm.LsmTree");
            Object tree = treeClass.getConstructor(Path.class, int.class).newInstance(dir, 64);

            Thread t = new Thread(() -> {
                try {
                    driver.run(treeClass, tree);
                } catch (Throwable e) {
                    unexpected.set(e);
                }
            }, "flush-worker-death-driver");
            t.setDaemon(true);
            t.start();
            t.join(DEADLINE_MS);
            assertFalse(t.isAlive(), "writer blocked after the flush worker died");
        }
        if (unexpected.get() != null) {
            throw new AssertionError("driver failed", unexpected.get());
        }
    }

    /** Writes until a call reports a failure; returns it, or null if none did. */
    private static Throwable driveUntilFailure(Class<?> treeClass, Object tree) throws Exception {
        Method put = treeClass.getMethod("put", String.class, String.class);
        Method flush = treeClass.getMethod("flush");
        for (int i = 0; i < 64; i++) {
            Throwable t = call(put, tree, "key" + i, "v".repeat(32));
            if (t != null) {
                return t;
            }
            t = call(flush, tree);
            if (t != null) {
                return t;
            }
        }
        return null;
    }

    private static void closeQuietly(Class<?> treeClass, Object tree) throws Exception {
        call(treeClass.getMethod("close"), tree);
    }

    private static Throwable call(Method m, Object target, Object... args) throws Exception {
        try {
            m.invoke(target, args);
            return null;
        } catch (InvocationTargetException e) {
            return e.getCause();
        }
    }

    private static boolean hasCause(Throwable t, Class<? extends Throwable> type) {
        for (Throwable c = t; c != null; c = c.getCause()) {
            if (type.isInstance(c)) {
                return true;
            }
        }
        return false;
    }
}
