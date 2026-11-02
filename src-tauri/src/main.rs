#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--detect-only`, `--integrate` and friends never open a window: the
    // `.run` installer and diagnostics depend on that.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = whisper_desktop_lib::handle_cli(&args) {
        std::process::exit(code);
    }

    // If the ZIP ships a WebView2 Fixed Runtime, use it instead of whatever is
    // installed on the machine. This makes the archive work on restricted
    // Windows images where the evergreen runtime is missing or blocked.
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let runtime = dir.join("webview2");
            if runtime.exists() {
                std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", runtime);
            }
        }
    }

    whisper_desktop_lib::run()
}
