const C = {
    reset: "\x1b[0m",
    bold: "\x1b[1m",
    dim: "\x1b[2m",
    cyan: "\x1b[36m",
    green: "\x1b[32m",
    yellow: "\x1b[33m",
    red: "\x1b[31m",
    magenta: "\x1b[35m",
    gray: "\x1b[90m",
};

export function log(msg = ""): void {
    process.stdout.write(msg + "\n");
}

export function warn(msg: string): void {
    process.stderr.write(`${C.yellow}[warn]${C.reset} ${msg}\n`);
}

export function err(msg: string): void {
    process.stderr.write(`${C.red}[error]${C.reset} ${msg}\n`);
}

export function assistant(msg: string): void {
    log(`${C.green}${C.bold}agent ❯${C.reset} ${msg}`);
}

export function toolTrace(name: string, detail = ""): void {
    log(
        `${C.magenta}  ⚙ ${name}${C.reset}${detail ? ` ${C.gray}${detail}${C.reset}` : ""}`
    );
}

export function banner(lines: string[]): void {
    const bar = C.cyan + "─".repeat(60) + C.reset;
    log(bar);
    for (const l of lines) log(`${C.cyan}│${C.reset} ${l}`);
    log(bar);
}

export const PROMPT = `${C.bold}${C.cyan}you ❯${C.reset} `;
