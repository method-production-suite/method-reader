//! The window takes the engine's real route catalog whole (the last part of
//! send-routes-ui.test.ts). The answers are the CLI's own `routes --json`,
//! frozen in rust/oracle/parity/screenplay.json, which `cargo xtask parity`
//! holds the Rust CLI to row by row: every route the engine can offer,
//! devices included, is read into a row, and every refusal is read as one.
//! Nothing is run here, so no Calibre and no device is asked.

use serde_json::Value;
use sp_app::model::send_routes::{routes_failure, routes_from};

fn frozen() -> serde_json::Map<String, Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/parity/screenplay.json"
    );
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(m) => m,
        _ => panic!("not an object"),
    }
}

#[test]
fn every_route_list_the_engine_answers_is_taken_whole() {
    let rows = frozen();
    let mut seen = 0;
    for name in ["routes", "routes-quick", "routes-after-send"] {
        let answer = &rows[name]["stdout"];
        let list = answer["routes"].as_array().expect(name);
        assert!(list.len() >= 5, "{name}: the whole catalog");
        let taken = routes_from(answer).unwrap_or_else(|| panic!("{name} was not taken"));
        assert_eq!(taken.routes.len(), list.len(), "{name}: no row dropped");
        assert_eq!(taken.chosen, answer["chosen"].as_str().unwrap(), "{name}");
        for (row, raw) in taken.routes.iter().zip(list) {
            assert_eq!(row.id, raw["id"].as_str().unwrap(), "{name}");
        }
        seen += 1;
    }
    assert_eq!(seen, 3);
}

#[test]
fn every_refusal_the_engine_answers_is_said_not_drawn() {
    let rows = frozen();
    let refusals: Vec<&String> = rows
        .keys()
        .filter(|k| {
            k.starts_with("routes-refuses") || *k == "routes-missing" || *k == "routes-nothing"
        })
        .collect();
    assert!(refusals.len() >= 10, "{refusals:?}");
    for name in refusals {
        let answer = &rows[name.as_str()]["stdout"];
        assert_eq!(answer["ok"], false, "{name}");
        assert!(routes_from(answer).is_none(), "{name}");
        assert!(!routes_failure(answer).trim().is_empty(), "{name}");
    }
}
