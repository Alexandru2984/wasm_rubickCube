import cube_leaderboard/validate
import gleam/string

pub fn names_are_trimmed_and_collapsed_test() {
  assert validate.name("  Ana   Maria ") == Ok("Ana Maria")
  assert validate.name("Alex") == Ok("Alex")
  assert validate.name("o'Neil_99.x-y") == Ok("o'Neil_99.x-y")
}

pub fn romanian_and_latin_names_are_fine_test() {
  assert validate.name("Ștefan Țăran") == Ok("Ștefan Țăran")
  assert validate.name("Îonuț") == Ok("Îonuț")
  assert validate.name("Zoë Müller") == Ok("Zoë Müller")
  assert validate.name("Łukasz") == Ok("Łukasz")
}

pub fn name_length_is_bounded_test() {
  assert validate.name("A") == Error(Nil)
  assert validate.name("  A  ") == Error(Nil)
  assert validate.name(string.repeat("a", 16)) == Ok(string.repeat("a", 16))
  assert validate.name(string.repeat("a", 17)) == Error(Nil)
  // Diacritics count as one character each.
  assert validate.name(string.repeat("ș", 16)) == Ok(string.repeat("ș", 16))
}

pub fn symbols_emoji_and_controls_are_refused_test() {
  assert validate.name("cube😀") == Error(Nil)
  assert validate.name("abc\u{202E}def") == Error(Nil)
  assert validate.name("abc\u{200B}def") == Error(Nil)
  assert validate.name("a<b>c") == Error(Nil)
  assert validate.name("tab\there") == Error(Nil)
  assert validate.name("line\nbreak") == Error(Nil)
  assert validate.name("2×3") == Error(Nil)
  assert validate.name("Ωmega") == Error(Nil)
  assert validate.name("---") == Error(Nil)
  assert validate.name("..") == Error(Nil)
}

pub fn offensive_names_are_refused_test() {
  assert validate.name("fuck") == Error(Nil)
  assert validate.name("F.u.c.k you") == Error(Nil)
  assert validate.name("FUCKER") == Error(Nil)
  assert validate.name("pula") == Error(Nil)
  assert validate.name("Pulă mare") == Error(Nil)
  assert validate.name("p1zd4") == Error(Nil)
  assert validate.name("sugi") == Error(Nil)
  assert validate.name("n1gg3r") == Error(Nil)
}

pub fn innocent_names_that_contain_bad_substrings_pass_test() {
  assert validate.name("Popular") == Ok("Popular")
  assert validate.name("Curvature") == Ok("Curvature")
  assert validate.name("Muiere") == Ok("Muiere")
  assert validate.name("Sussex") == Ok("Sussex")
  assert validate.name("Futurist") == Ok("Futurist")
  assert validate.name("Cureteanu") == Ok("Cureteanu")
}

pub fn tokens_are_32_lowercase_hex_test() {
  assert validate.token("0123456789abcdef0123456789abcdef")
  assert !validate.token("0123456789ABCDEF0123456789abcdef")
  assert !validate.token("0123456789abcdef0123456789abcde")
  assert !validate.token("0123456789abcdef0123456789abcdeg")
  assert !validate.token("")
}

pub fn time_and_pace_bounds_test() {
  assert !validate.time(2999)
  assert validate.time(3000)
  assert validate.time(3_600_000)
  assert !validate.time(3_600_001)
  // 15 turns per second is the ceiling.
  assert validate.pace(150, 10_000)
  assert !validate.pace(151, 10_000)
}
