/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use log::{debug, warn};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::joystick::{
    Joystick, JoystickId, JoystickType, VirtualJoystickConnection, VirtualJoystickDescription,
};
use sdl3::{GamepadSubsystem, JoystickSubsystem};
use servo::{
    GamepadDelegate, GamepadEvent, GamepadHapticEffectRequest, GamepadHapticEffectRequestType,
    GamepadHapticEffectType, GamepadIndex, GamepadInputBounds, GamepadSupportedHapticEffects,
    GamepadUpdateType, InputEvent, WebView,
};

use crate::running_app_state::RunningAppState;

/// How often `App::new_events` polls SDL for gamepad activity while `Running` — the same
/// cadence the old GilRs-backed implementation blocked on (`next_event_blocking(Some(...))`).
/// SDL only allows the calls this module makes (`sdl3::init()` and friends) from the thread
/// that first called `main()`, unlike GilRs, which was happy running on its own dedicated
/// background thread -- see `CUSTOMIZATIONS.md`'s SDL3 gamepad entry for why. Polling is driven
/// from `App::new_events`/`set_running_control_flow` instead.
pub(crate) const ACTIVE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Hot-plug discovery does not need the input cadence used by an active controller. Keeping the
/// 100ms timer armed before any controller is connected needlessly wakes an otherwise-idle game
/// ten times per second.
const IDLE_GAMEPAD_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// macOS keeps SDL gamepad support off by default: `sdl3::init().gamepad()` once hung the shell
/// there (see `CUSTOMIZATIONS.md`'s SDL3 gamepad entries). The exact value `1` opts in at runtime,
/// so CI can exercise it with a timeout and a real Mac can try it without a special build.
const MACOS_GAMEPAD_ENV: &str = "ROVES_MACOS_GAMEPAD";

/// CI-only: the exact value `1` attaches an SDL virtual gamepad and keeps toggling its South
/// button and left stick for a while, so a smoke test without a physical controller can prove
/// SDL -> Servo -> Web Gamepad API delivery all the way into the page.
const VIRTUAL_GAMEPAD_ENV: &str = "ROVES_TEST_VIRTUAL_GAMEPAD";
const VIRTUAL_GAMEPAD_TOGGLE_INTERVAL: Duration = Duration::from_millis(500);
const VIRTUAL_GAMEPAD_ACTIVE_FOR: Duration = Duration::from_secs(60);

fn env_flag_is_one(value: Option<std::ffi::OsString>) -> bool {
    value.as_deref() == Some(std::ffi::OsStr::new("1"))
}

/// Whether the SDL gamepad delegate should be created at all on this platform.
pub(crate) fn runtime_enabled() -> bool {
    !cfg!(target_os = "macos") || env_flag_is_one(std::env::var_os(MACOS_GAMEPAD_ENV))
}

/// Drives the `ROVES_TEST_VIRTUAL_GAMEPAD=1` virtual device. Field order is drop order: close the
/// opened joystick, then detach the virtual device, then release the subsystem reference.
struct VirtualGamepadTest {
    joystick: Joystick,
    _connection: VirtualJoystickConnection,
    joystick_subsystem: JoystickSubsystem,
    started: Instant,
    last_toggle: Instant,
    pressed: bool,
    toggles: u32,
}

impl VirtualGamepadTest {
    fn attach(sdl_context: &sdl3::Sdl) -> Option<Self> {
        let joystick_subsystem = sdl_context
            .joystick()
            .inspect_err(|error| warn!("Virtual gamepad: no SDL joystick subsystem ({error})"))
            .ok()?;
        let description = VirtualJoystickDescription::new()
            .name("Roves CI Virtual Gamepad")
            .joystick_type(JoystickType::Gamepad)
            .with_axes([
                Axis::LeftX,
                Axis::LeftY,
                Axis::RightX,
                Axis::RightY,
                Axis::TriggerLeft,
                Axis::TriggerRight,
            ])
            .with_buttons([
                Button::South,
                Button::East,
                Button::West,
                Button::North,
                Button::Back,
                Button::Guide,
                Button::Start,
                Button::LeftStick,
                Button::RightStick,
                Button::LeftShoulder,
                Button::RightShoulder,
                Button::DPadUp,
                Button::DPadDown,
                Button::DPadLeft,
                Button::DPadRight,
            ]);
        let connection = joystick_subsystem
            .attach_virtual_joystick(description)
            .inspect_err(|error| warn!("Virtual gamepad: attach failed ({error})"))
            .ok()?;
        let joystick = joystick_subsystem
            .open(connection.id())
            .inspect_err(|error| warn!("Virtual gamepad: open failed ({error})"))
            .ok()?;
        log::info!("[roves-virtual-gamepad] attached id={}", connection.id());
        let now = Instant::now();
        Some(Self {
            joystick,
            _connection: connection,
            joystick_subsystem,
            started: now,
            last_toggle: now,
            pressed: false,
            toggles: 0,
        })
    }

