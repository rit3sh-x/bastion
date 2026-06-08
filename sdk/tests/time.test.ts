import { describe, expect, it } from "vitest";
import {
    seconds,
    minutes,
    hours,
    days,
    weeks,
    SUN,
    MON,
    TUE,
    WED,
    THU,
    FRI,
    SAT,
    T,
} from "@/time";

describe("duration helpers", () => {
    it("seconds = identity", () => {
        expect(seconds(30)).toBe(30);
    });
    it("minutes = n * 60", () => {
        expect(minutes(5)).toBe(300);
    });
    it("hours = n * 3600", () => {
        expect(hours(1)).toBe(3600);
        expect(hours(24)).toBe(86_400);
    });
    it("days = n * 86_400", () => {
        expect(days(1)).toBe(86_400);
        expect(days(7)).toBe(604_800);
    });
    it("weeks = n * 604_800", () => {
        expect(weeks(1)).toBe(604_800);
    });
    it("compose: 1 day = 24 hours = 1440 minutes", () => {
        expect(days(1)).toBe(hours(24));
        expect(hours(24)).toBe(minutes(1440));
    });
});

describe("day bits + T namespace", () => {
    it("day bit constants", () => {
        expect(SUN).toBe(1);
        expect(MON).toBe(2);
        expect(SAT).toBe(64);
    });
    it("workdays / weekend / everyday presets", () => {
        expect(T.workdays).toBe(MON | TUE | WED | THU | FRI);
        expect(T.weekend).toBe(SAT | SUN);
        expect(T.everyday).toBe(0b0111_1111);
    });
    it("daysMask composes bits", () => {
        expect(T.daysMask([MON, WED, FRI])).toBe(MON | WED | FRI);
    });
    it("minutesUtc(h, m)", () => {
        expect(T.minutesUtc(9)).toBe(540);
        expect(T.minutesUtc(17, 30)).toBe(1050);
    });
    it("minutesUtc rejects out-of-range", () => {
        expect(() => T.minutesUtc(25)).toThrow(RangeError);
        expect(() => T.minutesUtc(9, 60)).toThrow(RangeError);
    });
    it("minutesUtc treats 24:00 as end-of-day and rejects 24:xx", () => {
        expect(T.minutesUtc(24)).toBe(1440);
        expect(() => T.minutesUtc(24, 30)).toThrow(RangeError);
    });
});
