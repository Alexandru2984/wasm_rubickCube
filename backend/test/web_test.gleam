import cube_leaderboard/store
import cube_leaderboard/web.{Context, Request}
import gleam/dynamic/decode
import gleam/int
import gleam/json
import gleam/string

const origin = "https://cube.micutu.com"

const day = 20_395

/// Kociemba solution of day 20395's scramble (22 moves).
const solution = "R F2 D' R' B' R' U' B' R B' L2 D2 F2 U R2 U' F2 L2 U' B2 L2 U"

fn ctx_at(now_day: Int) -> web.Context {
  let assert Ok(started) = store.start(store.new(fn(_) { Ok(Nil) }))
  // Noon UTC, so "yesterday" and "tomorrow" are unambiguous.
  let now = now_day * web.day_ms + web.day_ms / 2
  Context(started.data, origin, now, counter())
}

fn counter() -> fn() -> String {
  // Unique enough for a test: 16 hex digits from the monotonic clock.
  fn() {
    let n = unique()
    string.pad_start(int.to_base16(n), 16, "0") |> string.lowercase
  }
}

@external(erlang, "erlang", "unique_integer")
fn unique_integer(options: List(UniqueOption)) -> Int

type UniqueOption {
  Positive
  Monotonic
}

fn unique() -> Int {
  unique_integer([Positive, Monotonic])
}

fn token(n: Int) -> String {
  string.pad_start(int.to_base16(n), 32, "0") |> string.lowercase
}

fn body(name: String, tok: String, ms: Int, sol: String) -> BitArray {
  let text =
    json.object([
      #("name", json.string(name)),
      #("token", json.string(tok)),
      #("ms", json.int(ms)),
      #("sol", json.string(sol)),
    ])
    |> json.to_string
  <<text:utf8>>
}

fn post(ctx, path: String, payload: BitArray) -> #(Int, String) {
  web.handle(
    ctx,
    Request("POST", path, "", "application/json", origin, "1.2.3.4", payload),
  )
}

fn post_from(ctx, ip: String, payload: BitArray) -> #(Int, String) {
  web.handle(
    ctx,
    Request(
      "POST",
      "/api/daily/20395",
      "",
      "application/json",
      origin,
      ip,
      payload,
    ),
  )
}

fn get(ctx, path: String, query: String) -> #(Int, String) {
  web.handle(ctx, Request("GET", path, query, "", "", "1.2.3.4", <<>>))
}

fn field(json_text: String, name: String, of: decode.Decoder(a)) -> a {
  let assert Ok(value) = json.parse(json_text, decode.at([name], of))
  value
}

fn error_code(response: #(Int, String)) -> String {
  field(response.1, "error", decode.string)
}

pub fn a_real_solve_is_ranked_test() {
  let ctx = ctx_at(day)
  let #(status, text) =
    post(ctx, "/api/daily/20395", body("Ana", token(1), 25_000, solution))
  assert status == 201
  assert field(text, "rank", decode.int) == 1
  assert field(text, "moves", decode.int) == 22
  assert field(text, "fresh", decode.bool)
  let id = field(text, "id", decode.string)

  let #(status, text) = get(ctx, "/api/daily/20395", "me=" <> id)
  assert status == 200
  assert field(text, "total", decode.int) == 1
  assert field(text, "today", decode.int) == day
  let me = decode.at(["me", "name"], decode.string)
  let assert Ok("Ana") = json.parse(text, me)
}

pub fn rotations_and_slices_in_the_log_still_verify_test() {
  let ctx = ctx_at(day)
  // Moves that cancel out, rotations included, don't break the proof.
  let padded = "x x' M M' " <> solution <> " y2 S2 S2"
  let #(status, text) =
    post(ctx, "/api/daily/20395", body("Bob", token(2), 30_000, padded))
  assert status == 201
  // x, x', y2 are free; M M' S2 S2 count.
  assert field(text, "moves", decode.int) == 26
}

pub fn an_unsolved_result_is_refused_test() {
  let ctx = ctx_at(day)
  let short = string.drop_end(solution, 2)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Cheat", token(3), 25_000, short),
    ))
    == "unsolved"
  // Yesterday's solution doesn't solve today's scramble.
  let ctx = ctx_at(day + 1)
  assert error_code(post(
      ctx,
      "/api/daily/20396",
      body("Cheat", token(3), 25_000, solution),
    ))
    == "unsolved"
}

pub fn only_today_and_yesterday_are_open_test() {
  let ctx = ctx_at(day + 1)
  let #(status, _) =
    post(ctx, "/api/daily/20395", body("Late", token(4), 25_000, solution))
  assert status == 201
  let ctx = ctx_at(day + 2)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Later", token(5), 25_000, solution),
    ))
    == "day_closed"
  let ctx = ctx_at(day - 1)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Early", token(6), 25_000, solution),
    ))
    == "day_closed"
}

