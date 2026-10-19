#!/usr/bin/env node
/**
 * `bun run dev` / `tauri dev` entry: start Vite, or **reuse** the Vite dev server
 * that is already listening on :1420.
 *
 * Why: `tauri dev` runs `beforeDevCommand` every time, so a Vite server left
 * running from a previous session makes the new one die with
 * "Port 1420 is already in use". Tauri only needs `devUrl` to answer, so if the
 * port already serves a Vite dev server we exit 0 immediately and let Tauri
 * attach to it (HMR still works - it is the same working tree).
 */
import { spawn } from "node:child_process";
import { request } from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const PORT = 1420;

/** `vite` (ours) | `free` | `foreign` (something else is on the port). */
function probe(host) {
  return new Promise((resolve) => {
    const req = request(
      { host, port: PORT, path: "/@vite/client", method: "GET", timeout: 800 },
      (res) => {
        res.resume();
        // a Vite dev server serves this module; anything else is not ours
        resolve(res.statusCode === 200 ? "vite" : "foreign");
      },
    );
    req.on("timeout", () => {
      req.destroy();
      resolve("foreign");
    });
    req.on("error", (err) => resolve(err.code === "ECONNREFUSED" ? "free" : "foreign"));
    req.end();
  });
}

async function state() {
  for (const host of ["127.0.0.1", "::1"]) {
    const result = await probe(host);
    if (result !== "free") return result;
  }
  return "free";
}

const current = await state();

if (current === "vite") {
  console.log(
    `[dev] port ${PORT} already serves a Vite dev server - reusing it ` +
      "(no second instance, HMR keeps working)",
  );
  process.exit(0);
}

if (current === "foreign") {
  console.error(
    `[dev] port ${PORT} is taken by something that is not Vite.\n` +
      `      find it with:  Get-NetTCPConnection -LocalPort ${PORT} -State Listen\n` +
      "      then stop it, or change the port in vite.config.ts + tauri.conf.json (devUrl).",
  );
  process.exit(1);
}

const cli = path.join(ROOT, "node_modules", "vite", "bin", "vite.js");
const child = spawn(process.execPath, [cli], { cwd: ROOT, stdio: "inherit" });

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal));
}
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  else process.exit(code ?? 0);
});
