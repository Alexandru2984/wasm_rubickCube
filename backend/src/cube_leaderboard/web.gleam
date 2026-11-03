//// The HTTP API, behind nginx at https://cube.micutu.com/api/:
////
//// - `GET  /api/health`
//// - `GET  /api/daily/<day>[?me=<id>]` → top 50, total, and your entry
//// - `POST /api/daily/<day>` `{name, token, ms, sol}` → your id and rank
//// - `GET  /api/daily/<day>/solve/<rank>` → a top-10 solution, once the
////   day is final
////
//// A submission is only ranked after the server regenerates that day's
//// scramble, replays the reported solution on it and finds the cube
//// solved. The move count is the server's own; the time is the client's,
//// bounded by sanity checks (a browser can always lie about a clock).

import cube_leaderboard/cube
import cube_leaderboard/ffi
import cube_leaderboard/notation
import cube_leaderboard/scramble
import cube_leaderboard/store.{type Entry}
import cube_leaderboard/validate
import gleam/bit_array
import gleam/dynamic/decode
import gleam/erlang/process.{type Subject}
import gleam/float
import gleam/int
import gleam/io
import gleam/json.{type Json}
import gleam/list
import gleam/option
import gleam/result
import gleam/string
import gleam/uri

pub const day_ms = 86_400_000

/// Board size returned by GET.
pub const top = 50

/// GET serves this many past days (and tomorrow, for clocks running ahead).
pub const history_days = 60

/// Yesterday's challenge still takes submissions this long into today (a
/// solve that crossed midnight, a short outage); then the day is final and
/// its best solutions can be watched.
pub const grace_ms = 3_600_000

pub type Context {
  Context(
    store: Subject(store.Message),
    /// Required Origin on POST, e.g. "https://cube.micutu.com".
    origin: String,
    now: Int,
    new_id: fn() -> String,
  )
}

pub type Request {
  Request(
    method: String,
    path: String,
    query: String,
    content_type: String,
    origin: String,
    /// Already reduced by `ffi.ip_key`.
    ip: String,
    body: BitArray,
  )
}

pub type Config {
  Config(store: Subject(store.Message), origin: String)
}

/// Set once at startup; read by every request process.
pub fn configure(config: Config) -> Nil {
  put_config(config)
}

@external(erlang, "cube_leaderboard_ffi", "put_global")
fn put_global(key: String, value: a) -> Nil

@external(erlang, "cube_leaderboard_ffi", "get_global")
fn get_global(key: String) -> a

fn put_config(config: Config) -> Nil {
  put_global("config", config)
}

fn get_config() -> Config {
  get_global("config")
}

/// Entry point from cube_leaderboard_httpd.erl (plain binaries in,
/// status + JSON body out).
pub fn handle_raw(
  method: String,
  uri: String,
  content_type: String,
  origin: String,
  real_ip: String,
  body: BitArray,
) -> #(Int, String) {
  let config = get_config()
  let #(path, query) = string.split_once(uri, "?") |> result.unwrap(#(uri, ""))
  let ctx = Context(config.store, config.origin, ffi.now_ms(), ffi.random_id)
  handle(
    ctx,
    Request(
      method,
      path,
      query,
      content_type,
      origin,
      ffi.ip_key(real_ip),
      body,
    ),
  )
}

pub fn handle(ctx: Context, req: Request) -> #(Int, String) {
  case string.split(req.path, "/"), req.method {
    ["", "api", "health"], "GET" | ["", "api", "health"], "HEAD" ->
      reply(200, json.object([#("ok", json.bool(True))]))
    ["", "api", "health"], _ -> error(405, "method")
    ["", "api", "daily", day], method ->
      case parse_day(day), method {
        Error(_), _ -> error(404, "not_found")
        Ok(day), "GET" | Ok(day), "HEAD" -> get_board(ctx, req, day)
        Ok(day), "POST" -> post_solve(ctx, req, day)
        Ok(_), _ -> error(405, "method")
      }
    ["", "api", "daily", day, "solve", rank], method ->
      case parse_day(day), int.parse(rank), method {
        Ok(day), Ok(rank), "GET" | Ok(day), Ok(rank), "HEAD" ->
          get_solution(ctx, day, rank)
        Ok(_), Ok(_), _ -> error(405, "method")
        _, _, _ -> error(404, "not_found")
      }
    _, _ -> error(404, "not_found")
  }
}

fn get_board(ctx: Context, req: Request, day: Int) -> #(Int, String) {
  let today = ctx.now / day_ms
  case day < today - history_days || day > today + 1 {
    True -> error(404, "not_found")
    False -> {
      let me =
        uri.parse_query(req.query)
        |> result.unwrap([])
        |> list.key_find("me")
        |> result.try(fn(id) {
          case validate.entry_id(id) {
            True -> Ok(id)
            False -> Error(Nil)
          }
        })
        |> option.from_result
      let board =
        process.call(ctx.store, 5000, fn(r) { store.GetBoard(day, me, top, r) })
      let final = is_final(day, ctx.now)
      reply(
        200,
        json.object([
          #("day", json.int(board.day)),
          #("today", json.int(today)),
          #("final", json.bool(final)),
          #("total", json.int(board.total)),
          #("top", json.array(board.top, ranked_row(_, final))),
          #("me", json.nullable(board.me, ranked)),
        ]),
      )
    }
  }
}

/// No more submissions: past days, and yesterday once the grace is over.
pub fn is_final(day: Int, now: Int) -> Bool {
  let today = now / day_ms
  day < today - 1 || { day == today - 1 && now - today * day_ms >= grace_ms }
}

fn get_solution(ctx: Context, day: Int, rank: Int) -> #(Int, String) {
  let today = ctx.now / day_ms
  case day < today - history_days || day > today + 1 {
    True -> error(404, "not_found")
    False -> {
      use <- check(is_final(day, ctx.now), 404, "not_final")
      let found =
        process.call(ctx.store, 5000, fn(r) { store.GetSolution(day, rank, r) })
      case found {
        Error(_) -> error(404, "no_solution")
        Ok(e) ->
          reply(
            200,
            json.object([
              #("day", json.int(day)),
              #("rank", json.int(rank)),
              #("name", json.string(e.name)),
              #("ms", json.int(e.ms)),
              #("moves", json.int(e.moves)),
              #("sol", json.string(e.sol)),
            ]),
          )
      }
    }
  }
}

