//! EGW quote-matching accuracy harness.
//!
//! Loads labelled cases, runs `search_egw_bm25` + `longest_shared_content_run` +
//! `egw_quote_score`, and reports fire / miss / false-fire by category.
//!
//! Usage (repo root):
//!   cargo run -p rhema-detection --features precompute-bin --release \
//!     --bin `egw_accuracy` -- \
//!     --cases data/detection-fixtures/egw-quote-cases.json

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rhema_bible::{BibleDb, EgwParagraph};
use rhema_detection::{egw_quote_score, longest_shared_content_run, EGW_QUOTE_RUN_FIRE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CaseMode {
    Fire,
    Hint,
    Silent,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureCase {
    category: String,
    text: String,
    mode: CaseMode,
    #[serde(default)]
    cue: bool,
    expected_book: Option<String>,
}

fn load_cases(path: &Path) -> Result<Vec<FixtureCase>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn book_matches(actual: &str, expected: &str) -> bool {
    let a = actual.to_ascii_lowercase();
    let e = expected.to_ascii_lowercase();
    a.contains(&e) || e.contains(&a)
}

fn accuracy_failed(misses: usize, false_fires: usize) -> bool {
    misses > 0 || false_fires > 0
}

struct Config {
    cases_path: PathBuf,
    db_path: PathBuf,
    candidates: usize,
}

fn parse_config() -> Config {
    let mut config = Config {
        cases_path: PathBuf::from("data/detection-fixtures/egw-quote-cases.json"),
        db_path: PathBuf::from("data/rhema.db"),
        candidates: 5,
    };

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cases" => {
                config.cases_path = PathBuf::from(args.next().expect("--cases needs a path"));
            }
            "--db" => {
                config.db_path = PathBuf::from(args.next().expect("--db needs a path"));
            }
            "--candidates" => {
                config.candidates = args
                    .next()
                    .expect("--candidates needs a number")
                    .parse()
                    .expect("candidates must be usize");
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    config
}

fn best_match(
    paragraphs: &[EgwParagraph],
    case: &FixtureCase,
) -> Option<(String, usize, f64, bool)> {
    paragraphs.iter().fold(None, |best, paragraph| {
        let run = longest_shared_content_run(&case.text, &paragraph.text);
        let Some((confidence, auto_queued)) = egw_quote_score(run.len, case.cue) else {
            return best;
        };
        let better = best.as_ref().is_none_or(|(_, len, conf, _)| {
            run.len > *len || (run.len == *len && confidence > *conf)
        });
        if better {
            Some((
                paragraph.book_title.clone(),
                run.len,
                confidence,
                auto_queued,
            ))
        } else {
            best
        }
    })
}

fn print_summary(
    by_cat: HashMap<String, (usize, usize)>,
    fires: usize,
    misses: usize,
    false_fires: usize,
    silents: usize,
) {
    println!();
    println!("By category (correct / total):");
    let mut cats: Vec<_> = by_cat.into_iter().collect();
    cats.sort_by(|a, b| a.0.cmp(&b.0));
    for (cat, (ok, total)) in cats {
        println!("  {cat:>14}  {ok}/{total}");
    }
    println!();
    println!("Fires: {fires}  misses: {misses}  false-fires: {false_fires}  silent-ok: {silents}");
}

fn main() {
    let config = parse_config();

    let cases = load_cases(&config.cases_path).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let db = BibleDb::open_readonly(&config.db_path).unwrap_or_else(|e| {
        eprintln!("open db {}: {e}", config.db_path.display());
        std::process::exit(1);
    });

    println!(
        "Loaded {} EGW cases from {}",
        cases.len(),
        config.cases_path.display()
    );
    println!();

    let mut by_cat: HashMap<String, (usize, usize)> = HashMap::new();
    let mut fires = 0usize;
    let mut false_fires = 0usize;
    let mut misses = 0usize;
    let mut silents = 0usize;

    for case in &cases {
        let paragraphs = db
            .search_egw_bm25(&case.text, config.candidates)
            .unwrap_or_default();
        let best = best_match(&paragraphs, case);

        let entry = by_cat.entry(case.category.clone()).or_insert((0, 0));
        entry.1 += 1;

        let ok = match case.mode {
            CaseMode::Silent => {
                if best.is_some() {
                    false_fires += 1;
                    println!(
                        "[{:>14}] want silent -> FALSE-FIRE {} run={} ({:.0}%)",
                        case.category,
                        best.as_ref().map_or("?", |(b, _, _, _)| b.as_str()),
                        best.as_ref().map_or(0, |(_, r, _, _)| *r),
                        best.as_ref().map_or(0.0, |(_, _, c, _)| *c * 100.0),
                    );
                    false
                } else {
                    silents += 1;
                    println!("[{:>14}] want silent -> OK silent", case.category);
                    true
                }
            }
            CaseMode::Fire | CaseMode::Hint => {
                match &best {
                    None => {
                        misses += 1;
                        // Diagnostic: show best raw run even if below fire threshold.
                        let raw = paragraphs
                            .iter()
                            .map(|p| {
                                let run = longest_shared_content_run(&case.text, &p.text);
                                (p.book_title.clone(), run.len)
                            })
                            .max_by_key(|(_, len)| *len);
                        println!(
                            "[{:>14}] want fire -> miss (best raw run {:?}, need {})",
                            case.category, raw, EGW_QUOTE_RUN_FIRE
                        );
                        false
                    }
                    Some((book, run, conf, auto_q)) => {
                        let book_ok = case
                            .expected_book
                            .as_ref()
                            .is_none_or(|expected| book_matches(book, expected));
                        if book_ok {
                            fires += 1;
                            println!(
                                "[{:>14}] want fire -> OK {} run={run} ({:.0}% auto_q={auto_q})",
                                case.category,
                                book,
                                conf * 100.0
                            );
                            true
                        } else {
                            false_fires += 1;
                            println!(
                                "[{:>14}] want {:?} -> WRONG {book} run={run} ({:.0}%)",
                                case.category,
                                case.expected_book,
                                conf * 100.0
                            );
                            false
                        }
                    }
                }
            }
        };

        if ok {
            entry.0 += 1;
        }
    }

    print_summary(by_cat, fires, misses, false_fires, silents);
    if accuracy_failed(misses, false_fires) {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::accuracy_failed;

    #[test]
    fn required_detection_miss_fails_the_accuracy_run() {
        assert!(accuracy_failed(1, 0));
    }

    #[test]
    fn fully_correct_run_succeeds() {
        assert!(!accuracy_failed(0, 0));
    }
}
