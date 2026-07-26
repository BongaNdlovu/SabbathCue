import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const directory = dirname(fileURLToPath(import.meta.url));
const AUTHORED_CORPUS_PATH = resolve(directory, "command-cases.json");
export const GENERATED_CORPUS_PATH = resolve(
  directory,
  "command-cases.generated.json",
);
export const GENERATED_TRANSCRIPTS_PATH = resolve(
  directory,
  "synthetic-command-transcripts.json",
);

const TOPICS = [
  "grace",
  "faith",
  "forgiveness",
  "hope",
  "prayer",
  "service",
  "discipleship",
  "the resurrection",
  "the covenant",
  "Christian character",
];
const BOOKS = [
  "Genesis",
  "Psalms",
  "Isaiah",
  "Matthew",
  "Luke",
  "John",
  "Romans",
  "Hebrews",
  "James",
  "Revelation",
];
const NAMES = [
  "Abraham",
  "Moses",
  "David",
  "Elijah",
  "Mary",
  "Peter",
  "Paul",
  "John",
  "Ruth",
  "Esther",
];
const ORDINARY_TEMPLATES = [
  "Today we continue our study of {topic}.",
  "The previous chapter introduced this theme through {name}.",
  "The next generation needs a living example of {topic}.",
  "These words show the patience of God toward his people.",
  "Nothing can hide the truth revealed in {book}.",
  "We look forward in faith because the promise still stands.",
  "The message becomes clear when we consider the whole passage.",
  "{name} returned home with a changed heart.",
  "The English translation preserves an important connection here.",
  "A life of service will display the character of Christ.",
  "Let us think about what {book} teaches concerning {topic}.",
  "The following day brought another test of their faith.",
  "God can remove the burden of guilt that people carry.",
  "We go back to this story because its lesson is easy to miss.",
  "The last verse of the hymn points the congregation toward hope.",
  "A screen can become a metaphor for what fear places before us.",
  "The net began to break under the weight of the catch.",
  "The amplified sound reached everyone standing in the courtyard.",
  "Bible translation work continues among communities worldwide.",
  "We will see the fruit of {topic} in ordinary acts of kindness.",
  "Put away bitterness and choose the way of reconciliation.",
  "Show kindness to strangers without expecting anything in return.",
  "Advance the kingdom through quiet and faithful service.",
  "The version of events in {book} emphasizes a different detail.",
];

const COMMAND_TEMPLATES = {
  hide: [
    "Hide the screen please",
    "Take that verse off the screen",
    "Clear the projected output",
    "Leave the screen empty for a moment",
    "I do not need that projected anymore",
    "Blank the output until I ask again",
    "Please remove that passage from the display",
    "Take the words away for now",
  ],
  show: [
    "Show the verse again",
    "Put that scripture back up",
    "Bring the passage onto the projector",
    "Let the congregation see that again",
    "Restore the verse display",
    "Please display that passage again",
    "Can we see that text once more",
    "Put it bak up on the screen",
  ],
  next: [
    "Next verse please",
    "Move forward one",
    "Take us to the following passage",
    "Advance to the next item",
    "Let us have the one after this",
    "Continue to the following verse",
    "Forward one please",
    "Go to the nex slide",
  ],
  previous: [
    "Previous verse please",
    "Go back one",
    "Return to what was displayed before",
    "Put the one before this back up",
    "Rewind to the last scripture",
    "Back one please",
    "Move back to the previous item",
    "Bring the previus verse back",
  ],
};

const TRANSLATIONS = [
  ["NIV", "Switch to NIV"],
  ["ESV", "Read it in the English Standard Version"],
  ["KJV", "Change to the King James Version"],
  ["NKJV", "Use the New King James Version"],
  ["NLT", "Can I have it in the NLT"],
  ["NET", "Use the New English Translation for this verse"],
  ["AMP", "Put that in the Amplified Bible"],
  ["MSG", "Could I see this from The Message"],
  ["SPARV", "Show that in Spanish"],
  ["AFR83", "Can we have it in Afrikaans"],
];

