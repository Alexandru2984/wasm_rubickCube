import cube_leaderboard/store.{Entry}
import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/string

fn entry(id: String, day: Int, ms: Int, moves: Int, at: Int) -> store.Entry {
  Entry(id, day, "N" <> id, ms, moves, at, "tok" <> id)
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
  let e = Entry("id", 20_760, "Ștefan \"q\"", 12_345, 61, 99, "t")
  let assert #(st, 1, 0) = store.load(fresh(), store.encode(e), 20_760)
  assert store.board(st, 20_760, None, 50).top == [#(1, e)]
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
