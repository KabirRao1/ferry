//! F08-AC10, F09-AC2, F10-AC6, F16-AC3, F17-AC6: every file format Ferry reads from
//! the workspace or the data directory parses any text without panicking,
//! and whatever it parses writes back to text that parses and writes the
//! same way again.
#![no_main]

use ferry_core::{collection, cookies, datafile, environment, history};
use libfuzzer_sys::fuzz_target;

const NOW: i64 = 1_792_264_040_000;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(file) = collection::parse_request(text, "Fallback") {
        let written = collection::write_request(&file);
        let again = collection::parse_request(&written, "Fallback").expect("request");
        assert_eq!(collection::write_request(&again), written);
    }
    if let Ok(file) = collection::parse_collection(text, "Fallback") {
        let written = collection::write_collection(&file);
        let again = collection::parse_collection(&written, "Fallback").expect("collection");
        assert_eq!(collection::write_collection(&again), written);
    }
    if let Ok(file) = collection::parse_folder(text, "Fallback") {
        let written = collection::write_folder(&file);
        let again = collection::parse_folder(&written, "Fallback").expect("folder");
        assert_eq!(collection::write_folder(&again), written);
    }
    if let Ok(names) = collection::parse_order(text) {
        let written = collection::write_order(&names);
        let again = collection::parse_order(&written).expect("order");
        assert_eq!(collection::write_order(&again), written);
    }
    if let Ok(file) = environment::parse(text, None) {
        let (shared, local) = environment::write(&file);
        let again = environment::parse(&shared, local.as_deref()).expect("environment");
        assert_eq!(environment::write(&again), (shared, local));
    }
    if let Ok(file) = history::parse(text) {
        let written = history::write(&file);
        let again = history::parse(&written).expect("history");
        assert_eq!(history::write(&again), written);
    }
    // F16-AC3: data files parse or are refused, never panic.
    if let Ok(rows) = datafile::parse(text) {
        assert!(!rows.is_empty());
    }
    if let Ok(jar) = cookies::parse(text, NOW) {
        let written = cookies::write(&jar);
        let again = cookies::parse(&written, NOW).expect("cookies");
        assert_eq!(cookies::write(&again), written);
    }
});
