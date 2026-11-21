// What the Chrome cases share: the fixture host, one for each worker in a new store, and a
// second origin a case can serve a page from (plan m5, task U1, items 5 and 7).
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { test as base, expect } from "@playwright/test";

export interface Host {
  /** `http://127.0.0.1:<port>/`. */
  address: string;
  /** `http://127.0.0.1:<port>`. */
  origin: string;
  port: number;
  token: string;
  /** The page's address with the token, as `whizbang play` opens it. */
  opening: string;
}

export interface OtherOrigin {
  /** `http://127.0.0.1:<another port>/`, which serves a blank page. */
  address: string;
}

/** The fixture host's binary: `scripts/e2e.sh` builds it and names it here. */
const BINARY = process.env.WHIZBANG_PLAY_FIXTURE ?? "../target/debug/examples/play_fixture";

async function startHost(store: string, more: string[] = []): Promise<{ host: Host; child: ChildProcess }> {
  const child = spawn(BINARY, ["--store", store, "--stdin-quit", ...more], { stdio: ["pipe", "pipe", "inherit"] });
  const line = await new Promise<string>((resolve, reject) => {
    const lines = createInterface({ input: child.stdout! });
    lines.once("line", (first) => {
      lines.close();
      resolve(first);
    });
    child.once("error", reject);
    child.once("exit", (code) => reject(new Error(`the fixture host exited (${code}) before it said its address`)));
  });
  // `http://127.0.0.1:<port>/#k=<token>`, as `whizbang play --no-open` prints it.
  const said = /^(http:\/\/127\.0\.0\.1:(\d+)\/)#k=([0-9a-f]{64})$/.exec(line.trim());
  if (!said) throw new Error(`the fixture host's first line is not its address and token: ${line}`);
  const [, address, port, token] = said as unknown as [string, string, string, string];
  return {
    child,
    host: { address, origin: address.replace(/\/$/, ""), port: Number(port), token, opening: `${address}#k=${token}` },
  };
}

async function stopHost(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null) return;
  const exited = new Promise<void>((resolve) => child.once("exit", () => resolve()));
  child.stdin?.end();
  const timer = setTimeout(() => child.kill("SIGKILL"), 5_000);
  await exited;
  clearTimeout(timer);
}

/**
 * A second fixture host of the case's own, started with these options (`--third-kit`) in a
 * new store, and stopped when `use` is done.
 */
export async function withHost<T>(more: string[], use: (host: Host) => Promise<T>): Promise<T> {
  const store = mkdtempSync(join(tmpdir(), "whizbang-e2e-"));
  const { host, child } = await startHost(store, more);
  try {
    return await use(host);
  } finally {
    await stopHost(child);
    rmSync(store, { recursive: true, force: true });
  }
}

export const test = base.extend<object, { host: Host; otherOrigin: OtherOrigin }>({
  host: [
    async ({}, use) => {
      const store = mkdtempSync(join(tmpdir(), "whizbang-e2e-"));
      const { host, child } = await startHost(store);
      try {
        await use(host);
      } finally {
        await stopHost(child);
        rmSync(store, { recursive: true, force: true });
      }
    },
    { scope: "worker" },
  ],
  otherOrigin: [
    async ({}, use) => {
      const server: Server = createServer((_request, response) => {
        response.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
        response.end("<!doctype html><title>another origin</title><p>another origin</p>");
      });
      await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
      const address = server.address();
      if (address === null || typeof address === "string") throw new Error("the other origin has no port");
      try {
        await use({ address: `http://127.0.0.1:${address.port}/` });
      } finally {
        await new Promise<void>((resolve) => server.close(() => resolve()));
      }
    },
    { scope: "worker" },
  ],
});

export { expect };
