/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::net::IpAddr;
use std::rc::Rc;

use malloc_size_of::malloc_size_of_is_0;
use malloc_size_of_derive::MallocSizeOf;
use serde::{Deserialize, Serialize};
use url::{Host, Origin, Url};
use uuid::Uuid;

/// Fixed id shared by every `file://` opaque origin — see `ImmutableOrigin::
/// new_opaque_for_file`'s doc comment for why this isn't a fresh `Uuid::new_v4()`
/// like the other opaque-origin constructors in this file.
const FILE_ORIGIN_ID: Uuid = Uuid::nil();

/// Fixed id shared by every `game://` opaque origin — same reasoning as
/// `FILE_ORIGIN_ID`, see `ImmutableOrigin::new_opaque_for_game_content`'s doc
/// comment. Distinct from `FILE_ORIGIN_ID` (a `game://` document and a
/// `file://` document are never meant to compare as same-origin with each
/// other, only with themselves) — any fixed, non-nil value works; `1` is
/// arbitrary.
const GAME_ORIGIN_ID: Uuid = Uuid::from_u128(1);

/// The origin of an URL
#[derive(Clone, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub enum ImmutableOrigin {
    /// A globally unique identifier
    Opaque(OpaqueOrigin),

    /// Consists of the URL's scheme, host and port
    Tuple(String, Host, u16),
}

pub trait DomainComparable {
    fn has_domain(&self) -> bool;
    fn immutable(&self) -> &ImmutableOrigin;
}

impl DomainComparable for OriginSnapshot {
    fn has_domain(&self) -> bool {
        self.1.is_some()
    }
    fn immutable(&self) -> &ImmutableOrigin {
        &self.0
    }
}

impl DomainComparable for MutableOrigin {
    fn has_domain(&self) -> bool {
        (self.0).1.borrow().is_some()
    }
    fn immutable(&self) -> &ImmutableOrigin {
        &(self.0).0
    }
}

impl ImmutableOrigin {
    pub fn new(url: &Url) -> ImmutableOrigin {
        if url.scheme() == "file" {
            return Self::new_opaque_for_file();
        }
        if url.scheme() == "game" {
            return Self::new_opaque_for_game_content();
        }

        match url.origin() {
            Origin::Opaque(_) => ImmutableOrigin::new_opaque(),
            Origin::Tuple(scheme, host, port) => ImmutableOrigin::Tuple(scheme, host, port),
        }
    }

    pub fn same_origin(&self, other: &impl DomainComparable) -> bool {
        self == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &impl DomainComparable) -> bool {
        !other.has_domain() && self == other.immutable()
    }