    fn drive(&mut self, now: Instant) {
        if now.saturating_duration_since(self.started) > VIRTUAL_GAMEPAD_ACTIVE_FOR ||
            now.saturating_duration_since(self.last_toggle) < VIRTUAL_GAMEPAD_TOGGLE_INTERVAL
        {
            return;
        }
        self.last_toggle = now;
        self.pressed = !self.pressed;
        self.toggles += 1;
        if self.toggles <= 4 {
            log::info!("[roves-virtual-gamepad] toggle #{} pressed={}", self.toggles, self.pressed);
        }
        let axis = if self.pressed { 30_000 } else { 0 };
        if let Err(error) = self
            .joystick
            .set_virtual_button(Button::South.to_ll().0 as u32, self.pressed)
            .and_then(|()| {
                self.joystick
                    .set_virtual_axis(Axis::LeftX.to_ll().0 as u32, axis)
            })
        {
            warn!("Virtual gamepad: input update failed ({error})");
        }
        self.joystick_subsystem.update();
    }
}

/// The lowest slot not in `used`: the same rule as Servo's `Navigator::select_gamepad_index`, so
/// the shell and the page agree on each gamepad's index as long as they see the same connects and
/// disconnects in the same order.
fn lowest_free_slot(used: impl Iterator<Item = usize> + Clone) -> usize {
    (0..)
        .find(|slot| !used.clone().any(|taken| taken == *slot))
        .expect("an unbounded range always has a free slot")
}

/// Dense slots `0..n` for already-sorted keys, for re-announcing to a fresh document (whose
/// `navigator.getGamepads()` starts empty and fills from 0).
fn compacted_slots<K: Copy>(sorted_keys: &[K]) -> Vec<(K, usize)> {
    sorted_keys
        .iter()
        .enumerate()
        .map(|(slot, key)| (*key, slot))
        .collect()
}

#[cfg(test)]
mod slot_tests {
    use super::{compacted_slots, lowest_free_slot};

    #[test]
    fn first_gamepad_gets_slot_zero_whatever_its_sdl_id() {
        assert_eq!(lowest_free_slot(std::iter::empty()), 0);
    }

    #[test]
    fn reuses_the_lowest_slot_freed_by_a_disconnect() {
        assert_eq!(lowest_free_slot([0, 2].into_iter()), 1);
        assert_eq!(lowest_free_slot([1, 2].into_iter()), 0);
        assert_eq!(lowest_free_slot([0, 1].into_iter()), 2);
    }

    #[test]
    fn compaction_numbers_from_zero_in_order() {
        assert_eq!(compacted_slots(&[7u32, 9, 12]), vec![(7, 0), (9, 1), (12, 2)]);
    }
}

fn gamepad_poll_interval(has_open_gamepads: bool, has_pending_haptics: bool) -> Duration {
    if has_open_gamepads || has_pending_haptics {
        ACTIVE_GAMEPAD_POLL_INTERVAL
    } else {
        IDLE_GAMEPAD_POLL_INTERVAL
    }
}

fn map_gamepad_axis(axis: Axis, value: i16) -> GamepadUpdateType {
    match axis {
        Axis::TriggerLeft | Axis::TriggerRight => {
            let mapped_index = if axis == Axis::TriggerLeft { 6 } else { 7 };
            let axis_value = value.max(0) as f64 / i16::MAX as f64;
            GamepadUpdateType::Button(mapped_index, axis_value)
        },
        Axis::LeftX | Axis::LeftY | Axis::RightX | Axis::RightY => {
            let mapped_axis = match axis {
                Axis::LeftX => 0,
                Axis::LeftY => 1,
                Axis::RightX => 2,
                Axis::RightY => 3,
                _ => unreachable!(),
            };
            let axis_value = value as f64
                / if value < 0 {
                    -(i16::MIN as f64)
                } else {
                    i16::MAX as f64
                };
            GamepadUpdateType::Axis(mapped_axis, axis_value)
        },
    }
}

