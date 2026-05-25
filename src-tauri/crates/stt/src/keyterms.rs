/// Returns Bible book names, common abbreviations, spoken forms, and theological terms
/// for use as Deepgram keyword boosting.
#[allow(clippy::too_many_lines)]
pub fn bible_keyterms() -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();

    // 66 Bible book names
    let books = [
        "Genesis",
        "Exodus",
        "Leviticus",
        "Numbers",
        "Deuteronomy",
        "Joshua",
        "Judges",
        "Ruth",
        "1 Samuel",
        "2 Samuel",
        "1 Kings",
        "2 Kings",
        "1 Chronicles",
        "2 Chronicles",
        "Ezra",
        "Nehemiah",
        "Esther",
        "Job",
        "Psalms",
        "Proverbs",
        "Ecclesiastes",
        "Song of Solomon",
        "Isaiah",
        "Jeremiah",
        "Lamentations",
        "Ezekiel",
        "Daniel",
        "Hosea",
        "Joel",
        "Amos",
        "Obadiah",
        "Jonah",
        "Micah",
        "Nahum",
        "Habakkuk",
        "Zephaniah",
        "Haggai",
        "Zechariah",
        "Malachi",
        "Matthew",
        "Mark",
        "Luke",
        "John",
        "Acts",
        "Romans",
        "1 Corinthians",
        "2 Corinthians",
        "Galatians",
        "Ephesians",
        "Philippians",
        "Colossians",
        "1 Thessalonians",
        "2 Thessalonians",
        "1 Timothy",
        "2 Timothy",
        "Titus",
        "Philemon",
        "Hebrews",
        "James",
        "1 Peter",
        "2 Peter",
        "1 John",
        "2 John",
        "3 John",
        "Jude",
        "Revelation",
    ];
    terms.extend(books.iter().map(ToString::to_string));

    // Spoken forms. These matter more than abbreviations for microphone input.
    let spoken = [
        "First Samuel",
        "Second Samuel",
        "First Kings",
        "Second Kings",
        "First Chronicles",
        "Second Chronicles",
        "First Corinthians",
        "Second Corinthians",
        "First Thessalonians",
        "Second Thessalonians",
        "First Timothy",
        "Second Timothy",
        "First Peter",
        "Second Peter",
        "First John",
        "Second John",
        "Third John",
        "Song of Songs",
    ];
    terms.extend(spoken.iter().map(ToString::to_string));

    // Theological terms
    let theological = [
        "justification",
        "sanctification",
        "propitiation",
        "eschatology",
        "atonement",
        "redemption",
        "righteousness",
        "covenant",
        "baptism",
        "resurrection",
        "crucifixion",
        "salvation",
        "repentance",
        "grace",
        "mercy",
        "forgiveness",
        "reconciliation",
        "glorification",
        "predestination",
        "sovereignty",
        "omniscience",
        "omnipotence",
        "trinity",
        "incarnation",
        "ascension",
        "transfiguration",
        "beatitudes",
        "tabernacle",
        "ark of the covenant",
        "Melchizedek",
        "Nebuchadnezzar",
    ];
    terms.extend(theological.iter().map(ToString::to_string));

    // Common abbreviations. These come last so the 100-keyterm cap prioritizes
    // words and phrases a speaker is likely to say aloud.
    let abbreviations = [
        "Gen", "Exod", "Lev", "Num", "Deut", "Josh", "Judg", "Sam", "Kgs", "Chr", "Neh", "Esth",
        "Ps", "Prov", "Eccl", "Isa", "Jer", "Lam", "Ezek", "Dan", "Hos", "Obad", "Mic", "Nah",
        "Hab", "Zeph", "Hag", "Zech", "Mal", "Matt", "Mk", "Lk", "Jn", "Rom", "Cor", "Gal", "Eph",
        "Phil", "Col", "Thess", "Tim", "Tit", "Phlm", "Heb", "Jas", "Pet", "Rev",
    ];
    terms.extend(abbreviations.iter().map(ToString::to_string));

    terms
}

