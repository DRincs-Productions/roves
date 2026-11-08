//! Web Audio bridge: web-audio-api 1.7.0 objects behind V8 cppgc wrappers.
//!
//! `src/web_audio.js` builds the Web API on these ops. The crate renders on its
//! own threads and runs `ended` callbacks on its event thread; those callbacks
//! only push the node id into `EndedSources`. The V8 thread drains the queue once
//! per frame (`op_audio_next_ended`) and polls context state there too, so no V8
//! call ever happens off the V8 thread. The crate reports spec errors
//! (IndexSizeError, RangeError, …) by panicking; `guarded` turns them into JS
//! errors carrying that name, and the JS layer validates arguments first so this
//! stays a safety net.

use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use deno_core::{
    OpState, op2,
    v8::cppgc::{GarbageCollected, Visitor},
};
use deno_error::JsErrorBox;
use web_audio_api::context::{
    AudioContext, AudioContextOptions, AudioContextState, BaseAudioContext, OfflineAudioContext,
};
use web_audio_api::node::{
    AudioDestinationNode, AudioNode, AudioScheduledSourceNode, ChannelCountMode,
    ChannelInterpretation, ConstantSourceNode, GainNode, OscillatorNode, OscillatorType,
};
use web_audio_api::{AudioListener, AudioParam};

const NODE_GAIN: u32 = 0;
const NODE_CONSTANT_SOURCE: u32 = 1;
const NODE_OSCILLATOR: u32 = 2;

/// Ids of finished sources, pushed by the crate's event thread and drained on
/// the V8 thread.
#[derive(Clone, Default)]
struct EndedSources(Arc<Mutex<VecDeque<u32>>>);

impl EndedSources {
    fn push(&self, id: u32) {
        let mut queue = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.push_back(id);
    }

    fn pop(&self) -> Option<u32> {
        let mut queue = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.pop_front()
    }
}

/// Runs a crate call, turning its spec-error panics into JS errors whose message
/// starts with the exception name (mapped to DOMException/RangeError in JS).
fn guarded<R>(operation: impl FnOnce() -> R) -> Result<R, JsErrorBox> {
    catch_unwind(AssertUnwindSafe(operation)).map_err(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("Web Audio backend error");
        JsErrorBox::generic(message.to_owned())
    })
}

pub struct NativeAudioContext {
    context: AudioContext,
    listener: OnceCell<Rc<AudioListener>>,
}

enum Node {
    Destination(AudioDestinationNode),
    Gain(GainNode),
    ConstantSource(ConstantSourceNode),
    Oscillator(OscillatorNode),
}

impl Node {
    fn audio_node(&self) -> &dyn AudioNode {
        match self {
            Self::Destination(node) => node,
            Self::Gain(node) => node,
            Self::ConstantSource(node) => node,
            Self::Oscillator(node) => node,
        }
    }

    /// Index of the node's AudioParams: gain, offset, frequency (0) and detune (1).
    fn has_param(&self, index: u32) -> bool {
        matches!(
            (self, index),
            (Self::Gain(_), 0) | (Self::ConstantSource(_), 0) | (Self::Oscillator(_), 0 | 1)
        )
    }
}

/// Shared with the node's AudioParam wrappers, which borrow the param from it.
pub struct NativeAudioNode(Rc<RefCell<Node>>);

enum ParamOwner {
    Node(Rc<RefCell<Node>>, u32),
    /// positionX/Y/Z, forwardX/Y/Z, upX/Y/Z.
    Listener(Rc<AudioListener>, u32),
}

pub struct NativeAudioParam(ParamOwner);

