package com.submillisecond.primers.springboot;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.test.context.TestPropertySource;
import org.springframework.test.web.servlet.MockMvc;

import static org.hamcrest.Matchers.is;
import static org.hamcrest.Matchers.notNullValue;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

/**
 * Black-box tests for {@link OrderRoutingController}.
 *
 * Two reasons for running through {@link MockMvc} rather than calling
 * the controller method directly:
 * <ol>
 *   <li>The fan-out path uses {@code Executors.newVirtualThreadPerTaskExecutor()}
 *       inside a try-with-resources. Calling the method directly is fine,
 *       but exercising the JSON serialisation contract through Spring's
 *       message converters catches accidental field renames in the result
 *       records.</li>
 *   <li>It pins the endpoint shape that the LoadDriver depends on -
 *       moving {@code /route} to another path would break the bench
 *       script as well as this test.</li>
 * </ol>
 *
 * Park budgets are shrunk to keep the test fast; the production defaults
 * (1ms per venue) live in {@code application.properties} and are
 * meaningful only when measuring under load.
 */
@SpringBootTest
@AutoConfigureMockMvc
@TestPropertySource(properties = {
        "routing.fanout=3",
        "routing.venue-park-micros=10"
})
final class OrderRoutingControllerTest {

    @Autowired private MockMvc mvc;

    @Test
    @DisplayName("/route returns the documented JSON shape")
    void routeReturnsExpectedShape() throws Exception {
        mvc.perform(get("/route").param("symbol", "AAPL").param("quantity", "100"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.symbol",         is("AAPL")))
                .andExpect(jsonPath("$.quantity",       is(100)))
                .andExpect(jsonPath("$.venuesQueried",  is(3)))
                .andExpect(jsonPath("$.best",           notNullValue()))
                .andExpect(jsonPath("$.best.venue",     notNullValue()))
                .andExpect(jsonPath("$.best.priceTicks", notNullValue()));
    }

    @Test
    @DisplayName("default query params produce a valid response")
    void defaultsProduceValidResponse() throws Exception {
        mvc.perform(get("/route"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.symbol",         is("AAPL")))
                .andExpect(jsonPath("$.quantity",       is(100)));
    }

    @Test
    @DisplayName("size on the best quote echoes the requested quantity")
    void bestQuoteEchoesRequestedQuantity() throws Exception {
        mvc.perform(get("/route").param("symbol", "MSFT").param("quantity", "5000"))
                .andExpect(status().isOk())
                .andExpect(jsonPath("$.best.sizeAvailable", is(5000)));
    }
}
