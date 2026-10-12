//! The verb handlers the CLI calls (`src/cli-*.ts`), as library functions:
//! each RETURNS its answer or an error that already knows its contract
//! code, and never prints. sp-cli owns stdout and the `{ ok, ... }`
//! envelope: the `--json` line is `{"ok":true,...answer}`.

pub mod app_settings;
pub mod export;
pub mod kfx;
pub mod routes;
pub mod settings;