impl NativeAudioParam {
    fn with<R>(&self, operation: impl FnOnce(&AudioParam) -> R) -> R {
        match &self.0 {
            ParamOwner::Node(node, index) => {
                let node = node.borrow();
                operation(match (&*node, *index) {
                    (Node::Gain(node), 0) => node.gain(),
                    (Node::ConstantSource(node), 0) => node.offset(),
                    (Node::Oscillator(node), 0) => node.frequency(),
                    (Node::Oscillator(node), 1) => node.detune(),
                    _ => unreachable!("checked by op_audio_node_param"),
                })
            },
            ParamOwner::Listener(listener, index) => operation(match *index {
                0 => listener.position_x(),
                1 => listener.position_y(),
                2 => listener.position_z(),
                3 => listener.forward_x(),
                4 => listener.forward_y(),
                5 => listener.forward_z(),
                6 => listener.up_x(),
                7 => listener.up_y(),
                8 => listener.up_z(),
                _ => unreachable!("checked by op_audio_context_listener_param"),
            }),
        }
    }
}

// SAFETY (all three): the Rust objects own no V8 references.
unsafe impl GarbageCollected for NativeAudioContext {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeAudioContext"
    }
}

unsafe impl GarbageCollected for NativeAudioNode {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeAudioNode"
    }
}

unsafe impl GarbageCollected for NativeAudioParam {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeAudioParam"
    }
}

fn wrap_node(node: Node) -> NativeAudioNode {
    NativeAudioNode(Rc::new(RefCell::new(node)))
}

fn not_a_source() -> JsErrorBox {
    JsErrorBox::type_error("Not an AudioScheduledSourceNode")
}

// ---- AudioContext ---------------------------------------------------------

/// Headless context: the "none" sink renders in real time without a device
/// (the crate's cpal backend is not enabled). `sample_rate` 0 keeps the default.
#[op2]
#[cppgc]
fn op_audio_context_create(sample_rate: f64) -> Result<NativeAudioContext, JsErrorBox> {
    let options = AudioContextOptions {
        sample_rate: (sample_rate > 0.0).then_some(sample_rate as f32),
        sink_id: "none".into(),
        ..Default::default()
    };
    let context = guarded(|| AudioContext::try_new(options))?
        .map_err(|error| JsErrorBox::generic(error.to_string()))?;
    Ok(NativeAudioContext {
        context,
        listener: OnceCell::new(),
    })
}

#[op2(fast)]
fn op_audio_context_sample_rate(#[cppgc] context: &NativeAudioContext) -> f64 {
    f64::from(context.context.sample_rate())
}

#[op2(fast)]
fn op_audio_context_current_time(#[cppgc] context: &NativeAudioContext) -> f64 {
    context.context.current_time()
}

#[op2(fast)]
fn op_audio_context_base_latency(#[cppgc] context: &NativeAudioContext) -> f64 {
    context.context.base_latency()
}

#[op2(fast)]
fn op_audio_context_output_latency(#[cppgc] context: &NativeAudioContext) -> f64 {
    context.context.output_latency()
}

#[op2]
#[string]
fn op_audio_context_state(#[cppgc] context: &NativeAudioContext) -> &'static str {
    match context.context.state() {
        AudioContextState::Suspended => "suspended",
        AudioContextState::Running => "running",
        AudioContextState::Closed => "closed",
    }
}

// resume/suspend/close wait for the render thread's acknowledgement (short,
// blocking the V8 thread); the JS promise settles on the next task.
#[op2(nofast)]
fn op_audio_context_resume(#[cppgc] context: &NativeAudioContext) -> Result<(), JsErrorBox> {
    guarded(|| context.context.resume_sync())
}

#[op2(nofast)]
fn op_audio_context_suspend(#[cppgc] context: &NativeAudioContext) -> Result<(), JsErrorBox> {
    guarded(|| context.context.suspend_sync())
}

#[op2(nofast)]
fn op_audio_context_close(#[cppgc] context: &NativeAudioContext) -> Result<(), JsErrorBox> {
    guarded(|| context.context.close_sync())
}

#[op2]
#[cppgc]
fn op_audio_context_destination(#[cppgc] context: &NativeAudioContext) -> NativeAudioNode {
    wrap_node(Node::Destination(context.context.destination()))
}

