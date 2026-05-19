package com.submillisecond.recipes.lsm;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public final class Demo {
    public static void main(String[] args) throws IOException {
        Path dir = Files.createTempDirectory("lsm-demo-");
        System.out.println("data dir: " + dir);

        try (LsmTree lsm = new LsmTree(dir, 256)) {
            lsm.put("AAPL", "150.10");
            lsm.put("MSFT", "320.55");
            lsm.put("GOOG", "140.20");
            lsm.flush();                                 // SSTable_0

            lsm.put("AAPL", "150.42");                   // shadow older value
            lsm.delete("MSFT");                          // tombstone
            lsm.put("NVDA", "900.00");
            lsm.flush();                                 // SSTable_1

            System.out.println("AAPL = " + lsm.get("AAPL").orElse("<absent>"));
            System.out.println("MSFT = " + lsm.get("MSFT").orElse("<absent>"));
            System.out.println("GOOG = " + lsm.get("GOOG").orElse("<absent>"));
            System.out.println("NVDA = " + lsm.get("NVDA").orElse("<absent>"));
            System.out.println("sstables: " + lsm.sstableCount());
        }
    }
}
