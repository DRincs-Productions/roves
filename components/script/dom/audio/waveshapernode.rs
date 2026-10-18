/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use dom_struct::dom_struct;
use js::context::JSContext;
use js::rust::{CustomAutoRooterGuard, HandleObject};
use js::typedarray::{Float32, Float32Array, HeapFloat32Array};
use script_bindings::cell::DomRefCell;
use script_bindings::reflector::reflect_dom_object_with_proto;
use script_bindings::trace::RootedTraceableBox;
use servo_media::audio::audio_node::{AudioNodeInit, AudioNodeMessage};
use servo_media::audio::wave_shaper_node::{
    OverSampleType as ServoMediaOverSampleType, WaveShaperNodeMessage, WaveShaperNodeOptions,
};

use crate::conversions::Convert;
use crate::dom::audio::audionode::{AudioNode, AudioNodeOptionsHelper};
use crate::dom::audio::baseaudiocontext::BaseAudioContext;
use crate::dom::bindings::buffer_source::HeapBufferSource;
use crate::dom::bindings::codegen::Bindings::AudioNodeBinding::{
    ChannelCountMode, ChannelInterpretation,
};
use crate::dom::bindings::codegen::Bindings::WaveShaperNodeBinding::{
    OverSampleType, WaveShaperNodeMethods, WaveShaperOptions,
};
use crate::dom::bindings::error::{Error, ErrorResult, Fallible};
use crate::dom::bindings::root::DomRoot;
use crate::dom::window::Window;

#[dom_struct]
pub(crate) struct WaveShaperNode {
    node: AudioNode,
    /// The curve as the page set it, returned unchanged by the getter.
    #[ignore_malloc_size_of = "mozjs"]
    curve: DomRefCell<HeapBufferSource<Float32>>,
    oversample: Cell<OverSampleType>,
}

impl WaveShaperNode {
    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    pub(crate) fn new_inherited(
        cx: &mut JSContext,
        context: &BaseAudioContext,
        options: &WaveShaperOptions,
    ) -> Fallible<WaveShaperNode> {
        let node_options =
            options
                .parent
                .unwrap_or(2, ChannelCountMode::Max, ChannelInterpretation::Speakers);
        let curve_data: Option<Vec<f32>> = options
            .curve
            .as_ref()
            .map(|curve| curve.iter().map(|value| **value).collect());
        if curve_data.as_ref().is_some_and(|curve| curve.len() < 2) {
            return Err(Error::InvalidState(None));
        }
        let node = AudioNode::new_inherited(
            cx,
            AudioNodeInit::WaveShaperNode(WaveShaperNodeOptions {
                curve: curve_data.clone(),
                oversample: options.oversample.convert(),
            }),
            context,
            node_options,
            1, // inputs
            1, // outputs
        )?;
        let curve = HeapBufferSource::default();
        if let Some(data) = curve_data {
            curve.set_data(cx, &data).map_err(|_| Error::JSFailed)?;
        }
        Ok(WaveShaperNode {
            node,
            curve: DomRefCell::new(curve),
            oversample: Cell::new(options.oversample),
        })
    }

    pub(crate) fn new(
        cx: &mut JSContext,
        window: &Window,
        context: &BaseAudioContext,
        options: &WaveShaperOptions,
    ) -> Fallible<DomRoot<WaveShaperNode>> {
        Self::new_with_proto(cx, window, None, context, options)
    }

    #[cfg_attr(crown, expect(crown::unrooted_must_root))]
    fn new_with_proto(
        cx: &mut JSContext,
        window: &Window,
        proto: Option<HandleObject>,
        context: &BaseAudioContext,
        options: &WaveShaperOptions,
    ) -> Fallible<DomRoot<WaveShaperNode>> {
        let node = WaveShaperNode::new_inherited(cx, context, options)?;
        Ok(reflect_dom_object_with_proto(
            cx,
            Box::new(node),
            window,
            proto,
        ))
    }
}

impl WaveShaperNodeMethods<crate::DomTypeHolder> for WaveShaperNode {
    /// <https://webaudio.github.io/web-audio-api/#dom-waveshapernode-waveshapernode>
    fn Constructor(
        cx: &mut JSContext,
        window: &Window,
        proto: Option<HandleObject>,
        context: &BaseAudioContext,
        options: &WaveShaperOptions,
    ) -> Fallible<DomRoot<WaveShaperNode>> {
        WaveShaperNode::new_with_proto(cx, window, proto, context, options)
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-waveshapernode-curve>
    fn GetCurve(&self) -> Option<RootedTraceableBox<HeapFloat32Array>> {
        let curve = self.curve.borrow();
        if curve.is_initialized() {
            curve.get_typed_array().ok()
        } else {
            None
        }
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-waveshapernode-curve>
    fn SetCurve(
        &self,
        cx: &mut JSContext,
        value: Option<CustomAutoRooterGuard<Float32Array>>,
    ) -> ErrorResult {
        let data = match &value {
            Some(array) => {
                let data = array.to_vec().ok_or(Error::JSFailed)?;
                if data.len() < 2 {
                    return Err(Error::InvalidState(None));
                }
                Some(data)
            },
            None => None,
        };
        let stored = match &data {
            Some(data) => {
                let new_curve = HeapBufferSource::default();
                new_curve.set_data(cx, data).map_err(|_| Error::JSFailed)?;
                new_curve
            },
            None => HeapBufferSource::default(),
        };
        *self.curve.borrow_mut() = stored;
        self.node.message(AudioNodeMessage::WaveShaperNode(
            WaveShaperNodeMessage::SetCurve(data),
        ));
        Ok(())
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-waveshapernode-oversample>
    fn Oversample(&self) -> OverSampleType {
        self.oversample.get()
    }

    /// <https://webaudio.github.io/web-audio-api/#dom-waveshapernode-oversample>
    fn SetOversample(&self, oversample: OverSampleType) {
        self.oversample.set(oversample);
        self.node.message(AudioNodeMessage::WaveShaperNode(
            WaveShaperNodeMessage::SetOversample(oversample.convert()),
        ));
    }
}

impl Convert<ServoMediaOverSampleType> for OverSampleType {
    fn convert(self) -> ServoMediaOverSampleType {
        match self {
            OverSampleType::None => ServoMediaOverSampleType::None,
            OverSampleType::_2x => ServoMediaOverSampleType::Double,
            OverSampleType::_4x => ServoMediaOverSampleType::Quadruple,
        }
    }
}
