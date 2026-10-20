//! F15-AC4, ADR-0044: any script source runs in the sandbox without
//! crashing Ferry: it ends with an outcome (often an error), within the
//! limits, and its variable changes only ever touch the local layer.
#![no_main]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_core::scripts::Phase;
use ferry_core::variables::{Ambient, Variable};
use ferry_script::{
    Host, Info, Input, Level, Location, Script, ScriptRequest, ScriptResponse, Variables, run,
};
use libfuzzer_sys::fuzz_target;

struct Fake(u64);

impl Ambient for Fake {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(1_792_264_040_000)
    }
    fn random(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }
}

impl Host for Fake {
    fn send(&mut self, _: ScriptRequest) -> Result<ScriptResponse, String> {
        Err("no network while fuzzing".to_owned())
    }
    fn console(&mut self, _: Level, _: String) {}
    fn ambient(&mut self) -> &mut dyn Ambient {
        self
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let host: Rc<RefCell<dyn Host>> = Rc::new(RefCell::new(Fake(7)));
    let cancel = Arc::new(AtomicBool::new(false));
    let info = Info::default();
    let shared = vec![Variable::new("token", "shared")];
    for phase in [Phase::PreRequest, Phase::PostResponse] {
        let mut request = ScriptRequest::default();
        let response = ScriptResponse {
            code: 200,
            body: "{\"a\": [1, 2]}".to_owned(),
            ..ScriptResponse::default()
        };
        let mut variables = Variables {
            environment: Some(shared.clone()),
            globals: shared.clone(),
            collection: Some(shared.clone()),
            ..Variables::default()
        };
        let outcome = run(
            &[Script {
                location: Location::Request,
                source: source.to_owned(),
            }],
            Input {
                phase,
                request: &mut request,
                response: (phase == Phase::PostResponse).then_some(&response),
                variables: &mut variables,
                info: &info,
                cancel: &cancel,
            },
            &host,
        );
        assert!(!outcome.cancelled);
        // Whatever the script did, the shared value is still known.
        for list in [&variables.globals, variables.environment.as_ref().unwrap()] {
            if let Some(v) = list.iter().find(|v| v.key == "token") {
                match &v.origin {
                    ferry_core::variables::Origin::Shared => assert_eq!(v.value, "shared"),
                    ferry_core::variables::Origin::Overridden { shared } => {
                        assert_eq!(shared, "shared");
                    }
                    ferry_core::variables::Origin::LocalOnly => panic!("{v:?}"),
                }
            }
        }
    }
});
