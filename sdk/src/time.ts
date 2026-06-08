export const seconds = (n: number): number => n;
export const minutes = (n: number): number => n * 60;
export const hours = (n: number): number => n * 3_600;
export const days = (n: number): number => n * 86_400;
export const weeks = (n: number): number => n * 604_800;

export const SUN = 1 << 0;
export const MON = 1 << 1;
export const TUE = 1 << 2;
export const WED = 1 << 3;
export const THU = 1 << 4;
export const FRI = 1 << 5;
export const SAT = 1 << 6;

const WORKDAYS = MON | TUE | WED | THU | FRI;
const WEEKEND = SAT | SUN;
const EVERYDAY = 0b0111_1111;

export const T = {
    SUN,
    MON,
    TUE,
    WED,
    THU,
    FRI,
    SAT,
    workdays: WORKDAYS,
    weekend: WEEKEND,
    everyday: EVERYDAY,

    daysMask(bits: readonly number[]): number {
        return bits.reduce((acc, b) => acc | b, 0);
    },

    minutesUtc(hour: number, minute = 0): number {
        if (!Number.isInteger(hour) || hour < 0 || hour > 24) {
            throw new RangeError(`hour out of range: ${hour}`);
        }
        if (!Number.isInteger(minute) || minute < 0 || minute > 59) {
            throw new RangeError(`minute out of range: ${minute}`);
        }
        if (hour === 24 && minute !== 0) {
            throw new RangeError(`minute out of range for 24:00: ${minute}`);
        }
        return hour * 60 + minute;
    },
} as const;
