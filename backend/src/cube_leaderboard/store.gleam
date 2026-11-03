//// Leaderboard storage: an append-only JSON-lines file, replayed into
//// memory at startup, owned by one actor so writes are serialised.
////
//// Each line is an entry or a tombstone (`{"del":"<id>"}`, written by the
//// moderation script). No IP address is ever written: the per-address cap
//// lives in memory only and is forgotten on restart.

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

pub type Entry {
  /// `token`: SHA-256 (hex) of the client's secret, so one browser gets one
  /// entry a day and can find it again. `at`: server time, ms.
  Entry(
    id: String,
    day: Int,
    name: String,
    ms: Int,
    moves: Int,
    at: Int,
    token: String,
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
}

pub fn new(append: fn(String) -> Result(Nil, String)) -> State {
  State(append, dict.new(), dict.new(), dict.new())
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
          case set.contains(deleted, e.id) || e.day < today - keep_days {
            True -> st
            False -> insert(st, e).0
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
        False, False ->
          case state.append(encode(entry)) {
            Error(reason) -> #(state, Error(Storage(reason)))
            Ok(Nil) -> {
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

/// Faster time first, then fewer moves, then whoever submitted first.
pub fn compare(a: Entry, b: Entry) -> order.Order {
  int.compare(a.ms, b.ms)
  |> order.lazy_break_tie(fn() { int.compare(a.moves, b.moves) })
  |> order.lazy_break_tie(fn() { int.compare(a.at, b.at) })
}

fn insert(state: State, entry: Entry) -> #(State, Int) {
  let entries = dict.get(state.days, entry.day) |> result.unwrap([])
  let #(ahead, behind) =
    list.split_while(entries, fn(e) { compare(e, entry) != order.Gt })
  let state =
    State(
      ..state,
      days: dict.insert(
        state.days,
        entry.day,
        list.append(ahead, [entry, ..behind]),
      ),
      tokens: dict.insert(state.tokens, #(entry.day, entry.token), entry.id),
    )
  #(state, list.length(ahead) + 1)
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
  )
}

// --- File format -----------------------------------------------------------

type Record {
  Add(Entry)
  Delete(String)
}

pub fn encode(e: Entry) -> String {
  json.object([
    #("id", json.string(e.id)),
    #("day", json.int(e.day)),
    #("name", json.string(e.name)),
    #("ms", json.int(e.ms)),
    #("moves", json.int(e.moves)),
    #("at", json.int(e.at)),
    #("tok", json.string(e.token)),
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
    decode.success(Add(Entry(id, day, name, ms, moves, at, token)))
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
  }
}
