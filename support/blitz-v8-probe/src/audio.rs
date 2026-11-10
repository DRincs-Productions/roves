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
//!
//! `decodeAudioData` hands the bytes to the crate's decoder (Symphonia, resampled
//! to the context rate) on one dedicated decoder thread, so decoding never runs
//! on the V8 thread; finished results are queued like `ended` and drained once
//! per frame (`op_audio_next_decoded`/`op_audio_take_decoded`).

use std::any::Any;
use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use deno_core::{
    OpState, op2,
    v8::cppgc::{GarbageCollected, Visitor},
};
use deno_error::JsErrorBox;
use web_audio_api::context::{
    AudioContext, AudioContextOptions, AudioContextState, BaseAudioContext, OfflineAudioContext,
};
use web_audio_api::node::{
    AudioBufferSourceNode, AudioDestinationNode, AudioNode, AudioScheduledSourceNode,
    ChannelCountMode, ChannelInterpretation, ChannelMergerNode, ChannelSplitterNode,
    ConstantSourceNode, GainNode, OscillatorNode, OscillatorType, StereoPannerNode,
};
use web_audio_api::{AudioBuffer, AudioListener, AudioParam};

const NODE_GAIN: u32 = 0;
const NODE_CONSTANT_SOURCE: u32 = 1;
const NODE_OSCILLATOR: u32 = 2;
const NODE_CHANNEL_MERGER: u32 = 3;
const NODE_CHANNEL_SPLITTER: u32 = 4;
const NODE_STEREO_PANNER: u32 = 5;
const NODE_BUFFER_SOURCE: u32 = 6;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Ids of finished sources, pushed by the crate's event thread and drained on
/// the V8 thread.
#[derive(Clone, Default)]
struct EndedSources(Arc<Mutex<VecDeque<u32>>>);

impl EndedSources {
    fn push(&self, id: u32) {
        lock(&self.0).push_back(id);
    }

    fn pop(&self) -> Option<u32> {
        lock(&self.0).pop_front()
    }
}

type DecodeResult = Result<AudioBuffer, String>;
type DecodeJob = Pin<
    Box<dyn Future<Output = Result<AudioBuffer, Box<dyn std::error::Error + Send + Sync>>> + Send>,
>;

/// The decoder thread (started on the first `decodeAudioData`, one job at a
/// time so a burst of preloads cannot saturate every core) and its results,
/// drained on the V8 thread. The thread exits when the runtime drops the sender.
#[derive(Default)]
struct Decoder {
    jobs: Option<mpsc::Sender<(u32, DecodeJob)>>,
    finished: Arc<Mutex<VecDeque<(u32, DecodeResult)>>>,
    /// The result of the id last returned by `op_audio_next_decoded`.
    taken: Option<DecodeResult>,
}

impl Decoder {
    fn submit(&mut self, id: u32, job: DecodeJob) -> Result<(), JsErrorBox> {
        if self.jobs.is_none() {
            let (sender, receiver) = mpsc::channel::<(u32, DecodeJob)>();
            let finished = Arc::clone(&self.finished);
            std::thread::Builder::new()
                .name("roves-audio-decode".into())
                .spawn(move || {
                    for (id, job) in receiver {
                        let result = catch_unwind(AssertUnwindSafe(|| block_on(job)))
                            .map_err(|payload| panic_message(&*payload))
                            .and_then(|decoded| decoded.map_err(|error| error.to_string()));
                        lock(&finished).push_back((id, result));
                    }
                })
                .map_err(|error| {
                    JsErrorBox::generic(format!("Cannot start the audio decoder thread: {error}"))
                })?;
            self.jobs = Some(sender);
        }
        self.jobs
            .as_ref()
            .and_then(|jobs| jobs.send((id, job)).ok())
            .ok_or_else(|| JsErrorBox::generic("The audio decoder thread has stopped"))
    }
}

/// Drives the crate's decode future to completion on the decoder thread (it does
/// blocking IO and finishes on its first poll, but any wake-up is honoured).
fn block_on<F: Future>(future: F) -> F::Output {
    struct ThreadWaker(std::thread::Thread);
    impl std::task::Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        std::thread::park();
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("Web Audio backend error")
        .to_owned()
}

