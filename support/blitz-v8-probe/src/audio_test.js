// Low-level bridge check (before the DOM bootstrap): native context and gain
// node handles through the ops src/web_audio.js builds the Web API on.
(() => {
  const ops = Deno.core.ops;
  const NODE_GAIN = 0;
  const context = ops.op_audio_context_create(0);
  try {
    const rate = ops.op_audio_context_sample_rate(context);
    if (!Number.isFinite(rate) || rate <= 0) {
      throw new Error(`Invalid audio sample rate: ${rate}`);
    }
    if (ops.op_audio_context_state(context) !== "running") {
      throw new Error("Headless audio context did not start");
    }
    const gainNode = ops.op_audio_context_create_node(context, NODE_GAIN, 0);
    const gain = ops.op_audio_node_param(gainNode, 0);
    if (ops.op_audio_param_value(gain) !== 1 || ops.op_audio_param_default_value(gain) !== 1) {
      throw new Error("Unexpected default gain");
    }
    ops.op_audio_param_set_value(gain, 0.25);
    if (ops.op_audio_param_value(gain) !== 0.25) {
      throw new Error("The native gain did not retain its new value");
    }
    ops.op_audio_node_connect(gainNode, ops.op_audio_context_destination(context), 0, 0);
    ops.op_audio_node_disconnect(gainNode);
    // A backend panic (here: an output index out of range, which web_audio.js
    // rejects before reaching Rust) becomes a JS error instead of an abort.
    let caught = null;
    try {
      ops.op_audio_node_connect(gainNode, ops.op_audio_context_destination(context), 7, 0);
    } catch (error) {
      caught = error;
    }
    if (!(caught instanceof Error)) {
      throw new Error("An out-of-range output must raise a JS error, not abort");
    }
    // ChannelMergerNode: numberOfInputs from the factory argument, one output.
    const NODE_CHANNEL_MERGER = 3;
    const merger = ops.op_audio_context_create_node(context, NODE_CHANNEL_MERGER, 2);
    if (ops.op_audio_node_inputs(merger) !== 2 || ops.op_audio_node_outputs(merger) !== 1) {
      throw new Error("The native ChannelMergerNode must have 2 inputs and 1 output");
    }
    if (ops.op_audio_node_channel_count(merger) !== 1 || ops.op_audio_node_channel_count_mode(merger) !== 2) {
      throw new Error("The native ChannelMergerNode must be mono per input in explicit mode");
    }
    ops.op_audio_node_connect(gainNode, merger, 0, 1);
    ops.op_audio_node_connect(merger, ops.op_audio_context_destination(context), 0, 0);
    ops.op_audio_node_disconnect(merger);
  } finally {
    ops.op_audio_context_close(context);
  }
  if (ops.op_audio_context_state(context) !== "closed") {
    throw new Error("Headless audio context did not close");
  }
})();