#[op2]
#[cppgc]
fn op_audio_context_create_node(
    #[cppgc] context: &NativeAudioContext,
    kind: u32,
) -> Result<NativeAudioNode, JsErrorBox> {
    let context = &context.context;
    let node = guarded(|| match kind {
        NODE_GAIN => Some(Node::Gain(context.create_gain())),
        NODE_CONSTANT_SOURCE => Some(Node::ConstantSource(context.create_constant_source())),
        NODE_OSCILLATOR => Some(Node::Oscillator(context.create_oscillator())),
        _ => None,
    })?;
    node.map(wrap_node)
        .ok_or_else(|| JsErrorBox::type_error("Unknown audio node kind"))
}

#[op2]
#[cppgc]
fn op_audio_context_listener_param(
    #[cppgc] context: &NativeAudioContext,
    index: u32,
) -> Result<NativeAudioParam, JsErrorBox> {
    if index > 8 {
        return Err(JsErrorBox::type_error("Unknown AudioListener parameter"));
    }
    let listener = context
        .listener
        .get_or_init(|| Rc::new(context.context.listener()))
        .clone();
    Ok(NativeAudioParam(ParamOwner::Listener(listener, index)))
}

/// Id of the next source that ended, or -1.
#[op2(nofast)]
fn op_audio_next_ended(state: &mut OpState) -> f64 {
    state.borrow::<EndedSources>().pop().map_or(-1.0, f64::from)
}

// ---- AudioNode ------------------------------------------------------------

#[op2]
#[cppgc]
fn op_audio_node_param(
    #[cppgc] node: &NativeAudioNode,
    index: u32,
) -> Result<NativeAudioParam, JsErrorBox> {
    if !node.0.borrow().has_param(index) {
        return Err(JsErrorBox::type_error("Unknown AudioParam for this node"));
    }
    Ok(NativeAudioParam(ParamOwner::Node(node.0.clone(), index)))
}

#[op2(fast)]
fn op_audio_node_inputs(#[cppgc] node: &NativeAudioNode) -> u32 {
    node.0.borrow().audio_node().number_of_inputs() as u32
}

#[op2(fast)]
fn op_audio_node_outputs(#[cppgc] node: &NativeAudioNode) -> u32 {
    node.0.borrow().audio_node().number_of_outputs() as u32
}

#[op2(fast)]
fn op_audio_node_channel_count(#[cppgc] node: &NativeAudioNode) -> u32 {
    node.0.borrow().audio_node().channel_count() as u32
}

#[op2(nofast)]
fn op_audio_node_set_channel_count(
    #[cppgc] node: &NativeAudioNode,
    count: u32,
) -> Result<(), JsErrorBox> {
    guarded(|| {
        node.0
            .borrow()
            .audio_node()
            .set_channel_count(count as usize)
    })
}

#[op2(fast)]
fn op_audio_node_channel_count_mode(#[cppgc] node: &NativeAudioNode) -> u32 {
    match node.0.borrow().audio_node().channel_count_mode() {
        ChannelCountMode::Max => 0,
        ChannelCountMode::ClampedMax => 1,
        ChannelCountMode::Explicit => 2,
    }
}

#[op2(nofast)]
fn op_audio_node_set_channel_count_mode(
    #[cppgc] node: &NativeAudioNode,
    mode: u32,
) -> Result<(), JsErrorBox> {
    let mode = match mode {
        0 => ChannelCountMode::Max,
        1 => ChannelCountMode::ClampedMax,
        2 => ChannelCountMode::Explicit,
        _ => return Err(JsErrorBox::type_error("Unknown channel count mode")),
    };
    guarded(|| node.0.borrow().audio_node().set_channel_count_mode(mode))
}

#[op2(fast)]
fn op_audio_node_channel_interpretation(#[cppgc] node: &NativeAudioNode) -> u32 {
    match node.0.borrow().audio_node().channel_interpretation() {
        ChannelInterpretation::Speakers => 0,
        ChannelInterpretation::Discrete => 1,
    }
}