    /// Creates a new opaque origin that is only equal to itself.
    pub fn new_opaque() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin {
            id: Uuid::new_v4(),
            is_for_data_worker_from_secure_context: false,
            is_file_origin: false,
            is_game_content_origin: false,
        })
    }

    /// For use in mixed security context tests because data: URL workers inherit contexts
    pub fn new_opaque_data_url_worker() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin {
            id: Uuid::new_v4(),
            is_for_data_worker_from_secure_context: true,
            is_file_origin: false,
            is_game_content_origin: false,
        })
    }

    /// Kiosk/embedded fork: unlike every other opaque-origin constructor here, this one
    /// does *not* mint a fresh random id per call — it always returns the same fixed id
    /// (see `FILE_ORIGIN_ID` below), so two `file://` URLs (even the exact same URL
    /// resolved twice) compare as same-origin instead of never matching anything, ever.
    ///
    /// Upstream minted a new `Uuid::new_v4()` here on every call, which is defensible
    /// per spec (opaque origins are meant to be globally unique) but means the Fetch
    /// same-origin check (`is_url_potentially_trustworthy`/`should_request_be_blocked_
    /// as_mixed_content` and friends) can never succeed for anything loaded from
    /// `file://` — including a page fetching its own sibling files. That specifically
    /// breaks external `<script type="module" src="...">` (the module-script spec always
    /// fetches those in CORS mode, which needs same-origin here), i.e. any normal Vite/
    /// webpack/etc. build opened via a `file://` URL — see `../TODO.md`'s "schermata
    /// bianca" writeup for the manual repro. Mainstream browsers don't have this problem
    /// in practice (they treat `file://` content as usable roughly the way a real site
    /// would be — the spec itself leaves this underspecified, see
    /// <https://github.com/whatwg/html/issues/3099>), which this change now matches for
    /// this fork's purposes: this build only ever opens one `file://` document (the
    /// game's own bundled `dist/index.html`) and never exposes navigation to any other
    /// `file://` URL (no address bar, no tabs — see the toolbar-removal patch), so
    /// treating all `file://` origins as the same origin has no realistic downside here.
    pub fn new_opaque_for_file() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin {
            id: FILE_ORIGIN_ID,
            is_for_data_worker_from_secure_context: false,
            is_file_origin: true,
            is_game_content_origin: false,
        })
    }

    /// Same reasoning and same fork-specific carve-out as `new_opaque_for_file` above —
    /// see that method's doc comment — for `game://`, the virtual-root scheme bundled
    /// game content is served under instead of a raw `file://` path (see
    /// `ports/servoshell/desktop/protocols/game.rs` and CUSTOMIZATIONS.md's "Virtual
    /// content root (game: protocol)" entry). A tuple origin was considered instead of
    /// reusing this opaque-with-a-fixed-id pattern (unlike `file://`, `game://` URLs do
    /// have a real, meaningful host component), but would have meant auditing every other
    /// origin-consuming code path in this engine that currently assumes a tuple origin
    /// only ever comes from `http(s)`/`ws(s)` — far larger a change than this fork's actual
    /// need (one first-party trusted local content origin, same as `file://` already is).
    pub fn new_opaque_for_game_content() -> ImmutableOrigin {
        ImmutableOrigin::Opaque(OpaqueOrigin {
            id: GAME_ORIGIN_ID,
            is_for_data_worker_from_secure_context: false,
            is_file_origin: false,
            is_game_content_origin: true,
        })
    }

    pub fn scheme(&self) -> Option<&str> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(ref scheme, _, _) => Some(&**scheme),
        }
    }

    pub fn host(&self) -> Option<&Host> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(_, ref host, _) => Some(host),
        }
    }

    pub fn port(&self) -> Option<u16> {
        match *self {
            ImmutableOrigin::Opaque(_) => None,
            ImmutableOrigin::Tuple(_, _, port) => Some(port),
        }
    }

    pub fn into_url_origin(self) -> Origin {
        match self {
            ImmutableOrigin::Opaque(_) => Origin::new_opaque(),
            ImmutableOrigin::Tuple(scheme, host, port) => Origin::Tuple(scheme, host, port),
        }
    }

    /// Return whether this origin is a (scheme, host, port) tuple
    /// (as opposed to an opaque origin).
    pub fn is_tuple(&self) -> bool {
        matches!(self, ImmutableOrigin::Tuple(..))
    }

    pub fn is_file_origin(&self) -> bool {
        matches!(
            self,
            ImmutableOrigin::Opaque(OpaqueOrigin {
                is_file_origin: true,
                ..
            })
        )
    }

    pub fn is_game_content_origin(&self) -> bool {
        matches!(
            self,
            ImmutableOrigin::Opaque(OpaqueOrigin {
                is_game_content_origin: true,
                ..
            })
        )
    }

    /// Kiosk/embedded fork: whether this origin is allowed to use Storage-Standard
    /// storage (`localStorage`/`sessionStorage`/`indexedDB`) despite `file://` documents
    /// being opaque origins (see `new_opaque_for_file`'s doc comment above).
    ///
    /// Per spec, opaque origins never get a storage shelf (`is_tuple()` alone gates it in
    /// upstream) — correct for e.g. `data:`/`blob:`-without-origin, which really are
    /// meant to be storage-isolated from everything, including each other. `file://` here
    /// is different: `new_opaque_for_file` already made every `file://` document compare
    /// as the *same* fixed origin (`FILE_ORIGIN_ID`) specifically because this fork only
    /// ever opens one `file://` document at a time (the game's own bundled `dist/`), with
    /// no navigation to any other origin — the same reasoning that made stabilizing the
    /// origin safe there applies just as well to storage: there is no second `file://`
    /// document around to leak into or collide with.
    ///
    /// Deliberately narrower than making `file://` a tuple origin outright (which would
    /// also affect Cookie Store, CORS/same-origin checks, mixed content, etc. — see
    /// `is_potentially_trustworthy` below for the one other spot this fork already carves
    /// out a `file://` exception, for the same reason): this only lifts the storage
    /// restriction, so everything else stays spec-strict.
    pub fn can_access_storage(&self) -> bool {
        self.is_tuple() || self.is_file_origin() || self.is_game_content_origin()
    }

    pub fn is_for_data_worker_from_secure_context(&self) -> bool {
        matches!(
            self,
            ImmutableOrigin::Opaque(OpaqueOrigin {
                is_for_data_worker_from_secure_context: true,
                ..
            })
        )
    }

    /// <https://w3c.github.io/webappsec-secure-contexts/#is-origin-trustworthy>
    pub fn is_potentially_trustworthy(&self) -> bool {
        // 1. If origin is an opaque origin return "Not Trustworthy"
        if let ImmutableOrigin::Opaque(opaque_origin) = self {
            // The webappsec spec assumes that file:// urls have a tuple origin,
            // which is implementation defined.
            // See <https://github.com/w3c/webappsec-secure-contexts/issues/66>.
            //
            // They're not tuple origins in our implementation (which is the more correct choice),
            // so we have to return here instead of Step 6.
            if opaque_origin.is_file_origin || opaque_origin.is_game_content_origin {
                return true;
            }
            return false;
        }

        if let ImmutableOrigin::Tuple(scheme, host, _) = self {
            // 3. If origin’s scheme is either "https" or "wss", return "Potentially Trustworthy"
            if scheme == "https" || scheme == "wss" {
                return true;
            }

            // 6. If origin’s scheme is "file", return "Potentially Trustworthy".
            // NOTE: The comment at Step 1 explains why this is unreachable here.
            debug_assert_ne!(scheme, "file", "File URLs don't have a tuple origin");

            // 4. If origin’s host matches one of the CIDR notations 127.0.0.0/8 or ::1/128,
            // return "Potentially Trustworthy".
            if let Ok(ip_addr) = host.to_string().parse::<IpAddr>() {
                return ip_addr.is_loopback();
            }
            // 5. If the user agent conforms to the name resolution rules in
            // [let-localhost-be-localhost] and one of the following is true:
            // * origin’s host is "localhost" or "localhost."
            // * origin’s host ends with ".localhost" or ".localhost."
            // then return "Potentially Trustworthy".
            if let Host::Domain(domain) = host &&
                (domain == "localhost" || domain.ends_with(".localhost"))
            {
                return true;
            }
        }

        // 9. Return "Not Trustworthy".
        false
    }

    /// <https://html.spec.whatwg.org/multipage/#ascii-serialisation-of-an-origin>
    pub fn ascii_serialization(&self) -> String {
        self.clone().into_url_origin().ascii_serialization()
    }
}

