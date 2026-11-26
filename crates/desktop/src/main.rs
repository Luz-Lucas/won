#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    sentinela_desktop::migrate::migrate_data_dirs();
    #[cfg(windows)]
    sentinela_desktop::win::run();
    #[cfg(not(windows))]
    eprintln!("sentinela-desktop só roda no Windows.");
}
