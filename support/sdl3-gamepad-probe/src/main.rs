use std::time::SystemTime;

use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button};
use sdl3::joystick::{JoystickType, VirtualJoystickDescription};

fn main() {
    eprintln!("probe start: {:?}", SystemTime::now());

    eprintln!("before sdl3::init");
    let sdl = match sdl3::init() {
        Ok(sdl) => {
            eprintln!("after sdl3::init: success");
            sdl
        },
        Err(error) => {
            eprintln!("after sdl3::init: error: {error}");
            std::process::exit(1);
        },
    };

    eprintln!("before Sdl::gamepad");
    let gamepad_subsystem = match sdl.gamepad() {
        Ok(gamepad) => {
            eprintln!("after Sdl::gamepad: success");
            gamepad
        },
        Err(error) => {
            eprintln!("after Sdl::gamepad: error: {error}");
            std::process::exit(1);
        },
    };

    eprintln!("before Sdl::event_pump");
    let mut event_pump = match sdl.event_pump() {
        Ok(pump) => {
            eprintln!("after Sdl::event_pump: success");
            pump
        },
        Err(error) => {
            eprintln!("after Sdl::event_pump: error: {error}");
            std::process::exit(1);
        },
    };

    eprintln!("before attaching virtual SDL gamepad");
    let joystick_subsystem = sdl.joystick().expect("initialize SDL joystick subsystem");
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
    let virtual_gamepad = joystick_subsystem
        .attach_virtual_joystick(description)
        .expect("attach virtual SDL gamepad");
    let id = virtual_gamepad.id();
    assert!(
        gamepad_subsystem.is_gamepad(id),
        "SDL did not recognize the virtual gamepad"
    );
    joystick_subsystem.update();
    assert!(
        event_pump
            .poll_iter()
            .any(|event| matches!(event, Event::GamepadAdded { which, .. } if which == id)),
        "SDL did not emit GamepadAdded for the virtual gamepad"
    );
    let gamepad = gamepad_subsystem
        .open(id)
        .expect("open virtual SDL gamepad");
    let joystick = joystick_subsystem
        .open(id)
        .expect("open virtual SDL joystick");
    eprintln!(
        "virtual SDL gamepad attached and opened: {}",
        gamepad
            .name()
            .unwrap_or_else(|| "Unknown gamepad".to_owned())
    );

    eprintln!("before virtual gamepad axis input");
    joystick
        .set_virtual_axis(Axis::LeftX.to_ll().0 as u32, 12_345)
        .expect("set virtual gamepad axis");
    joystick_subsystem.update();
    assert!(
        event_pump.poll_iter().any(|event| matches!(
            event,
            Event::GamepadAxisMotion { which, axis: Axis::LeftX, value: 12_345, .. } if which == id
        )),
        "SDL did not emit the virtual gamepad axis input"
    );
    eprintln!("virtual gamepad axis input received");

    eprintln!("before virtual gamepad button input");
    joystick
        .set_virtual_button(Button::South.to_ll().0 as u32, true)
        .expect("set virtual gamepad button");
    joystick_subsystem.update();
    assert!(
        event_pump.poll_iter().any(|event| matches!(
            event,
            Event::GamepadButtonDown { which, button: Button::South, .. } if which == id
        )),
        "SDL did not emit the virtual gamepad button input"
    );
    eprintln!("virtual gamepad button input received");

    drop(gamepad);
    drop(joystick);
    eprintln!("before virtual gamepad disconnect");
    drop(virtual_gamepad);
    joystick_subsystem.update();
    assert!(
        event_pump
            .poll_iter()
            .any(|event| matches!(event, Event::GamepadRemoved { which, .. } if which == id)),
        "SDL did not emit GamepadRemoved for the virtual gamepad"
    );
    eprintln!("virtual gamepad disconnect received");

    eprintln!("probe complete: {:?}", SystemTime::now());
}
