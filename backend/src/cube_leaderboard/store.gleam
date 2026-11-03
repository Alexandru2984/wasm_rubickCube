//// Leaderboard storage: an append-only JSON-lines file, replayed into
//// memory at startup, owned by one actor so writes are serialised.
////
//// Each line is an entry or a tombstone (`{"del":"<id>"}`, written by the
//// moderation script). No IP address is ever written: the per-address cap
//// lives in memory only and is forgotten on restart.
////
//// Solutions are kept only for a day's best few entries, so they can be
//// replayed once the day is over: an entry carries its solution in the file
//// only if it ranked in the top `replay_top` when it arrived, and in memory
//// only while it is still there.

import gleam/dict.{type Dict}
import gleam/dynamic/decode
import gleam/erlang/process.{type Subject}
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/order
import gleam/otp/actor
import gleam/result
import gleam/set
import gleam/string

/// Days kept in memory (and served); older lines stay in the file.
pub const keep_days = 60

/// Entries per day, a ceiling against floods.
pub const max_per_day = 20_000

/// Accepted submissions per address (or IPv6 /64) per day.
pub const max_per_ip = 5

/// Ranks whose solutions are kept for replay.
pub const replay_top = 10

/// Lines per day that may carry a solution (a newcomer in the top 10 writes
/// one), so a stream of ever-faster fakes can't grow the file much.
pub const max_solutions_per_day = 200

pub type Entry {
  /// `token`: SHA-256 (hex) of the client's secret, so one browser gets one
  /// entry a day and can find it again. `at`: server time, ms. `sol`: the
  /// verified solution, or "" when it isn't kept.
  Entry(
    id: String,
    day: Int,
    name: String,
    ms: Int,
    moves: Int,
    at: Int,
    token: String,
    sol: String,
  )
}

pub type Placed {
  /// `fresh`: False when this token had already submitted for the day; the
  /// existing entry is returned unchanged (first result stands).
  Placed(entry: Entry, rank: Int, total: Int, fresh: Bool)
}

pub type SubmitError {
  IpLimit
  DayFull
  Storage(String)
}