/// Runs a crate call, turning its spec-error panics into JS errors whose message
/// starts with the exception name (mapped to DOMException/RangeError in JS).
fn guarded<R>(operation: impl FnOnce() -> R) -> Result<R, JsErrorBox> {
    catch_unwind(AssertUnwindSafe(operation))
        .map_err(|payload| JsErrorBox::generic(panic_message(&*payload)))
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
    ChannelMerger(ChannelMergerNode),
    ChannelSplitter(ChannelSplitterNode),
    StereoPanner(StereoPannerNode),
    BufferSource(AudioBufferSourceNode),
}

impl Node {
    fn audio_node(&self) -> &dyn AudioNode {
        match self {
            Self::Destination(node) => node,
            Self::Gain(node) => node,
            Self::ConstantSource(node) => node,
            Self::Oscillator(node) => node,
            Self::ChannelMerger(node) => node,
            Self::ChannelSplitter(node) => node,
            Self::StereoPanner(node) => node,
            Self::BufferSource(node) => node,
        }
    }

    /// Index of the node's AudioParams: gain, offset, pan, frequency or
    /// playbackRate (0) and detune (1).
    fn has_param(&self, index: u32) -> bool {
        matches!(
            (self, index),
            (Self::Gain(_), 0)
                | (Self::ConstantSource(_), 0)
                | (Self::StereoPanner(_), 0)
                | (Self::Oscillator(_), 0 | 1)
                | (Self::BufferSource(_), 0 | 1)
        )
    }