type Submission {
  Submission(name: String, token: String, ms: Int, sol: String)
}

fn submission_decoder() -> decode.Decoder(Submission) {
  // The engine reports fractional milliseconds; either form is fine.
  let ms = decode.one_of(decode.int, [decode.map(decode.float, float.round)])
  use name <- decode.field("name", decode.string)
  use token <- decode.field("token", decode.string)
  use ms <- decode.field("ms", ms)
  use sol <- decode.field("sol", decode.string)
  decode.success(Submission(name, token, ms, sol))
}

fn post_solve(ctx: Context, req: Request, day: Int) -> #(Int, String) {
  let today = ctx.now / day_ms
  let json_body =
    string.starts_with(string.lowercase(req.content_type), "application/json")
  use <- check(json_body, 415, "content_type")
  use <- check(req.origin == ctx.origin, 403, "origin")
  // Today's challenge, or yesterday's for a solve that crossed midnight.
  use <- check(
    day == today || { day == today - 1 && !is_final(day, ctx.now) },
    422,
    "day_closed",
  )
  let submission =
    bit_array.to_string(req.body)
    |> result.try(fn(text) {
      json.parse(text, submission_decoder()) |> result.replace_error(Nil)
    })
  case submission {
    Error(_) -> error(400, "bad_request")
    Ok(sub) -> {
      use <- check(validate.token(sub.token), 400, "bad_request")
      case validate.name(sub.name) {
        Error(_) -> error(422, "name")
        Ok(name) -> {
          use <- check(validate.time(sub.ms), 422, "time")
          case notation.parse(sub.sol) {
            Error(_) -> error(422, "solution")
            Ok(parsed) -> {
              use <- check(solves(day, parsed), 422, "unsolved")
              use <- check(validate.pace(parsed.moves, sub.ms), 422, "time")
              let entry =
                store.Entry(
                  id: ctx.new_id(),
                  day: day,
                  name: name,
                  ms: sub.ms,
                  moves: parsed.moves,
                  at: ctx.now,
                  token: ffi.sha256_hex(sub.token),
                  sol: notation.canonical(sub.sol),
                )
              save(ctx, entry, req.ip, today)
            }
          }
        }
      }
    }
  }
}

/// Replays the day's scramble and the reported solution.
pub fn solves(day: Int, parsed: notation.Parsed) -> Bool {
  cube.solved()
  |> cube.apply_all(scramble.to_turns(scramble.daily(day)))
  |> cube.apply_all(parsed.turns)
  |> cube.is_solved
}

fn save(ctx: Context, entry: Entry, ip: String, today: Int) -> #(Int, String) {
  let result =
    process.call(ctx.store, 5000, fn(r) { store.Submit(entry, ip, today, r) })
  case result {
    Ok(placed) -> {
      let status = case placed.fresh {
        True -> 201
        False -> 200
      }
      reply(
        status,
        json.object([
          #("id", json.string(placed.entry.id)),
          #("rank", json.int(placed.rank)),
          #("total", json.int(placed.total)),
          #("name", json.string(placed.entry.name)),
          #("ms", json.int(placed.entry.ms)),
          #("moves", json.int(placed.entry.moves)),
          #("fresh", json.bool(placed.fresh)),
        ]),
      )
    }
    Error(store.IpLimit) -> error(429, "limit")
    Error(store.DayFull) -> error(503, "full")
    Error(store.Storage(reason)) -> {
      io.println_error("cube-leaderboard: write failed: " <> reason)
      error(503, "storage")
    }
  }
}

/// A board row; `replay` says its solution can be fetched.
fn ranked_row(pair: #(Int, Entry), final: Bool) -> Json {
  let #(rank, e) = pair
  json.object([
    #("rank", json.int(rank)),
    #("name", json.string(e.name)),
    #("ms", json.int(e.ms)),
    #("moves", json.int(e.moves)),
    #("replay", json.bool(final && e.sol != "")),
  ])
}

fn ranked(pair: #(Int, Entry)) -> Json {
  let #(rank, e) = pair
  json.object([
    #("rank", json.int(rank)),
    #("name", json.string(e.name)),
    #("ms", json.int(e.ms)),
    #("moves", json.int(e.moves)),
  ])
}

fn parse_day(text: String) -> Result(Int, Nil) {
  let digits =
    string.length(text) >= 1
    && string.length(text) <= 6
    && list.all(string.to_graphemes(text), fn(g) {
      string.contains("0123456789", g)
    })
  case digits {
    True -> int.parse(text)
    False -> Error(Nil)
  }
}

fn check(ok: Bool, status: Int, code: String, next: fn() -> #(Int, String)) {
  case ok {
    True -> next()
    False -> error(status, code)
  }
}

fn reply(status: Int, body: Json) -> #(Int, String) {
  #(status, json.to_string(body))
}

fn error(status: Int, code: String) -> #(Int, String) {
  reply(status, json.object([#("error", json.string(code))]))
}
