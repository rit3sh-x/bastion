import { policyData } from "../generated";
import type { PolicyDataArgs } from "../generated";

export interface TimeOfDayWindowInput {
    /** Minutes-since-UTC-midnight the window opens (inclusive). Use `T.minutesUtc`. */
    startMinute: number;
    /** Minutes-since-UTC-midnight the window closes (exclusive). */
    endMinute: number;
    /** Bitmask of allowed weekdays. Build with `T.daysMask` / `T.workdays` etc. */
    daysMask: number;
}

/** Only allow calls within a UTC time-of-day window on selected days. Stateless. */
export const TimeOfDayWindow = (input: TimeOfDayWindowInput): PolicyDataArgs =>
    policyData("TimeOfDayWindow", {
        startMinute: input.startMinute,
        endMinute: input.endMinute,
        daysMask: input.daysMask,
    });
