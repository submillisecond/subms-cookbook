/**
 * A working LSM tree (memtable, immutable SSTables, bloom-filter trailer) with sub-millisecond reads at p99 on a 50k-entry workload.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-lsm-tree">https://www.submillisecond.com/cookbook/recipes/subms-lsm-tree</a>
 */
package com.submillisecond.recipes.lsm;
