//// cube-leaderboard: the daily-challenge leaderboard for cube.micutu.com.
////
//// Configuration (environment):
//// - CUBE_LB_PORT    loopback port (default 8737)
//// - CUBE_LB_DATA    data directory, holds entries.jsonl (default ./data)
//// - CUBE_LB_ORIGIN  Origin required on POST (default https://cube.micutu.com)

import cube_leaderboard/ffi
import cube_leaderboard/store
import cube_leaderboard/web
import gleam/erlang/process
import gleam/int
import gleam/io
import gleam/result

pub fn main() -> Nil {
  let port =
    ffi.getenv("CUBE_LB_PORT", "8737") |> int.parse |> result.unwrap(8737)
  let data = ffi.getenv("CUBE_LB_DATA", "./data")
  let origin = ffi.getenv("CUBE_LB_ORIGIN", "https://cube.micutu.com")
  let file = data <> "/entries.jsonl"
  let today = ffi.now_ms() / web.day_ms

  let content = case ffi.read_file(file) {
    Ok(content) -> content
    Error("enoent") -> ""
    Error(reason) -> fail("can't read " <> file <> ": " <> reason)
  }
  // Fail at startup, not on the first submission, if the file can't be
  // written.
  case ffi.touch(file) {
    Ok(Nil) -> Nil
    Error(reason) -> fail("can't write " <> file <> ": " <> reason)
  }
  let #(state, loaded, bad) =
    store.new(ffi.append_line(file, _)) |> store.load(content, today)

  let assert Ok(started) = store.start(state)
  // Not linked: if the store dies, the monitor below exits the VM and
  // systemd restarts the service with a clean replay of the file.
  process.unlink(started.pid)
  web.configure(web.Config(started.data, origin))

  let httpd = case ffi.start_httpd(port, data) {
    Ok(pid) -> pid
    Error(reason) ->
      fail("can't listen on " <> int.to_string(port) <> ": " <> reason)
  }
  io.println(
    "cube-leaderboard on 127.0.0.1:"
    <> int.to_string(port)
    <> ", "
    <> int.to_string(loaded)
    <> " entries loaded"
    <> case bad {
      0 -> ""
      n -> ", " <> int.to_string(n) <> " malformed lines skipped"
    },
  )

  let stopped =
    process.new_selector()
    |> process.select_specific_monitor(process.monitor(started.pid), fn(_) {
      "store"
    })
    |> process.select_specific_monitor(process.monitor(httpd), fn(_) { "httpd" })
    |> process.selector_receive_forever
  fail(stopped <> " stopped")
}

fn fail(message: String) -> a {
  io.println_error("cube-leaderboard: " <> message)
  ffi.halt(1)
  panic as message
}