/// Opaque identifier for URLs that have file or other schemes
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct OpaqueOrigin {
    id: Uuid,
    /// Workers created from `data:` urls will have opaque origins but need to be treated
    /// as inheriting the secure context they were created in. This tracks that the origin
    /// was created in such a context
    is_for_data_worker_from_secure_context: bool,
    /// `file://` URLs are *usually* treated as opaque, but not always. This flag serves
    /// as an indicator that they need special handling in certain cases.
    ///
    /// See <https://github.com/whatwg/html/issues/3099>.
    is_file_origin: bool,
    /// Same idea as `is_file_origin`, for this fork's `game://` virtual-content-root
    /// scheme — see `ImmutableOrigin::new_opaque_for_game_content`'s doc comment.
    is_game_content_origin: bool,
}

malloc_size_of_is_0!(OpaqueOrigin);

/// A snapshot of a MutableOrigin at a moment in time.
#[derive(Clone, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub struct OriginSnapshot(ImmutableOrigin, Option<Host>);

impl OriginSnapshot {
    pub fn immutable(&self) -> &ImmutableOrigin {
        &self.0
    }

    pub fn has_domain(&self) -> bool {
        self.1.is_some()
    }

    pub fn same_origin(&self, other: &impl DomainComparable) -> bool {
        self.immutable() == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &OriginSnapshot) -> bool {
        if let Some(ref self_domain) = self.1 {
            if let Some(ref other_domain) = other.1 {
                self_domain == other_domain && self.0.scheme() == other.0.scheme()
            } else {
                false
            }
        } else {
            self.0.same_origin_domain(other)
        }
    }
}

