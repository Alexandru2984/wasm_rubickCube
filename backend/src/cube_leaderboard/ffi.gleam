//// Typed bindings to src/cube_leaderboard_ffi.erl.

import gleam/erlang/process.{type Pid}

@external(erlang, "cube_leaderboard_ffi", "now_ms")
pub fn now_ms() -> Int

@external(erlang, "cube_leaderboard_ffi", "sha256_hex")
pub fn sha256_hex(data: String) -> String

/// 64 random bits from the OS CSPRNG, as 16 lowercase hex digits.
@external(erlang, "cube_leaderboard_ffi", "random_id")
pub fn random_id() -> String

@external(erlang, "cube_leaderboard_ffi", "read_file")
pub fn read_file(path: String) -> Result(String, String)

/// Creates the file if needed and checks it can be appended to.
@external(erlang, "cube_leaderboard_ffi", "touch")
pub fn touch(path: String) -> Result(Nil, String)

/// Appends `line` and a newline, then fdatasync()s.
@external(erlang, "cube_leaderboard_ffi", "append_line")
pub fn append_line(path: String, line: String) -> Result(Nil, String)

@external(erlang, "cube_leaderboard_ffi", "getenv")
pub fn getenv(name: String, default: String) -> String

@external(erlang, "cube_leaderboard_ffi", "halt")
pub fn halt(code: Int) -> Nil

/// The IPv4 address, the IPv6 /64, or "unknown".
@external(erlang, "cube_leaderboard_ffi", "ip_key")
pub fn ip_key(address: String) -> String

@external(erlang, "cube_leaderboard_ffi", "start_httpd")
pub fn start_httpd(port: Int, root: String) -> Result(Pid, String)
