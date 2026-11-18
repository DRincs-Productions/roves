// Plays the start of visual-novel-template through the stdio MCP server and
// the game's own Pixi'VN Game.testing bridge (live setProps/setActions).
// The dist must come from template-testing-optin.mjs. Usage:
//   node playthrough.mjs <dev-mcp probe executable> <game dist> [--out <result.json>]
// Prints one `ROVES_MCP_PLAYTHROUGH {...}` line and exits 1 on any failure.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

const [probe, dist, outFlag, out] = process.argv.slice(2);
assert.ok(probe && dist && (outFlag === undefined || (outFlag === "--out" && out)),
  "Usage: node playthrough.mjs <dev-mcp probe executable> <game dist> [--out <result.json>]");

const PLAYER_NAME = "RovesMcp";
const MIN_DISTINCT_STATES = 4;
const MAX_ADVANCES = 25;
const SERVER_EXIT_BUDGET_MS = 180_000;
const started = Date.now();
const milestones = [];
const result = { status: "failed", jitless: process.env.ROVES_PROBE_JITLESS === "1", milestones };
const milestone = (name, detail = {}) => milestones.push({ name, at_ms: Date.now() - started, ...detail });

const server = spawn(process.execPath, [fileURLToPath(new URL("./server.mjs", import.meta.url)), probe, dist],
  { stdio: ["pipe", "pipe", "inherit"], windowsHide: true });
const closed = once(server, "close");
const lines = createInterface({ input: server.stdout })[Symbol.asyncIterator]();
const deadline = setTimeout(() => server.kill(), 600_000);
let nextId = 0;

function send(message) {
  server.stdin.write(JSON.stringify({ jsonrpc: "2.0", ...message }) + "\n");
}

async function rpc(method, params) {
  const id = ++nextId;
  send({ id, method, params });
  while (true) {
    const { done, value } = await lines.next();
    if (done) throw new Error(`MCP server closed while waiting for ${method}`);
    const reply = JSON.parse(value);
    if (reply.id !== id) continue;
    if (reply.error) throw new Error(`${method}: ${JSON.stringify(reply.error).slice(0, 300)}`);
    return reply.result;
  }
}

// evaluate_script only accepts synchronous, JSON-serializable results.
async function evaluate(source) {
  const reply = await rpc("tools/call", { name: "evaluate_script", arguments: { source } });
  const text = reply.content?.[0]?.text ?? "";
  if (reply.isError) throw new Error("evaluate_script: " + text.slice(0, 500));
  return JSON.parse(text);
}

async function poll(what, source, accept, timeoutMs, intervalMs = 200) {
  const until = Date.now() + timeoutMs;
  let last;
  while (true) {
    last = await evaluate(source);
    if (accept(last)) return last;
    if (Date.now() >= until) {
      throw new Error(`${what} not reached within ${timeoutMs / 1000} s; last: ${JSON.stringify(last).slice(0, 600)}`);
    }
    await delay(intervalMs);
  }
}

// Bounded schedule-and-poll: the action promise is settled in the page and
// its outcome is read back by later synchronous evaluations.
async function act(what, expression, timeoutMs = 60_000) {
  const id = await evaluate(`(() => {
    const jobs = (globalThis.__rovesMcpJobs ??= []);
    const id = jobs.push({ status: "pending" }) - 1;
    Promise.resolve().then(() => (${expression})).then(
      () => { jobs[id] = { status: "fulfilled" }; },
      (error) => { jobs[id] = { status: "rejected", error: String(error?.stack ?? error).slice(0, 600) }; });
    return id;
  })()`);
  const job = await poll(what, `globalThis.__rovesMcpJobs[${id}]`, (job) => job.status !== "pending", timeoutMs);
  if (job.status === "rejected") throw new Error(`${what} rejected: ${job.error}`);
}

const STATE = `(() => {
  const p = window.pixiVN;
  const s = p.getState();
  const d = s.dialogue;
  const c = d?.character;
  return {
    text: d == null ? null : (Array.isArray(d.text) ? d.text.join(" ") : String(d.text ?? "")).slice(0, 300),
    character: typeof c === "string" ? c : c?.id ?? null,
    choices: s.choices?.map((choice) => ({ index: choice.choiceIndex, text: String(choice.text).slice(0, 80) })) ?? null,
    input: s.input.isRequired,
    canContinue: s.canContinue,
    canGoBack: s.canGoBack,
    label: s.currentLabelId ?? null,
    step: s.stepCounter,
    path: location.pathname + location.hash,
    errors: p.errors.map((entry) => String(entry.error?.stack ?? entry.error).slice(0, 300)),
  };
})()`;
const state = () => evaluate(STATE);
const actionable = (s) => s.canContinue || s.input || (s.choices?.length ?? 0) > 0;

function assertNoErrors(s, where) {
  assert.deepEqual(s.errors, [], `Game.testing captured game errors ${where}`);
}

