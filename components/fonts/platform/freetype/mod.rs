/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

pub mod font;
mod freetype_face;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod font_list;

#[cfg(target_os = "android")]
mod android {
    pub mod font_list;
    mod xml;
}
#[cfg(target_os = "android")]
pub use self::android::font_list;

mod library_handle;
