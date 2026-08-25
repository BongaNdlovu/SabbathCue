import { SITE } from "../../_lib/site";
import { FAQS } from "../sections/faq-section.data";

const ORG_ID = `${SITE.url}/#organization`;
const SITE_ID = `${SITE.url}/#website`;
const APP_ID = `${SITE.url}/#software`;
const FAQ_ID = `${SITE.url}/#faq`;

export function StructuredData() {
  const graph = {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "Organization",
        "@id": ORG_ID,
        name: SITE.legalName,
        alternateName: SITE.name,
        url: SITE.url,
        logo: {
          "@type": "ImageObject",
          url: `${SITE.url}/sabbathcue-logo.png`,
          width: 1024,
          height: 1024,
        },
        sameAs: [SITE.socials.github],
        foundingDate: SITE.founded,
      },
      {
        "@type": "WebSite",
        "@id": SITE_ID,
        url: SITE.url,
        name: SITE.name,
        description: SITE.description,
        inLanguage: "en",
        publisher: { "@id": ORG_ID },
      },
      {
        "@type": "SoftwareApplication",
        "@id": APP_ID,
        name: SITE.name,
        url: SITE.url,
        description: SITE.description,
        applicationCategory: "MultimediaApplication",
        operatingSystem: SITE.operatingSystems.join(", "),
        downloadUrl: SITE.repo.download,
        installUrl: SITE.repo.download,
        softwareVersion: SITE.repo.installerVersion,
        license: "https://opensource.org/licenses/MIT",
        isAccessibleForFree: true,
        offers: {
          "@type": "Offer",
          price: "0",
          priceCurrency: "USD",
          availability: "https://schema.org/InStock",
        },
        publisher: { "@id": ORG_ID },
        featureList: [
          "Real-time speech transcription from live sermon audio (offline Vosk or cloud STT)",
          "Automatic Bible verse and Ellen G. White reference detection from citations and quotes",
          "Voice-controlled live translation switching during sermons",
          "Reading Mode with hands-free voice navigation",
          "Guided Projector Setup for physical screens and multi-monitor setups",
          "Broadcast-ready transparent scripture overlays via NDI 6 for OBS and vMix",
          "Optional AI candidate ranking with DeepSeek and Cerebras",
          "Remote control via OSC and REST HTTP API",
        ],
        keywords:
          "Bible verse detection, sermon transcription, Ellen G White, NDI overlay, church projector software, live scripture projection, worship media",
      },
      {
        "@type": "FAQPage",
        "@id": FAQ_ID,
        mainEntity: FAQS.map((f) => ({
          "@type": "Question",
          name: f.question,
          acceptedAnswer: {
            "@type": "Answer",
            text: f.answer,
          },
        })),
      },
    ],
  };

  return (
    <script
      type="application/ld+json"
      dangerouslySetInnerHTML={{ __html: JSON.stringify(graph) }}
    />
  );
}
