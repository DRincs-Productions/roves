/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::Builder;

use byte_slice_cast::*;
use gstreamer;
use gstreamer::prelude::*;
use gstreamer_app::{AppSrc, AppSrcCallbacks};
use gstreamer_audio;
use servo_media_audio::block::{Chunk, FRAMES_PER_BLOCK};
use servo_media_audio::render_thread::AudioRenderThreadMsg;
use servo_media_audio::sink::{AudioSink, AudioSinkError};
use servo_media_streams::MediaSocket;

use crate::media_stream::GstreamerMediaSocket;

const DEFAULT_SAMPLE_RATE: f32 = 44100.;

/// How many render quanta (128 frames, 2.9 ms at 44.1 kHz) go into one buffer pushed to GStreamer.
/// Pushing a buffer, and the "need data" round trip that follows, has a fixed cost: doing it
/// every quantum made the audio thread one of the busiest in a page that only plays sound. Four
/// quanta is about 11.6 ms, the size of the sink's own 10 ms periods (see `init`).
const QUANTA_PER_BUFFER: u32 = 4;

pub struct GStreamerAudioSink {
    pipeline: gstreamer::Pipeline,
    appsrc: Arc<AppSrc>,
    sample_rate: Cell<f32>,
    audio_info: RefCell<Option<gstreamer_audio::AudioInfo>>,
    sample_offset: Cell<u64>,
    /// Interleaved samples of the quanta rendered since the last buffer was pushed.
    pending: RefCell<Vec<f32>>,
    pending_quanta: Cell<u32>,
}

impl GStreamerAudioSink {
    pub fn new() -> Result<Self, AudioSinkError> {
        if let Some(category) = gstreamer::DebugCategory::get("openslessink") {
            category.set_threshold(gstreamer::DebugLevel::Trace);
        }
        gstreamer::init().map_err(|error| {
            AudioSinkError::Backend(format!("GStreamer init failed: {error:?}"))
        })?;

        let appsrc = gstreamer::ElementFactory::make("appsrc")
            .build()
            .map_err(|error| {
                AudioSinkError::Backend(format!("appsrc creation failed: {error:?}"))
            })?;
        let appsrc = appsrc.downcast::<AppSrc>().unwrap();

        Ok(Self {
            pipeline: gstreamer::Pipeline::new(),
            appsrc: Arc::new(appsrc),
            sample_rate: Cell::new(DEFAULT_SAMPLE_RATE),
            audio_info: RefCell::new(None),
            sample_offset: Cell::new(0),
            pending: RefCell::new(Vec::new()),
            pending_quanta: Cell::new(0),
        })
    }
}

impl GStreamerAudioSink {
    fn set_audio_info(&self, sample_rate: f32, channels: u8) -> Result<(), AudioSinkError> {
        let audio_info = gstreamer_audio::AudioInfo::builder(
            gstreamer_audio::AUDIO_FORMAT_F32,
            sample_rate as u32,
            channels.into(),
        )
        .build()
        .map_err(|error| AudioSinkError::Backend(format!("AudioInfo failed: {error:?}")))?;
        self.appsrc.set_caps(audio_info.to_caps().ok().as_ref());
        *self.audio_info.borrow_mut() = Some(audio_info);
        Ok(())
    }

    fn set_channels_if_changed(&self, channels: u8) -> Result<(), AudioSinkError> {
        let curr_channels = match self.audio_info.borrow().as_ref() {
            Some(ch) => ch.channels(),
            _ => {
                return Ok(());
            },
        };
        if channels != curr_channels as u8 {
            self.set_audio_info(self.sample_rate.get(), channels)?;
        }
        Ok(())
    }
}

impl GStreamerAudioSink {
    /// Push the rendered quanta queued so far as one buffer.
    fn flush_pending(&self) -> Result<(), AudioSinkError> {
        let quanta = self.pending_quanta.replace(0);
        if quanta == 0 {
            return Ok(());
        }
        let mut samples = self.pending.borrow_mut();
        let sample_rate = self.sample_rate.get() as u64;
        let n_samples = FRAMES_PER_BLOCK.0 * quanta as u64;
        let mut buffer = gstreamer::Buffer::with_size(samples.len() * 4).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();
            let sample_offset = self.sample_offset.get();
            // Calculate the current timestamp (PTS) and the next one,
            // and calculate the duration from the difference instead of
            // simply the number of samples to prevent rounding errors
            let pts = gstreamer::ClockTime::from_nseconds(
                sample_offset
                    .mul_div_floor(gstreamer::ClockTime::SECOND.nseconds(), sample_rate)
                    .unwrap(),
            );
            let next_pts: gstreamer::ClockTime = gstreamer::ClockTime::from_nseconds(
                (sample_offset + n_samples)
                    .mul_div_floor(gstreamer::ClockTime::SECOND.nseconds(), sample_rate)
                    .unwrap(),
            );
            buffer.set_pts(Some(pts));
            buffer.set_duration(next_pts - pts);
            // XXXManishearth if we have a safe way to convert
            // from Box<[f32]> to Box<[u8]> (similarly for Vec)
            // we can use Buffer::from_slice instead
            buffer
                .copy_from_slice(0, samples.as_mut_slice().as_mut_byte_slice())
                .expect("copying failed");
            self.sample_offset.set(sample_offset + n_samples);
        }
        samples.clear();
        drop(samples);

        self.appsrc
            .push_buffer(buffer)
            .map(|_| ())
            .map_err(|_| AudioSinkError::BufferPushFailed)
    }
}

