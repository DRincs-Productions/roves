/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#[cfg(test)]
mod test;

#[cfg(not(target_os = "android"))]
mod backtrace;
mod crash_handler;
#[cfg(not(target_os = "android"))]
pub(crate) mod desktop;
#[cfg(target_os = "android")]
mod egl;
#[cfg(not(target_os = "android"))]
mod panic_hook;
mod parser;
mod prefs;
mod protocols;
#[cfg(not(target_os = "android"))]
mod resources;
mod running_app_state;
mod window;

pub mod platform {
    #[cfg(target_os = "macos")]
    pub use crate::platform::macos::deinit;

    #[cfg(target_os = "macos")]
    pub mod macos;

    #[cfg(not(target_os = "macos"))]
    pub fn deinit(_clean_shutdown: bool) {}
}

#[cfg(not(target_os = "android"))]
pub fn main() {
    desktop::cli::main()
}

pub fn init_crypto() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Error initializing crypto provider");
}

pub fn init_tracing(filter_directives: Option<&str>) {
    #[cfg(not(feature = "tracing"))]
    {
        if filter_directives.is_some() {
            log::debug!("The tracing feature was not selected - ignoring trace filter directives");
        }
    }
    #[cfg(feature = "tracing")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry();

        #[cfg(feature = "tracing-perfetto")]
        let subscriber = {
            // Set up a PerfettoLayer for performance tracing.
            // The servo.pftrace file can be uploaded to https://ui.perfetto.dev for analysis.
            let file = std::fs::File::create("servo.pftrace").unwrap();
            let perfetto_layer = tracing_perfetto::PerfettoLayer::new(std::sync::Mutex::new(file))
                .with_filter_by_marker(|field_name| field_name == "servo_profiling")
                .with_debug_annotations(true);
            subscriber.with(perfetto_layer)
        };

        // Deep, developer/profiling-build-only tracing (frame/CPU/GPU zones, allocations,
        // locks) via a live Tracy connection — see docs/DEPENDENCY_REVIEW.md's "Tracy e
        // Perfetto" decision. Never enabled by default (see the `tracing-tracy` feature's own
        // gating below and in ports/servoshell/Cargo.toml) — the release build story stays the
        // existing `tracing-perfetto` exporter above plus Servo's own `--profile`/
        // `--profiler-trace-path` CSV/trace-dump profiler (components/profile/time.rs),
        // neither of which need a live client attached to produce output.
        #[cfg(feature = "tracing-tracy")]
        let subscriber = subscriber.with(tracing_tracy::TracyLayer::default());

        // Filter events and spans by the directives in SERVO_TRACING, using EnvFilter as a global filter.
        // <https://docs.rs/tracing-subscriber/0.3.18/tracing_subscriber/layer/index.html#global-filtering>
        let filter_builder = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::level_filters::LevelFilter::OFF.into());
        let filter = if let Some(filters) = &filter_directives {
            filter_builder.parse_lossy(filters)
        } else {
            filter_builder
                .with_env_var("SERVO_TRACING")
                .from_env_lossy()
        };

        let subscriber = subscriber.with(filter);

        // Same as SubscriberInitExt::init, but avoids initialising the tracing-log compat layer,
        // since it would break Servo’s FromScriptLogger and FromEmbederLogger.
        // <https://docs.rs/tracing-subscriber/0.3.18/tracing_subscriber/util/trait.SubscriberInitExt.html#method.init>
        // <https://docs.rs/tracing/0.1.40/tracing/#consuming-log-records>
        tracing::subscriber::set_global_default(subscriber)
            .expect("Failed to set tracing subscriber");

        // Capture a first event, including the explicit wallclock time.
        // The event itself is useful when investigating startup time.
        // `wallclock_ns` allows us to ground the time, so we can compare
        // against an external timestamp from before starting servoshell.
        // In practice, the perfetto timestamp seems to be the same, but
        // we shouldn't assume this, since different backends may behave differently.
        servo::profile_traits::info_event!(
            "servoshell::startup_tracing_initialized",
            wallclock_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or(0)
        );
    }
}

pub const VERSION: &str = concat!("Servo ", env!("CARGO_PKG_VERSION"), "-", env!("GIT_SHA"));