    fn buffer_source(&mut self) -> Result<&mut AudioBufferSourceNode, JsErrorBox> {
        match self {
            Self::BufferSource(source) => Ok(source),
            _ => Err(JsErrorBox::type_error("Not an AudioBufferSourceNode")),
        }
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

/// A decoded (or acquired) buffer. Cloning the crate's AudioBuffer shares its
/// channel data, so handing it to a source node copies no samples.
pub struct NativeAudioBuffer(RefCell<AudioBuffer>);

impl NativeAudioParam {
    fn with<R>(&self, operation: impl FnOnce(&AudioParam) -> R) -> R {
        match &self.0 {
            ParamOwner::Node(node, index) => {
                let node = node.borrow();
                operation(match (&*node, *index) {
                    (Node::Gain(node), 0) => node.gain(),
                    (Node::ConstantSource(node), 0) => node.offset(),
                    (Node::StereoPanner(node), 0) => node.pan(),
                    (Node::Oscillator(node), 0) => node.frequency(),
                    (Node::Oscillator(node), 1) => node.detune(),
                    (Node::BufferSource(node), 0) => node.playback_rate(),
                    (Node::BufferSource(node), 1) => node.detune(),
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

// SAFETY (all four): the Rust objects own no V8 references.
unsafe impl GarbageCollected for NativeAudioBuffer {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeAudioBuffer"
    }
}

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

/// `count`: numberOfInputs (merger) or numberOfOutputs (splitter), validated
/// against [1, 32] by web_audio.js; ignored by the other kinds.
#[op2]
#[cppgc]
fn op_audio_context_create_node(
    #[cppgc] context: &NativeAudioContext,
    kind: u32,
    count: u32,
) -> Result<NativeAudioNode, JsErrorBox> {
    let context = &context.context;
    let count = count as usize;
    let node = guarded(|| match kind {
        NODE_GAIN => Some(Node::Gain(context.create_gain())),
        NODE_CONSTANT_SOURCE => Some(Node::ConstantSource(context.create_constant_source())),
        NODE_OSCILLATOR => Some(Node::Oscillator(context.create_oscillator())),
        NODE_CHANNEL_MERGER => Some(Node::ChannelMerger(context.create_channel_merger(count))),
        NODE_CHANNEL_SPLITTER => Some(Node::ChannelSplitter(
            context.create_channel_splitter(count),
        )),
        NODE_STEREO_PANNER => Some(Node::StereoPanner(context.create_stereo_panner())),
        NODE_BUFFER_SOURCE => Some(Node::BufferSource(context.create_buffer_source())),
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

/// Queues `bytes` (the contents of the detached ArrayBuffer) for decoding at the
/// context's sample rate; the result is reported under `id`.
#[op2(nofast)]
fn op_audio_context_decode(
    state: &mut OpState,
    #[cppgc] context: &NativeAudioContext,
    #[buffer(copy)] bytes: Vec<u8>,
    id: u32,
) -> Result<(), JsErrorBox> {
    // The future owns the bytes and the sample rate only: decoding happens when
    // the decoder thread polls it.
    let job: DecodeJob = Box::pin(context.context.decode_audio_data(Cursor::new(bytes)));
    state.borrow_mut::<Decoder>().submit(id, job)
}

/// Id of the next finished decode, or -1; its result is then taken with
/// `op_audio_take_decoded`.
#[op2(nofast)]
fn op_audio_next_decoded(state: &mut OpState) -> f64 {
    let decoder = state.borrow_mut::<Decoder>();
    let Some((id, result)) = lock(&decoder.finished).pop_front() else {
        return -1.0;
    };
    decoder.taken = Some(result);
    f64::from(id)
}

#[op2]
#[cppgc]
fn op_audio_take_decoded(state: &mut OpState) -> Result<NativeAudioBuffer, JsErrorBox> {
    match state.borrow_mut::<Decoder>().taken.take() {
        Some(Ok(buffer)) => Ok(NativeAudioBuffer(RefCell::new(buffer))),
        Some(Err(message)) => Err(JsErrorBox::generic(message)),
        None => Err(JsErrorBox::generic("No decoded audio data pending")),
    }
}

// ---- AudioBuffer ----------------------------------------------------------

/// Silent buffer, validated by web_audio.js (1–32 channels, length ≥ 1, sample
/// rate in [3000, 768000]); filled with `op_audio_buffer_copy_to`.
#[op2]
#[cppgc]
fn op_audio_buffer_create(
    channels: u32,
    length: u32,
    sample_rate: f64,
) -> Result<NativeAudioBuffer, JsErrorBox> {
    let samples = vec![vec![0.0_f32; length as usize]; channels as usize];
    guarded(|| AudioBuffer::from(samples, sample_rate as f32))
        .map(|buffer| NativeAudioBuffer(RefCell::new(buffer)))
}

#[op2(fast)]
fn op_audio_buffer_channels(#[cppgc] buffer: &NativeAudioBuffer) -> u32 {
    buffer.0.borrow().number_of_channels() as u32
}

#[op2(fast)]
fn op_audio_buffer_length(#[cppgc] buffer: &NativeAudioBuffer) -> u32 {
    buffer.0.borrow().length() as u32
}

#[op2(fast)]
fn op_audio_buffer_sample_rate(#[cppgc] buffer: &NativeAudioBuffer) -> f64 {
    f64::from(buffer.0.borrow().sample_rate())
}

fn check_channel(buffer: &AudioBuffer, channel: u32) -> Result<usize, JsErrorBox> {
    let channel = channel as usize;
    if channel < buffer.number_of_channels() {
        Ok(channel)
    } else {
        Err(JsErrorBox::generic(format!(
            "IndexSizeError - channel {channel} is out of range"
        )))
    }
}

/// Copies samples from `offset` of `channel` into `destination`, the bytes of a
/// Float32Array (native endianness), as far as both reach.
#[op2(nofast)]
fn op_audio_buffer_copy_from(
    #[cppgc] buffer: &NativeAudioBuffer,
    channel: u32,
    offset: u32,
    #[buffer] destination: &mut [u8],
) -> Result<(), JsErrorBox> {
    let buffer = buffer.0.borrow();
    let channel = check_channel(&buffer, channel)?;
    let samples = buffer
        .get_channel_data(channel)
        .get(offset as usize..)
        .unwrap_or_default();
    for (bytes, sample) in destination.chunks_exact_mut(4).zip(samples) {
        bytes.copy_from_slice(&sample.to_ne_bytes());
    }
    Ok(())
}

/// Copies the bytes of a Float32Array into `channel` from `offset`, as far as
/// both reach. Only used to fill a buffer from `op_audio_buffer_create` before it
/// is handed to a source (web_audio.js keeps game-written samples in JS).
#[op2(nofast)]
fn op_audio_buffer_copy_to(
    #[cppgc] buffer: &NativeAudioBuffer,
    channel: u32,
    offset: u32,
    #[buffer] source: &[u8],
) -> Result<(), JsErrorBox> {
    let mut buffer = buffer.0.borrow_mut();
    let channel = check_channel(&buffer, channel)?;
    let samples = buffer
        .get_channel_data_mut(channel)
        .get_mut(offset as usize..)
        .unwrap_or_default();
    for (sample, bytes) in samples.iter_mut().zip(source.chunks_exact(4)) {
        *sample = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    Ok(())
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
        Node::BufferSource(source) => guarded(|| source.start_at(when)),
        _ => Err(not_a_source()),
    }
}

#[op2(nofast)]
fn op_audio_source_stop(#[cppgc] node: &NativeAudioNode, when: f64) -> Result<(), JsErrorBox> {
    match &mut *node.0.borrow_mut() {
        Node::ConstantSource(source) => guarded(|| source.stop_at(when)),
        Node::Oscillator(source) => guarded(|| source.stop_at(when)),
        Node::BufferSource(source) => guarded(|| source.stop_at(when)),
        _ => Err(not_a_source()),
    }
}

// ---- AudioBufferSourceNode --------------------------------------------------

/// `duration` < 0: play to the end of the buffer (or forever when looping).
#[op2(nofast)]
fn op_audio_buffer_source_start(
    #[cppgc] node: &NativeAudioNode,
    when: f64,
    offset: f64,
    duration: f64,
) -> Result<(), JsErrorBox> {
    let mut node = node.0.borrow_mut();
    let source = node.buffer_source()?;
    let duration = if duration < 0.0 { f64::MAX } else { duration };
    guarded(|| source.start_at_with_offset_and_duration(when, offset, duration))
}

/// Hands the buffer's (shared) samples to the render thread; web_audio.js calls
/// it once, when the content is acquired.
#[op2(nofast)]
fn op_audio_buffer_source_set_buffer(
    #[cppgc] node: &NativeAudioNode,
    #[cppgc] buffer: &NativeAudioBuffer,
) -> Result<(), JsErrorBox> {
    let buffer = buffer.0.borrow().clone();
    let mut node = node.0.borrow_mut();
    let source = node.buffer_source()?;
    guarded(|| source.set_buffer(buffer))
}

#[op2(nofast)]
fn op_audio_buffer_source_set_loop(
    #[cppgc] node: &NativeAudioNode,
    enabled: bool,
    start: f64,
    end: f64,
) -> Result<(), JsErrorBox> {
    let mut node = node.0.borrow_mut();
    let source = node.buffer_source()?;
    guarded(|| {
        source.set_loop(enabled);
        source.set_loop_start(start);
        source.set_loop_end(end);
    })
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
        Node::BufferSource(source) => source.set_onended(move |_| ended.push(id)),
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
        op_audio_context_decode,
        op_audio_next_decoded,
        op_audio_take_decoded,
        op_audio_buffer_create,
        op_audio_buffer_channels,
        op_audio_buffer_length,
        op_audio_buffer_sample_rate,
        op_audio_buffer_copy_from,
        op_audio_buffer_copy_to,
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
        op_audio_buffer_source_start,
        op_audio_buffer_source_set_buffer,
        op_audio_buffer_source_set_loop,
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
        state.put(Decoder::default());
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
    check_offline_channel_routing();
    check_offline_stereo_panner();
    check_offline_buffer_source();
}

/// 16-bit PCM mono WAV holding `samples`.
fn pcm16_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1_u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16_u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    wav
}

/// The decoder and source behind decodeAudioData/createBufferSource: a WAV of
/// 128 frames at 0.5 renders as 128 frames of 0.5 followed by silence, and
/// bytes that are not audio fail instead of decoding to silence.
fn check_offline_buffer_source() {
    const FRAMES: usize = 256;
    const DECODED: usize = 128;
    let mut context = OfflineAudioContext::new(1, FRAMES, 48_000.0);
    let decoded = context
        .decode_audio_data_sync(Cursor::new(pcm16_wav(&[16_384; DECODED], 48_000)))
        .expect("a 16-bit PCM WAV should decode");
    assert_eq!(
        (decoded.number_of_channels(), decoded.length()),
        (1, DECODED)
    );
    assert_channel(&decoded, 0, 0.5, "decoded WAV");
    assert!(
        context
            .decode_audio_data_sync(Cursor::new(vec![0_u8; 64]))
            .is_err(),
        "bytes that are not audio must not decode"
    );

    let mut source = context.create_buffer_source();
    source.set_buffer(decoded);
    source.connect(&context.destination());
    source.start();
    let rendered = context.start_rendering_sync();
    let samples = rendered.get_channel_data(0);
    assert!(
        samples[..DECODED - 1]
            .iter()
            .all(|sample| (sample - 0.5).abs() < 2e-3)
            && samples[DECODED + 1..]
                .iter()
                .all(|sample| sample.abs() < 1e-6),
        "the buffer source should play the decoded frames once, got {:?} … {:?}",
        &samples[..4],
        &samples[DECODED - 2..DECODED + 2]
    );
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"web_audio_buffer_source_offline\",\"status\":\"passed\",\"frames\":{FRAMES}}}"
    );
}

fn assert_channel(
    rendered: &web_audio_api::AudioBuffer,
    channel: usize,
    expected: f32,
    what: &str,
) {
    let samples = rendered.get_channel_data(channel);
    assert!(
        samples
            .iter()
            .all(|sample| (sample - expected).abs() < 2e-3),
        "{what}: channel {channel} should render {expected}, got {:?}",
        &samples[..4.min(samples.len())]
    );
}

/// Two mono sources merged into stereo, split again and re-merged swapped: the
/// node kinds web_audio.js exposes as ChannelMerger/ChannelSplitter.
fn check_offline_channel_routing() {
    const FRAMES: usize = 256;
    let mut context = OfflineAudioContext::new(2, FRAMES, 48_000.0);
    let mut left = context.create_constant_source();
    let mut right = context.create_constant_source();
    left.offset().set_value(0.25);
    right.offset().set_value(0.75);
    let merger = context.create_channel_merger(2);
    let splitter = context.create_channel_splitter(2);
    let swapped = context.create_channel_merger(2);
    assert_eq!(
        (merger.number_of_inputs(), merger.number_of_outputs()),
        (2, 1)
    );
    assert_eq!(
        (splitter.number_of_inputs(), splitter.number_of_outputs()),
        (1, 2)
    );

    left.connect_from_output_to_input(&merger, 0, 0);
    right.connect_from_output_to_input(&merger, 0, 1);
    merger.connect(&splitter);
    splitter.connect_from_output_to_input(&swapped, 1, 0);
    splitter.connect_from_output_to_input(&swapped, 0, 1);
    swapped.connect(&context.destination());
    left.start();
    right.start();

    let rendered = context.start_rendering_sync();
    assert_channel(&rendered, 0, 0.75, "merger/splitter routing");
    assert_channel(&rendered, 1, 0.25, "merger/splitter routing");
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"web_audio_channel_routing_offline\",\"status\":\"passed\",\"frames\":{FRAMES}}}"
    );
}

/// Equal-power panning of a mono source (Web Audio §1.31.4): pan 0.5 gives
/// cos(3π/8) left and sin(3π/8) right.
fn check_offline_stereo_panner() {
    const FRAMES: usize = 256;
    let mut context = OfflineAudioContext::new(2, FRAMES, 48_000.0);
    let mut source = context.create_constant_source();
    let panner = context.create_stereo_panner();
    panner.pan().set_value(0.5);
    source.connect(&panner);
    panner.connect(&context.destination());
    source.start();

    let rendered = context.start_rendering_sync();
    let angle = 0.75 * std::f32::consts::FRAC_PI_2;
    assert_channel(&rendered, 0, angle.cos(), "stereo panner");
    assert_channel(&rendered, 1, angle.sin(), "stereo panner");
    println!(
        "ROVES_PROBE_RESULT {{\"check\":\"web_audio_stereo_panner_offline\",\"status\":\"passed\",\"frames\":{FRAMES}}}"
    );
}