/// A representation of an [origin](https://html.spec.whatwg.org/multipage/#origin-2).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MutableOrigin(Rc<(ImmutableOrigin, RefCell<Option<Host>>)>);

malloc_size_of_is_0!(MutableOrigin);

impl MutableOrigin {
    pub fn from_snapshot(snapshot: OriginSnapshot) -> MutableOrigin {
        MutableOrigin(Rc::new((snapshot.0, RefCell::new(snapshot.1))))
    }

    pub fn snapshot(&self) -> OriginSnapshot {
        OriginSnapshot(self.0.0.clone(), self.0.1.borrow().clone())
    }

    pub fn new(origin: ImmutableOrigin) -> MutableOrigin {
        MutableOrigin(Rc::new((origin, RefCell::new(None))))
    }

    pub fn immutable(&self) -> &ImmutableOrigin {
        &(self.0).0
    }

    pub fn is_tuple(&self) -> bool {
        self.immutable().is_tuple()
    }

    /// See `ImmutableOrigin::can_access_storage`'s doc comment.
    pub fn can_access_storage(&self) -> bool {
        self.immutable().can_access_storage()
    }

    pub fn scheme(&self) -> Option<&str> {
        self.immutable().scheme()
    }

    pub fn host(&self) -> Option<&Host> {
        self.immutable().host()
    }

    pub fn port(&self) -> Option<u16> {
        self.immutable().port()
    }

    pub fn same_origin(&self, other: &MutableOrigin) -> bool {
        self.immutable() == other.immutable()
    }

    pub fn same_origin_domain(&self, other: &MutableOrigin) -> bool {
        if let Some(ref self_domain) = *(self.0).1.borrow() {
            if let Some(ref other_domain) = *(other.0).1.borrow() {
                self_domain == other_domain &&
                    self.immutable().scheme() == other.immutable().scheme()
            } else {
                false
            }
        } else {
            self.immutable().same_origin_domain(other)
        }
    }

    pub fn domain(&self) -> Option<Host> {
        (self.0).1.borrow().clone()
    }

    pub fn set_domain(&self, domain: Host) {
        *(self.0).1.borrow_mut() = Some(domain);
    }

    pub fn has_domain(&self) -> bool {
        (self.0).1.borrow().is_some()
    }

    /// <https://html.spec.whatwg.org/multipage/#concept-origin-effective-domain>
    pub fn effective_domain(&self) -> Option<Host> {
        // Step 1. If origin is an opaque origin, then return null.
        if !self.is_tuple() {
            return None;
        }
        self.immutable()
            .host()
            // Step 2. If origin's domain is non-null, then return origin's domain.
            // Step 3. Return origin's host.
            .map(|host| self.domain().unwrap_or_else(|| host.clone()))
    }
}
