export const DEFAULT_RAIL_HOLD_MS = 2000
export const MIN_RAIL_HOLD_MS = 500
export const MAX_RAIL_HOLD_MS = 10_000
export const RAIL_HOLD_STEP_MS = 100

export function normalizeRailHoldMs(value: unknown): number {
  return typeof value === 'number' &&
    Number.isInteger(value) &&
    value >= MIN_RAIL_HOLD_MS &&
    value <= MAX_RAIL_HOLD_MS &&
    value % RAIL_HOLD_STEP_MS === 0
    ? value
    : DEFAULT_RAIL_HOLD_MS
}
