//! Offline command-classification primitives.
//!
//! This module is intentionally detached from command execution. It exists so
//! deterministic rules, a `MiniLM` classification head, and optional external
//! models can be evaluated against the same labeled corpus.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::error::DetectionError;
use crate::semantic::embedder::TextEmbedder;

/// Closed command set accepted by the evaluation harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandIntent {
    /// Ordinary speech or an unsupported command.
    None,
    /// Remove the current presentation from output.
    Hide,
    /// Restore or display the current presentation.
    Show,
    /// Advance to the next presentation item.
    Next,
    /// Return to the previous presentation item.
    Previous,
    /// Change the active Bible translation.
    SwitchTranslation,
}

impl CommandIntent {
    /// Stable order used by classifier weights and reports.
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Hide,
        Self::Show,
        Self::Next,
        Self::Previous,
        Self::SwitchTranslation,
    ];

    fn index(self) -> usize {
        match self {
            Self::None => 0,
            Self::Hide => 1,
            Self::Show => 2,
            Self::Next => 3,
            Self::Previous => 4,
            Self::SwitchTranslation => 5,
        }
    }
}

/// Expected or predicted command and its optional validated argument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandLabel {
    /// Predicted intent.
    pub intent: CommandIntent,
    /// Translation abbreviation for `switch_translation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<String>,
}

impl CommandLabel {
    /// Construct a label without an argument.
    #[must_use]
    pub const fn intent(intent: CommandIntent) -> Self {
        Self {
            intent,
            translation: None,
        }
    }

    /// Construct and normalize a translation-switch label.
    #[must_use]
    pub fn translation(abbreviation: &str) -> Self {
        Self {
            intent: CommandIntent::SwitchTranslation,
            translation: Some(abbreviation.trim().to_ascii_uppercase()),
        }
    }

    /// Reject arguments outside the closed schema.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self.intent {
            CommandIntent::SwitchTranslation => self
                .translation
                .as_deref()
                .is_some_and(is_supported_translation),
            _ => self.translation.is_none(),
        }
    }
}

/// Dataset partition. Test and safety cases are never used for training.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetSplit {
    Train,
    Validation,
    Test,
    Safety,
}

/// One authored benchmark utterance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCase {
    /// Stable human-readable identifier.
    pub id: String,
    /// Dataset partition.
    pub split: DatasetSplit,
    /// Group used to keep related paraphrases together.
    pub family: String,
    /// Transcript text supplied to every classifier.
    pub text: String,
    /// Gold command label.
    pub expected: CommandLabel,
}

/// Classifier output. Raw model text is diagnostic-only and never executable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPrediction {
    /// Validated prediction.
    pub label: CommandLabel,
    /// Classifier confidence in the range `0..=1`.
    pub confidence: f32,
    /// Optional raw external-model response for debugging invalid output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

impl CommandPrediction {
    fn none() -> Self {
        Self {
            label: CommandLabel::intent(CommandIntent::None),
            confidence: 1.0,
            raw: None,
        }
    }
}

/// Deterministic high-precision baseline used by the benchmark.
#[derive(Debug, Default)]
pub struct DeterministicCommandClassifier;

