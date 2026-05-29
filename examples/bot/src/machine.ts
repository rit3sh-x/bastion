import { log } from "./ui";

export type DemoState =
    | "INIT"
    | "SESSION_READY"
    | "ALLOWANCE_GRANTED"
    | "GUARD_ENFORCED"
    | "TRADE_SETTLED"
    | "REVOKED"
    | "DONE";

const NEXT: Record<DemoState, readonly DemoState[]> = {
    INIT: ["SESSION_READY"],
    SESSION_READY: ["ALLOWANCE_GRANTED"],
    ALLOWANCE_GRANTED: ["GUARD_ENFORCED"],
    GUARD_ENFORCED: ["TRADE_SETTLED"],
    TRADE_SETTLED: ["REVOKED"],
    REVOKED: ["DONE"],
    DONE: [],
};

export interface StateEvent {
    seq: number;
    from: DemoState;
    to: DemoState;
    detail: Record<string, unknown>;
}

export class DemoMachine {
    private state: DemoState = "INIT";
    private seq = 0;
    private readonly trail: StateEvent[] = [];

    transition(to: DemoState, detail: Record<string, unknown> = {}): void {
        if (!NEXT[this.state].includes(to)) {
            throw new Error(`illegal demo transition: ${this.state} → ${to}`);
        }
        this.seq += 1;
        this.trail.push({ seq: this.seq, from: this.state, to, detail });
        this.state = to;
        log(`▸ ${JSON.stringify({ seq: this.seq, state: to, ...detail })}`);
    }

    get current(): DemoState {
        return this.state;
    }

    get events(): readonly StateEvent[] {
        return this.trail;
    }
}