pub type Board {
  Board(
    day: Int,
    total: Int,
    top: List(#(Int, Entry)),
    me: Option(#(Int, Entry)),
  )
}

pub opaque type State {
  State(
    append: fn(String) -> Result(Nil, String),
    days: Dict(Int, List(Entry)),
    tokens: Dict(#(Int, String), String),
    ips: Dict(#(Int, String), Int),
    /// Lines with a solution written per day (see `max_solutions_per_day`).
    sols: Dict(Int, Int),
  )
}

pub type Message {
  Submit(
    entry: Entry,
    ip: String,
    today: Int,
    reply: Subject(Result(Placed, SubmitError)),
  )
  GetBoard(day: Int, me: Option(String), limit: Int, reply: Subject(Board))
  GetSolution(day: Int, rank: Int, reply: Subject(Result(Entry, Nil)))
}

pub fn new(append: fn(String) -> Result(Nil, String)) -> State {
  State(append, dict.new(), dict.new(), dict.new(), dict.new())
}

// --- Pure operations -------------------------------------------------------

/// Replays a JSON-lines file. Returns the state and how many entries were
/// loaded; malformed lines are skipped (and counted) rather than fatal.
pub fn load(state: State, content: String, today: Int) -> #(State, Int, Int) {
  let records =
    content
    |> string.split("\n")
    |> list.filter(fn(line) { string.trim(line) != "" })
    |> list.map(fn(line) { json.parse(line, record_decoder()) })
  let bad = list.count(records, result.is_error)
  let records = result.values(records)
  let deleted =
    list.fold(records, set.new(), fn(acc, r) {
      case r {
        Delete(id) -> set.insert(acc, id)
        Add(_) -> acc
      }
    })
  let state =
    list.fold(records, state, fn(st, r) {
      case r {
        Add(e) ->
          case e.day < today - keep_days {
            True -> st
            False -> {
              let st = case e.sol {
                "" -> st
                _ -> count_solution(st, e.day)
              }
              case set.contains(deleted, e.id) {
                True -> st
                False -> insert(st, e).0
              }
            }
          }
        Delete(_) -> st
      }
    })
  let loaded = dict.fold(state.days, 0, fn(n, _, es) { n + list.length(es) })
  #(state, loaded, bad)
}

pub fn submit(
  state: State,
  entry: Entry,
  ip: String,
  today: Int,
) -> #(State, Result(Placed, SubmitError)) {
  let state = prune(state, today)
  let day = entry.day
  let entries = dict.get(state.days, day) |> result.unwrap([])
  let previous =
    dict.get(state.tokens, #(day, entry.token))
    |> result.try(find(entries, _))
  case previous {
    Ok(#(rank, existing)) -> #(
      state,
      Ok(Placed(existing, rank, list.length(entries), False)),
    )
    Error(_) -> {
      let used = dict.get(state.ips, #(day, ip)) |> result.unwrap(0)
      case used >= max_per_ip, list.length(entries) >= max_per_day {
        True, _ -> #(state, Error(IpLimit))
        _, True -> #(state, Error(DayFull))
        False, False -> {
          let entry = keep_solution(state, entries, entry)
          case state.append(encode(entry)) {
            Error(reason) -> #(state, Error(Storage(reason)))
            Ok(Nil) -> {
              let state = case entry.sol {
                "" -> state
                _ -> count_solution(state, day)
              }
              let #(state, rank) = insert(state, entry)
              let ips = dict.insert(state.ips, #(day, ip), used + 1)
              #(
                State(..state, ips: ips),
                Ok(Placed(entry, rank, list.length(entries) + 1, True)),
              )
            }
          }
        }
      }
    }
  }
}

pub fn board(state: State, day: Int, me: Option(String), limit: Int) -> Board {
  let entries = dict.get(state.days, day) |> result.unwrap([])
  let top =
    entries
    |> list.take(limit)
    |> list.index_map(fn(e, i) { #(i + 1, e) })
  let mine = case me {
    Some(id) -> option.from_result(find(entries, id))
    None -> None
  }
  Board(day, list.length(entries), top, mine)
}

/// The entry at `rank` with its solution, if that rank keeps one.
pub fn solution(state: State, day: Int, rank: Int) -> Result(Entry, Nil) {
  case rank >= 1 && rank <= replay_top {
    False -> Error(Nil)
    True -> {
      let entries = dict.get(state.days, day) |> result.unwrap([])
      case list.drop(entries, rank - 1) {
        [e, ..] if e.sol != "" -> Ok(e)
        _ -> Error(Nil)
      }
    }
  }
}

/// Faster time first, then fewer moves, then whoever submitted first.
pub fn compare(a: Entry, b: Entry) -> order.Order {
  int.compare(a.ms, b.ms)
  |> order.lazy_break_tie(fn() { int.compare(a.moves, b.moves) })
  |> order.lazy_break_tie(fn() { int.compare(a.at, b.at) })
}

/// Drops the solution unless the entry would enter the top `replay_top`
/// and the day still has room for solutions in the file.
fn keep_solution(state: State, entries: List(Entry), entry: Entry) -> Entry {
  let rank =
    entries
    |> list.take(replay_top)
    |> list.take_while(fn(e) { compare(e, entry) != order.Gt })
    |> list.length
  let written = dict.get(state.sols, entry.day) |> result.unwrap(0)
  case rank < replay_top && written < max_solutions_per_day {
    True -> entry
    False -> Entry(..entry, sol: "")
  }
}

fn count_solution(state: State, day: Int) -> State {
  let written = dict.get(state.sols, day) |> result.unwrap(0)
  State(..state, sols: dict.insert(state.sols, day, written + 1))
}

/// Inserts in rank order; whoever is pushed out of the top `replay_top`
/// loses its solution (memory holds at most that many per day).
fn insert(state: State, entry: Entry) -> #(State, Int) {
  let entries = dict.get(state.days, entry.day) |> result.unwrap([])
  let #(ahead, behind) =
    list.split_while(entries, fn(e) { compare(e, entry) != order.Gt })
  let rank = list.length(ahead) + 1
  let merged = case rank > replay_top {
    True -> list.append(ahead, [Entry(..entry, sol: ""), ..behind])
    False ->
      list.append(ahead, [entry, ..behind])
      |> list.index_map(fn(e, i) {
        case i >= replay_top && e.sol != "" {
          True -> Entry(..e, sol: "")
          False -> e
        }
      })
  }
  let state =
    State(
      ..state,
      days: dict.insert(state.days, entry.day, merged),
      tokens: dict.insert(state.tokens, #(entry.day, entry.token), entry.id),
    )
  #(state, rank)
}

fn find(entries: List(Entry), id: String) -> Result(#(Int, Entry), Nil) {
  entries
  |> list.index_map(fn(e, i) { #(i + 1, e) })
  |> list.find(fn(pair) { { pair.1 }.id == id })
}

fn prune(state: State, today: Int) -> State {
  let oldest = today - keep_days
  State(
    ..state,
    days: dict.filter(state.days, fn(day, _) { day >= oldest }),
    tokens: dict.filter(state.tokens, fn(key, _) { key.0 >= oldest }),
    ips: dict.filter(state.ips, fn(key, _) { key.0 >= today - 1 }),
    sols: dict.filter(state.sols, fn(day, _) { day >= oldest }),
  )
}

// --- File format -----------------------------------------------------------

type Record {
  Add(Entry)
  Delete(String)
}

pub fn encode(e: Entry) -> String {
  let sol = case e.sol {
    "" -> []
    sol -> [#("sol", json.string(sol))]
  }
  json.object([
    #("id", json.string(e.id)),
    #("day", json.int(e.day)),
    #("name", json.string(e.name)),
    #("ms", json.int(e.ms)),
    #("moves", json.int(e.moves)),
    #("at", json.int(e.at)),
    #("tok", json.string(e.token)),
    ..sol
  ])
  |> json.to_string
}

fn record_decoder() -> decode.Decoder(Record) {
  let entry = {
    use id <- decode.field("id", decode.string)
    use day <- decode.field("day", decode.int)
    use name <- decode.field("name", decode.string)
    use ms <- decode.field("ms", decode.int)
    use moves <- decode.field("moves", decode.int)
    use at <- decode.field("at", decode.int)
    use token <- decode.field("tok", decode.string)
    use sol <- decode.optional_field("sol", "", decode.string)
    decode.success(Add(Entry(id, day, name, ms, moves, at, token, sol)))
  }
  let tombstone = {
    use id <- decode.field("del", decode.string)
    decode.success(Delete(id))
  }
  decode.one_of(tombstone, [entry])
}

// --- Actor -----------------------------------------------------------------

pub fn start(
  state: State,
) -> Result(actor.Started(Subject(Message)), actor.StartError) {
  actor.new(state)
  |> actor.on_message(handle)
  |> actor.start
}

fn handle(state: State, message: Message) -> actor.Next(State, Message) {
  case message {
    Submit(entry, ip, today, reply) -> {
      let #(state, result) = submit(state, entry, ip, today)
      process.send(reply, result)
      actor.continue(state)
    }
    GetBoard(day, me, limit, reply) -> {
      process.send(reply, board(state, day, me, limit))
      actor.continue(state)
    }
    GetSolution(day, rank, reply) -> {
      process.send(reply, solution(state, day, rank))
      actor.continue(state)
    }
  }
}