impl DeterministicCommandClassifier {
    /// Classify command phrases without executing them.
    #[must_use]
    pub fn predict(&self, text: &str) -> CommandPrediction {
        let normalized = normalize(text);

        if let Some(translation) = translation_from_command(&normalized) {
            return CommandPrediction {
                label: CommandLabel::translation(translation),
                confidence: 1.0,
                raw: None,
            };
        }

        let intent = if contains_phrase(
            &normalized,
            &[
                "hide the verse",
                "hide that verse",
                "hide the screen",
                "clear the screen",
                "take that off the screen",
                "remove that from the screen",
            ],
        ) {
            CommandIntent::Hide
        } else if contains_phrase(
            &normalized,
            &[
                "show the verse",
                "show that verse",
                "put that on the screen",
                "put it back on the screen",
                "restore the verse",
            ],
        ) {
            CommandIntent::Show
        } else if is_navigation_command(
            &normalized,
            &[
                "next verse",
                "next slide",
                "next item",
                "move forward",
                "go forward",
            ],
        ) {
            CommandIntent::Next
        } else if is_navigation_command(
            &normalized,
            &[
                "previous verse",
                "previous slide",
                "previous item",
                "go back one",
                "move back",
                "put the previous",
            ],
        ) {
            CommandIntent::Previous
        } else {
            return CommandPrediction::none();
        };

        CommandPrediction {
            label: CommandLabel::intent(intent),
            confidence: 1.0,
            raw: None,
        }
    }
}

/// Serialized linear softmax head trained over `MiniLM` embeddings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearCommandHead {
    dimension: usize,
    weights: Vec<Vec<f32>>,
    bias: Vec<f32>,
    command_threshold: f32,
}

impl LinearCommandHead {
    /// Train a deterministic multiclass linear head and tune its command
    /// threshold on the validation set.
    ///
    /// # Errors
    ///
    /// Returns an error when training data is empty or embedding dimensions
    /// are inconsistent.
    pub fn train(
        train: &[(Vec<f32>, CommandIntent)],
        validation: &[(Vec<f32>, CommandIntent)],
    ) -> Result<Self, DetectionError> {
        let dimension = train
            .first()
            .map(|(embedding, _)| embedding.len())
            .ok_or_else(|| DetectionError::Internal("command training set is empty".into()))?;
        if dimension == 0
            || train
                .iter()
                .chain(validation.iter())
                .any(|(embedding, _)| embedding.len() != dimension)
        {
            return Err(DetectionError::Internal(
                "command embedding dimensions are inconsistent".into(),
            ));
        }

        let mut head = Self {
            dimension,
            weights: vec![vec![0.0; dimension]; CommandIntent::ALL.len()],
            bias: vec![0.0; CommandIntent::ALL.len()],
            command_threshold: 0.0,
        };
        head.fit(train);
        head.command_threshold = head.select_threshold(validation);
        Ok(head)
    }

    /// Predict from a precomputed `MiniLM` embedding.
    ///
    /// # Errors
    ///
    /// Returns an error when the embedding dimension differs from the trained
    /// head.
    pub fn predict_embedding(
        &self,
        embedding: &[f32],
    ) -> Result<CommandPrediction, DetectionError> {
        if embedding.len() != self.dimension {
            return Err(DetectionError::Internal(format!(
                "command head expects {} dimensions, received {}",
                self.dimension,
                embedding.len()
            )));
        }
        let probabilities = self.probabilities(embedding);
        let (index, confidence) = probabilities
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or((0, 1.0));
        let mut intent = CommandIntent::ALL[index];
        if intent != CommandIntent::None && confidence < self.command_threshold {
            intent = CommandIntent::None;
        }
        Ok(CommandPrediction {
            label: CommandLabel::intent(intent),
            confidence,
            raw: None,
        })
    }

    /// Embed and classify one utterance.
    ///
    /// # Errors
    ///
    /// Propagates embedding or dimension errors.
    pub fn predict_text(
        &self,
        embedder: &dyn TextEmbedder,
        text: &str,
    ) -> Result<CommandPrediction, DetectionError> {
        let embedding = embedder.embed(text)?;
        self.predict_embedding_for_text(&embedding, text)
    }

    /// Classify a precomputed embedding while resolving arguments
    /// deterministically from the original text.
    ///
    /// # Errors
    ///
    /// Returns an error when the embedding dimension differs from the trained
    /// head.
    pub fn predict_embedding_for_text(
        &self,
        embedding: &[f32],
        text: &str,
    ) -> Result<CommandPrediction, DetectionError> {
        let mut prediction = self.predict_embedding(embedding)?;
        if prediction.label.intent != CommandIntent::None && !is_command_shaped(text) {
            prediction.label = CommandLabel::intent(CommandIntent::None);
            return Ok(prediction);
        }
        if prediction.label.intent == CommandIntent::SwitchTranslation {
            prediction.label.translation =
                translation_in_text(&normalize(text)).map(str::to_string);
        }
        Ok(prediction)
    }