#[op2(nofast)]
fn op_audio_node_set_channel_interpretation(
    #[cppgc] node: &NativeAudioNode,
    interpretation: u32,
) -> Result<(), JsErrorBox> {
    let interpretation = match interpretation {
        0 => ChannelInterpretation::Speakers,
        1 => ChannelInterpretation::Discrete,
        _ => return Err(JsErrorBox::type_error("Unknown channel interpretation")),
    };
    guarded(|| {
        node.0
            .borrow()
            .audio_node()
            .set_channel_interpretation(interpretation)
    })
}

#[op2(fast)]
fn op_audio_node_max_channel_count(#[cppgc] node: &NativeAudioNode) -> u32 {
    match &*node.0.borrow() {
        Node::Destination(destination) => destination.max_channel_count() as u32,
        _ => 0,
    }
}

#[op2(nofast)]
fn op_audio_node_connect(
    #[cppgc] source: &NativeAudioNode,
    #[cppgc] destination: &NativeAudioNode,
    output: u32,
    input: u32,
) -> Result<(), JsErrorBox> {
    let source = source.0.borrow();
    let destination = destination.0.borrow();
    guarded(|| {
        source.audio_node().connect_from_output_to_input(
            destination.audio_node(),
            output as usize,
            input as usize,
        );
    })
}

#[op2(nofast)]
fn op_audio_node_connect_param(
    #[cppgc] source: &NativeAudioNode,
    #[cppgc] param: &NativeAudioParam,
    output: u32,
) -> Result<(), JsErrorBox> {
    let source = source.0.borrow();
    param.with(|param| {
        guarded(|| {
            source
                .audio_node()
                .connect_from_output_to_input(param, output as usize, 0);
        })
    })
}

#[op2(nofast)]
fn op_audio_node_disconnect(#[cppgc] node: &NativeAudioNode) -> Result<(), JsErrorBox> {
    guarded(|| node.0.borrow().audio_node().disconnect())
}

#[op2(nofast)]
fn op_audio_node_disconnect_output(
    #[cppgc] node: &NativeAudioNode,
    output: u32,
) -> Result<(), JsErrorBox> {
    guarded(|| {
        node.0
            .borrow()
            .audio_node()
            .disconnect_output(output as usize)
    })
}

/// `output`/`input` -1: every connection to `destination` (from that output).
fn disconnect_from(
    source: &dyn AudioNode,
    destination: &dyn AudioNode,
    output: i32,
    input: i32,
) -> Result<(), JsErrorBox> {
    guarded(|| match (usize::try_from(output), usize::try_from(input)) {
        (Err(_), _) => source.disconnect_dest(destination),
        (Ok(output), Err(_)) => source.disconnect_dest_from_output(destination, output),
        (Ok(output), Ok(input)) => {
            source.disconnect_dest_from_output_to_input(destination, output, input)
        },
    })
}

#[op2(nofast)]
fn op_audio_node_disconnect_node(
    #[cppgc] source: &NativeAudioNode,
    #[cppgc] destination: &NativeAudioNode,
    output: i32,
    input: i32,
) -> Result<(), JsErrorBox> {
    let source = source.0.borrow();
    let destination = destination.0.borrow();
    disconnect_from(source.audio_node(), destination.audio_node(), output, input)
}

#[op2(nofast)]
fn op_audio_node_disconnect_param(
    #[cppgc] source: &NativeAudioNode,
    #[cppgc] param: &NativeAudioParam,
    output: i32,
) -> Result<(), JsErrorBox> {
    let source = source.0.borrow();
    param.with(|param| disconnect_from(source.audio_node(), param, output, -1))
}

// ---- AudioScheduledSourceNode / OscillatorNode -----------------------------

#[op2(nofast)]
fn op_audio_source_start(#[cppgc] node: &NativeAudioNode, when: f64) -> Result<(), JsErrorBox> {
    match &mut *node.0.borrow_mut() {
        Node::ConstantSource(source) => guarded(|| source.start_at(when)),
        Node::Oscillator(source) => guarded(|| source.start_at(when)),
        _ => Err(not_a_source()),
    }
}

