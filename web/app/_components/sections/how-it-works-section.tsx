import { IconMicrophone, IconSearch, IconScreenShare } from "@tabler/icons-react";
import { Container } from "../ui/container";
import { FeatureCard } from "../ui/feature-card";
import { Reveal } from "../ui/reveal";
import { SectionHeading } from "./section-heading";

const STEPS = [
  {
    icon: IconMicrophone,
    title: "1. Listen",
    body: "Captures live sermon audio and transcribes in real time using local offline Vosk or streaming cloud STT (Soniox, Deepgram, Speechmatics).",
  },
  {
    icon: IconSearch,
    title: "2. Detect",
    body: "Instantly matches spoken citations, paraphrased passages, and EGW writings using direct parsing and ONNX multi-vector semantic embeddings.",
  },
  {
    icon: IconScreenShare,
    title: "3. Display",
    body: "Projects scriptures seamlessly to HDMI screens with Guided Projector Setup and streams transparent overlays to OBS Studio & vMix via NDI 6.",
  },
] as const;

export function HowItWorksSection() {
  return (
    <section
      id="how-it-works"
      aria-labelledby="how-it-works-heading"
      className="py-20 lg:py-[120px]"
    >
      <Container className="flex flex-col gap-10 md:gap-14 lg:gap-16">
        <Reveal>
          <SectionHeading id="how-it-works-heading">How it works</SectionHeading>
        </Reveal>
        <div className="grid grid-cols-1 md:grid-cols-3 md:[&>*]:-ml-px md:[&>*]:-mt-px">
          {STEPS.map((s, i) => (
            <Reveal key={s.title} delay={i * 80} className="flex">
              <FeatureCard
                icon={s.icon}
                title={s.title}
                body={s.body}
                iconTone="accent"
              />
            </Reveal>
          ))}
        </div>
      </Container>
    </section>
  );
}