#[cfg(test)]
mod poll_interval_tests {
    use super::{ACTIVE_GAMEPAD_POLL_INTERVAL, IDLE_GAMEPAD_POLL_INTERVAL, gamepad_poll_interval};

    #[test]
    fn idle_without_a_controller_uses_the_hotplug_interval() {
        assert_eq!(
            gamepad_poll_interval(false, false),
            IDLE_GAMEPAD_POLL_INTERVAL
        );
    }

    #[test]
    fn an_open_controller_uses_the_input_interval() {
        assert_eq!(
            gamepad_poll_interval(true, false),
            ACTIVE_GAMEPAD_POLL_INTERVAL
        );
    }

    #[test]
    fn pending_haptics_keep_the_input_interval_active() {
        assert_eq!(
            gamepad_poll_interval(false, true),
            ACTIVE_GAMEPAD_POLL_INTERVAL
        );
    }
}

#[cfg(test)]
mod mapping_tests {
    use super::{Axis, Button, GamepadUpdateType, ServoshellGamepadDelegate, map_gamepad_axis};

    #[test]
    fn maps_standard_buttons_and_ignores_non_standard_buttons() {
        let standard = [
            (Button::South, 0),
            (Button::East, 1),
            (Button::West, 2),
            (Button::North, 3),
            (Button::LeftShoulder, 4),
            (Button::RightShoulder, 5),
            (Button::Back, 8),
            (Button::Start, 9),
            (Button::LeftStick, 10),
            (Button::RightStick, 11),
            (Button::DPadUp, 12),
            (Button::DPadDown, 13),
            (Button::DPadLeft, 14),
            (Button::DPadRight, 15),
            (Button::Guide, 16),
        ];

        for (button, expected_index) in standard {
            assert_eq!(
                ServoshellGamepadDelegate::map_gamepad_button(button),
                expected_index
            );
        }
        assert_eq!(
            ServoshellGamepadDelegate::map_gamepad_button(Button::Misc1),
            17
        );
    }

    #[test]
    fn maps_standard_axes_and_preserves_downward_positive_y() {
        let axes = [
            (Axis::LeftX, 0),
            (Axis::LeftY, 1),
            (Axis::RightX, 2),
            (Axis::RightY, 3),
        ];

        for (axis, expected_index) in axes {
            assert!(
                matches!(map_gamepad_axis(axis, 0), GamepadUpdateType::Axis(index, _) if index == expected_index)
            );
        }

        // The SDL Y axis follows the Web Gamepad convention: positive means down.
        let update = map_gamepad_axis(Axis::LeftY, 16_384);
        assert!(matches!(update, GamepadUpdateType::Axis(1, value) if value > 0.0));
    }

    #[test]
    fn maps_axis_extremes_to_the_standard_range() {
        assert!(
            matches!(map_gamepad_axis(Axis::LeftX, i16::MIN), GamepadUpdateType::Axis(0, value) if value == -1.0)
        );
        assert!(
            matches!(map_gamepad_axis(Axis::LeftX, i16::MAX), GamepadUpdateType::Axis(0, value) if value == 1.0)
        );
    }

    #[test]
    fn clamps_trigger_values_to_zero_through_one() {
        for raw in [i16::MIN, 0, i16::MAX] {
            assert!(
                matches!(map_gamepad_axis(Axis::TriggerLeft, raw), GamepadUpdateType::Button(6, value) if (0.0..=1.0).contains(&value))
            );
            assert!(
                matches!(map_gamepad_axis(Axis::TriggerRight, raw), GamepadUpdateType::Button(7, value) if (0.0..=1.0).contains(&value))
            );
        }
        assert!(
            matches!(map_gamepad_axis(Axis::TriggerLeft, i16::MIN), GamepadUpdateType::Button(6, value) if value == 0.0)
        );
        assert!(
            matches!(map_gamepad_axis(Axis::TriggerRight, i16::MAX), GamepadUpdateType::Button(7, value) if value == 1.0)
        );
    }
}

