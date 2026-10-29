//! F11-AC2..AC6: importing any text never panics, and an imported
//! collection or environment exports to text that imports and exports the
//! same way again.
#![no_main]

use ferry_core::import::postman::{export_collection, export_environment};
use ferry_core::import::{Import, import};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    match import(text).map(|i| i.import) {
        Ok(Import::Collection(c)) => {
            let written = export_collection(&c);
            let Ok(Import::Collection(again)) = import(&written).map(|i| i.import) else {
                panic!("an export is not a collection: {written}");
            };
            assert_eq!(export_collection(&again), written);
        }
        Ok(Import::Environment(env)) => {
            let name = env.name.clone().unwrap_or_default();
            let written = export_environment(&name, &env.variables);
            let Ok(Import::Environment(again)) = import(&written).map(|i| i.import) else {
                panic!("an export is not an environment: {written}");
            };
            let again_name = again.name.clone().unwrap_or_default();
            assert_eq!(export_environment(&again_name, &again.variables), written);
        }
        Ok(Import::Request(_)) | Err(_) => {}
    }
});
