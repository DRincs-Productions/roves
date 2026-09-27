use std::time::SystemTime;

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
    let _gamepad = match sdl.gamepad() {
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
    match sdl.event_pump() {
        Ok(_event_pump) => eprintln!("after Sdl::event_pump: success"),
        Err(error) => {
            eprintln!("after Sdl::event_pump: error: {error}");
            std::process::exit(1);
        },
    }

    eprintln!("probe complete: {:?}", SystemTime::now());
}
