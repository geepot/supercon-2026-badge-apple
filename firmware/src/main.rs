#[cfg(target_os = "espidf")]
mod badge;
#[cfg(target_os = "espidf")]
mod display;

#[cfg(target_os = "espidf")]
fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    // Log and stop rather than reboot: a missing video needs a rebuild, and a
    // faulted panel should not be put through repeated high-voltage cycles.
    if let Err(error) = badge::run() {
        log::error!("{error:#}");
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("Build firmware with: ./scripts/firmware.sh build");
}
