import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

export const tools = [
  tool("hide_output", "Hide or clear the current projected content"),
  tool("show_output", "Show or restore the current projected content"),
  tool("next_item", "Advance to the next verse or presentation item"),
  tool(
    "previous_item",
    "Return to the previous verse or presentation item",
  ),
  tool("switch_translation", "Change the Bible translation", {
    translation: {
      type: "string",
      enum: [
        "NIV",
        "ESV",
        "KJV",
        "NKJV",
        "NLT",
        "NET",
        "AMP",
        "MSG",
        "SPARV",
        "AFR83",
      ],
    },
  }),
];

const systemMessage =
  "Classify only explicit operator commands. Ordinary sermon speech must produce no tool call. Never infer or emit a Bible reference.";

function tool(name, description, properties = {}) {
  return {
    type: "function",
    function: {
      name,
      description,
      parameters: {
        type: "object",
        properties,
        required: name === "switch_translation" ? ["translation"] : [],
        additionalProperties: false,
      },
    },
  };
}

export function conversationForCase(testCase) {
  const assistant =
    testCase.expected.intent === "none"
      ? { role: "assistant", content: "NONE" }
      : {
          role: "assistant",
          tool_calls: [
            {
              type: "function",
              function: {
                name: toolName(testCase.expected.intent),
                arguments:
                  testCase.expected.intent === "switch_translation"
                    ? { translation: testCase.expected.translation }
                    : {},
              },
            },
          ],
        };

  return {
    id: testCase.id,
    family: testCase.family,
    messages: [
      { role: "developer", content: systemMessage },
      { role: "user", content: testCase.text },
      assistant,
    ],
    tools,
  };
}

function toolName(intent) {
  const names = {
    hide: "hide_output",
    show: "show_output",
    next: "next_item",
    previous: "previous_item",
    switch_translation: "switch_translation",
  };
  const name = names[intent];
  if (!name) {
    throw new Error(`Unsupported command intent: ${intent}`);
  }
  return name;
}

async function writeJsonl(path, rows) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, `${rows.map((row) => JSON.stringify(row)).join("\n")}\n`);
}

async function main() {
  const corpusPath = resolve(
    process.argv[2] ??
      "data/command-classification/command-cases.json",
  );
  const outputDirectory = resolve(
    process.argv[3] ??
      "src-tauri/target/functiongemma-training",
  );
  const cases = JSON.parse(await readFile(corpusPath, "utf8"));
  const training = cases
    .filter((testCase) => testCase.split === "train")
    .map(conversationForCase);
  const validation = cases
    .filter((testCase) => testCase.split === "validation")
    .map(conversationForCase);

  await writeJsonl(resolve(outputDirectory, "train.jsonl"), training);
  await writeJsonl(resolve(outputDirectory, "validation.jsonl"), validation);
  console.log(
    `Exported ${training.length} training and ${validation.length} validation conversations to ${outputDirectory}`,
  );
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  await main();
}
