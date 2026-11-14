// Prevents an extra console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(
        args.first().map(String::as_str),
        Some("client" | "--no-gui" | "server" | "--server")
    ) {
        attach_parent_console();
        let result = if matches!(
            args.first().map(String::as_str),
            Some("server" | "--server")
        ) {
            let server_args = args[1..].to_vec();
            ncy_server::run(&server_args)
        } else {
            ncy_desktop_lib::cli::run(&args)
        };
        if let Err(error) = result {
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