    /// Selected confidence threshold for non-`none` commands.
    #[must_use]
    pub const fn command_threshold(&self) -> f32 {
        self.command_threshold
    }

    fn fit(&mut self, train: &[(Vec<f32>, CommandIntent)]) {
        const EPOCHS: usize = 500;
        const LEARNING_RATE: f32 = 0.35;
        const L2: f32 = 0.0005;

        #[expect(
            clippy::cast_precision_loss,
            reason = "small authored dataset counts are exactly representable"
        )]
        let sample_scale = 1.0 / train.len() as f32;
        for _ in 0..EPOCHS {
            let mut weight_gradient = vec![vec![0.0; self.dimension]; CommandIntent::ALL.len()];
            let mut bias_gradient = vec![0.0; CommandIntent::ALL.len()];

            for (embedding, expected) in train {
                let probabilities = self.probabilities(embedding);
                for (class, probability) in probabilities.iter().copied().enumerate() {
                    let target = f32::from(class == expected.index());
                    let error = probability - target;
                    bias_gradient[class] += error;
                    for (gradient, feature) in weight_gradient[class]
                        .iter_mut()
                        .zip(embedding.iter().copied())
                    {
                        *gradient += error * feature;
                    }
                }
            }

            for class in 0..CommandIntent::ALL.len() {
                self.bias[class] -= LEARNING_RATE * bias_gradient[class] * sample_scale;
                for (weight, gradient) in self.weights[class]
                    .iter_mut()
                    .zip(weight_gradient[class].iter().copied())
                {
                    let regularized = gradient * sample_scale + L2 * *weight;
                    *weight -= LEARNING_RATE * regularized;
                }
            }
        }
    }

    fn probabilities(&self, embedding: &[f32]) -> Vec<f32> {
        let mut logits = self
            .weights
            .iter()
            .zip(&self.bias)
            .map(|(weights, bias)| {
                weights
                    .iter()
                    .zip(embedding)
                    .map(|(weight, feature)| weight * feature)
                    .sum::<f32>()
                    + bias
            })
            .collect::<Vec<_>>();
        let maximum = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        for logit in &mut logits {
            *logit = (*logit - maximum).exp();
        }
        let sum = logits.iter().sum::<f32>();
        if sum > 0.0 {
            for logit in &mut logits {
                *logit /= sum;
            }
        }
        logits
    }

    fn select_threshold(&mut self, validation: &[(Vec<f32>, CommandIntent)]) -> f32 {
        if validation.is_empty() {
            return 0.0;
        }

        let mut best_score = f64::NEG_INFINITY;
        let mut best_false_positives = usize::MAX;
        let mut best_threshold = 1.0;
        for step in 0..=70 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "threshold grid contains only 71 small integers"
            )]
            let threshold = 0.30 + step as f32 * 0.01;
            self.command_threshold = threshold;
            let mut false_positives = 0;
            let mut expected_intents = Vec::with_capacity(validation.len());
            let mut predicted_intents = Vec::with_capacity(validation.len());
            for (embedding, expected) in validation {
                let prediction = self
                    .predict_embedding(embedding)
                    .map_or(CommandIntent::None, |value| value.label.intent);
                if *expected == CommandIntent::None && prediction != CommandIntent::None {
                    false_positives += 1;
                }
                expected_intents.push(*expected);
                predicted_intents.push(prediction);
            }
            let score = macro_f1_for_intents(&expected_intents, &predicted_intents);
            if score > best_score
                || ((score - best_score).abs() < f64::EPSILON
                    && false_positives < best_false_positives)
            {
                best_score = score;
                best_false_positives = false_positives;
                best_threshold = threshold;
            }
        }
        best_threshold
    }
}

