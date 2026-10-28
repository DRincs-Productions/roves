/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Product policy shared by script-side preflight and constellation-side defenses.
use std::sync::Once;

pub use embedder_traits::GameDocumentOperation;

/// Reports each unsupported category once per process, without retaining URLs or globals.
pub fn report_blocked_document_operation(operation: GameDocumentOperation) -> bool {
    static WARNINGS: [Once; 4] = [Once::new(), Once::new(), Once::new(), Once::new()];
    let (index, name) = match operation {
        GameDocumentOperation::Navigation => (0, "full-document navigation"),
        GameDocumentOperation::Reload => (1, "reload"),
        GameDocumentOperation::Frame => (2, "iframe document"),
        GameDocumentOperation::HistoryDocument => (3, "history document replacement"),
    };
    let mut first = false;
    WARNINGS[index].call_once(|| {
        first = true;
        log::warn!("Roves blocked {name}: games keep one document; use same-document History/hash routes");
    });
    first
}
