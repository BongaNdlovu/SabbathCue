import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  buildGeneratedCommandCorpus,
  buildSyntheticTranscripts,
  GENERATED_CORPUS_PATH,
  GENERATED_TRANSCRIPTS_PATH,
} from "./generate-command-transcripts.mjs";

const generated = buildGeneratedCommandCorpus();
const synthetic = generated.filter((entry) => entry.synthetic);
const transcripts = buildSyntheticTranscripts();

test("generator creates one hundred synthetic sermon transcripts", () => {
  assert.equal(transcripts.length, 100);
});

test("every synthetic transcript contains seventeen utterances", () => {
  assert.equal(
    transcripts.filter((transcript) => transcript.utterances.length !== 17)
      .length,
    0,
  );
});

test("synthetic speakers never cross dataset partitions", () => {
  const splitsBySpeaker = new Map();
  for (const entry of synthetic) {
    const splits = splitsBySpeaker.get(entry.speakerId) ?? new Set();
    splits.add(entry.split);
    splitsBySpeaker.set(entry.speakerId, splits);
  }

  assert.equal(
    [...splitsBySpeaker.values()].filter((splits) => splits.size !== 1).length,
    0,
  );
});

test("generated training corpus has balanced command coverage", () => {
  const trainCommands = synthetic.filter(
    (entry) => entry.split === "train" && entry.expected.intent !== "none",
  );
  const counts = Object.groupBy(
    trainCommands,
    (entry) => entry.expected.intent,
  );

  assert.deepEqual(
    Object.fromEntries(
      Object.entries(counts).map(([intent, entries]) => [
        intent,
        entries.length,
      ]),
    ),
    {
      hide: 16,
      show: 16,
      next: 16,
      previous: 16,
      switch_translation: 16,
    },
  );
});

test("generated corpus retains only authored test and safety cases", () => {
  const generatedEvaluationCases = generated.filter(
    (entry) =>
      entry.synthetic &&
      (entry.split === "test" || entry.split === "safety"),
  );

  assert.equal(generatedEvaluationCases.length, 0);
});

test("committed generated corpus matches deterministic output", () => {
  const committed = JSON.parse(readFileSync(GENERATED_CORPUS_PATH, "utf8"));

  assert.deepEqual(committed, generated);
});

test("committed synthetic transcripts match deterministic output", () => {
  const committed = JSON.parse(
    readFileSync(GENERATED_TRANSCRIPTS_PATH, "utf8"),
  );

  assert.deepEqual(committed, transcripts);
});
