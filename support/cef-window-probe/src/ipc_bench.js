// Deno side of the opt-in Deno -> Blink IPC benchmark. Runs as a classic script in a plain
// deno_core 0.413.0 JsRuntime inside the CEF browser process: no Deno.* namespace API and no
// extension other than the three ops below. Each message goes Deno -> CEF UI thread ->
// CefFrame::SendProcessMessage(PID_RENDERER) -> the fixture page -> reply to PID_BROWSER.
"use strict";
((ops) => {
  const ITEMS = 256;
  const BATCH_SIZES = [1, 16, ITEMS];
  const BATCH_ROUNDS = 64;
  const CHAIN_STEPS = ITEMS;
  const CHAIN_ROUNDS = 4;
  const WARMUP = 20;
  // Integer CSS px widths are exact in Blink's 1/64 px LayoutUnit at any device scale factor.
  const TOLERANCE_PX = 1 / 64;

  const failures = [];
  let mismatches = 0;
  const check = (condition, what) => {
    if (!condition) {
      mismatches++;
      if (failures.length < 16) failures.push(what);
    }
    return condition;
  };
  const now = () => ops.op_roves_ipc_now_us();
  const send = async (message) => JSON.parse(await ops.op_roves_ipc_send(JSON.stringify(message)));
  const round1 = (value) => Math.round(value * 10) / 10;
  const stats = (samples) => {
    const sorted = [...samples].sort((a, b) => a - b);
    const at = (q) => sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil(q * sorted.length) - 1))];
    const total = sorted.reduce((sum, value) => sum + value, 0);
    return {
      count: sorted.length,
      minUs: round1(sorted[0]),
      p50Us: round1(at(0.5)),
      p95Us: round1(at(0.95)),
      maxUs: round1(sorted[sorted.length - 1]),
      totalUs: round1(total),
    };
  };

  async function batched(size) {
    const rtt = [];
    const blink = [];
    for (let round = 0; round < BATCH_ROUNDS; round++) {
      const updates = [];
      let widthSum = 0;
      let textLen = 0;
      for (let index = 0; index < size; index++) {
        const width = 20 + ((round * 31 + index * 7) % 200);
        const text = `r${round}i${index}`;
        updates.push([index, width, text]);
        widthSum += width;
        textLen += text.length;
      }
      const start = now();
      const reply = await send({ op: "batch", updates });
      rtt.push(now() - start);
      blink.push(reply.blinkMs * 1000);
      check(
        reply.ok === true && reply.applied === size && reply.widthSum === widthSum && reply.textLen === textLen,
        `batch size=${size} round=${round} expected=${widthSum}/${textLen} got=${JSON.stringify(reply)}`,
      );
    }
    const roundTrip = stats(rtt);
    return {
      name: `batch-${size}`,
      updatesPerMessage: size,
      messages: BATCH_ROUNDS,
      roundTrip,
      blinkHandler: stats(blink),
      perUpdateP50Us: round1(roundTrip.p50Us / size),
    };
  }

  // Each width depends on the geometry Blink returned for the previous one, so every step is
  // a full round trip plus a forced layout; nothing can be batched.
  async function dependentChain() {
    const rtt = [];
    const blink = [];
    const chains = [];
    let maxDeviationPx = 0;
    for (let round = 0; round < CHAIN_ROUNDS; round++) {
      let width = 40 + round;
      const chainStart = now();
      for (let step = 0; step < CHAIN_STEPS; step++) {
        const start = now();
        const reply = await send({ op: "step", width });
        rtt.push(now() - start);
        blink.push(reply.blinkMs * 1000);
        const deviation = Math.abs(reply.width - width);
        if (Number.isFinite(deviation)) maxDeviationPx = Math.max(maxDeviationPx, deviation);
        if (!check(reply.ok === true && deviation <= TOLERANCE_PX, `chain round=${round} step=${step} width=${width} got=${JSON.stringify(reply)}`)) {
          break;
        }
        width = 20 + ((Math.round(reply.width) * 13 + step) % 300);
      }
      chains.push(now() - chainStart);
    }
    return {
      name: `dependent-chain-${CHAIN_STEPS}`,
      updatesPerMessage: 1,
      messages: rtt.length,
      roundTrip: stats(rtt),
      blinkHandler: stats(blink),
      chain: stats(chains),
      maxDeviationPx,
    };
  }

  async function main() {
    const info = await send({ op: "reset" });
    check(info.ok === true && info.items === ITEMS, `fixture reset: ${JSON.stringify(info)}`);
    check(Array.isArray(info.exposed) && info.exposed.length === 0, `native names exposed to the page: ${info.exposed}`);
    for (let i = 0; i < WARMUP; i++) await send({ op: "step", width: 20 + i });
    const scenarios = [];
    for (const size of BATCH_SIZES) scenarios.push(await batched(size));
    const chain = await dependentChain();
    scenarios.push(chain);
    const full = scenarios.find((scenario) => scenario.updatesPerMessage === ITEMS);
    return {
      schema: "roves-cef-ipc-bench/1",
      ok: mismatches === 0,
      mismatches,
      failures,
      scenarios,
      comparison: {
        updates: ITEMS,
        batchedMessageP50Us: full.roundTrip.p50Us,
        dependentChainP50Us: chain.chain.p50Us,
        chainOverBatch: round1(chain.chain.p50Us / full.roundTrip.p50Us),
      },
    };
  }

  main().then(
    (result) => ops.op_roves_ipc_done(JSON.stringify(result)),
    (error) =>
      ops.op_roves_ipc_done(
        JSON.stringify({ schema: "roves-cef-ipc-bench/1", ok: false, error: String(error?.stack ?? error) }),
      ),
  );
})(Deno.core.ops);
