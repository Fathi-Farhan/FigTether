/**
 * Bounded reconnect backoff.
 *
 * The schedule grows exponentially, is capped, and has a finite number of
 * attempts. There is no retry loop that can run forever: the caller stops
 * after {@link BackoffConfig.maxAttempts} and asks the user to pair again.
 */

/** Configuration of the reconnect schedule. */
export interface BackoffConfig {
  /** Delay of the first retry in milliseconds. */
  readonly baseMs: number;
  /** Growth factor applied per attempt. */
  readonly factor: number;
  /** Upper bound of a single delay in milliseconds. */
  readonly maxMs: number;
  /** Total number of attempts, including the first one. */
  readonly maxAttempts: number;
  /** Relative jitter applied to a delay, between 0 and 1. */
  readonly jitterRatio: number;
}

/** Default reconnect schedule used by the bridge client. */
export const DEFAULT_BACKOFF: BackoffConfig = {
  baseMs: 500,
  factor: 2,
  maxMs: 15_000,
  maxAttempts: 6,
  jitterRatio: 0.2,
};

/** Source of randomness, injectable so that the schedule stays testable. */
export type RandomSource = () => number;

/**
 * Delay before the given attempt.
 *
 * Attempt numbers are one based: attempt 1 is the first retry. The delay is
 * capped at {@link BackoffConfig.maxMs} and never negative.
 */
export function backoffDelayMs(
  attempt: number,
  config: BackoffConfig = DEFAULT_BACKOFF,
  random: RandomSource = Math.random,
): number {
  const safeAttempt = Math.max(1, Math.trunc(attempt));
  const base = config.baseMs * Math.pow(config.factor, safeAttempt - 1);
  const capped = Math.min(base, config.maxMs);
  const ratio = Math.min(Math.max(config.jitterRatio, 0), 1);
  const offset = capped * ratio * (random() - 0.5) * 2;
  return Math.max(0, Math.round(capped + offset));
}

/** True while another attempt is allowed by the schedule. */
export function canRetry(attempt: number, config: BackoffConfig = DEFAULT_BACKOFF): boolean {
  return Math.trunc(attempt) < config.maxAttempts;
}

/** The full schedule without jitter, useful for tests and for the UI hint. */
export function backoffSchedule(config: BackoffConfig = DEFAULT_BACKOFF): readonly number[] {
  const delays: number[] = [];
  for (let attempt = 1; attempt <= config.maxAttempts; attempt += 1) {
    delays.push(Math.min(config.baseMs * Math.pow(config.factor, attempt - 1), config.maxMs));
  }
  return delays;
}
