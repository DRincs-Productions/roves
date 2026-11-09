//! Network policy for the probe's remote `fetch` (src/fetch.js, deno_fetch 0.284.0).
//!
//! Games may contact any *safe* origin without an allow-list. Safe means:
//! - `https:` only, no credentials in the URL;
//! - the destination is a public IP address. IP literals are checked here
//!   ([`check_request_url`], from JS before every hop and again in deno_fetch's
//!   `request_builder_hook` right before the request is sent); host names are
//!   checked by [`PublicResolver`] on the addresses the connection will actually
//!   use (hyper connects to exactly the addresses the resolver returns, so a DNS
//!   answer that later flips to a private address — DNS rebinding — is checked
//!   on every new connection, not only at the first lookup);
//! - the document's Content-Security-Policy (`<meta http-equiv>` in `<head>`)
//!   allows it as `connect-src`, checked with the `content-security-policy`
//!   crate Roves' production engine uses;
//! - CORS, redirect revalidation and response filtering are in src/fetch.js.
//!
//! The only exception is the fixture's own loopback test server, registered by
//! [`allow_fixture_loopback`] in fixture mode (no `--game`); a game run never
//! registers it, and nothing reachable from JS can.
//!
//! Proxies: deno_fetch's `create_http_client` always applies the environment
//! proxies (`proxy::from_env`), even with `Options.proxy = None`. Through a proxy
//! the resolver above would only see the proxy's address, not the destination, so
//! while any proxy variable `from_env` reads is set every request is refused
//! (fail closed), the fixture loopback included ([`configured_proxy_variable`]).
//! `NO_PROXY` is not trusted as an exemption.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Mutex, OnceLock};

use blitz_html::HtmlDocument;
use content_security_policy as csp;
use deno_core::url::{Host, Url};
use deno_core::{OpState, op2};
use deno_error::JsErrorBox;

/// Fixture mode only: the plain-http loopback server `main.rs` starts for the
/// deterministic fetch tests.
static FIXTURE_LOOPBACK: OnceLock<SocketAddr> = OnceLock::new();

/// Host names [`PublicResolver`] refused, for the fixture's self-check.
static REFUSED_HOSTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn allow_fixture_loopback(address: SocketAddr) {
    assert!(
        address.ip().is_loopback(),
        "fixture server must be loopback"
    );
    FIXTURE_LOOPBACK
        .set(address)
        .expect("the fixture loopback server is registered once");
}

pub fn refused_hosts() -> Vec<String> {
    REFUSED_HOSTS
        .lock()
        .map(|hosts| hosts.clone())
        .unwrap_or_default()
}

fn is_fixture_loopback(url: &Url) -> bool {
    FIXTURE_LOOPBACK.get().is_some_and(|address| {
        url.scheme() == "http"
            && url.host()
                == Some(Host::Ipv4(match address.ip() {
                    IpAddr::V4(ip) => ip,
                    IpAddr::V6(_) => return false,
                }))
            && url.port() == Some(address.port())
    })
}

/// Addresses a game may connect to. `std`'s `is_global` is unstable and no
/// maintained crate in the graph classifies addresses, so the IANA special-purpose
/// registries are listed here (conservatively: unsure ranges are refused).
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified() ||
        ip.is_loopback() ||
        ip.is_private() ||
        ip.is_link_local() ||
        ip.is_broadcast() ||
        ip.is_documentation() ||
        ip.is_multicast() ||
        a == 0 || // 0.0.0.0/8 "this network"
        (a == 100 && (64..128).contains(&b)) || // 100.64.0.0/10 shared (CGNAT)
        (a == 192 && b == 0 && c == 0) || // 192.0.0.0/24 IETF protocol assignments
        (a == 192 && b == 88 && c == 99) || // 192.88.99.0/24 6to4 relay
        (a == 198 && (b == 18 || b == 19)) || // 198.18.0.0/15 benchmarking
        a >= 240) // 240.0.0.0/4 reserved
}

fn embedded_v4(high: u16, low: u16) -> Ipv4Addr {
    let [a, b] = high.to_be_bytes();
    let [c, d] = low.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    // ::ffff:a.b.c.d reaches the IPv4 address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let s = ip.segments();
    // NAT64 well-known prefix 64:ff9b::/96 and 6to4 2002::/16 embed an IPv4 destination.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return is_public_v4(embedded_v4(s[6], s[7]));
    }
    if s[0] == 0x2002 {
        return is_public_v4(embedded_v4(s[1], s[2]));
    }
    // Only global unicast 2000::/3 is public: this excludes ::, ::1, IPv4-compatible
    // ::/96, unique local fc00::/7, link-local fe80::/10, site-local and multicast.
    if s[0] & 0xe000 != 0x2000 {
        return false;
    }
    // Inside 2000::/3: 2001::/23 IETF assignments (Teredo, ORCHID, benchmarking),
    // documentation 2001:db8::/32 and 3fff::/20.
    !((s[0] == 0x2001 && s[1] < 0x0200)
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        || (s[0] == 0x3fff && s[1] < 0x1000))
}