/// Aggregate safety and quality measurements for one classifier.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMetrics {
    /// Evaluated case count.
    pub total: usize,
    /// Exact-intent accuracy.
    pub accuracy: f64,
    /// Macro-averaged F1 over all intents.
    pub macro_f1: f64,
    /// Ordinary utterances incorrectly classified as commands.
    pub false_commands: usize,
    /// Command utterances incorrectly classified as ordinary speech.
    pub missed_commands: usize,
    /// Argument errors after the intent was correctly recognized.
    pub argument_errors: usize,
    /// Expected-intent rows and predicted-intent columns.
    pub confusion: BTreeMap<CommandIntent, BTreeMap<CommandIntent, usize>>,
}

/// Score validated predictions against cases in a selected partition.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "authored benchmark counts are small and used only for reporting"
)]
pub fn score_predictions(
    cases: &[CommandCase],
    predictions: &[CommandPrediction],
) -> CommandMetrics {
    assert_eq!(
        cases.len(),
        predictions.len(),
        "case and prediction counts must match"
    );

    let mut correct = 0;
    let mut false_commands = 0;
    let mut missed_commands = 0;
    let mut argument_errors = 0;
    let mut confusion = BTreeMap::new();

    for (case, prediction) in cases.iter().zip(predictions) {
        let expected = case.expected.intent;
        let predicted = prediction.label.intent;
        *confusion
            .entry(expected)
            .or_insert_with(BTreeMap::new)
            .entry(predicted)
            .or_insert(0) += 1;

        if expected == predicted {
            correct += 1;
            if expected == CommandIntent::SwitchTranslation
                && case.expected.translation != prediction.label.translation
            {
                argument_errors += 1;
            }
        } else if expected == CommandIntent::None {
            false_commands += 1;
        } else if predicted == CommandIntent::None {
            missed_commands += 1;
        }
    }

    let macro_f1 = CommandIntent::ALL
        .iter()
        .map(|intent| {
            let true_positive = confusion
                .get(intent)
                .and_then(|row| row.get(intent))
                .copied()
                .unwrap_or_default() as f64;
            let false_positive = confusion
                .iter()
                .filter(|(expected, _)| *expected != intent)
                .map(|(_, row)| row.get(intent).copied().unwrap_or_default())
                .sum::<usize>() as f64;
            let false_negative = confusion
                .get(intent)
                .map(|row| {
                    row.iter()
                        .filter(|(predicted, _)| *predicted != intent)
                        .map(|(_, count)| *count)
                        .sum::<usize>()
                })
                .unwrap_or_default() as f64;
            let denominator = 2.0 * true_positive + false_positive + false_negative;
            if denominator > 0.0 {
                2.0 * true_positive / denominator
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / CommandIntent::ALL.len() as f64;

    CommandMetrics {
        total: cases.len(),
        accuracy: if cases.is_empty() {
            0.0
        } else {
            f64::from(correct) / cases.len() as f64
        },
        macro_f1,
        false_commands,
        missed_commands,
        argument_errors,
        confusion,
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "authored validation counts are small and used only for threshold tuning"
)]
fn macro_f1_for_intents(expected: &[CommandIntent], predicted: &[CommandIntent]) -> f64 {
    CommandIntent::ALL
        .iter()
        .map(|intent| {
            let true_positive = expected
                .iter()
                .zip(predicted)
                .filter(|(gold, guess)| *gold == intent && *guess == intent)
                .count() as f64;
            let false_positive = expected
                .iter()
                .zip(predicted)
                .filter(|(gold, guess)| *gold != intent && *guess == intent)
                .count() as f64;
            let false_negative = expected
                .iter()
                .zip(predicted)
                .filter(|(gold, guess)| *gold == intent && *guess != intent)
                .count() as f64;
            let denominator = 2.0 * true_positive + false_positive + false_negative;
            if denominator > 0.0 {
                2.0 * true_positive / denominator
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / CommandIntent::ALL.len() as f64
}

/// Validate corpus invariants that prevent train/test leakage.
///
/// # Errors
///
/// Returns a human-readable description of the first invalid invariant.
pub fn validate_cases(cases: &[CommandCase]) -> Result<(), String> {
    if cases.is_empty() {
        return Err("command corpus is empty".into());
    }

    let mut ids = BTreeSet::new();
    let mut family_splits = BTreeMap::<&str, DatasetSplit>::new();
    let mut observed_splits = BTreeSet::new();
    let mut train_intents = BTreeSet::new();

    for case in cases {
        if case.id.trim().is_empty() || !ids.insert(case.id.as_str()) {
            return Err(format!("duplicate or empty case id: {}", case.id));
        }
        if case.text.trim().is_empty() {
            return Err(format!("case {} has empty text", case.id));
        }
        if !case.expected.is_valid() {
            return Err(format!("case {} has an invalid command label", case.id));
        }
        if let Some(previous) = family_splits.insert(case.family.as_str(), case.split) {
            if previous != case.split {
                return Err(format!(
                    "family {} leaks across {:?} and {:?}",
                    case.family, previous, case.split
                ));
            }
        }
        if case.split == DatasetSplit::Safety && case.expected.intent != CommandIntent::None {
            return Err(format!("safety case {} must use intent none", case.id));
        }
        if case.split == DatasetSplit::Train {
            train_intents.insert(case.expected.intent);
        }
        observed_splits.insert(case.split);
    }

    for split in [
        DatasetSplit::Train,
        DatasetSplit::Validation,
        DatasetSplit::Test,
        DatasetSplit::Safety,
    ] {
        if !observed_splits.contains(&split) {
            return Err(format!("command corpus has no {split:?} cases"));
        }
    }
    for intent in CommandIntent::ALL {
        if !train_intents.contains(&intent) {
            return Err(format!("training split has no {intent:?} examples"));
        }
    }
    Ok(())
}

fn normalize(text: &str) -> String {
    text.to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn is_command_shaped(text: &str) -> bool {
    let normalized = normalize(text);
    if normalized.starts_with("do not ") {
        return false;
    }
    if normalized.starts_with("i do not need ") {
        return contains_phrase(
            &normalized,
            &["projected", "projection", "screen", "output", "display"],
        );
    }
    if matches!(
        normalized.as_str(),
        "hide" | "show" | "next" | "previous" | "forward" | "back"
    ) {
        return true;
    }

    let body = [
        "please ",
        "could you ",
        "would you ",
        "can you ",
        "could i ",
        "can i ",
        "can we ",
        "let us ",
        "let the congregation ",
    ]
    .iter()
    .find_map(|prefix| normalized.strip_prefix(prefix))
    .unwrap_or(&normalized);
    let has_presentation_target = contains_phrase(
        body,
        &[
            " verse",
            " scripture",
            " passage",
            " screen",
            " output",
            " display",
            " projector",
            " projected",
            " projection",
            " words",
            " text",
            " slide",
            " item",
            " that",
            " this",
            " it ",
            " it",
            " one",
        ],
    );
    let has_navigation_target = has_presentation_target
        || contains_phrase(
            body,
            &[" forward", " back", " before", " after", " following"],
        );

    if [
        "hide ", "show ", "clear ", "blank ", "take ", "remove ", "put ", "restore ", "display ",
        "bring ", "see ", "leave ", "have ",
    ]
    .iter()
    .any(|start| body.starts_with(start))
        && has_presentation_target
    {
        return true;
    }
    if [
        "next ",
        "previous ",
        "forward ",
        "back ",
        "go ",
        "move ",
        "continue ",
        "advance ",
        "return ",
        "rewind ",
    ]
    .iter()
    .any(|start| body.starts_with(start))
        && has_navigation_target
    {
        return true;
    }
    ["switch ", "change ", "read ", "use "]
        .iter()
        .any(|start| body.starts_with(start))
        && translation_in_text(body).is_some()
}

fn is_navigation_command(text: &str, phrases: &[&str]) -> bool {
    const COMMAND_STARTS: &[&str] = &[
        "next ",
        "previous ",
        "go ",
        "move ",
        "continue ",
        "advance ",
        "take us ",
        "let us ",
        "forward ",
        "back ",
        "return ",
        "rewind ",
        "put ",
    ];
    COMMAND_STARTS.iter().any(|start| text.starts_with(start))
        && phrases.iter().any(|phrase| text.contains(phrase))
}

fn translation_from_command(text: &str) -> Option<&'static str> {
    const COMMAND_CUES: &[&str] = &[
        "switch to ",
        "change to ",
        "read in ",
        "show it in ",
        "show that in ",
        "put that in ",
        "give me ",
        "can i have it in ",
        "can we have it in ",
    ];
    if !COMMAND_CUES.iter().any(|cue| text.contains(cue)) {
        return None;
    }

    translation_in_text(text)
}

fn translation_in_text(text: &str) -> Option<&'static str> {
    const TRANSLATIONS: &[(&str, &str)] = &[
        ("new international version", "NIV"),
        ("english standard version", "ESV"),
        ("king james version", "KJV"),
        ("new king james version", "NKJV"),
        ("new living translation", "NLT"),
        ("new english translation", "NET"),
        ("amplified bible", "AMP"),
        ("amplified", "AMP"),
        ("the message", "MSG"),
        ("spanish", "SPARV"),
        ("afrikaans", "AFR83"),
        ("niv", "NIV"),
        ("esv", "ESV"),
        ("kjv", "KJV"),
        ("nkjv", "NKJV"),
        ("nlt", "NLT"),
        ("net", "NET"),
        ("amp", "AMP"),
        ("msg", "MSG"),
    ];
    TRANSLATIONS
        .iter()
        .find_map(|(name, abbreviation)| text.contains(name).then_some(*abbreviation))
}

fn is_supported_translation(value: &str) -> bool {
    matches!(
        value,
        "NIV" | "ESV" | "KJV" | "NKJV" | "NLT" | "NET" | "AMP" | "MSG" | "SPARV" | "AFR83"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct KeywordEmbedder;

    impl TextEmbedder for KeywordEmbedder {
        fn embed(&self, text: &str) -> Result<Vec<f32>, DetectionError> {
            Ok(match text {
                "hide" => vec![1.0, 0.0],
                "show" => vec![0.0, 1.0],
                _ => vec![0.0, 0.0],
            })
        }

        fn dimension(&self) -> usize {
            2
        }
    }

    #[test]
    fn deterministic_classifier_drops_bare_translation_words() {
        let prediction = DeterministicCommandClassifier.predict("The English text says net");

        assert_eq!(prediction.label.intent, CommandIntent::None);
    }

    #[test]
    fn deterministic_classifier_requires_translation_command_cue() {
        let prediction = DeterministicCommandClassifier.predict("Could we switch to the NIV?");

        assert_eq!(prediction.label, CommandLabel::translation("NIV"));
    }

    #[test]
    fn command_label_rejects_unknown_translation() {
        let label = CommandLabel::translation("invented");

        assert!(!label.is_valid());
    }

    #[test]
    fn linear_head_learns_separable_embeddings() {
        let train = vec![
            (vec![0.0, 0.0], CommandIntent::None),
            (vec![1.0, 0.0], CommandIntent::Hide),
            (vec![0.9, 0.0], CommandIntent::Hide),
            (vec![0.0, 1.0], CommandIntent::Show),
            (vec![0.0, 0.9], CommandIntent::Show),
        ];
        let head = LinearCommandHead::train(&train, &train).unwrap();

        let prediction = head.predict_text(&KeywordEmbedder, "hide").unwrap();

        assert_eq!(prediction.label.intent, CommandIntent::Hide);
    }

    #[test]
    fn linear_head_abstains_for_declarative_sermon_speech() {
        let head = LinearCommandHead {
            dimension: 1,
            weights: vec![vec![0.0]; CommandIntent::ALL.len()],
            bias: vec![0.0, 10.0, 0.0, 0.0, 0.0, 0.0],
            command_threshold: 0.3,
        };

        let prediction = head
            .predict_embedding_for_text(&[0.0], "The younger son went back home")
            .unwrap();

        assert_eq!(prediction.label.intent, CommandIntent::None);
    }

    #[test]
    fn linear_head_keeps_explicit_operator_command() {
        let head = LinearCommandHead {
            dimension: 1,
            weights: vec![vec![0.0]; CommandIntent::ALL.len()],
            bias: vec![0.0, 10.0, 0.0, 0.0, 0.0, 0.0],
            command_threshold: 0.3,
        };

        let prediction = head
            .predict_embedding_for_text(&[0.0], "Please hide the screen")
            .unwrap();

        assert_eq!(prediction.label.intent, CommandIntent::Hide);
    }

    fn authored_command_cases() -> Vec<CommandCase> {
        serde_json::from_str(include_str!(
            "../../../../data/command-classification/command-cases.json"
        ))
        .unwrap()
    }

    #[test]
    fn command_shape_gate_accepts_every_held_out_command() {
        for case in authored_command_cases().iter().filter(|case| {
            case.split == DatasetSplit::Test && case.expected.intent != CommandIntent::None
        }) {
            assert!(
                is_command_shaped(&case.text),
                "command-shape gate rejected {}: {}",
                case.id,
                case.text
            );
        }
    }

    #[test]
    fn command_shape_gate_rejects_every_safety_utterance() {
        for case in authored_command_cases()
            .iter()
            .filter(|case| case.split == DatasetSplit::Safety)
        {
            assert!(
                !is_command_shaped(&case.text),
                "command-shape gate accepted {}: {}",
                case.id,
                case.text
            );
        }
    }

    #[test]
    fn score_counts_false_commands_and_misses() {
        let cases = vec![
            CommandCase {
                id: "ordinary".into(),
                split: DatasetSplit::Test,
                family: "ordinary".into(),
                text: "ordinary speech".into(),
                expected: CommandLabel::intent(CommandIntent::None),
            },
            CommandCase {
                id: "hide".into(),
                split: DatasetSplit::Test,
                family: "hide".into(),
                text: "hide".into(),
                expected: CommandLabel::intent(CommandIntent::Hide),
            },
        ];
        let predictions = vec![
            CommandPrediction {
                label: CommandLabel::intent(CommandIntent::Show),
                confidence: 0.9,
                raw: None,
            },
            CommandPrediction::none(),
        ];

        let metrics = score_predictions(&cases, &predictions);

        assert_eq!(metrics.false_commands, 1);
        assert_eq!(metrics.missed_commands, 1);
    }

    #[test]
    fn validation_rejects_family_leakage() {
        let cases = vec![
            CommandCase {
                id: "one".into(),
                split: DatasetSplit::Train,
                family: "shared".into(),
                text: "ordinary".into(),
                expected: CommandLabel::intent(CommandIntent::None),
            },
            CommandCase {
                id: "two".into(),
                split: DatasetSplit::Test,
                family: "shared".into(),
                text: "ordinary again".into(),
                expected: CommandLabel::intent(CommandIntent::None),
            },
        ];

        let error = validate_cases(&cases).unwrap_err();

        assert!(error.contains("leaks"));
    }
}
