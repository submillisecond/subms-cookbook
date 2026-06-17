package com.submillisecond.recipes.timer.features;

/**
 * Recurring scheduler glued to a {@link CronSchedule}. After each fire
 * the scheduler computes the next deadline from the cron schedule and
 * re-arms.
 */
public final class CronScheduler {

    private final CronSchedule schedule;
    private long lastFireEpoch;

    public CronScheduler(CronSchedule schedule, long nowEpoch) {
        this.schedule = schedule;
        this.lastFireEpoch = nowEpoch;
    }

    public CronSchedule schedule() { return schedule; }

    /**
     * Epoch-second the schedule will next fire, given the current
     * epoch second. Returns -1 if no firing within the schedule's
     * look-ahead horizon.
     */
    public long nextFire(long nowEpoch) {
        long after = Math.max(nowEpoch, lastFireEpoch + 1);
        return schedule.nextAfter(after);
    }

    /** Mark a fire at {@code epoch} consumed. */
    public void recordFire(long epoch) {
        this.lastFireEpoch = epoch;
    }
}