/// The first proxy environment variable deno_fetch 0.284.0's `proxy::from_env`
/// would use, if any: `ALL_PROXY`/`all_proxy`, `HTTPS_PROXY`/`https_proxy` and,
/// outside a CGI context (`REQUEST_METHOD` unset), `HTTP_PROXY`/`http_proxy`.
/// Any non-empty value counts, even one `from_env` could not parse (fail closed).
/// Read once: the probe never changes its environment.
pub fn configured_proxy_variable() -> Option<&'static str> {
    static CONFIGURED: OnceLock<Option<&'static str>> = OnceLock::new();
    *CONFIGURED.get_or_init(|| {
        let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
        let mut names = vec!["ALL_PROXY", "all_proxy", "HTTPS_PROXY", "https_proxy"];
        if std::env::var_os("REQUEST_METHOD").is_none() {
            names.extend(["HTTP_PROXY", "http_proxy"]);
        }
        names.into_iter().find(|name| set(name))
    })
}

/// Static part of the policy, for one request URL (every redirect hop too):
/// no configured environment proxy, https, no URL credentials, no non-public IP
/// literal. Host names are left to [`PublicResolver`], which sees the addresses
/// actually connected to.
pub fn check_request_url(url: &Url) -> Result<(), String> {
    if let Some(variable) = configured_proxy_variable() {
        return Err(format!(
            "{url}: refused because the proxy variable {variable} is set; games may not fetch through a proxy"
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("URLs with embedded credentials are not allowed".to_string());
    }
    if is_fixture_loopback(url) {
        return Ok(());
    }
    if url.scheme() != "https" {
        return Err(format!(
            "{url}: games may only contact https: origins, not {}",
            url.scheme()
        ));
    }
    let ip = match url.host() {
        Some(Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(Host::Ipv6(ip)) => IpAddr::V6(ip),
        Some(Host::Domain(_)) => return Ok(()),
        None => return Err(format!("{url}: no host")),
    };
    if is_public_ip(ip) {
        Ok(())
    } else {
        Err(format!(
            "{url}: {ip} is a non-public address; games may only contact public hosts"
        ))
    }
}

/// deno_fetch `request_builder_hook`: the same static check on the URI op_fetch
/// is about to send, so a request that bypassed src/fetch.js is still refused.
pub fn request_hook(request: &mut http::Request<deno_fetch::ReqBody>) -> Result<(), JsErrorBox> {
    let url = Url::parse(&request.uri().to_string())
        .map_err(|error| JsErrorBox::type_error(format!("fetch: {error}")))?;
    check_request_url(&url).map_err(|error| JsErrorBox::type_error(format!("fetch: {error}")))
}

/// deno_fetch DNS resolver that refuses names resolving to any non-public
/// address. The whole answer is refused (not filtered) so a mixed public/private
/// answer cannot be used to reach the private one.
#[derive(Debug, Default)]
pub struct PublicResolver;

impl deno_fetch::dns::Resolve for PublicResolver {
    fn resolve(
        &self,
        name: hyper_util::client::legacy::connect::dns::Name,
    ) -> deno_fetch::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            // Port 0: hyper sets the request's port on the returned addresses.
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if let Some(refused) = addresses.iter().find(|address| !is_public_ip(address.ip())) {
                if let Ok(mut hosts) = REFUSED_HOSTS.lock() {
                    hosts.push(host.clone());
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "{host} resolves to the non-public address {}; games may only contact public hosts",
                        refused.ip()
                    ),
                ));
            }
            Ok(addresses.into_iter())
        })
    }
}

/// The document's enforced CSP, from `<meta http-equiv="Content-Security-Policy">`
/// elements in `<head>` when the page was parsed (game files carry no headers).
pub struct DocumentCsp(Option<csp::CspList>);

