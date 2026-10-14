/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::thread::JoinHandle;

use log::debug;
use paint_api::{CrossProcessPaintApi, PainterSurfmanDetailsMap, WebRenderExternalImageIdManager};
use servo_canvas_traits::webgl::{WebGLContextId, WebGLMsg, WebGLThreads, webgl_channel};
use surfman::Device;
use surfman::chains::SwapChains;

use crate::webgl_thread::{WebGLContextBusyMap, WebGLThread, WebGLThreadInit};

pub struct WebGLComm {
    pub webgl_threads: WebGLThreads,
    pub swap_chains: SwapChains<WebGLContextId, Device>,
    pub busy_webgl_context_map: WebGLContextBusyMap,
    /// A `JoinHandle` which can be joined after sending the `WebGLMsg::Exit`
    /// message to the `WebGLThread`.
    pub join_handle: JoinHandle<()>,
}

impl WebGLComm {
    /// Creates a new `WebGLComm` object.
    pub fn new(
        paint_api: CrossProcessPaintApi,
        external_image_id_manager: WebRenderExternalImageIdManager,
        painter_surfman_details_map: PainterSurfmanDetailsMap,
    ) -> WebGLComm {
        debug!("WebGLThreads::new()");
        let (sender, receiver) = webgl_channel::<WebGLMsg>().unwrap();
        let swap_chains = SwapChains::new();
        let busy_webgl_context_map = WebGLContextBusyMap::default();

        // This implementation creates a single `WebGLThread` for all the pipelines.
        let init = WebGLThreadInit {
            paint_api,
            external_image_id_manager,
            sender: sender.clone(),
            receiver,
            webrender_swap_chains: swap_chains.clone(),
            painter_surfman_details_map,
            busy_webgl_context_map: busy_webgl_context_map.clone(),
        };

        let join_handle = WebGLThread::run_on_own_thread(init);

        WebGLComm {
            webgl_threads: WebGLThreads(sender),
            swap_chains,
            busy_webgl_context_map,
            join_handle,
        }
    }
}
