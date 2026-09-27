import { spawn } from "node:child_process";
import { platform } from "node:os";

const executable = process.argv[2];
if (!executable) {
  console.error("usage: node run-with-timeout.mjs <probe-executable> [arguments...]");
  process.exit(2);
}

const timeoutMs = Number(process.env.SDL3_PROBE_TIMEOUT_MS ?? 90_000);
if (!Number.isFinite(timeoutMs) || timeoutMs < 1) {
  console.error("SDL3_PROBE_TIMEOUT_MS must be a positive number");
  process.exit(2);
}

const executablePath = platform() === "win32" && !executable.endsWith(".exe")
  ? `${executable}.exe`
  : executable;
const child = spawn(executablePath, process.argv.slice(3), { stdio: "inherit" });
let timedOut = false;
const timer = setTimeout(() => {
  timedOut = true;
  console.error(`SDL3 gamepad probe exceeded ${timeoutMs} ms; terminating the isolated process.`);
  child.kill("SIGKILL");
}, timeoutMs);

child.on("error", (error) => {
  clearTimeout(timer);
  console.error(`could not start SDL3 gamepad probe: ${error.message}`);
  process.exitCode = 1;
});
child.on("exit", (code, signal) => {
  clearTimeout(timer);
  if (timedOut || code !== 0) {
    console.error(`SDL3 gamepad probe exited with code ${code}, signal ${signal}`);
    process.exitCode = code ?? 1;
  }
});