/// A rumble request whose `start_delay` hasn't elapsed yet. SDL's `set_rumble` has no delay
/// parameter of its own (unlike GilRs' `Replay` scheduling), so the delay is emulated here by
/// holding the request until `poll` (called roughly every `GAMEPAD_POLL_INTERVAL`) notices
/// `fire_at` has passed. SDL also has no "effect completed" event (unlike GilRs'
/// `ForceFeedbackEffectCompleted`) — `request.succeeded()` is therefore reported as soon as the
/// rumble is actually started, not when it finishes; SDL stops it on its own after `duration_ms`.
struct PendingHapticEffect {
    request: GamepadHapticEffectRequest,
    joystick_id: JoystickId,
    fire_at: Instant,
    strong_magnitude: u16,
    weak_magnitude: u16,
    duration_ms: u32,
}

struct SdlGamepadState {
    // Kept alive alongside the subsystems borrowed from it; never read directly again after
    // `init_sdl`. There is deliberately no `EventPump` here: sdl3-rs allows only one per
    // process and SDL has a single event queue, both owned by `event_loop.rs`, which forwards
    // gamepad events to [`ServoshellGamepadDelegate::handle_sdl_event`].
    _sdl_context: sdl3::Sdl,
    gamepad_subsystem: GamepadSubsystem,
    open_gamepads: HashMap<JoystickId, Gamepad>,
    /// `navigator.getGamepads()` slot of each open gamepad. Servo hands every new gamepad the
    /// lowest free slot page-side and indexes later updates and haptic requests by it, while
    /// SDL instance IDs start above 0 and only grow, so the SDL ID cannot be used directly.
    slots: HashMap<JoystickId, usize>,
    /// Only the first few button presses are logged, as a delivery breadcrumb.
    logged_buttons: u8,
    pending_haptic_effects: HashMap<usize, PendingHapticEffect>,
    virtual_test: Option<VirtualGamepadTest>,
}

pub(crate) struct ServoshellGamepadDelegate {
    sender: Sender<GamepadHapticEffectRequest>,
    receiver: Receiver<GamepadHapticEffectRequest>,
    // `None` if SDL failed to initialize (e.g. no gamepad support on this platform/environment)
    // -- `poll` and haptic handling then silently no-op instead of panicking.
    sdl_state: RefCell<Option<SdlGamepadState>>,
    /// When `poll` last ran; see [`Self::poll_if_due`].
    last_poll: Cell<Option<Instant>>,
}

impl ServoshellGamepadDelegate {
    pub(crate) fn new() -> Self {
        let (tx, rx) = channel::<GamepadHapticEffectRequest>();
        Self {
            sender: tx,
            receiver: rx,
            sdl_state: RefCell::new(Self::init_sdl()),
            last_poll: Cell::new(None),
        }
    }

    /// Use a low-frequency hot-plug probe while no controller is active, then restore the original
    /// input cadence as soon as a controller is opened or a delayed haptic effect is pending.
    pub(crate) fn poll_interval(&self) -> Duration {
        let sdl_state = self.sdl_state.borrow();
        let Some(sdl_state) = sdl_state.as_ref() else {
            return IDLE_GAMEPAD_POLL_INTERVAL;
        };
        gamepad_poll_interval(
            !sdl_state.open_gamepads.is_empty(),
            !sdl_state.pending_haptic_effects.is_empty(),
        )
    }

