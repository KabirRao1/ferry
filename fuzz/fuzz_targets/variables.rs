//! F08-AC4, FND-017: resolving any template against any variables never
//! panics, ends quickly, and never grows the text by more than
//! `MAX_GROWTH`.
#![no_main]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ferry_core::variables::{Ambient, MAX_GROWTH, Resolver, ScopeKind, Variable, Vars};
use libfuzzer_sys::fuzz_target;

struct Fixed(u64);

impl Ambient for Fixed {
    fn now(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_millis(1_792_264_040_000)
    }
    fn random(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // `key=value` lines define variables; the last line is the template.
    let mut lines: Vec<&str> = text.lines().collect();
    let Some(template) = lines.pop() else {
        return;
    };
    let variables: Vec<Variable> = lines
        .iter()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| Variable::new(key, value))
        .collect();
    let (shared, rest) = variables.split_at(variables.len() / 2);
    let vars = Vars::new()
        .with(ScopeKind::Environment, rest)
        .with(ScopeKind::Global, shared);
    let mut ambient = Fixed(7);
    let mut resolver = Resolver::new(&vars, &mut ambient);
    let out = resolver.resolve(template);
    assert!(
        out.len() <= template.len() + MAX_GROWTH,
        "grew by {} bytes",
        out.len() - template.len()
    );
    let _ = resolver.finish();
});
