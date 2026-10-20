import { describe, expect, it } from "vitest";

import { backoffDelayMs, backoffSchedule, canRetry, DEFAULT_BACKOFF } from "../src/backoff.js";

const NO_JITTER = (): number => 0.5;

describe("backoffDelayMs", () => {
  it("grows exponentially from the base delay", () => {
    const config = { ...DEFAULT_BACKOFF, jitterRatio: 0 };
    expect(backoffDelayMs(1, config, NO_JITTER)).toBe(500);
    expect(backoffDelayMs(2, config, NO_JITTER)).toBe(1000);
    expect(backoffDelayMs(3, config, NO_JITTER)).toBe(2000);
    expect(backoffDelayMs(4, config, NO_JITTER)).toBe(4000);
  });

  it("never exceeds the maximum delay", () => {
    const config = { ...DEFAULT_BACKOFF, jitterRatio: 0 };
    expect(backoffDelayMs(10, config, NO_JITTER)).toBe(config.maxMs);
    expect(backoffDelayMs(50, config, NO_JITTER)).toBe(config.maxMs);
  });

  it("keeps the jitter inside the configured bound", () => {
    const config = { ...DEFAULT_BACKOFF, baseMs: 1000, maxMs: 1000, jitterRatio: 0.2 };
    const low = backoffDelayMs(1, config, () => 0);
    const high = backoffDelayMs(1, config, () => 1);
    expect(low).toBe(800);
    expect(high).toBe(1200);
  });

  it("never returns a negative delay", () => {
    const config = { ...DEFAULT_BACKOFF, baseMs: 10, maxMs: 10, jitterRatio: 1 };
    expect(backoffDelayMs(1, config, () => 0)).toBeGreaterThanOrEqual(0);
  });

  it("treats an attempt below one as the first attempt", () => {
    const config = { ...DEFAULT_BACKOFF, jitterRatio: 0 };
    expect(backoffDelayMs(0, config, NO_JITTER)).toBe(500);
    expect(backoffDelayMs(-3, config, NO_JITTER)).toBe(500);
  });
});

describe("canRetry", () => {
  it("stops after the configured number of attempts", () => {
    const config = { ...DEFAULT_BACKOFF, maxAttempts: 3 };
    expect(canRetry(1, config)).toBe(true);
    expect(canRetry(2, config)).toBe(true);
    expect(canRetry(3, config)).toBe(false);
    expect(canRetry(4, config)).toBe(false);
  });
});

describe("backoffSchedule", () => {
  it("has one entry per attempt and stays inside the cap", () => {
    const schedule = backoffSchedule();
    expect(schedule.length).toBe(DEFAULT_BACKOFF.maxAttempts);
    for (const delay of schedule) {
      expect(delay).toBeLessThanOrEqual(DEFAULT_BACKOFF.maxMs);
      expect(delay).toBeGreaterThan(0);
    }
  });
});
