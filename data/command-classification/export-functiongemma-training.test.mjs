import assert from "node:assert/strict";
import test from "node:test";

import {
  conversationForCase,
  tools,
} from "./export-functiongemma-training.mjs";

test("translation case exports one closed tool call", () => {
  const conversation = conversationForCase({
    id: "translation",
    family: "translation",
    text: "show it in NIV",
    expected: { intent: "switch_translation", translation: "NIV" },
  });

  assert.deepEqual(
    conversation.messages[2].tool_calls[0].function.arguments,
    { translation: "NIV" },
  );
  assert.equal(conversation.tools, tools);
});

test("ordinary speech exports no tool call", () => {
  const conversation = conversationForCase({
    id: "ordinary",
    family: "ordinary",
    text: "show kindness to one another",
    expected: { intent: "none" },
  });

  assert.equal(conversation.messages[2].content, "NONE");
  assert.equal("tool_calls" in conversation.messages[2], false);
});
