/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use crate::audio_node::{AudioNodeEngine, AudioNodeMessage, AudioNodeType, BlockInfo, ChannelInfo};
use crate::block::Chunk;

/// What remains of a node the graph no longer needs (see `AudioGraph::dispose_node`): it keeps
/// the node's id valid, so a late message or connection cannot reach another node, but holds no
/// audio data and always produces silence.
#[derive(AudioNodeCommon)]
pub(crate) struct DeadNode {
    channel_info: ChannelInfo,
}

impl DeadNode {
    pub(crate) fn new() -> Self {
        Self {
            channel_info: ChannelInfo::default(),
        }
    }
}

impl AudioNodeEngine for DeadNode {
    fn node_type(&self) -> AudioNodeType {
        AudioNodeType::GainNode
    }

    fn process(&mut self, _inputs: Chunk, _info: &BlockInfo) -> Chunk {
        self.mute_node()
    }

    fn message(&mut self, msg: AudioNodeMessage, _sample_rate: f32) {
        // A parameter read still gets an answer so the caller does not wait for one.
        if let AudioNodeMessage::GetParamValue(_, tx) = msg {
            let _ = tx.send(0.);
        }
    }
}
