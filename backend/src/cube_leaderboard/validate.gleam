//// Input checks for a submission. Everything here is pure, so it's tested
//// directly (test/validate_test.gleam).

import gleam/bool
import gleam/list
import gleam/string

pub const min_name = 2

pub const max_name = 16

/// Faster than any 3x3 single on record; anything quicker is a script.
pub const min_ms = 3000

/// An hour: the engine's timer keeps running, but nobody ranks that.
pub const max_ms = 3_600_000

/// Turns per second above which a solve is treated as automated
/// (the fastest humans peak around 12–14 for a full solve).
pub const max_tps = 15

/// Display name: trimmed, inner spaces collapsed, 2–16 characters of
/// ASCII letters/digits, space, `. _ - '` and Latin letters with
/// diacritics (U+00C0–U+024F, so Romanian ă â î ș ț and most European
/// names fit). No emoji, symbols, control or bidi characters, so a name
/// can't impersonate layout or reorder text around it.
pub fn name(raw: String) -> Result(String, Nil) {
  let cleaned =
    raw
    |> string.split(" ")
    |> list.filter(fn(part) { part != "" })
    |> string.join(" ")
  let points = string.to_utf_codepoints(cleaned)
  let length = list.length(points)
  use <- bool.guard(length < min_name || length > max_name, Error(Nil))
  use <- bool.guard(!list.all(points, allowed), Error(Nil))
  use <- bool.guard(!list.any(points, alphanumeric), Error(Nil))
  use <- bool.guard(offensive(cleaned), Error(Nil))
  Ok(cleaned)
}

/// The client's secret: 128 random bits as 32 lowercase hex digits.
pub fn token(raw: String) -> Bool {
  string.length(raw) == 32
  && list.all(string.to_utf_codepoints(raw), fn(cp) {
    let c = string.utf_codepoint_to_int(cp)
    { c >= 0x30 && c <= 0x39 } || { c >= 0x61 && c <= 0x66 }
  })
}

/// An entry id as returned on submit: 16 lowercase hex digits.
pub fn entry_id(raw: String) -> Bool {
  string.length(raw) == 16
  && list.all(string.to_utf_codepoints(raw), fn(cp) {
    let c = string.utf_codepoint_to_int(cp)
    { c >= 0x30 && c <= 0x39 } || { c >= 0x61 && c <= 0x66 }
  })
}

pub fn time(ms: Int) -> Bool {
  ms >= min_ms && ms <= max_ms
}

/// `moves / seconds <= max_tps`, in integers.
pub fn pace(moves: Int, ms: Int) -> Bool {
  moves * 1000 <= max_tps * ms
}

fn allowed(cp: UtfCodepoint) -> Bool {
  let c = string.utf_codepoint_to_int(cp)
  alphanumeric(cp)
  || c == 0x20
  || c == 0x2E
  || c == 0x5F
  || c == 0x2D
  || c == 0x27
}

fn alphanumeric(cp: UtfCodepoint) -> Bool {
  let c = string.utf_codepoint_to_int(cp)
  { c >= 0x30 && c <= 0x39 }
  || { c >= 0x41 && c <= 0x5A }
  || { c >= 0x61 && c <= 0x7A }
  // Latin-1 Supplement + Latin Extended-A/B letters, minus × and ÷.
  || { c >= 0xC0 && c <= 0x24F && c != 0xD7 && c != 0xF7 }
}

// --- Name filter -----------------------------------------------------------
//
// Best effort, not a moderation system: names are public, so the worst
// slurs and obscenities (Romanian and English) are refused, including
// with diacritics, leetspeak or separators. Anything that slips through is
// removed with deploy/backend/cube-lb-moderate.

/// Matched anywhere, after folding: long or distinctive enough not to
/// occur inside ordinary words or names.
const anywhere = [
  "fuck", "nigger", "nigga", "faggot", "cunt", "whore", "bitch", "asshole",
  "dickhead", "motherfuck", "retard", "hitler", "pizda", "labagiu", "poponar",
  "bulangiu", "cacat", "sugipula", "dutezincur", "futumi", "futui", "mortiitai",
  "mortiimati",
]

/// Matched as whole words only (they occur inside innocent words:
/// "popular" contains "pula", "curvature" contains "curva", "muiere"
/// contains "muie").
const words = [
  "pula", "curva", "muie", "fut", "futu", "fute", "sugi", "sex", "porn", "nazi",
  "rape", "slut", "dick", "cock", "pussy", "fag", "tigan", "cur",
]

pub fn offensive(name: String) -> Bool {
  let folded = fold(name)
  let squashed = string.concat(folded)
  list.any(anywhere, fn(bad) { string.contains(squashed, fold_word(bad)) })
  || list.any(folded, fn(word) {
    list.any(words, fn(bad) { word == fold_word(bad) })
  })
}

/// Lowercase, strip diacritics, undo common leetspeak, and split into
/// words on spaces and punctuation.
fn fold(name: String) -> List(String) {
  name
  |> string.lowercase
  |> string.to_graphemes
  |> list.map(fold_char)
  |> string.concat
  |> string.split(" ")
  |> list.filter(fn(w) { w != "" })
}

fn fold_word(word: String) -> String {
  word |> fold |> string.concat
}

fn fold_char(ch: String) -> String {
  case ch {
    "." | "_" | "-" | "'" -> " "
    "0" -> "o"
    "1" -> "i"
    "3" -> "e"
    "4" -> "a"
    "5" -> "s"
    "7" -> "t"
    "8" -> "b"
    "à" | "á" | "â" | "ã" | "ä" | "å" | "ā" | "ă" | "ą" -> "a"
    "ç" | "ć" | "ĉ" | "ċ" | "č" -> "c"
    "ď" | "đ" -> "d"
    "è" | "é" | "ê" | "ë" | "ē" | "ĕ" | "ė" | "ę" | "ě" -> "e"
    "ĝ" | "ğ" | "ġ" | "ģ" -> "g"
    "ì" | "í" | "î" | "ï" | "ĩ" | "ī" | "ĭ" | "į" | "ı" -> "i"
    "ł" | "ľ" | "ĺ" | "ļ" -> "l"
    "ñ" | "ń" | "ņ" | "ň" -> "n"
    "ò" | "ó" | "ô" | "õ" | "ö" | "ø" | "ō" | "ŏ" | "ő" -> "o"
    "ŕ" | "ř" -> "r"
    "ś" | "ŝ" | "ş" | "š" | "ș" -> "s"
    "ţ" | "ť" | "ț" -> "t"
    "ù" | "ú" | "û" | "ü" | "ũ" | "ū" | "ŭ" | "ů" | "ű" | "ų" -> "u"
    "ý" | "ÿ" -> "y"
    "ź" | "ż" | "ž" -> "z"
    other -> other
  }
}
