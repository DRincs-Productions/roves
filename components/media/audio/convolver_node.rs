/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use fft_convolver::TwoStageFFTConvolver;
use malloc_size_of_derive::MallocSizeOf;

use crate::audio_node::{AudioNodeEngine, AudioNodeType, BlockInfo, ChannelInfo};
use crate::block::{Block, Chunk, FRAMES_PER_BLOCK_USIZE};
use crate::buffer_source_node::AudioBuffer;

/// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-normalize>: the gain applied to
/// the impulse response is calibrated against this power and sample rate.
const GAIN_CALIBRATION: f32 = 0.00125;
const GAIN_CALIBRATION_SAMPLE_RATE: f32 = 44100.;
const MIN_POWER: f32 = 0.000125;

/// Options for constructing a [`ConvolverNode`].
#[derive(Clone, Debug, Default, MallocSizeOf)]
pub struct ConvolverNodeOptions {
    pub buffer: Option<AudioBuffer>,
    pub disable_normalization: bool,
}

/// Control messages directed to ConvolverNodes.
#[derive(Clone, Debug, MallocSizeOf)]
pub enum ConvolverNodeMessage {
    SetBuffer(Option<AudioBuffer>),
    SetNormalize(bool),
}

/// Convolution with an impulse response, in partitioned FFT blocks (the `fft-convolver` crate).
///
/// <https://webaudio.github.io/web-audio-api/#ConvolverNode>
///
/// Supports one- and two-channel impulse responses: a mono response is applied to each input
/// channel, a stereo one gives a stereo output.
#[derive(AudioNodeCommon)]
pub(crate) struct ConvolverNode {
    channel_info: ChannelInfo,
    buffer: Option<AudioBuffer>,
    normalize: bool,
    /// The impulse response scaled by the normalization, one `Vec` per channel.
    response: Vec<Vec<f32>>,
    /// One convolver per output channel (each one keeps the state of its own signal).
    convolvers: Vec<TwoStageFFTConvolver<f32>>,
    /// Quanta of silence seen since the last sound; the tail of the response is over after
    /// [`Self::tail_quanta`] of them.
    silent_quanta: usize,
}

impl ConvolverNode {
    pub fn new(options: ConvolverNodeOptions, channel_info: ChannelInfo) -> Self {
        let mut node = Self {
            channel_info,
            buffer: options.buffer,
            normalize: !options.disable_normalization,
            response: Vec::new(),
            convolvers: Vec::new(),
            silent_quanta: usize::MAX,
        };
        node.rebuild();
        node
    }

    fn handle_message(&mut self, message: ConvolverNodeMessage, _sample_rate: f32) {
        match message {
            ConvolverNodeMessage::SetBuffer(buffer) => self.buffer = buffer,
            ConvolverNodeMessage::SetNormalize(normalize) => self.normalize = normalize,
        }
        self.rebuild();
    }

    /// The factor the response is scaled by when normalization is on.
    fn normalization_scale(buffer: &AudioBuffer) -> f32 {
        let channels = buffer.chans() as usize;
        let sum_of_squares: f32 = buffer
            .buffers
            .iter()
            .flat_map(|channel| channel.iter())
            .map(|sample| sample * sample)
            .sum();
        let power = (sum_of_squares / (channels * buffer.len()) as f32).sqrt();
        let power = if power.is_finite() {
            power.max(MIN_POWER)
        } else {
            MIN_POWER
        };
        let mut scale = GAIN_CALIBRATION / power;
        scale *= GAIN_CALIBRATION_SAMPLE_RATE / buffer.sample_rate;
        if channels == 4 {
            scale *= 0.5;
        }
        scale
    }

    fn rebuild(&mut self) {
        self.convolvers.clear();
        self.response.clear();
        self.silent_quanta = usize::MAX;
        let Some(buffer) = self.buffer.as_ref().filter(|buffer| !buffer.is_empty()) else {
            return;
        };
        let scale = if self.normalize {
            Self::normalization_scale(buffer)
        } else {
            1.
        };
        // Only the first two channels are used (mono and stereo responses).
        for channel in buffer.buffers.iter().take(2) {
            self.response
                .push(channel.iter().map(|sample| sample * scale).collect());
        }
    }

    /// A convolver for output channel `channel`, built the first time it is needed.
    fn convolver(&mut self, channel: usize) -> Option<&mut TwoStageFFTConvolver<f32>> {
        while self.convolvers.len() <= channel {
            let response = &self.response[self.convolvers.len().min(self.response.len() - 1)];
            let mut convolver = TwoStageFFTConvolver::default();
            convolver
                .init_default(FRAMES_PER_BLOCK_USIZE, response)
                .ok()?;
            self.convolvers.push(convolver);
        }
        self.convolvers.get_mut(channel)
    }

    /// How many silent quanta follow a sound before the response has died out.
    fn tail_quanta(&self) -> usize {
        let frames = self.response.first().map_or(0, Vec::len);
        frames / FRAMES_PER_BLOCK_USIZE + 2
    }
}

impl AudioNodeEngine for ConvolverNode {
    fn node_type(&self) -> AudioNodeType {
        AudioNodeType::ConvolverNode
    }

    fn process(&mut self, mut inputs: Chunk, _info: &BlockInfo) -> Chunk {
        debug_assert!(inputs.len() == 1);
        // Without an impulse response the output is silence.
        if self.response.is_empty() {
            inputs.blocks[0] = Block::default();
            return inputs;
        }
        if inputs.blocks[0].is_silence() {
            self.silent_quanta = self.silent_quanta.saturating_add(1);
            if self.silent_quanta > self.tail_quanta() {
                return inputs;
            }
        } else {
            self.silent_quanta = 0;
        }

        let input = &inputs.blocks[0];
        let input_channels = if input.is_silence() {
            1
        } else {
            input.chan_count() as usize
        };
        // A mono response keeps the input's channel count; a stereo response makes stereo.
        let output_channels = if self.response.len() == 1 {
            input_channels.min(2)
        } else {
            2
        };
        let silence = [0.; FRAMES_PER_BLOCK_USIZE];
        let mut output = Block::for_channels_explicit(output_channels as u8);
        for channel in 0..output_channels {
            let source: &[f32] = if input.is_silence() {
                &silence
            } else {
                input.data_chan(channel.min(input_channels - 1) as u8)
            };
            let mut samples = [0.; FRAMES_PER_BLOCK_USIZE];
            samples.copy_from_slice(source);
            let Some(convolver) = self.convolver(channel) else {
                continue;
            };
            let _ = convolver.process(&samples, output.data_chan_mut(channel as u8));
        }
        inputs.blocks[0] = output;
        inputs
    }

    make_message_handler!(ConvolverNode: handle_message);
}
