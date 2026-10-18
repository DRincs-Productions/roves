/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use js::context::JSContext;
use js::rust::HandleObject;
use script_bindings::reflector::reflect_dom_object_with_proto;
use servo_media::audio::audio_node::{AudioNodeInit, AudioNodeMessage};
use servo_media::audio::buffer_source_node::AudioBuffer as ServoMediaAudioBuffer;
use servo_media::audio::convolver_node::{ConvolverNodeMessage, ConvolverNodeOptions};

use crate::dom::audio::audiobuffer::AudioBuffer;
use crate::dom::audio::audionode::{AudioNode, AudioNodeOptionsHelper};
use crate::dom::audio::baseaudiocontext::BaseAudioContext;
use crate::dom::bindings::codegen::Bindings::AudioNodeBinding::{
    ChannelCountMode, ChannelInterpretation,
};
use crate::dom::bindings::codegen::Bindings::BaseAudioContextBinding::BaseAudioContextMethods;
use crate::dom::bindings::codegen::Bindings::ConvolverNodeBinding::{
    ConvolverNodeMethods, ConvolverOptions,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::root::{DomRoot, MutNullableDom};
use crate::dom::window::Window;

#[dom_struct]
pub(crate) struct ConvolverNode {
    node: AudioNode,
    buffer: MutNullableDom<AudioBuffer>,
    normalize: Cell<bool>,
}

/// The samples of an impulse response, if the context can use them: the same sample rate as the
/// context and one or two channels.
///
/// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-buffer>
fn response_channels(
    cx: &mut JSContext,
    context: &BaseAudioContext,
    buffer: &AudioBuffer,
) -> Fallible<ServoMediaAudioBuffer> {
    let channels = buffer
        .get_channels(cx)
        .clone()
        .ok_or(Error::InvalidState(None))?;
    if channels.sample_rate != *context.SampleRate() || !(1..=2).contains(&channels.chans()) {
        return Err(Error::NotSupported(None));
    }
    Ok(channels)
}

impl ConvolverNode {
    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    pub(crate) fn new_inherited(
        cx: &mut JSContext,
        context: &BaseAudioContext,
        options: &ConvolverOptions,
    ) -> Fallible<ConvolverNode> {
        let node_options = options.parent.unwrap_or(
            2,
            ChannelCountMode::Clamped_max,
            ChannelInterpretation::Speakers,
        );
        let buffer = options.buffer.as_ref().and_then(|buffer| buffer.as_ref());
        let response = match buffer {
            Some(buffer) => Some(response_channels(cx, context, buffer)?),
            None => None,
        };
        let node = AudioNode::new_inherited(
            cx,
            AudioNodeInit::ConvolverNode(ConvolverNodeOptions {
                buffer: response,
                disable_normalization: options.disableNormalization,
            }),
            context,
            node_options,
            1, // inputs
            1, // outputs
        )?;
        let result = ConvolverNode {
            node,
            buffer: Default::default(),
            normalize: Cell::new(!options.disableNormalization),
        };
        result.buffer.set(buffer.map(|buffer| &**buffer));
        Ok(result)
    }

    pub(crate) fn new(
        cx: &mut JSContext,
        window: &Window,
        context: &BaseAudioContext,
        options: &ConvolverOptions,
    ) -> Fallible<DomRoot<ConvolverNode>> {
        Self::new_with_proto(cx, window, None, context, options)
    }

    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    fn new_with_proto(
        cx: &mut JSContext,
        window: &Window,
        proto: Option<HandleObject>,
        context: &BaseAudioContext,
        options: &ConvolverOptions,
    ) -> Fallible<DomRoot<ConvolverNode>> {
        let node = ConvolverNode::new_inherited(cx, context, options)?;
        Ok(reflect_dom_object_with_proto(
            cx,
            Box::new(node),
            window,
            proto,
        ))
    }
}

impl ConvolverNodeMethods<crate::DomTypeHolder> for ConvolverNode {
    /// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-convolvernode>
    fn Constructor(
        cx: &mut JSContext,
        window: &Window,
        proto: Option<HandleObject>,
        context: &BaseAudioContext,
        options: &ConvolverOptions,
    ) -> Fallible<DomRoot<ConvolverNode>> {
        ConvolverNode::new_with_proto(cx, window, proto, context, options)
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-buffer>
    fn GetBuffer(&self) -> Option<DomRoot<AudioBuffer>> {
        self.buffer.get()
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-buffer>
    fn SetBuffer(&self, cx: &mut JSContext, new_buffer: Option<&AudioBuffer>) -> ErrorResult {
        let response = match new_buffer {
            Some(buffer) => Some(response_channels(cx, &self.node.context(), buffer)?),
            None => None,
        };
        self.buffer.set(new_buffer);
        self.node.message(AudioNodeMessage::ConvolverNode(
            ConvolverNodeMessage::SetBuffer(response),
        ));
        Ok(())
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-normalize>
    fn Normalize(&self) -> bool {
        self.normalize.get()
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-convolvernode-normalize>
    fn SetNormalize(&self, normalize: bool) {
        self.normalize.set(normalize);
        self.node.message(AudioNodeMessage::ConvolverNode(
            ConvolverNodeMessage::SetNormalize(normalize),
        ));
    }
}
