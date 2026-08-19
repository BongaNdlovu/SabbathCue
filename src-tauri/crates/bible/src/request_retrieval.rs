//! Grounded request retrieval planning for biblical topic and event searches.

/// A grounded request query or candidate strategy for narrative/topic retrieval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundedRequestCandidate {
    pub topic_id: &'static str,
    pub trigger_keywords: &'static [&'static str],
    pub secondary_keywords: &'static [&'static str],
    pub fts_queries: &'static [&'static str],
    pub expected_references: &'static [&'static str],
}

pub static GROUNDED_REQUEST_REGISTRY: &[GroundedRequestCandidate] = &[
    GroundedRequestCandidate {
        topic_id: "peter_in_prison",
        trigger_keywords: &["peter"],
        secondary_keywords: &["prison", "jail", "stocks"],
        fts_queries: &["Peter prison", "Peter was kept in prison"],
        expected_references: &["Acts 12:5"],
    },
    GroundedRequestCandidate {
        topic_id: "paul_and_silas",
        trigger_keywords: &["paul", "silas"],
        secondary_keywords: &["prison", "jail", "sing", "sang", "praises", "midnight"],
        fts_queries: &["Paul Silas", "Paul and Silas prayed"],
        expected_references: &["Acts 16:25"],
    },
    GroundedRequestCandidate {
        topic_id: "jesus_calms_storm",
        trigger_keywords: &["storm", "tempest", "wind", "calm"],
        secondary_keywords: &["boat", "ship", "sea", "calm", "waves", "storm"],
        fts_queries: &["rebuked the wind", "peace be still", "great calm", "wind ceased"],
        expected_references: &["Mark 4:39", "Matthew 8:26", "Luke 8:24"],
    },
    GroundedRequestCandidate {
        topic_id: "lazarus_resurrection",
        trigger_keywords: &["lazarus"],
        secondary_keywords: &["come out", "come forth", "grave", "dead", "tomb", "loud voice"],
        fts_queries: &["Lazarus come forth", "Lazarus", "loud voice Lazarus"],
        expected_references: &["John 11:43"],
    },
    GroundedRequestCandidate {
        topic_id: "born_again",
        trigger_keywords: &["born again", "nicodemus"],
        secondary_keywords: &[],
        fts_queries: &["\"born again\"", "except a man be born again"],
        expected_references: &["John 3:3"],
    },
    GroundedRequestCandidate {
        topic_id: "baptism_of_jesus",
        trigger_keywords: &["baptiz", "baptism"],
        secondary_keywords: &["jesus", "john", "jordan"],
        fts_queries: &["baptized Jesus", "baptized of him", "\"baptized\""],
        expected_references: &["Matthew 3:13", "Mark 1:9", "Luke 3:21"],
    },
    GroundedRequestCandidate {
        topic_id: "joseph_in_pit",
        trigger_keywords: &["joseph"],
        secondary_keywords: &["pit", "well", "cistern", "cast", "thrown"],
        fts_queries: &["Joseph pit", "cast him into a pit", "empty there was no water"],
        expected_references: &["Genesis 37:24"],
    },
    GroundedRequestCandidate {
        topic_id: "mark_of_the_beast",
        trigger_keywords: &["mark of the beast", "beast"],
        secondary_keywords: &["mark", "hand", "forehead", "buy or sell", "number"],
        fts_queries: &[
            "mark of the beast",
            "receive a mark",
            "mark in their right hand",
            "number of his name",
        ],
        expected_references: &["Revelation 13:16", "Revelation 13:17", "Revelation 14:9"],
    },
    GroundedRequestCandidate {
        topic_id: "jesus_walking_on_water",
        trigger_keywords: &[
            "walking on the sea",
            "walking on water",
            "walk on water",
            "walking on",
            "walk on",
        ],
        secondary_keywords: &["water", "sea", "ship", "boat"],
        fts_queries: &[
            "walking on the sea",
            "Jesus went unto them walking",
            "walking on water",
        ],
        expected_references: &["Matthew 14:25", "John 6:19", "Mark 6:48"],
    },
    GroundedRequestCandidate {
        topic_id: "such_a_time_as_this",
        trigger_keywords: &["such a time as this", "for such a time"],
        secondary_keywords: &[],
        fts_queries: &["for such a time as this", "come to the kingdom for such a time"],
        expected_references: &["Esther 4:14"],
    },
    GroundedRequestCandidate {
        topic_id: "amos_silence",
        trigger_keywords: &["prudent shall keep silence", "evil time"],
        secondary_keywords: &[],
        fts_queries: &["prudent shall keep silence", "evil time"],
        expected_references: &["Amos 5:13"],
    },
    GroundedRequestCandidate {
        topic_id: "armour_of_god",
        trigger_keywords: &["whole armour of god", "armour of god", "wiles of the devil"],
        secondary_keywords: &[],
        fts_queries: &["whole armour of God", "wiles of the devil"],
        expected_references: &["Ephesians 6:11"],
    },
    GroundedRequestCandidate {
        topic_id: "everlasting_gospel",
        trigger_keywords: &["everlasting gospel", "three angels"],
        secondary_keywords: &[],
        fts_queries: &["everlasting gospel", "fear God and give glory"],
        expected_references: &["Revelation 14:6"],
    },
    GroundedRequestCandidate {
        topic_id: "hope_of_glory",
        trigger_keywords: &["hope of glory"],
        secondary_keywords: &[],
        fts_queries: &["Christ in you the hope of glory", "hope of glory"],
        expected_references: &["Colossians 1:27"],
    },
    GroundedRequestCandidate {
        topic_id: "lamb_slain",
        trigger_keywords: &["lamb slain"],
        secondary_keywords: &["foundation", "world", "book of life"],
        fts_queries: &["Lamb slain from the foundation of the world", "lamb slain"],
        expected_references: &["Revelation 13:8"],
    },
];

/// A structured plan compiled from an incoming natural-language or speech request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestRetrievalPlan {
    pub normalized_query: String,
    pub direct_queries: Vec<String>,
    pub topic_candidates: Vec<GroundedRequestCandidate>,
}

impl RequestRetrievalPlan {
    pub fn plan(input: &str) -> Self {
        let normalized = crate::search::strip_conversational_preamble(input);
        let lower = normalized.to_ascii_lowercase();
        let mut candidates = Vec::new();
        let mut direct_queries = Vec::new();

        for candidate in GROUNDED_REQUEST_REGISTRY {
            let primary_match = candidate
                .trigger_keywords
                .iter()
                .any(|kw| lower.contains(kw));
            if !primary_match {
                continue;
            }

            let secondary_match = candidate.secondary_keywords.is_empty()
                || candidate
                    .secondary_keywords
                    .iter()
                    .any(|kw| lower.contains(kw));

            if secondary_match {
                candidates.push((*candidate).clone());
                for query in candidate.fts_queries {
                    if !direct_queries.contains(&query.to_string()) {
                        direct_queries.push(query.to_string());
                    }
                }
            }
        }

        Self {
            normalized_query: normalized,
            direct_queries,
            topic_candidates: candidates,
        }
    }

    pub fn fts_queries(&self) -> &[String] {
        &self.direct_queries
    }
}