#[op2(nofast)]
fn op_audio_source_stop(#[cppgc] node: &NativeAudioNode, when: f64) -> Result<(), JsErrorBox> {
    match &mut *node.0.borrow_mut() {
        Node::ConstantSource(source) => guarded(|| source.stop_at(when)),
        Node::Oscillator(source) => guarded(|| source.stop_at(when)),
        _ => Err(not_a_source()),
    }
}

/// Queues `id` as an `ended` notification when the source finishes.
#[op2(nofast)]
fn op_audio_source_watch_ended(
    state: &mut OpState,
    #[cppgc] node: &NativeAudioNode,
    id: u32,
) -> Result<(), JsErrorBox> {
    let ended = state.borrow::<EndedSources>().clone();
    match &mut *node.0.borrow_mut() {
        Node::ConstantSource(source) => source.set_onended(move |_| ended.push(id)),
        Node::Oscillator(source) => source.set_onended(move |_| ended.push(id)),
        _ => return Err(not_a_source()),
    }
    Ok(())
}

#[op2(fast)]
fn op_audio_oscillator_type(#[cppgc] node: &NativeAudioNode) -> u32 {
    match &*node.0.borrow() {
        Node::Oscillator(oscillator) => match oscillator.type_() {
            OscillatorType::Sine => 0,
            OscillatorType::Square => 1,
            OscillatorType::Sawtooth => 2,
            OscillatorType::Triangle => 3,
            OscillatorType::Custom => 4,
        },
        _ => 0,
    }
}

#[op2(nofast)]
fn op_audio_oscillator_set_type(
    #[cppgc] node: &NativeAudioNode,
    kind: u32,
) -> Result<(), JsErrorBox> {
    let kind = match kind {
        0 => OscillatorType::Sine,
        1 => OscillatorType::Square,
        2 => OscillatorType::Sawtooth,
        3 => OscillatorType::Triangle,
        _ => return Err(JsErrorBox::type_error("Unsupported oscillator type")),
    };
    match &mut *node.0.borrow_mut() {
        Node::Oscillator(oscillator) => guarded(|| oscillator.set_type(kind)),
        _ => Err(JsErrorBox::type_error("Not an OscillatorNode")),
    }
}

// ---- AudioParam -----------------------------------------------------------

#[op2(fast)]
fn op_audio_param_value(#[cppgc] param: &NativeAudioParam) -> f64 {
    f64::from(param.with(|param| param.value()))
}

#[op2(fast)]
fn op_audio_param_set_value(
    #[cppgc] param: &NativeAudioParam,
    value: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| guarded(|| param.set_value(value as f32)).map(drop))
}

#[op2(fast)]
fn op_audio_param_default_value(#[cppgc] param: &NativeAudioParam) -> f64 {
    f64::from(param.with(|param| param.default_value()))
}

#[op2(fast)]
fn op_audio_param_min_value(#[cppgc] param: &NativeAudioParam) -> f64 {
    f64::from(param.with(|param| param.min_value()))
}

#[op2(fast)]
fn op_audio_param_max_value(#[cppgc] param: &NativeAudioParam) -> f64 {
    f64::from(param.with(|param| param.max_value()))
}

#[op2(nofast)]
fn op_audio_param_set_value_at_time(
    #[cppgc] param: &NativeAudioParam,
    value: f64,
    start_time: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| guarded(|| param.set_value_at_time(value as f32, start_time)).map(drop))
}

#[op2(nofast)]
fn op_audio_param_linear_ramp(
    #[cppgc] param: &NativeAudioParam,
    value: f64,
    end_time: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| {
        guarded(|| param.linear_ramp_to_value_at_time(value as f32, end_time)).map(drop)
    })
}

#[op2(nofast)]
fn op_audio_param_exponential_ramp(
    #[cppgc] param: &NativeAudioParam,
    value: f64,
    end_time: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| {
        guarded(|| param.exponential_ramp_to_value_at_time(value as f32, end_time)).map(drop)
    })
}

