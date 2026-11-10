// Web Audio for games (loaded by dom.js) on web-audio-api 1.7.0, the maintained
// Rust crate behind src/audio.rs; no Deno extension implements Web Audio.
// The crate renders on its own threads and nothing calls into V8 from them:
// `ended` is queued in Rust and, like `statechange`, dispatched here on the V8
// thread by `dispatchPendingEvents`, which the frame loop calls once per frame.
// Covered: what Tone.js 15.1.22 and standardized-audio-context 25.3.77 need to
// start (AudioContext lifecycle, AudioParam automation, AudioListener, gain,
// constant source and oscillator nodes, connect/disconnect), plus the channel
// merger/splitter and the native stereo panner (without createStereoPanner,
// standardized-audio-context fakes it with a merger, wave shapers and a looping
// buffer source, which Pixi'VN's sound init hit), and what Tone.Player needs to
// play sound files: AudioBuffer, AudioBufferSourceNode (buffer, loop points,
// playbackRate/detune, start offset/duration, `ended`) and decodeAudioData,
// which decodes natively (the crate's Symphonia decoder, resampled to the
// context rate) on a decoder thread and settles from dispatchPendingEvents.
// Not yet: the other node types, OfflineAudioContext (standardized-audio-
// context's own AudioBuffer constructor asks for one), ScriptProcessorNode
// and AudioWorklet (see TODO.md); those globals are absent rather than faked.
globalThis.__rovesCreateWebAudio = (dependencies) => {
  "use strict";
  const { ops, webidl, EventTarget, Event, DOMException, setIsTrusted, setTimeout, reportException } = dependencies;

  const INTERNAL = Symbol("internal");
  const NODE_GAIN = 0;
  const NODE_CONSTANT_SOURCE = 1;
  const NODE_OSCILLATOR = 2;
  const NODE_CHANNEL_MERGER = 3;
  const NODE_CHANNEL_SPLITTER = 4;
  const NODE_STEREO_PANNER = 5;
  const NODE_BUFFER_SOURCE = 6;
  const CHANNEL_COUNT_MODES = ["max", "clamped-max", "explicit"];
  const CHANNEL_INTERPRETATIONS = ["speakers", "discrete"];
  const OSCILLATOR_TYPES = ["sine", "square", "sawtooth", "triangle", "custom"];
  const LATENCY_HINTS = ["balanced", "interactive", "playback"];
  const LISTENER_PARAMS = ["positionX", "positionY", "positionZ", "forwardX", "forwardY", "forwardZ", "upX", "upY", "upZ"];
  const DOM_ERRORS = ["IndexSizeError", "InvalidStateError", "InvalidAccessError", "NotSupportedError"];
  let nextId = 1;

  const illegalConstructor = () => {
    throw new TypeError("Illegal constructor");
  };
  const domError = (name, message) => new DOMException(message, name);
  // The crate reports spec errors as panics named after the exception; src/audio.rs
  // turns them into errors with that prefix. Arguments are validated here first.
  function backend(operation) {
    try {
      return operation();
    } catch (error) {
      const message = String(error?.message ?? error);
      const name = DOM_ERRORS.find((candidate) => message.startsWith(candidate));
      if (name !== undefined) throw domError(name, message);
      if (message.startsWith("RangeError")) throw new RangeError(message);
      if (message.startsWith("TypeError")) throw new TypeError(message);
      throw error;
    }
  }
  const float = (value, prefix) => webidl.converters.float(value, prefix);
  const double = (value, prefix) => webidl.converters.double(value, prefix);
  const unsignedLong = (value, prefix) => webidl.converters["unsigned long"](value, prefix);
  function nonNegative(value, name, prefix) {
    const number = double(value, prefix);
    if (number < 0) throw new RangeError(`${prefix}: ${name} must be non-negative, got ${number}`);
    return number;
  }
  function dictionary(value, what) {
    if (value === undefined || value === null) return {};
    if (typeof value !== "object" && typeof value !== "function") throw new TypeError(`${what} must be an object`);
    return value;
  }
  function brand(records, object) {
    const record = records.get(object);
    if (record === undefined) throw new TypeError("Illegal invocation");
    return record;
  }

  function fire(target, type) {
    const event = new Event(type);
    setIsTrusted(event, true);
    target.dispatchEvent(event);
  }
  // `on<type>` event handler attributes: one listener per object calls the
  // current handler, so reassigning keeps the listener's position.
  const handlers = new WeakMap();
  function defineEventHandler(prototype, type) {
    Object.defineProperty(prototype, `on${type}`, {
      configurable: true,
      enumerable: true,
      get() {
        return handlers.get(this)?.[type] ?? null;
      },
      set(value) {
        let slots = handlers.get(this);
        if (slots === undefined) {
          slots = {};
          handlers.set(this, slots);
        }
        if (!(type in slots)) {
          this.addEventListener(type, (event) => {
            const handler = slots[type];
            if (typeof handler === "function") return handler.call(this, event);
          });
        }
        slots[type] = typeof value === "function" ? value : null;
      },
    });
  }

  // ---- AudioParam ----------------------------------------------------------

  const params = new WeakMap();
  class AudioParam {
    constructor(token, handle) {
      if (token !== INTERNAL) illegalConstructor();
      params.set(this, handle);
    }
    get value() {
      return ops.op_audio_param_value(brand(params, this));
    }
    set value(value) {
      const handle = brand(params, this);
      const number = float(value, "Failed to set 'value' on 'AudioParam'");
      backend(() => ops.op_audio_param_set_value(handle, number));
    }
    get defaultValue() {
      return ops.op_audio_param_default_value(brand(params, this));
    }
    get minValue() {
      return ops.op_audio_param_min_value(brand(params, this));
    }
    get maxValue() {
      return ops.op_audio_param_max_value(brand(params, this));
    }
    setValueAtTime(value, startTime) {
      const handle = brand(params, this);
      const prefix = "Failed to execute 'setValueAtTime' on 'AudioParam'";
      const number = float(value, prefix);
      const time = nonNegative(startTime, "startTime", prefix);
      backend(() => ops.op_audio_param_set_value_at_time(handle, number, time));
      return this;
    }
    linearRampToValueAtTime(value, endTime) {
      const handle = brand(params, this);
      const prefix = "Failed to execute 'linearRampToValueAtTime' on 'AudioParam'";
      const number = float(value, prefix);
      const time = nonNegative(endTime, "endTime", prefix);
      backend(() => ops.op_audio_param_linear_ramp(handle, number, time));
      return this;
    }
    exponentialRampToValueAtTime(value, endTime) {
      const handle = brand(params, this);
      const prefix = "Failed to execute 'exponentialRampToValueAtTime' on 'AudioParam'";
      const number = float(value, prefix);
      if (number === 0) throw new RangeError(`${prefix}: value must not be 0`);
      const time = nonNegative(endTime, "endTime", prefix);
      backend(() => ops.op_audio_param_exponential_ramp(handle, number, time));
      return this;
    }
    setTargetAtTime(target, startTime, timeConstant) {
      const handle = brand(params, this);
      const prefix = "Failed to execute 'setTargetAtTime' on 'AudioParam'";
      const number = float(target, prefix);
      const time = nonNegative(startTime, "startTime", prefix);
      const constant = nonNegative(timeConstant, "timeConstant", prefix);
      backend(() => ops.op_audio_param_set_target(handle, number, time, constant));
      return this;
    }
    setValueCurveAtTime(values, startTime, duration) {
      const handle = brand(params, this);
      const prefix = "Failed to execute 'setValueCurveAtTime' on 'AudioParam'";
      if (values === null || typeof values !== "object" || typeof values[Symbol.iterator] !== "function") {
        throw new TypeError(`${prefix}: values must be a sequence of numbers`);
      }
      const curve = Float32Array.from(values, (value) => float(value, prefix));
      if (curve.length < 2) throw domError("InvalidStateError", `${prefix}: the curve needs at least 2 values`);
      const time = nonNegative(startTime, "startTime", prefix);
      const length = double(duration, prefix);
      if (!(length > 0)) throw new RangeError(`${prefix}: duration must be positive, got ${length}`);
      const bytes = new Uint8Array(curve.buffer);
      backend(() => ops.op_audio_param_set_value_curve(handle, bytes, time, length));
      return this;
    }
    cancelScheduledValues(cancelTime) {
      const handle = brand(params, this);
      const time = nonNegative(cancelTime, "cancelTime", "Failed to execute 'cancelScheduledValues' on 'AudioParam'");
      backend(() => ops.op_audio_param_cancel_scheduled_values(handle, time));
      return this;
    }
    cancelAndHoldAtTime(cancelTime) {
      const handle = brand(params, this);
      const time = nonNegative(cancelTime, "cancelTime", "Failed to execute 'cancelAndHoldAtTime' on 'AudioParam'");
      backend(() => ops.op_audio_param_cancel_and_hold(handle, time));
      return this;
    }
  }

  // ---- AudioNode -----------------------------------------------------------

  const nodes = new WeakMap();
  // A started source stays alive until `ended`, even when the game drops it.
  const playingSources = new Map();

  class AudioNode extends EventTarget {
    constructor(token, context, handle) {
      if (token !== INTERNAL) illegalConstructor();
      super();
      nodes.set(this, { id: nextId++, context, handle, params: [], started: false });
    }
    get context() {
      return brand(nodes, this).context;
    }
    get numberOfInputs() {
      return ops.op_audio_node_inputs(brand(nodes, this).handle);
    }
    get numberOfOutputs() {
      return ops.op_audio_node_outputs(brand(nodes, this).handle);
    }
    get channelCount() {
      return ops.op_audio_node_channel_count(brand(nodes, this).handle);
    }
    set channelCount(value) {
      const { handle } = brand(nodes, this);
      const count = unsignedLong(value, "Failed to set 'channelCount' on 'AudioNode'");
      if (count === 0 || count > 32) throw domError("NotSupportedError", `channelCount ${count} is outside [1, 32]`);
      if (this instanceof AudioDestinationNode && count > this.maxChannelCount) {
        throw domError("IndexSizeError", `channelCount ${count} exceeds maxChannelCount ${this.maxChannelCount}`);
      }
      channelRules.get(this)?.count?.(count);
      backend(() => ops.op_audio_node_set_channel_count(handle, count));
    }
    get channelCountMode() {
      return CHANNEL_COUNT_MODES[ops.op_audio_node_channel_count_mode(brand(nodes, this).handle)];
    }
    set channelCountMode(value) {
      const { handle } = brand(nodes, this);
      const mode = CHANNEL_COUNT_MODES.indexOf(String(value));
      if (mode < 0) return;
      channelRules.get(this)?.mode?.(CHANNEL_COUNT_MODES[mode]);
      backend(() => ops.op_audio_node_set_channel_count_mode(handle, mode));
    }
    get channelInterpretation() {
      return CHANNEL_INTERPRETATIONS[ops.op_audio_node_channel_interpretation(brand(nodes, this).handle)];
    }
    set channelInterpretation(value) {
      const { handle } = brand(nodes, this);
      const interpretation = CHANNEL_INTERPRETATIONS.indexOf(String(value));
      if (interpretation < 0) return;
      channelRules.get(this)?.interpretation?.(CHANNEL_INTERPRETATIONS[interpretation]);
      backend(() => ops.op_audio_node_set_channel_interpretation(handle, interpretation));
    }
    connect(destination, output = 0, input = 0) {
      const source = brand(nodes, this);
      const prefix = "Failed to execute 'connect' on 'AudioNode'";
      const outputIndex = unsignedLong(output, prefix);
      if (outputIndex >= this.numberOfOutputs) throw domError("IndexSizeError", `${prefix}: output ${outputIndex} is out of range`);
      const param = params.get(destination);
      if (param !== undefined) {
        if (paramContexts.get(destination) !== source.context) {
          throw domError("InvalidAccessError", `${prefix}: the AudioParam belongs to another context`);
        }
        backend(() => ops.op_audio_node_connect_param(source.handle, param, outputIndex));
        return undefined;
      }
      const target = nodes.get(destination);
      if (target === undefined) throw new TypeError(`${prefix}: destination is not an AudioNode or AudioParam`);
      if (target.context !== source.context) {
        throw domError("InvalidAccessError", `${prefix}: the destination belongs to another context`);
      }
      const inputIndex = unsignedLong(input, prefix);
      if (inputIndex >= destination.numberOfInputs) throw domError("IndexSizeError", `${prefix}: input ${inputIndex} is out of range`);
      backend(() => ops.op_audio_node_connect(source.handle, target.handle, outputIndex, inputIndex));
      return destination;
    }
    disconnect(destinationOrOutput, output, input) {
      const source = brand(nodes, this);
      const prefix = "Failed to execute 'disconnect' on 'AudioNode'";
      if (destinationOrOutput === undefined) {
        backend(() => ops.op_audio_node_disconnect(source.handle));
        return;
      }
      const index = (value, what, limit) => {
        if (value === undefined) return -1;
        const number = unsignedLong(value, prefix);
        if (number >= limit) throw domError("IndexSizeError", `${prefix}: ${what} ${number} is out of range`);
        return number;
      };
      if (typeof destinationOrOutput === "number") {
        const outputIndex = index(destinationOrOutput, "output", this.numberOfOutputs);
        backend(() => ops.op_audio_node_disconnect_output(source.handle, outputIndex));
        return;
      }
      const outputIndex = index(output, "output", this.numberOfOutputs);
      const param = params.get(destinationOrOutput);
      if (param !== undefined) {
        backend(() => ops.op_audio_node_disconnect_param(source.handle, param, outputIndex));
        return;
      }
      const target = nodes.get(destinationOrOutput);
      if (target === undefined) throw new TypeError(`${prefix}: destination is not an AudioNode or AudioParam`);
      const inputIndex = index(input, "input", destinationOrOutput.numberOfInputs);
      backend(() => ops.op_audio_node_disconnect_node(source.handle, target.handle, outputIndex, inputIndex));
    }
  }

  // AudioParams remember their context for connect()'s same-context check.
  const paramContexts = new WeakMap();
  function nodeParam(node, context, index) {
    const record = nodes.get(node);
    const param = new AudioParam(INTERNAL, ops.op_audio_node_param(record.handle, index));
    paramContexts.set(param, context);
    record.params[index] = param;
    return param;
  }
  function ownParam(node, Class, index) {
    if (!(node instanceof Class)) throw new TypeError("Illegal invocation");
    return brand(nodes, node).params[index];
  }
  function applyNodeOptions(node, options) {
    if (options.channelCount !== undefined) node.channelCount = options.channelCount;
    for (const [name, values] of [["channelCountMode", CHANNEL_COUNT_MODES], ["channelInterpretation", CHANNEL_INTERPRETATIONS]]) {
      if (options[name] === undefined) continue;
      if (!values.includes(String(options[name]))) throw new TypeError(`'${options[name]}' is not a valid ${name}`);
      node[name] = options[name];
    }
  }
  function createNode(context, kind, count = 0) {
    const { handle } = brand(contexts, context);
    return backend(() => ops.op_audio_context_create_node(handle, kind, count));
  }
  // Node-specific channel constraints (Web Audio §1.14, §1.15, §1.31), checked
  // before the backend, which only panics on them.
  const channelRules = new WeakMap();
  const fixed = (what, expected, name) => (value) => {
    if (value !== expected) throw domError(name, `${what} must be ${expected}, got ${value}`);
  };
  function nodeCount(value, what, prefix) {
    const count = unsignedLong(value, prefix);
    if (count < 1 || count > 32) throw domError("IndexSizeError", `${prefix}: ${what} ${count} is outside [1, 32]`);
    return count;
  }

  class AudioDestinationNode extends AudioNode {
    get maxChannelCount() {
      return ops.op_audio_node_max_channel_count(brand(nodes, this).handle);
    }
  }

  class GainNode extends AudioNode {
    constructor(context, options) {
      options = dictionary(options, "GainOptions");
      super(INTERNAL, context, createNode(context, NODE_GAIN));
      nodeParam(this, context, 0);
      applyNodeOptions(this, options);
      if (options.gain !== undefined) this.gain.value = options.gain;
    }
    get gain() {
      return ownParam(this, GainNode, 0);
    }
  }

  // `start` runs the node-specific native start once the source is watched.
  function startSource(node, record, start) {
    if (record.started) throw domError("InvalidStateError", "start() can only be called once");
    backend(() => ops.op_audio_source_watch_ended(record.handle, record.id));
    backend(start);
    record.started = true;
    playingSources.set(record.id, node);
  }

  class AudioScheduledSourceNode extends AudioNode {
    start(when = 0) {
      const record = brand(nodes, this);
      const time = nonNegative(when, "when", "Failed to execute 'start' on 'AudioScheduledSourceNode'");
      startSource(this, record, () => ops.op_audio_source_start(record.handle, time));
    }
    stop(when = 0) {
      const record = brand(nodes, this);
      const time = nonNegative(when, "when", "Failed to execute 'stop' on 'AudioScheduledSourceNode'");
      if (!record.started) throw domError("InvalidStateError", "stop() called before start()");
      backend(() => ops.op_audio_source_stop(record.handle, time));
    }
  }
  defineEventHandler(AudioScheduledSourceNode.prototype, "ended");

  class ConstantSourceNode extends AudioScheduledSourceNode {
    constructor(context, options) {
      options = dictionary(options, "ConstantSourceOptions");
      super(INTERNAL, context, createNode(context, NODE_CONSTANT_SOURCE));
      nodeParam(this, context, 0);
      applyNodeOptions(this, options);
      if (options.offset !== undefined) this.offset.value = options.offset;
    }
    get offset() {
      return ownParam(this, ConstantSourceNode, 0);
    }
  }

  // Custom waveforms (PeriodicWave, setPeriodicWave) are not exposed yet.
  class OscillatorNode extends AudioScheduledSourceNode {
    constructor(context, options) {
      options = dictionary(options, "OscillatorOptions");
      if (options.periodicWave !== undefined) throw domError("NotSupportedError", "PeriodicWave is not supported yet");
      super(INTERNAL, context, createNode(context, NODE_OSCILLATOR));
      nodeParam(this, context, 0);
      nodeParam(this, context, 1);
      applyNodeOptions(this, options);
      if (options.type !== undefined) {
        if (!OSCILLATOR_TYPES.includes(String(options.type))) throw new TypeError(`'${options.type}' is not a valid OscillatorType`);
        this.type = options.type;
      }
      if (options.frequency !== undefined) this.frequency.value = options.frequency;
      if (options.detune !== undefined) this.detune.value = options.detune;
    }
    get frequency() {
      return ownParam(this, OscillatorNode, 0);
    }
    get detune() {
      return ownParam(this, OscillatorNode, 1);
    }
    get type() {
      if (!(this instanceof OscillatorNode)) throw new TypeError("Illegal invocation");
      return OSCILLATOR_TYPES[ops.op_audio_oscillator_type(brand(nodes, this).handle)];
    }
    set type(value) {
      if (!(this instanceof OscillatorNode)) throw new TypeError("Illegal invocation");
      const { handle } = brand(nodes, this);
      const kind = OSCILLATOR_TYPES.indexOf(String(value));
      if (kind < 0) return;
      if (OSCILLATOR_TYPES[kind] === "custom") throw domError("InvalidStateError", "use setPeriodicWave() for a custom waveform");
      backend(() => ops.op_audio_oscillator_set_type(handle, kind));
    }
  }

  // Each input is down-mixed to mono and becomes one channel of the single output.
  class ChannelMergerNode extends AudioNode {
    constructor(context, options) {
      options = dictionary(options, "ChannelMergerOptions");
      const prefix = "Failed to construct 'ChannelMergerNode'";
      const inputs = nodeCount(options.numberOfInputs === undefined ? 6 : options.numberOfInputs, "numberOfInputs", prefix);
      super(INTERNAL, context, createNode(context, NODE_CHANNEL_MERGER, inputs));
      channelRules.set(this, {
        count: fixed("ChannelMergerNode channelCount", 1, "InvalidStateError"),
        mode: fixed("ChannelMergerNode channelCountMode", "explicit", "InvalidStateError"),
      });
      applyNodeOptions(this, options);
    }
  }

  // Channel N of the input becomes output N.
  class ChannelSplitterNode extends AudioNode {
    constructor(context, options) {
      options = dictionary(options, "ChannelSplitterOptions");
      const prefix = "Failed to construct 'ChannelSplitterNode'";
      const outputs = nodeCount(options.numberOfOutputs === undefined ? 6 : options.numberOfOutputs, "numberOfOutputs", prefix);
      super(INTERNAL, context, createNode(context, NODE_CHANNEL_SPLITTER, outputs));
      channelRules.set(this, {
        count: fixed("ChannelSplitterNode channelCount", outputs, "InvalidStateError"),
        mode: fixed("ChannelSplitterNode channelCountMode", "explicit", "InvalidStateError"),
        interpretation: fixed("ChannelSplitterNode channelInterpretation", "discrete", "InvalidStateError"),
      });
      applyNodeOptions(this, options);
    }
  }

  // Equal-power panning of a mono or stereo input to a stereo output.
  class StereoPannerNode extends AudioNode {
    constructor(context, options) {
      options = dictionary(options, "StereoPannerOptions");
      super(INTERNAL, context, createNode(context, NODE_STEREO_PANNER));
      nodeParam(this, context, 0);
      channelRules.set(this, {
        count(count) {
          if (count > 2) throw domError("NotSupportedError", `StereoPannerNode channelCount ${count} exceeds 2`);
        },
        mode(mode) {
          if (mode === "max") throw domError("NotSupportedError", "StereoPannerNode channelCountMode cannot be 'max'");
        },
      });
      applyNodeOptions(this, options);
      if (options.pan !== undefined) this.pan.value = options.pan;
    }
    get pan() {
      return ownParam(this, StereoPannerNode, 0);
    }
  }

  // ---- AudioBuffer ---------------------------------------------------------

  // A decoded buffer keeps its samples in Rust (`handle`): copyFromChannel reads
  // them there and a source shares them without copying. The first
  // getChannelData()/copyToChannel() moves the content into JS Float32Arrays,
  // which the game may then write at any time, so from there on every
  // acquisition (Web Audio §1.4.3) copies them into a new native buffer.
  // Buffers built by the constructor start with JS arrays.
  const buffers = new WeakMap();
  const typedArrayTag = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype), Symbol.toStringTag).get;
  const arrayBufferByteLength = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "byteLength").get;
  const arrayBufferDetached = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, "detached").get;
  const arrayBufferTransfer = ArrayBuffer.prototype.transfer;
  function isArrayBuffer(value) {
    try {
      arrayBufferByteLength.call(value); // throws for anything else, SharedArrayBuffer included
      return true;
    } catch {
      return false;
    }
  }
  function float32Array(value, prefix) {
    if (typedArrayTag.call(value) !== "Float32Array") throw new TypeError(`${prefix}: parameter 1 is not of type 'Float32Array'.`);
    return value;
  }
  const bytesOf = (array, count = array.length) => new Uint8Array(array.buffer, array.byteOffset, count * 4);

  function checkBufferShape(numberOfChannels, length, sampleRate, prefix) {
    if (numberOfChannels < 1 || numberOfChannels > 32) {
      throw domError("NotSupportedError", `${prefix}: numberOfChannels ${numberOfChannels} is outside [1, 32]`);
    }
    if (length < 1) throw domError("NotSupportedError", `${prefix}: length must be at least 1`);
    if (!(sampleRate >= 3000 && sampleRate <= 768000)) {
      throw domError("NotSupportedError", `${prefix}: sampleRate ${sampleRate} is outside [3000, 768000]`);
    }
  }
  function materialize(record) {
    if (record.channels === null) {
      const { handle } = record;
      record.channels = Array.from({ length: record.numberOfChannels }, (_, index) => {
        const channel = new Float32Array(record.length);
        backend(() => ops.op_audio_buffer_copy_from(handle, index, 0, bytesOf(channel)));
        return channel;
      });
      record.handle = null;
    }
    return record.channels;
  }
  // The native buffer a source plays.
  function acquireContent(record) {
    if (record.channels === null) return record.handle;
    const handle = backend(() => ops.op_audio_buffer_create(record.numberOfChannels, record.length, record.sampleRate));
    record.channels.forEach((channel, index) => backend(() => ops.op_audio_buffer_copy_to(handle, index, 0, bytesOf(channel))));
    return handle;
  }
  function wrapDecodedBuffer(handle) {
    const buffer = Object.create(AudioBuffer.prototype);
    buffers.set(buffer, {
      handle,
      channels: null,
      numberOfChannels: ops.op_audio_buffer_channels(handle),
      length: ops.op_audio_buffer_length(handle),
      sampleRate: ops.op_audio_buffer_sample_rate(handle),
    });
    return buffer;
  }
  // Converted arguments of copyFromChannel/copyToChannel, with the number of
  // samples both sides reach (0 when bufferOffset is past the end).
  function channelCopy(buffer, array, channelNumber, bufferOffset, prefix) {
    const record = brand(buffers, buffer);
    float32Array(array, prefix);
    const channel = unsignedLong(channelNumber, prefix);
    const offset = unsignedLong(bufferOffset, prefix);
    if (channel >= record.numberOfChannels) throw domError("IndexSizeError", `${prefix}: channel ${channel} is out of range`);
    return { record, channel, offset, count: Math.min(array.length, Math.max(record.length - offset, 0)) };
  }

  class AudioBuffer {
    constructor(options) {
      const prefix = "Failed to construct 'AudioBuffer'";
      if (options === null || typeof options !== "object") throw new TypeError(`${prefix}: parameter 1 is not of type 'AudioBufferOptions'.`);
      for (const member of ["length", "sampleRate"]) {
        if (options[member] === undefined) throw new TypeError(`${prefix}: required member ${member} is undefined.`);
      }
      const length = unsignedLong(options.length, prefix);
      const numberOfChannels = options.numberOfChannels === undefined ? 1 : unsignedLong(options.numberOfChannels, prefix);
      const sampleRate = float(options.sampleRate, prefix);
      checkBufferShape(numberOfChannels, length, sampleRate, prefix);
      const channels = Array.from({ length: numberOfChannels }, () => new Float32Array(length));
      buffers.set(this, { handle: null, channels, numberOfChannels, length, sampleRate });
    }
    get sampleRate() {
      return brand(buffers, this).sampleRate;
    }
    get length() {
      return brand(buffers, this).length;
    }
    get duration() {
      const { length, sampleRate } = brand(buffers, this);
      return length / sampleRate;
    }
    get numberOfChannels() {
      return brand(buffers, this).numberOfChannels;
    }
    getChannelData(channel) {
      const record = brand(buffers, this);
      const prefix = "Failed to execute 'getChannelData' on 'AudioBuffer'";
      const index = unsignedLong(channel, prefix);
      if (index >= record.numberOfChannels) throw domError("IndexSizeError", `${prefix}: channel ${index} is out of range`);
      return materialize(record)[index];
    }
    copyFromChannel(destination, channelNumber, bufferOffset = 0) {
      const prefix = "Failed to execute 'copyFromChannel' on 'AudioBuffer'";
      const { record, channel, offset, count } = channelCopy(this, destination, channelNumber, bufferOffset, prefix);
      if (count === 0) return;
      if (record.channels !== null) {
        destination.set(record.channels[channel].subarray(offset, offset + count));
      } else {
        backend(() => ops.op_audio_buffer_copy_from(record.handle, channel, offset, bytesOf(destination, count)));
      }
    }
    copyToChannel(source, channelNumber, bufferOffset = 0) {
      const prefix = "Failed to execute 'copyToChannel' on 'AudioBuffer'";
      const { record, channel, offset, count } = channelCopy(this, source, channelNumber, bufferOffset, prefix);
      if (count === 0) return;
      materialize(record)[channel].set(source.subarray(0, count), offset);
    }
  }

  // ---- AudioBufferSourceNode -----------------------------------------------

  // The content is acquired once, at start() or, after start(), when the buffer
  // is set; the crate cannot drop it again, so setting the buffer back to null
  // after that does not silence a playing source (see TODO.md).
  const bufferSources = new WeakMap();
  function acquireBuffer(node, state) {
    if (state.acquired || state.buffer === null) return;
    state.acquired = true;
    const handle = acquireContent(buffers.get(state.buffer));
    backend(() => ops.op_audio_buffer_source_set_buffer(nodes.get(node).handle, handle));
  }
  function syncLoop(node, state) {
    backend(() => ops.op_audio_buffer_source_set_loop(nodes.get(node).handle, state.loop, state.loopStart, state.loopEnd));
  }

  class AudioBufferSourceNode extends AudioScheduledSourceNode {
    constructor(context, options) {
      options = dictionary(options, "AudioBufferSourceOptions");
      super(INTERNAL, context, createNode(context, NODE_BUFFER_SOURCE));
      nodeParam(this, context, 0);
      nodeParam(this, context, 1);
      bufferSources.set(this, { buffer: null, bufferSet: false, acquired: false, loop: false, loopStart: 0, loopEnd: 0 });
      applyNodeOptions(this, options);
      if (options.buffer !== undefined) this.buffer = options.buffer;
      if (options.playbackRate !== undefined) this.playbackRate.value = options.playbackRate;
      if (options.detune !== undefined) this.detune.value = options.detune;
      if (options.loop !== undefined) this.loop = options.loop;
      if (options.loopStart !== undefined) this.loopStart = options.loopStart;
      if (options.loopEnd !== undefined) this.loopEnd = options.loopEnd;
    }
    get playbackRate() {
      return ownParam(this, AudioBufferSourceNode, 0);
    }
    get detune() {
      return ownParam(this, AudioBufferSourceNode, 1);
    }
    get buffer() {
      return brand(bufferSources, this).buffer;
    }
    set buffer(value) {
      const state = brand(bufferSources, this);
      const prefix = "Failed to set the 'buffer' property on 'AudioBufferSourceNode'";
      if (value === null || value === undefined) {
        state.buffer = null;
        return;
      }
      if (!buffers.has(value)) throw new TypeError(`${prefix}: The provided value is not of type 'AudioBuffer'.`);
      if (state.bufferSet) throw domError("InvalidStateError", `${prefix}: the buffer can only be set once`);
      state.bufferSet = true;
      state.buffer = value;
      if (brand(nodes, this).started) acquireBuffer(this, state);
    }
    get loop() {
      return brand(bufferSources, this).loop;
    }
    set loop(value) {
      const state = brand(bufferSources, this);
      state.loop = Boolean(value);
      syncLoop(this, state);
    }
    get loopStart() {
      return brand(bufferSources, this).loopStart;
    }
    set loopStart(value) {
      const state = brand(bufferSources, this);
      state.loopStart = double(value, "Failed to set the 'loopStart' property on 'AudioBufferSourceNode'");
      syncLoop(this, state);
    }
    get loopEnd() {
      return brand(bufferSources, this).loopEnd;
    }
    set loopEnd(value) {
      const state = brand(bufferSources, this);
      state.loopEnd = double(value, "Failed to set the 'loopEnd' property on 'AudioBufferSourceNode'");
      syncLoop(this, state);
    }
    start(when = 0, offset = 0, duration = undefined) {
      const record = brand(nodes, this);
      const state = brand(bufferSources, this);
      const prefix = "Failed to execute 'start' on 'AudioBufferSourceNode'";
      const time = nonNegative(when, "when", prefix);
      const from = nonNegative(offset, "offset", prefix);
      const length = duration === undefined ? -1 : nonNegative(duration, "duration", prefix);
      startSource(this, record, () => {
        acquireBuffer(this, state);
        ops.op_audio_buffer_source_start(record.handle, time, from, length);
      });
    }
  }

  // ---- AudioListener -------------------------------------------------------

  const listeners = new WeakMap();
  class AudioListener {
    constructor(token, context, contextHandle) {
      if (token !== INTERNAL) illegalConstructor();
      listeners.set(
        this,
        LISTENER_PARAMS.map((_, index) => {
          const param = new AudioParam(INTERNAL, ops.op_audio_context_listener_param(contextHandle, index));
          paramContexts.set(param, context);
          return param;
        }),
      );
    }
    setPosition(x, y, z) {
      const values = brand(listeners, this);
      [x, y, z].forEach((value, index) => (values[index].value = value));
    }
    setOrientation(x, y, z, xUp, yUp, zUp) {
      const values = brand(listeners, this);
      [x, y, z, xUp, yUp, zUp].forEach((value, index) => (values[index + 3].value = value));
    }
  }
  LISTENER_PARAMS.forEach((name, index) => {
    Object.defineProperty(AudioListener.prototype, name, {
      configurable: true,
      enumerable: true,
      get() {
        return brand(listeners, this)[index];
      },
    });
  });

  // ---- AudioContext --------------------------------------------------------

  const contexts = new WeakMap();
  // A context that is not closed keeps rendering, so it is never collected.
  const openContexts = new Set();
  const nextTask = () => new Promise((resolve) => setTimeout(resolve, 0));

  // The context's control-thread state follows the rendering thread through a
  // queued task, firing `statechange` on every change (Web Audio §1.2). Open
  // contexts are resynchronised on every dispatch, i.e. once per frame.
  function syncState(context) {
    const record = contexts.get(context);
    if (record.state === "closed") return;
    const state = ops.op_audio_context_state(record.handle);
    if (state === record.state) return;
    record.state = state;
    if (state === "closed") {
      openContexts.delete(context);
      for (const [id, source] of playingSources) {
        if (nodes.get(source).context === context) playingSources.delete(id);
      }
    }
    fire(context, "statechange");
  }

  class BaseAudioContext extends EventTarget {
    constructor(token) {
      if (token !== INTERNAL) illegalConstructor();
      super();
    }
    get sampleRate() {
      return ops.op_audio_context_sample_rate(brand(contexts, this).handle);
    }
    get currentTime() {
      return ops.op_audio_context_current_time(brand(contexts, this).handle);
    }
    get state() {
      return brand(contexts, this).state;
    }
    get destination() {
      return brand(contexts, this).destination;
    }
    get listener() {
      return brand(contexts, this).listener;
    }
    createGain() {
      return new GainNode(this);
    }
    createConstantSource() {
      return new ConstantSourceNode(this);
    }
    createOscillator() {
      return new OscillatorNode(this);
    }
    createChannelMerger(numberOfInputs = 6) {
      return new ChannelMergerNode(this, { numberOfInputs });
    }
    createChannelSplitter(numberOfOutputs = 6) {
      return new ChannelSplitterNode(this, { numberOfOutputs });
    }
    createStereoPanner() {
      return new StereoPannerNode(this);
    }
    createBufferSource() {
      return new AudioBufferSourceNode(this);
    }
    createBuffer(numberOfChannels, length, sampleRate) {
      brand(contexts, this);
      return new AudioBuffer({ numberOfChannels, length, sampleRate });
    }
    // Web Audio §1.1.3 decodeAudioData: the ArrayBuffer is detached (its bytes
    // move to the decoder thread) and the promise and callbacks settle from
    // dispatchPendingEvents, i.e. in a later frame.
    decodeAudioData(audioData, successCallback, errorCallback) {
      const { handle } = brand(contexts, this);
      const prefix = "Failed to execute 'decodeAudioData' on 'BaseAudioContext'";
      if (!isArrayBuffer(audioData)) return Promise.reject(new TypeError(`${prefix}: parameter 1 is not of type 'ArrayBuffer'.`));
      for (const callback of [successCallback, errorCallback]) {
        if (callback !== undefined && callback !== null && typeof callback !== "function") {
          return Promise.reject(new TypeError(`${prefix}: the callback provided is not a function.`));
        }
      }
      const onSuccess = typeof successCallback === "function" ? successCallback : null;
      const onError = typeof errorCallback === "function" ? errorCallback : null;
      if (arrayBufferDetached.call(audioData)) {
        const error = domError("DataCloneError", `${prefix}: the ArrayBuffer is detached`);
        if (onError !== null) setTimeout(() => invokeCallback(onError, error), 0);
        return Promise.reject(error);
      }
      return new Promise((resolve, reject) => {
        const id = nextId++;
        const bytes = new Uint8Array(arrayBufferTransfer.call(audioData));
        backend(() => ops.op_audio_context_decode(handle, bytes, id));
        pendingDecodes.set(id, { resolve, reject, onSuccess, onError });
      });
    }
  }
  defineEventHandler(BaseAudioContext.prototype, "statechange");

  // decodeAudioData requests waiting for the decoder thread, by id.
  const pendingDecodes = new Map();
  function invokeCallback(callback, argument) {
    try {
      callback.call(undefined, argument);
    } catch (error) {
      reportException(error);
    }
  }
  function settleDecode(id) {
    const pending = pendingDecodes.get(id);
    pendingDecodes.delete(id);
    let buffer;
    try {
      buffer = wrapDecodedBuffer(ops.op_audio_take_decoded());
    } catch (cause) {
      if (pending === undefined) return;
      const error = domError("EncodingError", `Unable to decode audio data: ${String(cause?.message ?? cause)}`);
      pending.reject(error);
      if (pending.onError !== null) invokeCallback(pending.onError, error);
      return;
    }
    if (pending === undefined) return;
    pending.resolve(buffer);
    if (pending.onSuccess !== null) invokeCallback(pending.onSuccess, buffer);
  }

  class AudioContext extends BaseAudioContext {
    constructor(options) {
      super(INTERNAL);
      options = dictionary(options, "AudioContextOptions");
      const prefix = "Failed to construct 'AudioContext'";
      // The headless sink has no device latency to choose; the hint is validated only.
      const { latencyHint = "interactive", sampleRate } = options;
      if (typeof latencyHint === "string") {
        if (!LATENCY_HINTS.includes(latencyHint)) {
          throw new TypeError(`The provided value '${latencyHint}' is not a valid enum value of type AudioContextLatencyCategory.`);
        }
      } else {
        double(latencyHint, prefix);
      }
      let rate = 0;
      if (sampleRate !== undefined) {
        rate = float(sampleRate, prefix);
        if (!(rate >= 3000 && rate <= 768000)) throw domError("NotSupportedError", `${prefix}: sampleRate ${rate} is not supported`);
      }
      const handle = backend(() => ops.op_audio_context_create(rate));
      const record = { handle, state: "suspended", destination: null, listener: null };
      contexts.set(this, record);
      record.destination = new AudioDestinationNode(INTERNAL, this, ops.op_audio_context_destination(handle));
      record.listener = new AudioListener(INTERNAL, this, handle);
      // Starts "suspended"; the next dispatch moves it to "running".
      openContexts.add(this);
    }
    get baseLatency() {
      return ops.op_audio_context_base_latency(brand(contexts, this).handle);
    }
    get outputLatency() {
      return ops.op_audio_context_output_latency(brand(contexts, this).handle);
    }
    async resume() {
      await transition(this, "resume", ops.op_audio_context_resume);
    }
    async suspend() {
      await transition(this, "suspend", ops.op_audio_context_suspend);
    }
    async close() {
      await transition(this, "close", ops.op_audio_context_close);
    }
  }
  async function transition(context, name, operation) {
    const record = brand(contexts, context);
    if (record.state === "closed") throw domError("InvalidStateError", `Cannot ${name} a closed AudioContext`);
    backend(() => operation(record.handle));
    await nextTask();
    syncState(context);
  }

  // Called by the frame loop: delivers the notifications the audio threads queued.
  function dispatchPendingEvents() {
    for (const context of openContexts) syncState(context);
    for (let id = ops.op_audio_next_ended(); id >= 0; id = ops.op_audio_next_ended()) {
      const source = playingSources.get(id);
      if (source === undefined) continue;
      playingSources.delete(id);
      fire(source, "ended");
    }
    for (let id = ops.op_audio_next_decoded(); id >= 0; id = ops.op_audio_next_decoded()) settleDecode(id);
  }

  return {
    globals: {
      AudioContext,
      BaseAudioContext,
      AudioNode,
      AudioParam,
      AudioDestinationNode,
      AudioListener,
      AudioScheduledSourceNode,
      GainNode,
      ConstantSourceNode,
      OscillatorNode,
      ChannelMergerNode,
      ChannelSplitterNode,
      StereoPannerNode,
      AudioBuffer,
      AudioBufferSourceNode,
    },
    dispatchPendingEvents,
  };
};
