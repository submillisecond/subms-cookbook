package com.submillisecond.recipes.events;

/** Adapts an {@link EventBridge} into an {@link EventListener}. */
public final class BridgeListener implements EventListener {
    private final EventBridge bridge;

    public BridgeListener(EventBridge bridge) {
        this.bridge = bridge;
    }

    public String name() {
        return bridge.name();
    }

    @Override
    public void onEvent(Event event) {
        bridge.forward(event);
    }
}
