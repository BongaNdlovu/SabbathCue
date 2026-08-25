import {
  IconAdjustmentsHorizontal,
  IconBook,
  IconBooks,
  IconDeviceTv,
  IconLanguage,
  IconMicrophone,
  IconPlug,
  IconPresentation,
  IconScreenShare,
  IconSearch,
  IconSparkles,
} from "@tabler/icons-react";
import { Container } from "../ui/container";
import { FeatureCard } from "../ui/feature-card";
import { Reveal } from "../ui/reveal";
import { SectionHeading } from "./section-heading";

import type { Icon as TablerIcon } from "@tabler/icons-react";

type Feature = {
  icon: TablerIcon;
  title: string;
  body: string;
};

const FEATURES: Feature[] = [
  {
    icon: IconMicrophone,
    title: "Offline & Cloud Speech-to-Text",
    body: "Runs offline locally with bundled Vosk at zero API cost, or streams live with cloud providers like Soniox, Deepgram, and Speechmatics.",
  },
  {
    icon: IconSearch,
    title: "Hybrid Scripture & Quote Detection",
    body: "Combines direct reference regex, MiniLM-L6-v2 ONNX multi-vector embeddings (155k+ vectors), and verbatim quotation matching for high accuracy.",
  },
  {
    icon: IconBooks,
    title: "Ellen G. White (EGW) Writings",
    body: "Built-in full-text and semantic search across Patriarchs and Prophets, Desire of Ages, Great Controversy, Steps to Christ, and Education.",
  },
  {
    icon: IconLanguage,
    title: "Voice-Controlled Translation Switching",
    body: "Say 'read in NIV' or 'switch to ESV' during your sermon to immediately reflow the live display without touching the mouse.",
  },
  {
    icon: IconBook,
    title: "Reading Mode with Voice Navigation",
    body: "Locks context to the active book and chapter while reading, with natural voice navigation ('next verse', 'previous verse', 'chapter 5').",
  },
  {
    icon: IconDeviceTv,
    title: "Guided Projector & Screen Setup",
    body: "One-tap live HDMI/projector output with screen identification, hot-plug display detection, and plain-language display mode guidance.",
  },
  {
    icon: IconScreenShare,
    title: "NDI 6 Broadcast Overlays",
    body: "Streams transparent lower-thirds directly to OBS Studio, vMix, and Wirecast with visual canvas theme styling and typography presets.",
  },
  {
    icon: IconSparkles,
    title: "Optional AI Candidate Ranking",
    body: "Optional DeepSeek and Cerebras LLM ranking disambiguates complex sermon analogies and indirect scripture references.",
  },
  {
    icon: IconPresentation,
    title: "Remote Control & Slide Imports",
    body: "Control via OSC and REST API (Stream Deck, tablet, phone) with built-in conversion of PowerPoint (.pptx) and PDF presentation slides.",
  },
];

export function FeaturesSection() {
  return (
    <section
      id="features"
      aria-labelledby="features-heading"
      className="py-20 lg:py-[120px]"
    >
      <Container className="flex flex-col gap-10 md:gap-14">
        <Reveal>
          <SectionHeading id="features-heading">
            Everything your media team needs
          </SectionHeading>
        </Reveal>
        <div className="grid grid-cols-1 md:grid-cols-2 md:[&>*]:-ml-px md:[&>*]:-mt-px lg:grid-cols-3">
          {FEATURES.map((f, i) => (
            <Reveal key={f.title} delay={(i % 3) * 80} className="flex">
              <FeatureCard
                icon={f.icon}
                title={f.title}
                body={f.body}
                iconTone="accent"
              />
            </Reveal>
          ))}
        </div>
      </Container>
    </section>
  );
}
