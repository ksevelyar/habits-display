# Errors

## Contract
Contexts are the device's public API. Their fallible functions return `Result<T, AppError>`. Raw errors (`esp_radio::wifi::WifiError`, `edge_ws::io::Error`, `embedded_tls::Error`) stay inside contexts as implementation details and are logged at the failure site via `defmt::error!`.

## Types
`AppError` outcomes:

* `Config(&'static str)`: build-time misconfiguration, carries the offending setting name
* `Network(&'static str)`: connection could not be established or was lost, carries the failed stage, e.g. `"tcp connection failed"`
* `Timeout`: established connection went silent beyond the server ping interval, treated as dead

## Rules
* Failure sites log the raw error and propagate the coarse `AppError` variant