/// Returns a verse-only term list for the Vosk constrained grammar.
///
/// Includes canonical and spoken book names, parseable number words
/// (including `hundred` and `and`), and verse-navigation keywords.
/// Keeps a narrow set of explicit voice-control and translation terms
/// that the app already supports so local Vosk does not silently lose
/// those workflows.
/// Excludes `[unk]`, theological/worship terms, and general dictation
/// vocabulary so that Vosk narrows transcript coverage to Bible-
/// reference language.
#[allow(clippy::too_many_lines)]
pub fn verse_only_keyterms() -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();

    // 66 Bible book names (lowercased for grammar matching)
    let books = [
        "genesis",
        "exodus",
        "leviticus",
        "numbers",
        "deuteronomy",
        "joshua",
        "judges",
        "ruth",
        "1 samuel",
        "2 samuel",
        "1 kings",
        "2 kings",
        "1 chronicles",
        "2 chronicles",
        "ezra",
        "nehemiah",
        "esther",
        "job",
        "psalms",
        "psalm",
        "proverbs",
        "ecclesiastes",
        "song of solomon",
        "isaiah",
        "jeremiah",
        "lamentations",
        "ezekiel",
        "daniel",
        "hosea",
        "joel",
        "amos",
        "obadiah",
        "jonah",
        "micah",
        "nahum",
        "habakkuk",
        "zephaniah",
        "haggai",
        "zechariah",
        "malachi",
        "matthew",
        "mark",
        "luke",
        "john",
        "acts",
        "romans",
        "1 corinthians",
        "2 corinthians",
        "galatians",
        "ephesians",
        "philippians",
        "colossians",
        "1 thessalonians",
        "2 thessalonians",
        "1 timothy",
        "2 timothy",
        "titus",
        "philemon",
        "hebrews",
        "james",
        "1 peter",
        "2 peter",
        "1 john",
        "2 john",
        "3 john",
        "jude",
        "revelation",
    ];
    terms.extend(books.iter().map(ToString::to_string));

    // Spoken book forms
    let spoken = [
        "first samuel",
        "second samuel",
        "first kings",
        "second kings",
        "first chronicles",
        "second chronicles",
        "first corinthians",
        "second corinthians",
        "first thessalonians",
        "second thessalonians",
        "first timothy",
        "second timothy",
        "first peter",
        "second peter",
        "first john",
        "second john",
        "third john",
        "song of songs",
    ];
    terms.extend(spoken.iter().map(ToString::to_string));

    // Number words for chapter/verse parsing, including high-chapter support
    let numbers = [
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
        "thirty",
        "forty",
        "fifty",
        "sixty",
        "seventy",
        "eighty",
        "ninety",
        "hundred",
        "and",
    ];
    terms.extend(numbers.iter().map(ToString::to_string));

    // Verse-navigation and range keywords
    let navigation = [
        "chapter", "verse", "verses", "to", "through", "next", "previous",
    ];
    terms.extend(navigation.iter().map(ToString::to_string));

    // Keep the exact local voice-control commands that the app already supports.
    let voice_control = ["stop", "start", "transcribing", "stop transcribing"];
    terms.extend(voice_control.iter().map(ToString::to_string));

    // Minimal translation-command support so local Vosk still recognizes
    // the narrow command phrases that the app handles today.
    let translation_command_words = [
        "read",
        "switch",
        "show",
        "give",
        "in",
        "version",
        "translation",
        "bible",
    ];
    terms.extend(translation_command_words.iter().map(ToString::to_string));

    let translations = [
        "niv",
        "new international version",
        "esv",
        "english standard version",
        "nasb",
        "new american standard",
        "nkjv",
        "new king james",
        "kjv",
        "king james version",
        "king james",
        "nlt",
        "new living translation",
        "amp",
        "amplified",
        "amplified bible",
        "msg",
        "message",
        "the message",
        "csb",
        "christian standard bible",
        "hcsb",
        "holman christian standard",
        "rsv",
        "revised standard version",
        "nrsv",
        "new revised standard version",
        "net",
        "new english translation",
        "gnt",
        "good news translation",
        "gnb",
        "good news bible",
        "cev",
        "contemporary english version",
        "spanish",
        "reina valera",
        "french",
        "darby french",
        "portuguese",
        "biblia livre",
    ];
    terms.extend(translations.iter().map(ToString::to_string));

    terms.sort();
    terms.dedup();
    terms
}
