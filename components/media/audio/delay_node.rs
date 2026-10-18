/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use malloc_size_of_derive::MallocSizeOf;

use crate::audio_node::{AudioNodeEngine, AudioNodeType, BlockInfo, ChannelInfo};
use crate::block::{Block, Chunk, FRAMES_PER_BLOCK_USIZE, Tick};
use crate::param::{Param, ParamType};

/// Options for constructing a [`DelayNode`].
#[derive(Copy, Clone, Debug, MallocSizeOf)]
pub struct DelayNodeOptions {
    /// The largest delay, in seconds, that `delayTime` can reach.
    pub max_delay_time: f64,
    /// The initial `delayTime`, in seconds.
    pub delay_time: f32,
}

impl Default for DelayNodeOptions {
    fn default() -> Self {
        DelayNodeOptions {
            max_delay_time: 1.,
            delay_time: 0.,
        }
    }
}

/// The delay line: a ring buffer per channel, read with linear interpolation.
///
/// <https://webaudio.github.io/web-audio-api/#DelayNode>
///
/// In a feedback loop the graph makes the node a *cycle breaker* (see
/// [`AudioNodeEngine::set_cycle_breaker`]): its output for a render quantum is read before any
/// node that feeds it has run, and its input is written once the quantum is done, so the delay
/// can never be shorter than one render quantum there.
#[derive(AudioNodeCommon)]
pub(crate) struct DelayNode {
    channel_info: ChannelInfo,
    delay_time: Param,
    max_delay_time: f64,
    /// One ring buffer per channel; all the same length.
    ring: Vec<Vec<f32>>,
    /// Frames written so far (the ring's write position is this modulo its length).
    frames_written: u64,
    sample_rate: f32,
    cycle_breaker: bool,
}

impl DelayNode {
    pub fn new(options: DelayNodeOptions, channel_info: ChannelInfo) -> Self {
        let max = options.max_delay_time.max(0.);
        Self {
            channel_info,
            delay_time: Param::new(options.delay_time.clamp(0., max as f32)),
            max_delay_time: max,
            ring: Vec::new(),
            frames_written: 0,
            sample_rate: 0.,
            cycle_breaker: false,
        }
    }

    fn ring_len(&self) -> usize {
        self.ring.first().map_or(0, Vec::len)
    }

    /// Make the ring able to hold `channels` channels and the longest delay.
    fn prepare(&mut self, channels: usize, sample_rate: f32) {
        if self.sample_rate != sample_rate {
            self.sample_rate = sample_rate;
            self.ring.clear();
        }
        let len = (self.max_delay_time * sample_rate as f64).ceil() as usize +
            FRAMES_PER_BLOCK_USIZE +
            2;
        while self.ring.len() < channels {
            self.ring.push(vec![0.; len]);
        }
    }

    /// Append one render quantum of input (`None` is silence) to the delay line.
    fn write(&mut self, input: Option<&Block>, info: &BlockInfo) {
        let channels = input.map_or(self.ring.len().max(1), |block| block.chan_count() as usize);
        self.prepare(channels, info.sample_rate);
        let len = self.ring_len();
        for (channel, ring) in self.ring.iter_mut().enumerate() {
            for frame in 0..FRAMES_PER_BLOCK_USIZE {
                let index = (self.frames_written as usize + frame) % len;
                ring[index] = match input {
                    Some(block) if channel < block.chan_count() as usize => {
                        block.data_chan_frame(frame, channel as u8)
                    },
                    _ => 0.,
                };
            }
        }
        self.frames_written += FRAMES_PER_BLOCK_USIZE as u64;
    }

    /// Read one render quantum of output.
    fn read(&mut self, info: &BlockInfo) -> Block {
        if self.ring.is_empty() {
            return Block::default();
        }
        let len = self.ring_len() as i64;
        let written = self.frames_written as f64;
        let max_frames = (self.max_delay_time * info.sample_rate as f64).max(0.);
        // Inside a feedback loop this quantum's input is not written yet: the delay cannot be
        // shorter than the quantum. Otherwise the input was just written and ends at `written`.
        let (start, min_frames) = if self.cycle_breaker {
            (written, FRAMES_PER_BLOCK_USIZE as f64)
        } else {
            (written - FRAMES_PER_BLOCK_USIZE as f64, 0.)
        };
        let mut block = Block::for_channels_explicit(self.ring.len() as u8);
        for frame in 0..FRAMES_PER_BLOCK_USIZE {
            self.delay_time.update(info, Tick(frame as u64));
            let delay = (self.delay_time.value() as f64 * info.sample_rate as f64)
                .clamp(0., max_frames)
                .max(min_frames);
            let position = start + frame as f64 - delay;
            let base = position.floor();
            let fraction = (position - base) as f32;
            // Before the first frame the line holds silence.
            let sample = |ring: &[f32], index: f64| {
                if index < 0. {
                    0.
                } else {
                    ring[(index as i64).rem_euclid(len) as usize]
                }
            };
            for (channel, ring) in self.ring.iter().enumerate() {
                let first = sample(ring, base);
                let second = if base + 1. < written {
                    sample(ring, base + 1.)
                } else {
                    first
                };
                block.data_chan_mut(channel as u8)[frame] = first + (second - first) * fraction;
            }
        }
        block
    }
}

impl AudioNodeEngine for DelayNode {
    fn node_type(&self) -> AudioNodeType {
        AudioNodeType::DelayNode
    }

    fn process(&mut self, mut inputs: Chunk, info: &BlockInfo) -> Chunk {
        debug_assert!(inputs.len() == 1);
        if !self.cycle_breaker {
            let input = &inputs.blocks[0];
            self.write((!input.is_silence()).then_some(input), info);
        }
        inputs.blocks[0] = self.read(info);
        inputs
    }

    fn set_cycle_breaker(&mut self, breaker: bool) {
        self.cycle_breaker = breaker;
    }

    fn write_after_quantum(&mut self, inputs: Chunk, info: &BlockInfo) {
        let input = inputs.blocks.first().filter(|block| !block.is_silence());
        self.write(input, info);
    }

    fn get_param(&mut self, id: ParamType) -> &mut Param {
        match id {
            ParamType::DelayTime => &mut self.delay_time,
            _ => panic!("Unknown param {:?} for DelayNode", id),
        }
    }
}
