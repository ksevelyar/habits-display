fn main() {
    let timezone = std::env::var("TIMEZONE").unwrap();

    let power_str = std::env::var("WIFI_TRANSMIT_POWER").expect("WIFI_TRANSMIT_POWER must be set");
    let power: i8 = power_str
        .parse()
        .expect("WIFI_TRANSMIT_POWER must be a valid integer");
    assert!(
        (2..=20).contains(&power),
        "WIFI_TRANSMIT_POWER must be between 2 and 20 dBm, got {power}",
    );
    println!("cargo:rerun-if-env-changed=WIFI_TRANSMIT_POWER");

    let out = std::env::var("OUT_DIR").unwrap();
    std::fs::write(
        std::path::Path::new(&out).join("timezone.rs"),
        format!("pub const TIMEZONE: ::jiff::tz::TimeZone = ::jiff::tz::get!(\"{timezone}\");\n"),
    )
    .unwrap();
    println!("cargo:rerun-if-env-changed=TIMEZONE");

    linker_be_nice();
    println!("cargo:rustc-link-arg-tests=-Tembedded-test.x");
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
