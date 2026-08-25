export type Faq = { question: string; answer: string };

export const FAQS: readonly Faq[] = [
  {
    question: "What is SabbathCue?",
    answer:
      "SabbathCue is a real-time AI desktop application for church media teams and live broadcasts. It listens to sermon audio, transcribes speech live (offline with local Vosk or via cloud STT), detects Bible and Ellen G. White references with hybrid AI vector search, and outputs broadcast-ready overlays via NDI 6 alongside direct HDMI projector displays. It is built with Tauri v2, a React 19 frontend, and a high-performance Rust backend.",
  },
  {
    question: "Can SabbathCue run completely offline without an internet connection?",
    answer:
      "Yes. SabbathCue is built with an offline-first architecture. The desktop installer bundles the local Vosk speech-to-text engine, a local SQLite Bible and EGW database, and an INT8 ONNX vector embedding model. Your sermon audio never has to leave your local computer.",
  },
  {
    question: "What speech recognition (STT) options are supported?",
    answer:
      "SabbathCue defaults to local Vosk (free, zero API cost, offline). If you want managed cloud speech streaming, you can connect your own API key for Soniox, Deepgram (Nova-3), or Speechmatics from the in-app settings.",
  },
  {
    question: "What Bible translations and EGW books does SabbathCue support?",
    answer:
      "The public release includes public-domain translations: KJV, WEB (World English Bible), Reina-Valera 1909 (Spanish), J.N. Darby (French), and Biblia Livre (Portuguese), along with Afrikaans Bible support. It also indexes major Ellen G. White writings: Patriarchs and Prophets, Desire of Ages, The Great Controversy, Steps to Christ, and Education.",
  },
  {
    question: "Can I switch translations with my voice during a sermon?",
    answer:
      "Yes! You can say commands like 'read in NIV' or 'switch to ESV' live during a sermon. SabbathCue immediately updates the active display to the requested translation while keeping the current verse reference in view.",
  },
  {
    question: "What is Reading Mode and Voice Navigation?",
    answer:
      "When a pastor announces a chapter and begins expository reading ('Let us turn to Daniel chapter 1'), Reading Mode locks focus to that chapter to prevent false detections from common conversational phrases. You can navigate through the passage hands-free with voice commands like 'next verse', 'previous verse', 'chapter 3', or 'go back'.",
  },
  {
    question: "How do I connect SabbathCue to our projector or livestream?",
    answer:
      "For in-house displays, use Guided Projector Setup — a one-tap screen manager with screen identification, hot-plug detection, and display mode guidance. For livestreams, SabbathCue outputs native NDI 6 video with alpha transparency directly into OBS Studio, vMix, and Wirecast.",
  },
  {
    question: "What is optional AI Candidate Ranking?",
    answer:
      "When a pastor makes an indirect reference or complex analogy ('that passage where Elijah called down fire from heaven'), SabbathCue's optional AI candidate ranking can evaluate candidate verses locally surfaced using DeepSeek or Cerebras LLMs with your own API key to recommend the exact match.",
  },
  {
    question: "Can I control SabbathCue remotely from a tablet or Stream Deck?",
    answer:
      "Yes. SabbathCue includes an integrated OSC interface and a secure REST HTTP API, allowing you to trigger cues, switch translations, advance verses, and clear screens from an Elgato Stream Deck, TouchOSC, companion tablet, or custom automation script.",
  },
  {
    question: "How does the 14-day trial and subscription work?",
    answer:
      "You can download SabbathCue and start a full-featured 14-day free trial directly in the app with no credit card required. After the trial, you can subscribe via Paddle checkout or EFT (for South African churches) with monthly and discounted annual plans.",
  },
];
