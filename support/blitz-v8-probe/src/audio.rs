//! Offline backend check for the Web Audio bridge.

use web_audio_api::context::{BaseAudioContext, OfflineAudioContext};
use web_audio_api::node::{AudioNode, AudioScheduledSourceNode};

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