#[op2(nofast)]
fn op_audio_param_set_target(
    #[cppgc] param: &NativeAudioParam,
    value: f64,
    start_time: f64,
    time_constant: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| {
        guarded(|| param.set_target_at_time(value as f32, start_time, time_constant)).map(drop)
    })
}

#[op2(nofast)]
fn op_audio_param_cancel_scheduled_values(
    #[cppgc] param: &NativeAudioParam,
    cancel_time: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| guarded(|| param.cancel_scheduled_values(cancel_time)).map(drop))
}

#[op2(nofast)]
fn op_audio_param_cancel_and_hold(
    #[cppgc] param: &NativeAudioParam,
    cancel_time: f64,
) -> Result<(), JsErrorBox> {
    param.with(|param| guarded(|| param.cancel_and_hold_at_time(cancel_time)).map(drop))
}

/// `values`: the bytes of a Float32Array (native endianness).
#[op2(nofast)]
fn op_audio_param_set_value_curve(
    #[cppgc] param: &NativeAudioParam,
    #[buffer] values: &[u8],
    start_time: f64,
    duration: f64,
) -> Result<(), JsErrorBox> {
    let values: Vec<f32> = values
        .chunks_exact(4)
        .map(|bytes| f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect();
    param.with(|param| {
        guarded(|| param.set_value_curve_at_time(&values, start_time, duration)).map(drop)
    })
}

deno_core::extension!(
    roves_blitz_audio,
    ops = [
        op_audio_context_create,
        op_audio_context_sample_rate,
        op_audio_context_current_time,
        op_audio_context_base_latency,
        op_audio_context_output_latency,
        op_audio_context_state,
        op_audio_context_resume,
        op_audio_context_suspend,
        op_audio_context_close,
        op_audio_context_destination,
        op_audio_context_create_node,
        op_audio_context_listener_param,
        op_audio_next_ended,
        op_audio_node_param,
        op_audio_node_inputs,
        op_audio_node_outputs,
        op_audio_node_channel_count,
        op_audio_node_set_channel_count,
        op_audio_node_channel_count_mode,
        op_audio_node_set_channel_count_mode,
        op_audio_node_channel_interpretation,
        op_audio_node_set_channel_interpretation,
        op_audio_node_max_channel_count,
        op_audio_node_connect,
        op_audio_node_connect_param,
        op_audio_node_disconnect,
        op_audio_node_disconnect_output,
        op_audio_node_disconnect_node,
        op_audio_node_disconnect_param,
        op_audio_source_start,
        op_audio_source_stop,
        op_audio_source_watch_ended,
        op_audio_oscillator_type,
        op_audio_oscillator_set_type,
        op_audio_param_value,
        op_audio_param_set_value,
        op_audio_param_default_value,
        op_audio_param_min_value,
        op_audio_param_max_value,
        op_audio_param_set_value_at_time,
        op_audio_param_linear_ramp,
        op_audio_param_exponential_ramp,
        op_audio_param_set_target,
        op_audio_param_cancel_scheduled_values,
        op_audio_param_cancel_and_hold,
        op_audio_param_set_value_curve,
    ],
    state = |state| {
        state.put(EndedSources::default());
    },
);

pub fn check_offline_render() {
    const FRAMES: usize = 256;
    let mut context = OfflineAudioContext::new(1, FRAMES, 48_000.0);
    let mut source = context.create_constant_source();
    let gain = context.create_gain();

    source.offset().set_value(0.5);
    gain.gain().set_value(0.5);
    source.connect(&gain);
    gain.connect(&context.destination());
    source.start();

    let rendered = context.start_rendering_sync();
    let samples = rendered.get_channel_data(0);
    assert_eq!(samples.len(), FRAMES);
    assert!(
        samples.iter().all(|sample| (sample - 0.25).abs() < 1e-6),
        "the audio graph should render 0.5 × 0.5 for every frame"
    );
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"web_audio_backend_offline\",\"status\":\"passed\",\"frames\":{FRAMES}}}"
    );
}
