package com.submillisecond.guides.springboot;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

/**
 * Entry point. Nothing in this class is special - the interesting bit is
 * the single line in {@code application.properties}:
 * {@code spring.threads.virtual.enabled=true}.
 *
 * Under that setting Spring Boot 4 wires Tomcat's protocol handler to
 * spawn a virtual thread per accepted request and configures @Async /
 * TaskExecutor beans to use {@code Executors.newVirtualThreadPerTaskExecutor()}.
 * The application code does not know or care.
 */
@SpringBootApplication
public class Application {
    public static void main(String[] args) {
        SpringApplication.run(Application.class, args);
    }
}
