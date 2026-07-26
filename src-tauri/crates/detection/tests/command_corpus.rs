use std::path::Path;

use rhema_detection::command_eval::{validate_cases, CommandCase, DatasetSplit};

#[test]
fn command_corpus_has_isolated_complete_partitions() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/command-classification/command-cases.generated.json");
    let json = std::fs::read_to_string(path).unwrap();
    let cases = serde_json::from_str::<Vec<CommandCase>>(&json).unwrap();

    validate_cases(&cases).unwrap();

    let count = |split| cases.iter().filter(|case| case.split == split).count();
    assert_eq!(count(DatasetSplit::Train), 190);
    assert_eq!(count(DatasetSplit::Validation), 60);
    assert_eq!(count(DatasetSplit::Test), 18);
    assert_eq!(count(DatasetSplit::Safety), 30);
}
