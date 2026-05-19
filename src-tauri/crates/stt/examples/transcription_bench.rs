use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use rhema_stt::bench::{score_fixture, TranscriptFixture};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fixture_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/stt"));

    if !fixture_dir.exists() {
        println!(
            "STT benchmark skipped: fixture directory not found ({})",
            fixture_dir.display()
        );
        return Ok(());
    }

    let mut ran = 0usize;
    for entry in fs::read_dir(&fixture_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let fixture: TranscriptFixture = serde_json::from_str(&fs::read_to_string(&path)?)?;
        let Some(actual) = fixture.actual.as_deref() else {
            println!("{}: skipped (no actual transcript recorded)", fixture.name);
            continue;
        };

        let started = Instant::now();
        let result = score_fixture(&fixture, actual, started.elapsed());
        println!(
            "{}: similarity={:.3} scripture_terms={}/{} duration_ms={}",
            result.name,
            result.similarity,
            result.scripture_terms_found,
            result.scripture_terms_total,
            result.duration_ms
        );
        ran += 1;
    }

    if ran == 0 {
        println!(
            "STT benchmark skipped: no runnable fixtures in {}",
            fixture_dir.display()
        );
    }

    Ok(())
}