pub fn implausible_times_are_refused_test() {
  let ctx = ctx_at(day)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Fast", token(7), 2000, solution),
    ))
    == "time"
  // 22 + 60 = 82 turns in 5 s is 16.4 TPS.
  let padded = solution <> string.repeat(" R R'", 30)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Bot", token(8), 5000, padded),
    ))
    == "time"
  let #(status, _) =
    post(ctx, "/api/daily/20395", body("Quick", token(8), 6000, padded))
  assert status == 201
}

pub fn bad_input_is_refused_test() {
  let ctx = ctx_at(day)
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("x", token(9), 25_000, solution),
    ))
    == "name"
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Ok name", "NOTHEX", 25_000, solution),
    ))
    == "bad_request"
  assert error_code(post(ctx, "/api/daily/20395", <<"{not json":utf8>>))
    == "bad_request"
  assert error_code(post(ctx, "/api/daily/20395", <<0xFF, 0xFE>>))
    == "bad_request"
  assert error_code(post(
      ctx,
      "/api/daily/20395",
      body("Ok name", token(9), 25_000, "R U (R)"),
    ))
    == "solution"
  let wrong_type =
    web.handle(
      ctx,
      Request(
        "POST",
        "/api/daily/20395",
        "",
        "text/plain",
        origin,
        "1.2.3.4",
        body("Ana", token(9), 25_000, solution),
      ),
    )
  assert error_code(wrong_type) == "content_type"
  let wrong_origin =
    web.handle(
      ctx,
      Request(
        "POST",
        "/api/daily/20395",
        "",
        "application/json",
        "https://evil.example",
        "1.2.3.4",
        body("Ana", token(9), 25_000, solution),
      ),
    )
  assert error_code(wrong_origin) == "origin"
}

pub fn fractional_milliseconds_are_rounded_test() {
  let ctx = ctx_at(day)
  let payload = <<
    "{\"name\":\"Frac\",\"token\":\"":utf8,
    token(10):utf8,
    "\",\"ms\":25000.6,\"sol\":\"":utf8,
    solution:utf8,
    "\"}":utf8,
  >>
  let #(status, text) = post(ctx, "/api/daily/20395", payload)
  assert status == 201
  assert field(text, "ms", decode.int) == 25_001
}

pub fn resubmitting_returns_the_first_entry_test() {
  let ctx = ctx_at(day)
  let assert #(201, first) =
    post(ctx, "/api/daily/20395", body("Ana", token(11), 25_000, solution))
  let #(status, again) =
    post(ctx, "/api/daily/20395", body("Ana", token(11), 9000, solution))
  assert status == 200
  assert field(again, "id", decode.string) == field(first, "id", decode.string)
  assert field(again, "ms", decode.int) == 25_000
  assert !field(again, "fresh", decode.bool)
}

pub fn an_address_gets_five_entries_a_day_test() {
  let ctx = ctx_at(day)
  int.range(0, store.max_per_ip, Nil, fn(_, i) {
    let #(status, _) =
      post_from(ctx, "9.9.9.9", body("Ana", token(100 + i), 25_000, solution))
    assert status == 201
  })
  let #(status, text) =
    post_from(ctx, "9.9.9.9", body("Ana", token(200), 25_000, solution))
  assert status == 429
  assert field(text, "error", decode.string) == "limit"
}

pub fn routing_test() {
  let ctx = ctx_at(day)
  assert get(ctx, "/api/health", "").0 == 200
  assert get(ctx, "/api/nope", "").0 == 404
  assert get(ctx, "/api/daily/abc", "").0 == 404
  assert get(ctx, "/api/daily/-1", "").0 == 404
  assert get(ctx, "/api/daily/+20395", "").0 == 404
  assert get(ctx, "/api/daily/1234567", "").0 == 404
  // Outside the served window.
  assert get(ctx, "/api/daily/20000", "").0 == 404
  assert get(ctx, "/api/daily/20397", "").0 == 404
  assert get(ctx, "/api/daily/20396", "").0 == 200
  // A malformed ?me= is ignored, not an error.
  let assert #(200, text) = get(ctx, "/api/daily/20395", "me=../../etc&x=%ZZ")
  let assert Ok(Nil) = json.parse(text, decode.at(["me"], decode.success(Nil)))
  let delete =
    web.handle(
      ctx,
      Request("DELETE", "/api/daily/20395", "", "", "", "1.2.3.4", <<>>),
    )
  assert delete.0 == 405
}
