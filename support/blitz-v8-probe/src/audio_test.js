(() => {
  const ops = Deno.core.ops;
  const context = ops.op_audio_context_create();
  try {
    const rate = ops.op_audio_context_sample_rate(context);
    if (!Number.isFinite(rate) || rate <= 0) {
      throw new Error(`Invalid audio sample rate: ${rate}`);
    }
    if (ops.op_audio_context_state(context) !== "running") {
      throw new Error("Headless audio context did not start");
    }
  } finally {
    ops.op_audio_context_close_for_probe(context);
  }
  if (ops.op_audio_context_state(context) !== "closed") {
    throw new Error("Headless audio context did not close");
  }
})();
