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
    const gain = ops.op_audio_context_create_gain(context);
    if (ops.op_audio_gain_value(gain) !== 1) {
      throw new Error("Unexpected default gain");
    }
    ops.op_audio_gain_set_value(gain, 0.25);
    if (ops.op_audio_gain_value(gain) !== 0.25) {
      throw new Error("The native gain did not retain its new value");
    }
    try {
      ops.op_audio_gain_set_value(gain, Infinity);
      throw new Error("Expected a TypeError for a nonfinite gain");
    } catch (error) {
      if (!(error instanceof TypeError)) throw error;
    }
  } finally {
    ops.op_audio_context_close_for_probe(context);
  }
  if (ops.op_audio_context_state(context) !== "closed") {
    throw new Error("Headless audio context did not close");
  }
})();