function fillTemplate(template, sermonIndex) {
  return template
    .replaceAll("{topic}", TOPICS[sermonIndex % TOPICS.length])
    .replaceAll("{book}", BOOKS[(sermonIndex * 3) % BOOKS.length])
    .replaceAll("{name}", NAMES[(sermonIndex * 7) % NAMES.length]);
}

function syntheticCase({
  sermonIndex,
  lineIndex,
  speakerId,
  sermonId,
  split,
  text,
  expected,
}) {
  return {
    id: `synthetic-${sermonId}-line-${String(lineIndex + 1).padStart(2, "0")}`,
    split,
    family: `synthetic-${speakerId}`,
    text,
    expected,
    synthetic: true,
    speakerId,
    sermonId,
    source: "deterministic-template",
    sequence: sermonIndex * 100 + lineIndex,
  };
}

function buildSyntheticSermon(sermonIndex) {
  const speakerIndex = Math.floor(sermonIndex / 2);
  const speakerId = `speaker-${String(speakerIndex + 1).padStart(2, "0")}`;
  const sermonId = `sermon-${String(sermonIndex + 1).padStart(3, "0")}`;
  const split = speakerIndex < 40 ? "train" : "validation";
  const utterances = [];

  for (let lineIndex = 0; lineIndex < 12; lineIndex += 1) {
    const templateIndex =
      (sermonIndex * 5 + lineIndex * 7) % ORDINARY_TEMPLATES.length;
    utterances.push({
      text: fillTemplate(ORDINARY_TEMPLATES[templateIndex], sermonIndex),
      expected: { intent: "none" },
    });
  }

  for (const [offset, intent] of [
    [0, "hide"],
    [1, "show"],
    [2, "next"],
    [3, "previous"],
  ]) {
    const templates = COMMAND_TEMPLATES[intent];
    utterances.push({
      text: templates[(sermonIndex + offset * 3) % templates.length],
      expected: { intent },
    });
  }
  const [translation, text] = TRANSLATIONS[sermonIndex % TRANSLATIONS.length];
  utterances.push({
    text,
    expected: { intent: "switch_translation", translation },
  });

  return {
    sermonIndex,
    sermonId,
    speakerId,
    split,
    synthetic: true,
    utterances,
  };
}

export function buildSyntheticTranscripts() {
  return Array.from({ length: 100 }, (_, sermonIndex) =>
    buildSyntheticSermon(sermonIndex),
  );
}

export function buildGeneratedCommandCorpus() {
  const authoredCases = JSON.parse(readFileSync(AUTHORED_CORPUS_PATH, "utf8"));
  const syntheticCases = buildSyntheticTranscripts().flatMap((transcript) => {
    const selectedLines = [
      transcript.sermonIndex % 12,
      12 + (transcript.sermonIndex % 5),
    ];
    return selectedLines.map((lineIndex) =>
      syntheticCase({
        ...transcript,
        lineIndex,
        ...transcript.utterances[lineIndex],
      }),
    );
  });

  return [...syntheticCases, ...authoredCases];
}

export function writeGeneratedCommandCorpus() {
  const transcripts = buildSyntheticTranscripts();
  const corpus = buildGeneratedCommandCorpus();
  writeFileSync(
    GENERATED_TRANSCRIPTS_PATH,
    `${JSON.stringify(transcripts, null, 2)}\n`,
  );
  writeFileSync(GENERATED_CORPUS_PATH, `${JSON.stringify(corpus, null, 2)}\n`);
  return { corpus, transcripts };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { corpus, transcripts } = writeGeneratedCommandCorpus();
  const synthetic = corpus.filter((entry) => entry.synthetic);
  const speakers = new Set(synthetic.map((entry) => entry.speakerId)).size;
  console.log(
    `Generated ${transcripts.length} transcripts from ${speakers} isolated speakers (${corpus.length} benchmark cases).`,
  );
}
