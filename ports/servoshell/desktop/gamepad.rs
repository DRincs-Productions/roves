/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use log::{debug, warn};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, Gamepad};
use sdl3::joystick::JoystickId;
use sdl3::{EventPump, GamepadSubsystem};
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

fn gamepad_poll_interval(has_open_gamepads: bool, has_pending_haptics: bool) -> Duration {
    if has_open_gamepads || has_pending_haptics {
        ACTIVE_GAMEPAD_POLL_INTERVAL
    } else {
        IDLE_GAMEPAD_POLL_INTERVAL
    }
}

#[cfg(test)]
mod poll_interval_tests {
    use super::{
        ACTIVE_GAMEPAD_POLL_INTERVAL, IDLE_GAMEPAD_POLL_INTERVAL, gamepad_poll_interval,
    };

    #[test]
    fn idle_without_a_controller_uses_the_hotplug_interval() {
        assert_eq!(gamepad_poll_interval(false, false), IDLE_GAMEPAD_POLL_INTERVAL);
    }

    #[test]
    fn an_open_controller_uses_the_input_interval() {
        assert_eq!(gamepad_poll_interval(true, false), ACTIVE_GAMEPAD_POLL_INTERVAL);
    }

    #[test]
    fn pending_haptics_keep_the_input_interval_active() {
        assert_eq!(gamepad_poll_interval(false, true), ACTIVE_GAMEPAD_POLL_INTERVAL);
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
    // Kept alive alongside the subsystems/event pump borrowed from it; never read directly
    // again after `init_sdl`.
    _sdl_context: sdl3::Sdl,
    gamepad_subsystem: GamepadSubsystem,
    event_pump: EventPump,
    open_gamepads: HashMap<JoystickId, Gamepad>,
    pending_haptic_effects: HashMap<usize, PendingHapticEffect>,
}

pub(crate) struct ServoshellGamepadDelegate {
    sender: Sender<GamepadHapticEffectRequest>,
    receiver: Receiver<GamepadHapticEffectRequest>,
    // `None` if SDL failed to initialize (e.g. no gamepad support on this platform/environment)
    // -- `poll` and haptic handling then silently no-op instead of panicking.
    sdl_state: RefCell<Option<SdlGamepadState>>,
}

impl ServoshellGamepadDelegate {
    pub(crate) fn new() -> Self {
        let (tx, rx) = channel::<GamepadHapticEffectRequest>();
        Self {
            sender: tx,
            receiver: rx,
            sdl_state: RefCell::new(Self::init_sdl()),
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
        let event_pump = match sdl_context.event_pump() {
            Ok(pump) => pump,
            Err(error) => {
                warn!("Error creating SDL event pump for gamepad input ({error})");
                return None;
            },
        };
        Some(SdlGamepadState {
            _sdl_context: sdl_context,
            gamepad_subsystem,
            event_pump,
            open_gamepads: HashMap::new(),
            pending_haptic_effects: HashMap::new(),
        })
    }

    /// Drain any pending SDL gamepad activity and haptic effect requests. Called from
    /// `App::new_events` (main thread only — see this module's own doc comment).
    pub(crate) fn poll(&self, state: &RunningAppState) {
        let mut sdl_state = self.sdl_state.borrow_mut();
        let Some(sdl_state) = sdl_state.as_mut() else {
            return;
        };

        // Devices already connected before the first poll still produce their own
        // `Event::GamepadAdded` here, so unlike GilRs (whose `Gilrs::new()` doesn't emit
        // `Connected` for pre-existing devices), no separate startup enumeration is needed.
        while let Some(event) = sdl_state.event_pump.poll_event() {
            match event {
                Event::GamepadAdded { which, .. } => {
                    let gamepad = match sdl_state.gamepad_subsystem.open(which) {
                        Ok(gamepad) => gamepad,
                        Err(error) => {
                            warn!("Error opening gamepad {which} ({error})");
                            continue;
                        },
                    };
                    let name = gamepad.name().unwrap_or_else(|| "Unknown Gamepad".to_owned());
                    sdl_state.open_gamepads.insert(which, gamepad);
                    let index = GamepadIndex(which.raw() as usize);
                    Self::dispatch(state, event, name, index);
                },
                Event::GamepadRemoved { which, .. } => {
                    sdl_state.open_gamepads.remove(&which);
                    let index = GamepadIndex(which.raw() as usize);
                    Self::dispatch(state, event, String::new(), index);
                },
                Event::GamepadAxisMotion { which, .. } |
                Event::GamepadButtonDown { which, .. } |
                Event::GamepadButtonUp { which, .. } => {
                    let name = sdl_state
                        .open_gamepads
                        .get(&which)
                        .and_then(|gamepad| gamepad.name())
                        .unwrap_or_default();
                    let index = GamepadIndex(which.raw() as usize);
                    Self::dispatch(state, event, name, index);
                },
                _ => {},
            }
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
                        &sdl_state.open_gamepads,
                        &mut sdl_state.pending_haptic_effects,
                        &effect_type.clone(),
                        request,
                    );
                },
                GamepadHapticEffectRequestType::Stop => {
                    Self::stop_haptic_effect(
                        &mut sdl_state.open_gamepads,
                        &mut sdl_state.pending_haptic_effects,
                        request,
                    );
                },
            }
        }
    }

    fn dispatch(state: &RunningAppState, event: Event, name: String, index: GamepadIndex) {
        let Some(active_webview) =
            state.focused_window().and_then(|window| window.active_webview())
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
                match axis {
                    // The analog triggers: SDL reports these as axes (0..=32767), but the
                    // Gamepad spec's "Standard Gamepad" represents them as buttons 6/7 with an
                    // analog value -- same convention the old GilRs code already used.
                    Axis::TriggerLeft | Axis::TriggerRight => {
                        let mapped_index = if axis == Axis::TriggerLeft { 6 } else { 7 };
                        let axis_value = (value.max(0) as f64) / (i16::MAX as f64);
                        let update_type = GamepadUpdateType::Button(mapped_index, axis_value);
                        gamepad_event = Some(GamepadEvent::Updated(index, update_type));
                    },
                    // Map axis index and value to represent Standard Gamepad axis
                    // <https://www.w3.org/TR/gamepad/#dfn-represents-a-standard-gamepad-axis>
                    Axis::LeftX | Axis::LeftY | Axis::RightX | Axis::RightY => {
                        let mapped_axis: usize = match axis {
                            Axis::LeftX => 0,
                            Axis::LeftY => 1,
                            Axis::RightX => 2,
                            Axis::RightY => 3,
                            _ => unreachable!(),
                        };
                        // Unlike GilRs (whose Y axes are inverted relative to the spec, see the
                        // old code's own comment), SDL's Y axes already follow the Gamepad
                        // spec's convention directly (down is positive) -- no sign flip needed.
                        let axis_value = value as f64 /
                            if value < 0 { -(i16::MIN as f64) } else { i16::MAX as f64 };
                        let update_type = GamepadUpdateType::Axis(mapped_axis, axis_value);
                        gamepad_event = Some(GamepadEvent::Updated(index, update_type));
                    },
                }
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
        open_gamepads: &HashMap<JoystickId, Gamepad>,
        pending_haptic_effects: &mut HashMap<usize, PendingHapticEffect>,
        effect_type: &GamepadHapticEffectType,
        request: GamepadHapticEffectRequest,
    ) {
        let index = request.gamepad_index();
        let GamepadHapticEffectType::DualRumble(params) = effect_type;

        let Some((&joystick_id, _)) = open_gamepads
            .iter()
            .find(|(id, _)| id.raw() as usize == index)
        else {
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

        let Some(gamepad) = open_gamepads
            .values_mut()
            .find(|gamepad| gamepad.id().map(|id| id.raw() as usize) == Ok(index))
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