impl DocumentCsp {
    pub fn from_document(document: &HtmlDocument) -> Self {
        let root = document.root_node().id;
        let metas = document
            .query_selector_all_in(root, "head > meta[http-equiv]")
            .unwrap_or_default();
        let mut policies = Vec::new();
        for node in metas.into_iter().filter_map(|id| document.get_node(id)) {
            let attr = |name: &str| {
                node.attrs().and_then(|attrs| {
                    attrs
                        .iter()
                        .find(|attr| &*attr.name.local == name)
                        .map(|attr| attr.value.clone())
                })
            };
            let is_csp = attr("http-equiv")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("content-security-policy"));
            let Some(content) = attr("content").filter(|content| !content.is_empty()) else {
                continue;
            };
            if !is_csp {
                continue;
            }
            // HTML: parse with source "meta" and disposition "enforce", then drop
            // report-uri, frame-ancestors and sandbox (same as Roves' htmlmetaelement.rs).
            let mut policy = csp::Policy::parse(
                &content,
                csp::PolicySource::Meta,
                csp::PolicyDisposition::Enforce,
            );
            policy.directive_set.retain(|directive| {
                !matches!(
                    directive.name.as_str(),
                    "report-uri" | "frame-ancestors" | "sandbox"
                )
            });
            policies.push(policy);
        }
        Self((!policies.is_empty()).then(|| csp::CspList(policies)))
    }

    fn check(&self, url: &Url, document_url: &str, redirect_count: u32) -> Result<(), String> {
        let Some(list) = &self.0 else {
            return Ok(());
        };
        let origin = Url::parse(document_url)
            .map(|url| url.origin())
            .unwrap_or_else(|_| deno_core::url::Origin::new_opaque());
        // fetch() has the empty destination, so connect-src applies.
        let request = csp::Request {
            url: url.clone(),
            current_url: url.clone(),
            origin,
            redirect_count,
            destination: csp::Destination::None,
            initiator: csp::Initiator::None,
            nonce: String::new(),
            integrity_metadata: String::new(),
            parser_metadata: csp::ParserMetadata::None,
        };
        match list.should_request_be_blocked(&request).0 {
            csp::CheckResult::Allowed => Ok(()),
            csp::CheckResult::Blocked => Err(format!(
                "{url} is blocked by the document's Content-Security-Policy (connect-src)"
            )),
        }
    }
}

/// Checks one remote request hop before src/fetch.js sends it: the static URL
/// policy, then the document CSP. Refusals are also logged for game reports.
#[op2(fast)]
pub fn op_blitz_fetch_check(
    state: &mut OpState,
    #[string] url: &str,
    #[string] document_url: &str,
    redirect_count: u32,
) -> Result<(), JsErrorBox> {
    let parsed =
        Url::parse(url).map_err(|error| JsErrorBox::type_error(format!("fetch: {error}")))?;
    let result = check_request_url(&parsed).and_then(|()| {
        state
            .borrow::<DocumentCsp>()
            .check(&parsed, document_url, redirect_count)
    });
    result.map_err(|error| {
        eprintln!("ROVES_PROBE_FETCH_BLOCKED {error}");
        JsErrorBox::type_error(format!("fetch: {error}"))
    })
}

/// Fixture-mode self-check of the address classification and static URL policy
/// (deterministic, no network). Panics with the first wrong answer.
pub fn self_check() {
    // The fixture's fetch tests need direct connections; with a proxy configured
    // every request is refused (see the module docs), so say so up front.
    if let Some(variable) = configured_proxy_variable() {
        assert!(
            check_request_url(&Url::parse("https://1.1.1.1/").expect("test URL")).is_err(),
            "requests must be refused while {variable} is set"
        );
        panic!("unset {variable} to run the fixture: fetch is refused while a proxy is configured");
    }
    let public = [
        "1.1.1.1",
        "8.8.8.8",
        "2606:4700:4700::1111",
        "64:ff9b::808:808",
        "2002:808:808::1",
        "::ffff:1.1.1.1",
    ];
    let non_public = [
        "127.0.0.1",
        "127.1.2.3",
        "0.0.0.0",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "192.0.0.8",
        "198.18.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "240.0.0.1",
        "192.0.2.1",
        "::",
        "::1",
        "fc00::1",
        "fd12:3456::1",
        "fe80::1",
        "ff02::1",
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "64:ff9b::7f00:1",
        "2002:7f00:1::1",
        "2001:db8::1",
        "2001::1",
        "::127.0.0.1",
    ];
    for address in public {
        let ip: IpAddr = address.parse().expect("test address");
        assert!(is_public_ip(ip), "{address} should be public");
    }
    for address in non_public {
        let ip: IpAddr = address.parse().expect("test address");
        assert!(!is_public_ip(ip), "{address} should not be public");
    }
    let allowed = [
        "https://example.com/",
        "https://1.1.1.1/x",
        "https://[2606:4700::1]/",
    ];
    let refused = [
        "http://example.com/",
        "https://user:pw@example.com/",
        "https://127.0.0.1/",
        "https://0x7f.1/",
        "https://2130706433/",
        "https://[::1]/",
        "https://[::ffff:7f00:1]/",
        "https://10.0.0.1:8443/",
        "https://169.254.169.254/latest/meta-data/",
        "ftp://example.com/",
    ];
    for url in allowed {
        let parsed = Url::parse(url).expect("test URL");
        assert!(
            check_request_url(&parsed).is_ok(),
            "{url} should be allowed"
        );
    }
    for url in refused {
        let parsed = Url::parse(url).expect("test URL");
        assert!(
            check_request_url(&parsed).is_err(),
            "{url} should be refused"
        );
    }
}