    fn init_sdl() -> Option<SdlGamepadState> {
        let virtual_test_requested = env_flag_is_one(std::env::var_os(VIRTUAL_GAMEPAD_ENV));
        if virtual_test_requested {
            // CI windows are often unfocused (especially on macOS runners); SDL drops joystick
            // events for background apps unless told otherwise.
            if !sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1") {
                warn!("Virtual gamepad: could not allow background joystick events");
            }
        }
        log::info!("[roves-gamepad] initializing SDL gamepad subsystem");
        let sdl_context = match sdl3::init() {
            Ok(context) => context,
            Err(error) => {
                warn!("Error initializing SDL for gamepad input ({error})");
                return None;
            },
        };
        let gamepad_subsystem = match sdl_context.gamepad() {
            Ok(subsystem) => subsystem,
            Err(error) => {
                warn!("Error initializing SDL gamepad subsystem ({error})");
                return None;
            },
        };
        log::info!("[roves-gamepad] SDL gamepad subsystem ready");
        let virtual_test = virtual_test_requested
            .then(|| VirtualGamepadTest::attach(&sdl_context))
            .flatten();
        Some(SdlGamepadState {
            _sdl_context: sdl_context,
            gamepad_subsystem,
            open_gamepads: HashMap::new(),
            slots: HashMap::new(),
            logged_buttons: 0,
            pending_haptic_effects: HashMap::new(),
            virtual_test,
        })
    }

    /// Whether `event` belongs to this delegate. `event_loop.rs` owns the process's only SDL
    /// `EventPump` and hands these over via [`Self::handle_sdl_event`].
    pub(crate) fn is_gamepad_event(event: &Event) -> bool {
        matches!(
            event,
            Event::GamepadAdded { .. } |
                Event::GamepadRemoved { .. } |
                Event::GamepadAxisMotion { .. } |
                Event::GamepadButtonDown { .. } |
                Event::GamepadButtonUp { .. }
        )
    }

    /// Forward one SDL gamepad event to Servo. Devices already connected at startup still
    /// produce their own `Event::GamepadAdded` once the gamepad subsystem is initialized, so
    /// unlike GilRs (whose `Gilrs::new()` doesn't emit `Connected` for pre-existing devices),
    /// no separate startup enumeration is needed.
    pub(crate) fn handle_sdl_event(&self, state: &RunningAppState, event: Event) {
        let mut sdl_state = self.sdl_state.borrow_mut();
        let Some(sdl_state) = sdl_state.as_mut() else {
            return;
        };
        match event {
            Event::GamepadAdded { which, .. } => {
                let gamepad = match sdl_state.gamepad_subsystem.open(which) {
                    Ok(gamepad) => gamepad,
                    Err(error) => {
                        warn!("Error opening gamepad {which} ({error})");
                        return;
                    },
                };
                let name = gamepad
                    .name()
                    .unwrap_or_else(|| "Unknown Gamepad".to_owned());
                let slot = lowest_free_slot(sdl_state.slots.values().copied());
                log::info!("[roves-gamepad] connected id={which} slot={slot} name={name:?}");
                sdl_state.open_gamepads.insert(which, gamepad);
                sdl_state.slots.insert(which, slot);
                Self::dispatch(state, event, name, GamepadIndex(slot));
            },
            Event::GamepadRemoved { which, .. } => {
                log::info!("[roves-gamepad] disconnected id={which}");
                sdl_state.open_gamepads.remove(&which);
                if let Some(slot) = sdl_state.slots.remove(&which) {
                    Self::dispatch(state, event, String::new(), GamepadIndex(slot));
                }
            },
            Event::GamepadAxisMotion { which, .. } |
            Event::GamepadButtonDown { which, .. } |
            Event::GamepadButtonUp { which, .. } => {
                let Some(&slot) = sdl_state.slots.get(&which) else {
                    return;
                };
                if matches!(event, Event::GamepadButtonDown { .. }) && sdl_state.logged_buttons < 4
                {
                    sdl_state.logged_buttons += 1;
                    log::info!("[roves-gamepad] button down id={which} slot={slot}");
                }
                let name = sdl_state
                    .open_gamepads
                    .get(&which)
                    .and_then(|gamepad| gamepad.name())
                    .unwrap_or_default();
                Self::dispatch(state, event, name, GamepadIndex(slot));
            },
            _ => {},
        }
    }

    /// Announce every open gamepad to the active page again, in dense slots from 0. Called when
    /// a top-level load completes: a new document starts with an empty `navigator.getGamepads()`
    /// and Servo drops updates for gamepads it has not been told about, so a controller plugged
    /// in before the game loaded (or connected while the previous document was showing) would
    /// otherwise never work until reconnected.
    pub(crate) fn announce_connected_gamepads(&self, state: &RunningAppState) {
        let mut sdl_state = self.sdl_state.borrow_mut();
        let Some(sdl_state) = sdl_state.as_mut() else {
            return;
        };
        let mut by_old_slot: Vec<(usize, JoystickId)> = sdl_state
            .slots
            .iter()
            .map(|(&id, &slot)| (slot, id))
            .collect();
        by_old_slot.sort_by_key(|(slot, _)| *slot);
        let ids: Vec<JoystickId> = by_old_slot.into_iter().map(|(_, id)| id).collect();
        for (id, slot) in compacted_slots(&ids) {
            sdl_state.slots.insert(id, slot);
            let name = sdl_state
                .open_gamepads
                .get(&id)
                .and_then(|gamepad| gamepad.name())
                .unwrap_or_else(|| "Unknown Gamepad".to_owned());
            log::info!("[roves-gamepad] announced id={id} slot={slot} to the loaded page");
            Self::dispatch(
                state,
                Event::GamepadAdded {
                    timestamp: 0,
                    which: id,
                },
                name,
                GamepadIndex(slot),
            );
        }
    }

    /// [`Self::poll`] if at least [`Self::poll_interval`] has passed since it last ran. Called on
    /// every event-loop wake: `App::dispatch_new_events` only runs when a wait *times out*, which
    /// a page that keeps producing events (animation, redraws) can postpone indefinitely,
    /// starving delayed haptics and the CI virtual gamepad.
    pub(crate) fn poll_if_due(&self, state: &RunningAppState) {
        let due = self.last_poll.get().is_none_or(|last| {
            Instant::now().saturating_duration_since(last) >= self.poll_interval()
        });
        if due {
            self.poll(state);
        }
    }

    /// Drive the CI virtual gamepad and fire due haptic effect requests. Called from
    /// `App::new_events` (main thread only — see this module's own doc comment). Input events
    /// arrive separately through [`Self::handle_sdl_event`].
    pub(crate) fn poll(&self, _state: &RunningAppState) {
        self.last_poll.set(Some(Instant::now()));
        let mut sdl_state = self.sdl_state.borrow_mut();
        let Some(sdl_state) = sdl_state.as_mut() else {
            return;
        };

        if let Some(virtual_test) = sdl_state.virtual_test.as_mut() {
            virtual_test.drive(Instant::now());
        }

        let now = Instant::now();
        let ready: Vec<usize> = sdl_state
            .pending_haptic_effects
            .iter()
            .filter(|(_, effect)| now >= effect.fire_at)
            .map(|(index, _)| *index)
            .collect();
        for index in ready {
            let Some(effect) = sdl_state.pending_haptic_effects.remove(&index) else {
                continue;
            };
            let Some(gamepad) = sdl_state.open_gamepads.get_mut(&effect.joystick_id) else {
                effect.request.failed();
                continue;
            };
            match gamepad.set_rumble(
                effect.weak_magnitude,
                effect.strong_magnitude,
                effect.duration_ms,
            ) {
                Ok(()) => effect.request.succeeded(),
                Err(error) => {
                    debug!("Failed to start haptic effect: {error:?}");
                    effect.request.failed();
                },
            }
        }

        while let Ok(request) = self.receiver.try_recv() {
            match request.request_type() {
                GamepadHapticEffectRequestType::Play(effect_type) => {
                    Self::play_haptic_effect(
                        &sdl_state.slots,
                        &mut sdl_state.pending_haptic_effects,
                        &effect_type.clone(),
                        request,
                    );
                },
                GamepadHapticEffectRequestType::Stop => {
                    Self::stop_haptic_effect(
                        &mut sdl_state.open_gamepads,
                        &sdl_state.slots,
                        &mut sdl_state.pending_haptic_effects,
                        request,
                    );
                },
            }
        }
    }

    fn dispatch(state: &RunningAppState, event: Event, name: String, index: GamepadIndex) {
        let Some(active_webview) = state
            .focused_window()
            .and_then(|window| window.active_webview())
        else {
            return;
        };
        Self::handle_gamepad_events(event, name, index, active_webview);
    }

    /// Translate an SDL gamepad event into a `GamepadEvent` and forward it to the active
    /// webview.
    fn handle_gamepad_events(
        event: Event,
        name: String,
        index: GamepadIndex,
        active_webview: WebView,
    ) {
        let mut gamepad_event: Option<GamepadEvent> = None;
        match event {
            Event::GamepadButtonDown { button, .. } => {
                let mapped_index = Self::map_gamepad_button(button);
                // We only want to send this for a valid digital button, aka on/off only --
                // the analog triggers arrive as `GamepadAxisMotion` instead (see below), so
                // unlike the old GilRs mapping, `mapped_index` can never actually be 6 or 7
                // here; the exclusion is kept only for `17` (unmapped buttons).
                if mapped_index != 17 {
                    let update_type = GamepadUpdateType::Button(mapped_index, 1.0);
                    gamepad_event = Some(GamepadEvent::Updated(index, update_type));
                }
            },
            Event::GamepadButtonUp { button, .. } => {
                let mapped_index = Self::map_gamepad_button(button);
                if mapped_index != 17 {
                    let update_type = GamepadUpdateType::Button(mapped_index, 0.0);
                    gamepad_event = Some(GamepadEvent::Updated(index, update_type));
                }
            },
            Event::GamepadAxisMotion { axis, value, .. } => {
                gamepad_event = Some(GamepadEvent::Updated(index, map_gamepad_axis(axis, value)));
            },
            Event::GamepadAdded { .. } => {
                let bounds = GamepadInputBounds {
                    axis_bounds: (-1.0, 1.0),
                    button_bounds: (0.0, 1.0),
                };
                // SDL can drive trigger rumble too (`Gamepad::set_rumble_triggers`), but
                // capability detection isn't wired through this event yet -- conservatively
                // matching the old GilRs-era default until that's added.
                let supported_haptic_effects = GamepadSupportedHapticEffects {
                    supports_dual_rumble: true,
                    supports_trigger_rumble: false,
                };
                gamepad_event = Some(GamepadEvent::Connected(
                    index,
                    name,
                    bounds,
                    supported_haptic_effects,
                ));
            },
            Event::GamepadRemoved { .. } => {
                gamepad_event = Some(GamepadEvent::Disconnected(index));
            },
            _ => {},
        }

        if let Some(event) = gamepad_event {
            active_webview.notify_input_event(InputEvent::Gamepad(event));
        }
    }

    // Map button index and value to represent Standard Gamepad button
    // <https://www.w3.org/TR/gamepad/#dfn-represents-a-standard-gamepad-button>
    fn map_gamepad_button(button: Button) -> usize {
        match button {
            Button::South => 0,
            Button::East => 1,
            Button::West => 2,
            Button::North => 3,
            Button::LeftShoulder => 4,
            Button::RightShoulder => 5,
            // 6 and 7 (the analog triggers) never arrive as a `Button` in SDL -- see
            // `handle_gamepad_events`'s `GamepadAxisMotion` arm instead.
            Button::Back => 8,
            Button::Start => 9,
            Button::LeftStick => 10,
            Button::RightStick => 11,
            Button::DPadUp => 12,
            Button::DPadDown => 13,
            Button::DPadLeft => 14,
            Button::DPadRight => 15,
            Button::Guide => 16,
            _ => 17, // Other buttons do not map to "standard" gamepad mapping and are ignored
        }
    }

    fn play_haptic_effect(
        slots: &HashMap<JoystickId, usize>,
        pending_haptic_effects: &mut HashMap<usize, PendingHapticEffect>,
        effect_type: &GamepadHapticEffectType,
        request: GamepadHapticEffectRequest,
    ) {
        let index = request.gamepad_index();
        let GamepadHapticEffectType::DualRumble(params) = effect_type;

        let Some((&joystick_id, _)) = slots.iter().find(|(_, slot)| **slot == index) else {
            debug!("Couldn't find connected gamepad to play haptic effect on");
            request.failed();
            return;
        };

        let start_delay = Duration::from_millis(params.start_delay as u64);
        let duration_ms = params.duration as u32;
        let strong_magnitude = (params.strong_magnitude * u16::MAX as f64).round() as u16;
        let weak_magnitude = (params.weak_magnitude * u16::MAX as f64).round() as u16;

        pending_haptic_effects.insert(
            index,
            PendingHapticEffect {
                request,
                joystick_id,
                fire_at: Instant::now() + start_delay,
                strong_magnitude,
                weak_magnitude,
                duration_ms,
            },
        );
    }

    fn stop_haptic_effect(
        open_gamepads: &mut HashMap<JoystickId, Gamepad>,
        slots: &HashMap<JoystickId, usize>,
        pending_haptic_effects: &mut HashMap<usize, PendingHapticEffect>,
        request: GamepadHapticEffectRequest,
    ) {
        let index = request.gamepad_index();

        // A still-delayed (not yet started) effect: just drop it, nothing to stop on the
        // device itself yet.
        if pending_haptic_effects.remove(&index).is_some() {
            request.succeeded();
            return;
        }

        let Some(gamepad) = slots
            .iter()
            .find(|(_, slot)| **slot == index)
            .and_then(|(id, _)| open_gamepads.get_mut(id))
        else {
            request.failed();
            return;
        };

        match gamepad.set_rumble(0, 0, 0) {
            Ok(()) => request.succeeded(),
            Err(error) => {
                debug!("Failed to stop haptic effect: {error:?}");
                request.failed();
            },
        }
    }
}

impl GamepadDelegate for ServoshellGamepadDelegate {
    fn handle_haptic_effect_request(&self, request: GamepadHapticEffectRequest) {
        if self.sender.send(request).is_err() {
            warn!("Haptic effect couldn't be played!")
        }
    }
}