try {
  await rpc("initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "roves-playthrough", version: "0.0.0" },
  });
  send({ method: "notifications/initialized" });
  const { tools } = await rpc("tools/list", {});
  assert.ok(tools.some(({ name }) => name === "evaluate_script"), "evaluate_script tool missing");
  milestone("mcp_initialized");

  // The first evaluation launches the windowed game with isolated storage.
  const bridge = await poll("Game.testing bridge with live props and actions", `(() => {
    const p = window.pixiVN;
    if (!p) return { ready: false, deno: typeof globalThis.Deno };
    const props = p.props ?? {};
    const labels = p.getLabelIds();
    return {
      ready: typeof p.navigate === "function" && typeof props.navigate === "function" &&
        typeof props.t === "function" && typeof props.toast === "function" && labels.includes("start"),
      deno: typeof globalThis.Deno,
      props: Object.keys(props).sort(),
      labels: labels.length,
      path: location.pathname + location.hash,
      boot_errors: p.errors.map((entry) => String(entry.error?.stack ?? entry.error).slice(0, 300)),
    };
  })()`, (bridge) => bridge.ready || bridge.deno !== "undefined", 240_000, 500);
  assert.equal(bridge.deno, "undefined", "privileged Deno API exposed to the game");
  milestone("bridge_ready", bridge);
  await evaluate("window.pixiVN.clearErrors(), true");

  // The template's start action navigates to /game/narration, then starts.
  await act("Game.testing start", `window.pixiVN.start("start")`);
  let current = await poll("first dialogue", STATE, (s) => Boolean(s.text) && actionable(s), 60_000);
  assert.equal(current.label, "start", "start did not open the start label");
  assert.match(current.path, /\/game\/narration/, "start did not navigate to the narration screen");
  assertNoErrors(current, "after start");
  const states = [current];
  milestone("dialogue", current);

  let backChecked = false;
  let nameRequested = false;
  let nameShown = false;
  for (let advance = 0; advance < MAX_ADVANCES; advance++) {
    if (states.length >= MIN_DISTINCT_STATES && backChecked && nameShown) break;
    const before = current;
    if (before.choices?.length) {
      await act("select choice", `window.pixiVN.selectChoice(${before.choices[0].index})`);
      milestone("choice_selected", before.choices[0]);
    } else {
      if (before.input) {
        await evaluate(`window.pixiVN.setInput(${JSON.stringify(PLAYER_NAME)}), true`);
        nameRequested = true;
        milestone("input_answered", { value: PLAYER_NAME, prompt: before.text });
      }
      await act("continue", "window.pixiVN.continue()");
    }
    current = await poll("next step", STATE, (s) => s.step > before.step && actionable(s), 60_000);
    assertNoErrors(current, `at step ${current.step}`);
    if (current.text && current.text !== states.at(-1).text) {
      states.push(current);
      milestone("dialogue", current);
    }
    if (nameRequested && current.text?.includes(PLAYER_NAME)) nameShown = true;

    // Once two distinct states exist, go back once and replay forward.
    if (!backChecked && states.length === 2 && !nameRequested) {
      assert.ok(current.canGoBack, "canGoBack false after two dialogue states");
      await act("go back", "window.pixiVN.goBack()");
      const back = await poll("previous dialogue after goBack", STATE,
        (s) => s.text === states[0].text && actionable(s), 60_000);
      assertNoErrors(back, "after goBack");
      milestone("went_back", back);
      await act("continue after goBack", "window.pixiVN.continue()");
      current = await poll("replayed dialogue", STATE,
        (s) => s.text === states[1].text && actionable(s), 60_000);
      assertNoErrors(current, "after replay");
      milestone("replayed", current);
      backChecked = true;
    }
  }

  const distinct = new Set(states.map((s) => s.text)).size;
  assert.ok(distinct >= MIN_DISTINCT_STATES, `only ${distinct} distinct dialogue states`);
  assert.ok(backChecked, "goBack was never exercised");
  assert.ok(nameRequested, "the story never requested the player's name");
  assert.ok(nameShown, `the entered name ${PLAYER_NAME} never appeared in later dialogue`);
  const final = await state();
  assertNoErrors(final, "at the end");
  Object.assign(result, { status: "passed", distinct_states: distinct, final_step: final.step });
} catch (error) {
  result.reason = String(error?.message ?? error).slice(0, 1200);
} finally {
  clearTimeout(deadline);
  // Closing stdin closes the game and removes its isolated storage.
  server.stdin.end();
  // Above server.mjs's CHILD_EXIT_BUDGET_MS (150 s, itself above the Rust
  // GPU teardown budget), so a slow but bounded GPU drain is not cut short
  // and a hung game is reported by the server's own kill first.
  const [code, signal] = await Promise.race([closed, delay(SERVER_EXIT_BUDGET_MS).then(() => ["timeout", null])]);
  if (code === "timeout") server.kill();
  result.shutdown = { exit: code, signal };
  milestone("shut_down", { exit: code, signal });
  if (result.status === "passed" && code !== 0) {
    result.status = "failed";
    result.reason = `MCP server or game did not exit cleanly (${code}, ${signal})`;
  }
  result.duration_ms = Date.now() - started;
  const line = JSON.stringify(result);
  if (out) writeFileSync(out, JSON.stringify(result, null, 2));
  console.log("ROVES_MCP_PLAYTHROUGH " + line);
  process.exitCode = result.status === "passed" ? 0 : 1;
}
