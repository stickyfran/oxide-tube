use tube_oxide::diagnostics;
use std::time::Instant;

#[test]
fn test_diagnostics_overhead_when_disabled() {
    diagnostics::set_debug_enabled(false);

    let t0 = Instant::now();
    for _ in 0..100_000 {
        let s = diagnostics::stage_start("benchmark");
        diagnostics::stage_end("benchmark", s);
    }
    let elapsed = t0.elapsed();
    println!("100,000 telemetry calls when disabled took: {:?}", elapsed);
    assert!(elapsed.as_millis() < 50, "Telemetry overhead when disabled must be negligible");
}

#[test]
fn test_diagnostics_enabled() {
    diagnostics::set_debug_enabled(true);
    assert!(diagnostics::is_debug_enabled());
    let s = diagnostics::stage_start("test_stage");
    assert!(s.is_some());
    diagnostics::stage_end("test_stage", s);
    diagnostics::log_seek(1000, 5000, true);
    diagnostics::log_buffering("vsrc/queue2", 45, 45);
    diagnostics::log_pip("Acquired", 0x123456, "Surface bound");
    diagnostics::set_debug_enabled(false);
}
