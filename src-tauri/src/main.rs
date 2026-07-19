// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("client" | "--no-gui")
    ) {
        attach_parent_console();
        if let Err(error) = ncy_desktop_lib::cli::run(&args) {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    } else {
        ncy_desktop_lib::run()
    }
}

#[cfg(windows)]
fn attach_parent_console() {
    // Release builds use the Windows GUI subsystem so launching the normal app
    // never flashes a console. Headless mode explicitly reconnects to the shell
    // that launched it before Rust initializes stdout/stderr.
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}