impl AudioSink for GStreamerAudioSink {
    fn init(
        &self,
        sample_rate: f32,
        graph_thread_channel: Sender<AudioRenderThreadMsg>,
    ) -> Result<(), AudioSinkError> {
        self.sample_rate.set(sample_rate);
        self.set_audio_info(sample_rate, 2)?;
        self.appsrc.set_format(gstreamer::Format::Time);

        // Allow only a single chunk.
        self.appsrc.set_max_bytes(1);

        let appsrc = self.appsrc.clone();
        Builder::new()
            .name("GstAppSrcCallbacks".to_owned())
            .spawn(move || {
                let need_data = move |_: &AppSrc, _: u32| {
                    if let Err(e) = graph_thread_channel.send(AudioRenderThreadMsg::SinkNeedData) {
                        log::warn!("Error sending need data event: {:?}", e);
                    }
                };
                appsrc.set_callbacks(AppSrcCallbacks::builder().need_data(need_data).build());
            })
            .unwrap();

        let appsrc = self.appsrc.as_ref().clone().upcast();
        let resample = gstreamer::ElementFactory::make("audioresample")
            .build()
            .map_err(|error| {
                AudioSinkError::Backend(format!("audioresample creation failed: {error:?}"))
            })?;
        let convert = gstreamer::ElementFactory::make("audioconvert")
            .build()
            .map_err(|error| {
                AudioSinkError::Backend(format!("audioconvert creation failed: {error:?}"))
            })?;
        let sink = gstreamer::ElementFactory::make("autoaudiosink")
            .build()
            .map_err(|error| {
                AudioSinkError::Backend(format!("autoaudiosink creation failed: {error:?}"))
            })?;
        // The platform sink's default 200 ms ring buffer delays every Web Audio change, e.g. a
        // game pausing its music, by up to that much; Chrome stays around 10-20 ms. Ask the
        // real sink, created inside `autoaudiosink`, for a 40 ms buffer in 10 ms periods.
        if let Some(child_proxy) = sink.dynamic_cast_ref::<gstreamer::ChildProxy>() {
            child_proxy.connect_child_added(|_, child, _| {
                for (name, microseconds) in [("buffer-time", 40_000i64), ("latency-time", 10_000)] {
                    if child.has_property(name) {
                        child.set_property(name, microseconds);
                    }
                }
            });
        }
        self.pipeline
            .add_many([&appsrc, &resample, &convert, &sink])
            .map_err(|error| AudioSinkError::Backend(error.to_string()))?;
        gstreamer::Element::link_many([&appsrc, &resample, &convert, &sink])
            .map_err(|error| AudioSinkError::Backend(error.to_string()))?;

        Ok(())
    }

    fn init_stream(
        &self,
        channels: u8,
        sample_rate: f32,
        socket: Box<dyn MediaSocket>,
    ) -> Result<(), AudioSinkError> {
        self.sample_rate.set(sample_rate);
        self.set_audio_info(sample_rate, channels)?;
        self.appsrc.set_format(gstreamer::Format::Time);

        // Do not set max bytes or callback, we will push as needed

        let appsrc = self.appsrc.as_ref().clone().upcast();
        let convert = gstreamer::ElementFactory::make("audioconvert")
            .build()
            .map_err(|error| {
                AudioSinkError::Backend(format!("audioconvert creation failed: {error:?}"))
            })?;
        let sink = socket
            .as_any()
            .downcast_ref::<GstreamerMediaSocket>()
            .unwrap()
            .proxy_sink()
            .clone();

        self.pipeline
            .add_many([&appsrc, &convert, &sink])
            .map_err(|error| AudioSinkError::Backend(error.to_string()))?;
        gstreamer::Element::link_many([&appsrc, &convert, &sink])
            .map_err(|error| AudioSinkError::Backend(error.to_string()))?;

        Ok(())
    }

    fn play(&self) -> Result<(), AudioSinkError> {
        self.pipeline
            .set_state(gstreamer::State::Playing)
            .map(|_| ())
            .map_err(|_| AudioSinkError::StateChangeFailed)
    }

    fn stop(&self) -> Result<(), AudioSinkError> {
        // Samples rendered before a pause must not play after the resume.
        self.pending.borrow_mut().clear();
        self.pending_quanta.set(0);
        self.pipeline
            .set_state(gstreamer::State::Paused)
            .map(|_| ())
            .map_err(|_| AudioSinkError::StateChangeFailed)
    }

    fn has_enough_data(&self) -> bool {
        self.appsrc.current_level_bytes() >= self.appsrc.max_bytes()
    }

    fn push_data(&self, mut chunk: Chunk) -> Result<(), AudioSinkError> {
        if let Some(block) = chunk.blocks.first() {
            let channels = block.chan_count();
            let current = self
                .audio_info
                .borrow()
                .as_ref()
                .map(|info| info.channels() as u8);
            if current.is_some_and(|current| current != channels) {
                // The queued samples have the old channel layout: send them before the caps change.
                self.flush_pending()?;
            }
            self.set_channels_if_changed(channels)?;
        }

        let channels = self.audio_info.borrow().as_ref().unwrap().channels();
        // sometimes nothing reaches the output
        if chunk.is_empty() {
            chunk.blocks.push(Default::default());
            chunk.blocks[0].repeat(channels as u8);
        }
        debug_assert!(chunk.len() == 1);
        self.pending
            .borrow_mut()
            .extend_from_slice(&chunk.blocks[0].interleave());
        self.pending_quanta.set(self.pending_quanta.get() + 1);
        if self.pending_quanta.get() < QUANTA_PER_BUFFER {
            // Not enough for a buffer yet: the render thread keeps rendering (the appsrc is empty).
            return Ok(());
        }
        self.flush_pending()
    }

    fn set_eos_callback(&self, _: Box<dyn Fn(Box<dyn AsRef<[f32]>>) + Send + Sync + 'static>) {}
}

impl Drop for GStreamerAudioSink {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
