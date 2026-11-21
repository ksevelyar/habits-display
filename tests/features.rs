#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[embedded_test::setup]
fn initialize_debug_output() {
    rtt_target::rtt_init_defmt!();
}

mod features {
    mod night_mode;
    mod notifications;
    mod simulated_esp32;
}
