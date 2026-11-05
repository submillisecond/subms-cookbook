package com.submillisecond.recipes.seqlock;

public final class Demo {
    public static void main(String[] args) throws InterruptedException {
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(5), new long[] {10_000, 300, 10_001, 500, 0});
        SeqLock<long[]>.Writer writer = lock.writer();

        long[] torn = {0};
        Thread strategy = new Thread(() -> {
            SeqLock<long[]>.Reader reader = lock.reader();
            long last = 0;
            while (last < 10_000) {
                long[] top = reader.read();
                if (top[2] != top[0] + 1 || top[1] != 300 + top[4] % 1000) {
                    torn[0]++;
                }
                last = top[4];
            }
        });
        strategy.start();

        for (long i = 1; i <= 10_000; i++) {
            writer.set(new long[] {10_000 + i % 5, 300 + i % 1000, 10_001 + i % 5, 500, i});
        }
        strategy.join();

        System.out.println("writer published " + writer.version() + " snapshots");
        System.out.println("reader saw " + torn[0] + " torn snapshots");
    }
}
