package com.submillisecond.recipes.spsc;

public final class Demo {
    public static void main(String[] args) throws InterruptedException {
        SpscRingBuffer<Integer> q = new SpscRingBuffer<>(16);
        SpscRingBuffer<Integer>.Producer p = q.producer();
        SpscRingBuffer<Integer>.Consumer c = q.consumer();

        Thread producer = new Thread(() -> {
            for (int i = 0; i < 10; i++) {
                while (!p.tryPush(i)) { /* spin */ }
            }
        });
        StringBuilder seen = new StringBuilder("[");
        Thread consumer = new Thread(() -> {
            int got = 0;
            while (got < 10) {
                Integer v = c.tryPop();
                if (v != null) {
                    if (seen.length() > 1) seen.append(", ");
                    seen.append(v);
                    got++;
                }
            }
        });

        producer.start();
        consumer.start();
        producer.join();
        consumer.join();
        seen.append("]");
        System.out.println("consumed: " + seen);
    }
}
