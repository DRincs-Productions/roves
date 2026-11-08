//! Offline backend check for the Web Audio bridge.

use deno_core::{
    op2,
    v8::cppgc::{GarbageCollected, Visitor},
};
use deno_error::JsErrorBox;
use web_audio_api::context::{
    AudioContext, AudioContextOptions, AudioContextState, BaseAudioContext, OfflineAudioContext,
};
use web_audio_api::node::{AudioNode, AudioScheduledSourceNode, GainNode};

pub struct NativeAudioContext(AudioContext);
pub struct NativeGainNode(GainNode);

// SAFETY: the Rust node owns no V8 references.
unsafe impl GarbageCollected for NativeGainNode {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeGainNode"
    }
}

// SAFETY: the Rust audio context owns no V8 references.
unsafe impl GarbageCollected for NativeAudioContext {
    fn trace(&self, _visitor: &mut Visitor) {}

    fn get_name(&self) -> &'static std::ffi::CStr {
        c"RovesNativeAudioContext"
    }
}

#[op2]
#[cppgc]
fn op_audio_context_create() -> Result<NativeAudioContext, JsErrorBox> {
    AudioContext::try_new(AudioContextOptions {
        sink_id: "none".into(),
        ..Default::default()
    })
    .map(NativeAudioContext)
    .map_err(|error| JsErrorBox::generic(error.to_string()))
}

#[op2(fast)]
fn op_audio_context_sample_rate(#[cppgc] context: &NativeAudioContext) -> f64 {
    f64::from(context.0.sample_rate())
}

#[op2]
#[string]
fn op_audio_context_state(#[cppgc] context: &NativeAudioContext) -> &'static str {
    match context.0.state() {
        AudioContextState::Suspended => "suspended",
        AudioContextState::Running => "running",
        AudioContextState::Closed => "closed",
    }
}

#[op2(nofast)]
fn op_audio_context_close_for_probe(#[cppgc] context: &NativeAudioContext) {
    context.0.close_sync();
}

#[op2]
#[cppgc]
fn op_audio_context_create_gain(#[cppgc] context: &NativeAudioContext) -> NativeGainNode {
    NativeGainNode(context.0.create_gain())
}

#[op2(fast)]
fn op_audio_gain_value(#[cppgc] node: &NativeGainNode) -> f64 {
    f64::from(node.0.gain().value())
}

#[op2(fast)]
fn op_audio_gain_set_value(#[cppgc] node: &NativeGainNode, value: f64) -> Result<(), JsErrorBox> {
    let value = value as f32;
    if !value.is_finite() {
        return Err(JsErrorBox::type_error("Gain must be a finite float"));
    }
    node.0.gain().set_value(value);
    Ok(())
}

deno_core::extension!(
    roves_blitz_audio,
    ops = [
        op_audio_context_create,
        op_audio_context_sample_rate,
        op_audio_context_state,
        op_audio_context_close_for_probe,
        op_audio_context_create_gain,
        op_audio_gain_value,
        op_audio_gain_set_value,
    ],
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
