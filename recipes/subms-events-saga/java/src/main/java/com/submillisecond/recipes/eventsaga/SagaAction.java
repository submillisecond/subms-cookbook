package com.submillisecond.recipes.eventsaga;

/** A step action (forward or compensate). A thrown exception signals failure;
 * its message becomes the reason. */
@FunctionalInterface
public interface SagaAction {
    void run() throws Exception;
}
