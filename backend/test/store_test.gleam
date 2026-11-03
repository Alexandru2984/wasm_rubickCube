import cube_leaderboard/store.{Entry}
import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/string

fn entry(id: String, day: Int, ms: Int, moves: Int, at: Int) -> store.Entry {
  Entry(id, day, "N" <> id, ms, moves, at, "tok" <> id, "")
}

fn solved(id: String, day: Int, ms: Int, at: Int) -> store.Entry {
  Entry(..entry(id, day, ms, 40, at), sol: "sol-" <> id)
}

fn fresh() -> store.State {
  store.new(fn(_) { Ok(Nil) })
}

pub fn ranking_is_time_then_moves_then_first_test() {
  let st = fresh()
  let #(st, _) = store.submit(st, entry("a", 10, 20_000, 50, 3), "ip1", 10)
  let #(st, _) = store.submit(st, entry("b", 10, 15_000, 60, 4), "ip2", 10)
  let #(st, _) = store.submit(st, entry("c", 10, 20_000, 40, 5), "ip3", 10)
  let #(st, r) = store.submit(st, entry("d", 10, 20_000, 40, 6), "ip4", 10)
  let assert Ok(placed) = r
  assert placed.rank == 3
  assert placed.total == 4
  let board = store.board(st, 10, Some("a"), 50)
  assert list.map(board.top, fn(p) { { p.1 }.id }) == ["b", "c", "d", "a"]
  assert board.me == Some(#(4, entry("a", 10, 20_000, 50, 3)))
  assert board.total == 4
}

pub fn same_token_keeps_the_first_result_test() {
  let st = fresh()
  let #(st, _) = store.submit(st, entry("a", 10, 20_000, 50, 1), "ip", 10)
  let again = Entry(..entry("z", 10, 5000, 30, 2), token: "toka")
  let #(st, r) = store.submit(st, again, "ip", 10)
  let assert Ok(placed) = r
  assert !placed.fresh
  assert placed.entry.id == "a"
  assert placed.entry.ms == 20_000
  assert store.board(st, 10, None, 50).total == 1
}

pub fn addresses_are_capped_per_day_test() {
  let st =
    int.range(1, store.max_per_ip + 1, fresh(), fn(st, i) {
      let id = string.inspect(i)
      store.submit(st, entry(id, 10, 10_000 + i, 40, i), "ip", 10).0
    })
  let #(st, r) = store.submit(st, entry("x", 10, 9000, 40, 99), "ip", 10)
  assert r == Error(store.IpLimit)
  // Another address, or the same one tomorrow, is fine.
  let #(st, r) = store.submit(st, entry("y", 10, 9000, 40, 99), "other", 10)
  let assert Ok(_) = r
  let #(_, r) = store.submit(st, entry("w", 11, 9000, 40, 99), "ip", 11)
  let assert Ok(_) = r
}

pub fn failed_writes_are_not_ranked_test() {
  let st = store.new(fn(_) { Error("enospc") })
  let #(st, r) = store.submit(st, entry("a", 10, 20_000, 50, 1), "ip", 10)
  assert r == Error(store.Storage("enospc"))
  assert store.board(st, 10, None, 50).total == 0
}

pub fn load_replays_entries_and_tombstones_test() {
  let lines =
    [
      store.encode(entry("a", 90, 20_000, 50, 1)),
      store.encode(entry("b", 90, 10_000, 50, 2)),
      "{\"del\":\"b\"}",
      "not json",
      "",
      store.encode(entry("old", 1, 10_000, 50, 3)),
      store.encode(entry("c", 91, 12_000, 50, 4)),
    ]
    |> string.join("\n")
  let #(st, loaded, bad) = store.load(fresh(), lines, 91)
  // "b" was deleted, "old" is past the 60-day window.
  assert loaded == 2
  assert bad == 1
  assert store.board(st, 90, None, 50).total == 1
  assert store.board(st, 1, None, 50).total == 0
  // The token of a loaded entry is still taken.
  let #(_, r) = store.submit(st, entry("c", 91, 1, 1, 9), "ip", 91)
  let assert Ok(placed) = r
  assert !placed.fresh
}

pub fn records_round_trip_test() {
  let e = Entry("id", 20_760, "Ștefan \"q\"", 12_345, 61, 99, "t", "")
  let assert #(st, 1, 0) = store.load(fresh(), store.encode(e), 20_760)
  assert store.board(st, 20_760, None, 50).top == [#(1, e)]
  // A solution only appears in the line when there is one.
  assert !string.contains(store.encode(e), "sol")
  let s = Entry(..e, sol: "R U R'")
  let assert #(st, 1, 0) = store.load(fresh(), store.encode(s), 20_760)
  assert store.solution(st, 20_760, 1) == Ok(s)
}

/// Collects what the store writes, through the process dictionary.
fn recording() -> store.State {
  store.new(fn(line) {
    put("lines", [line, ..get("lines")])
    Ok(Nil)
  })
}

@external(erlang, "erlang", "put")
fn put_raw(key: String, value: List(String)) -> List(String)

@external(erlang, "erlang", "get")
fn get_raw(key: String) -> a

fn put(key: String, value: List(String)) -> Nil {
  put_raw(key, value)
  Nil
}

fn get(key: String) -> List(String) {
  let value = get_raw(key)
  case is_list(value) {
    True -> value
    False -> []
  }
}

@external(erlang, "erlang", "is_list")
fn is_list(value: a) -> Bool

pub fn solutions_are_kept_for_the_top_ten_only_test() {
  put("lines", [])
  // Twelve entries, each slower than the last: the first ten keep theirs.
  let st =
    int.range(1, 13, recording(), fn(st, i) {
      let id = string.inspect(i)
      store.submit(st, solved(id, 10, 10_000 + i, i), id, 10).0
    })
  let written = get("lines")
  assert list.count(written, string.contains(_, "\"sol\"")) == 10
  assert store.solution(st, 10, 1) |> result_id == Ok("1")
  assert store.solution(st, 10, 10) |> result_id == Ok("10")
  assert store.solution(st, 10, 11) == Error(Nil)
  assert store.solution(st, 10, 0) == Error(Nil)
  // A new #1 pushes #10 out, and #10 loses its solution in memory.
  let #(st, _) = store.submit(st, solved("fast", 10, 5000, 50), "f", 10)
  assert store.solution(st, 10, 1) |> result_id == Ok("fast")
  assert store.solution(st, 10, 10) |> result_id == Ok("9")
  assert store.solution(st, 10, 11) == Error(Nil)
  // Reloading the file gives the same picture.
  let #(again, _, _) =
    store.load(fresh(), string.join(list.reverse(get("lines")), "\n"), 10)
  assert store.solution(again, 10, 1) |> result_id == Ok("fast")
  assert store.solution(again, 10, 10) |> result_id == Ok("9")
  assert store.solution(again, 10, 11) == Error(Nil)
  assert store.board(again, 10, None, 50).total == 13
}

pub fn solution_lines_per_day_are_capped_test() {
  put("lines", [])
  // Ever-faster entries all reach #1; only the first few hundred keep a solution.
  let total = store.max_solutions_per_day + 5
  let st =
    int.range(0, total, recording(), fn(st, i) {
      let id = string.inspect(i)
      store.submit(st, solved(id, 10, 600_000 - i, i), id, 10).0
    })
  assert list.count(get("lines"), string.contains(_, "\"sol\""))
    == store.max_solutions_per_day
  // The newest #1 came after the cap: no solution to show.
  assert store.solution(st, 10, 1) == Error(Nil)
  // Another day is unaffected.
  let #(st, _) = store.submit(st, solved("next", 11, 9000, 1), "n", 11)
  assert store.solution(st, 11, 1) |> result_id == Ok("next")
}

fn result_id(r: Result(store.Entry, Nil)) -> Result(String, Nil) {
  case r {
    Ok(e) -> Ok(e.id)
    Error(e) -> Error(e)
  }
}

pub fn board_limits_top_but_finds_me_anywhere_test() {
  let st =
    int.range(1, 81, fresh(), fn(st, i) {
      let id = string.inspect(i)
      store.submit(st, entry(id, 10, 10_000 + i, 40, i), id, 10).0
    })
  let board = store.board(st, 10, Some("75"), 50)
  assert list.length(board.top) == 50
  assert board.total == 80
  let assert Some(#(75, _)) = board.me
  assert store.board(st, 10, Some("nope"), 50).me == None
}
