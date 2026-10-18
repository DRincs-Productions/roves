/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use malloc_size_of_derive::MallocSizeOf;

use crate::audio_node::{AudioNodeEngine, AudioNodeType, BlockInfo, ChannelInfo};
use crate::block::{Chunk, FRAMES_PER_BLOCK_USIZE, Tick};
use crate::param::{Param, ParamType};

/// Options for constructing a [`DynamicsCompressorNode`].
#[derive(Copy, Clone, Debug, MallocSizeOf)]
pub struct DynamicsCompressorNodeOptions {
    pub threshold: f32,
    pub knee: f32,
    pub ratio: f32,
    pub attack: f32,
    pub release: f32,
}

impl Default for DynamicsCompressorNodeOptions {
    fn default() -> Self {
        DynamicsCompressorNodeOptions {
            threshold: -24.,
            knee: 30.,
            ratio: 12.,
            attack: 0.003,
            release: 0.25,
        }
    }
}

/// A feed-forward compressor: a soft-knee gain curve on the loudest channel, smoothed with
/// attack and release times, plus the automatic make-up gain browsers apply.
///
/// <https://webaudio.github.io/web-audio-api/#DynamicsCompressorNode>
///
/// Unlike browsers there is no look-ahead delay, so a very sharp transient can pass before the
/// attack has pulled the gain down.
#[derive(AudioNodeCommon)]
pub(crate) struct DynamicsCompressorNode {
    channel_info: ChannelInfo,
    threshold: Param,
    knee: Param,
    ratio: Param,
    attack: Param,
    release: Param,
    /// The current gain reduction, in dB (zero or negative).
    reduction_db: f32,
}

/// The output level, in dB, for an input level `input_db` (<https://webaudio.github.io/web-audio-api/#compression-curve>).
fn curve_db(input_db: f32, threshold: f32, knee: f32, ratio: f32) -> f32 {
    let over = input_db - threshold;
    if 2. * over < -knee {
        input_db
    } else if 2. * over.abs() <= knee {
        // Inside the soft knee: a quadratic that joins the two straight parts.
        let t = over + knee / 2.;
        input_db + (1. / ratio - 1.) * t * t / (2. * knee)
    } else {
        threshold + over / ratio
    }
}

impl DynamicsCompressorNode {
    pub fn new(options: DynamicsCompressorNodeOptions, channel_info: ChannelInfo) -> Self {
        Self {
            channel_info,
            threshold: Param::new_krate(options.threshold),
            knee: Param::new_krate(options.knee),
            ratio: Param::new_krate(options.ratio),
            attack: Param::new_krate(options.attack),
            release: Param::new_krate(options.release),
            reduction_db: 0.,
        }
    }

    /// The smoothing coefficient for a time constant of `seconds`.
    fn coefficient(seconds: f32, sample_rate: f32) -> f32 {
        if seconds <= 0. {
            0.
        } else {
            (-1. / (seconds * sample_rate)).exp()
        }
    }
}

impl AudioNodeEngine for DynamicsCompressorNode {
    fn node_type(&self) -> AudioNodeType {
        AudioNodeType::DynamicsCompressionNode
    }

    fn process(&mut self, mut inputs: Chunk, info: &BlockInfo) -> Chunk {
        debug_assert!(inputs.len() == 1);
        for param in [
            &mut self.threshold,
            &mut self.knee,
            &mut self.ratio,
            &mut self.attack,
            &mut self.release,
        ] {
            param.update(info, Tick(0));
        }
        let threshold = self.threshold.value().clamp(-100., 0.);
        let knee = self.knee.value().clamp(0., 40.);
        let ratio = self.ratio.value().clamp(1., 20.);
        let attack = Self::coefficient(self.attack.value(), info.sample_rate);
        let release = Self::coefficient(self.release.value(), info.sample_rate);
        // Browsers lift the output so that a full-scale input comes out near full scale.
        let full_range_db = curve_db(0., threshold, knee, ratio);
        let makeup = 10_f32.powf(-full_range_db * 0.6 / 20.);

        let block = &mut inputs.blocks[0];
        if block.is_silence() && self.reduction_db > -0.01 {
            self.reduction_db = 0.;
            return inputs;
        }
        let channels = block.chan_count();
        let mut level = [0.; FRAMES_PER_BLOCK_USIZE];
        for channel in 0..channels {
            for (peak, sample) in level.iter_mut().zip(block.data_chan(channel)) {
                *peak = f32::max(*peak, sample.abs());
            }
        }
        let mut gains = [1.; FRAMES_PER_BLOCK_USIZE];
        for (gain, peak) in gains.iter_mut().zip(level) {
            let input_db = 20. * peak.max(1e-6).log10();
            let target = curve_db(input_db, threshold, knee, ratio) - input_db;
            // More reduction follows the attack time, less follows the release time.
            let coefficient = if target < self.reduction_db {
                attack
            } else {
                release
            };
            self.reduction_db = coefficient * self.reduction_db + (1. - coefficient) * target;
            *gain = 10_f32.powf(self.reduction_db / 20.) * makeup;
        }
        for channel in 0..channels {
            for (sample, gain) in block.data_chan_mut(channel).iter_mut().zip(gains) {
                *sample *= gain;
            }
        }
        inputs
    }

    fn get_param(&mut self, id: ParamType) -> &mut Param {
        match id {
            ParamType::Threshold => &mut self.threshold,
            ParamType::Knee => &mut self.knee,
            ParamType::Ratio => &mut self.ratio,
            ParamType::Attack => &mut self.attack,
            ParamType::Release => &mut self.release,
            _ => panic!("Unknown param {:?} for DynamicsCompressorNode", id),
        }
    }
}
