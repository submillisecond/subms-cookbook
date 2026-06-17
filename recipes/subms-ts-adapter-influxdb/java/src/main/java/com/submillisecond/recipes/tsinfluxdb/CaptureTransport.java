package com.submillisecond.recipes.tsinfluxdb;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;
import java.util.Optional;

/**
 * Records every request and replays a queued response. Test-only injection
 * point - keeps the adapter's request shaping under unit test without a network.
 */
public final class CaptureTransport implements TsInfluxTransport {

    private final List<TsHttpRequest> sent = new ArrayList<>();
    private final Deque<TsHttpResponse> responses = new ArrayDeque<>();

    public CaptureTransport(List<TsHttpResponse> responses) {
        this.responses.addAll(responses);
    }

    public static CaptureTransport ok(String body) {
        return new CaptureTransport(List.of(new TsHttpResponse(200, body)));
    }

    public List<TsHttpRequest> sent() {
        return sent;
    }

    public Optional<TsHttpRequest> last() {
        return sent.isEmpty() ? Optional.empty() : Optional.of(sent.get(sent.size() - 1));
    }

    @Override
    public TsHttpResponse send(TsHttpRequest req) {
        sent.add(req);
        if (responses.isEmpty()) {
            return new TsHttpResponse(204, "");
        }
        return responses.removeFirst();
    }
}
