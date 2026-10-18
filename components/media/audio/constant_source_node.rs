/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use malloc_size_of_derive::MallocSizeOf;

use crate::audio_node::{
    AudioNodeEngine, AudioNodeType, AudioScheduledSourceNodeMessage, BlockInfo, ChannelInfo,
    OnEndedCallback, ShouldPlay,
};
use crate::block::{Chunk, Tick};
use crate::param::{Param, ParamType};

#[derive(Copy, Clone, Debug, MallocSizeOf)]
pub struct ConstantSourceNodeOptions {
    pub offset: f32,
}

impl Default for ConstantSourceNodeOptions {
    fn default() -> Self {
        ConstantSourceNodeOptions { offset: 1. }
    }
}

#[derive(AudioScheduledSourceNode, AudioNodeCommon)]
pub(crate) struct ConstantSourceNode {
    channel_info: ChannelInfo,
    offset: Param,
    start_at: Option<Tick>,
    stop_at: Option<Tick>,
    onended_callback: Option<OnEndedCallback>,
    /// Set once the source has finished playing (see `is_finished`).
    ended: bool,
}

impl ConstantSourceNode {
    pub fn new(options: ConstantSourceNodeOptions, channel_info: ChannelInfo) -> Self {
        Self {
            channel_info,
            offset: Param::new(options.offset),
            start_at: None,
            stop_at: None,
            onended_callback: None,
            ended: false,
        }
    }

    pub fn update_parameters(&mut self, info: &BlockInfo, tick: Tick) -> bool {
        self.offset.update(info, tick)
    }
}

impl AudioNodeEngine for ConstantSourceNode {
    fn is_finished(&self) -> bool {
        self.ended
    }

    fn node_type(&self) -> AudioNodeType {
        AudioNodeType::ConstantSourceNode
    }

    fn process(&mut self, mut inputs: Chunk, info: &BlockInfo) -> Chunk {
        debug_assert!(inputs.is_empty());

        inputs.blocks.push(Default::default());

        let (start_at, stop_at) = match self.should_play_at(info.frame) {
            ShouldPlay::No => {
                return inputs;
            },
            ShouldPlay::Between(start, end) => (start, end),
        };

        // A source that plays for the whole quantum with an offset nothing is changing produces a
        // flat block: write it in one go, or leave it silent when the offset is zero so that the
        // nodes after it can skip their work too.
        let whole_quantum = start_at == Tick(0) && stop_at >= Tick::FRAMES_PER_BLOCK;
        if whole_quantum {
            self.update_parameters(info, Tick(0));
            if self.offset.is_constant() {
                let offset = self.offset.value();
                if offset != 0. {
                    let block = &mut inputs.blocks[0];
                    block.explicit_silence();
                    block.data_mut().fill(offset);
                }
                return inputs;
            }
        }

        {
            inputs.blocks[0].explicit_silence();

            let mut iter = inputs.blocks[0].iter();
            let mut offset = self.offset.value();
            while let Some(mut frame) = iter.next() {
                let tick = frame.tick();
                if tick < start_at {
                    continue;
                } else if tick > stop_at {
                    break;
                }
                // Tick 0 was handled above when the source plays for the whole quantum.
                if !(whole_quantum && tick == Tick(0)) && self.update_parameters(info, tick) {
                    offset = self.offset.value();
                }
                frame.mutate_with(|sample, _| *sample = offset);
            }
        }
        inputs
    }
    fn input_count(&self) -> u32 {
        0
    }

    fn get_param(&mut self, id: ParamType) -> &mut Param {
        match id {
            ParamType::Offset => &mut self.offset,
            _ => panic!("Unknown param {:?} for the offset", id),
        }
    }
    make_message_handler!(AudioScheduledSourceNode: handle_source_node_message);
}
