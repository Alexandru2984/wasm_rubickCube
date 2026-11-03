%% Small Erlang helpers the Gleam code calls through @external.
-module(cube_leaderboard_ffi).
-export([
    now_ms/0, sha256_hex/1, random_id/0, read_file/1, append_line/2, touch/1,
    getenv/2, halt/1, ip_key/1, put_global/2, get_global/1, start_httpd/2
]).

now_ms() -> os:system_time(millisecond).

sha256_hex(Data) -> binary:encode_hex(crypto:hash(sha256, Data), lowercase).

random_id() -> binary:encode_hex(crypto:strong_rand_bytes(8), lowercase).

read_file(Path) ->
    case file:read_file(Path) of
        {ok, Bin} -> {ok, Bin};
        {error, Reason} -> {error, reason(Reason)}
    end.

%% Opens for appending (creating the file) without writing anything.
touch(Path) ->
    case file:open(Path, [append, raw, binary]) of
        {ok, Fd} -> _ = file:close(Fd), {ok, nil};
        {error, Reason} -> {error, reason(Reason)}
    end.

%% One line per write, flushed to disk before the client hears "saved".
append_line(Path, Line) ->
    case file:open(Path, [append, raw, binary]) of
        {ok, Fd} ->
            Result =
                case file:write(Fd, [Line, $\n]) of
                    ok -> file:datasync(Fd);
                    Error -> Error
                end,
            _ = file:close(Fd),
            case Result of
                ok -> {ok, nil};
                {error, Reason} -> {error, reason(Reason)}
            end;
        {error, Reason} ->
            {error, reason(Reason)}
    end.

getenv(Name, Default) ->
    case os:getenv(binary_to_list(Name)) of
        false -> Default;
        "" -> Default;
        Value -> unicode:characters_to_binary(Value)
    end.

halt(Code) -> erlang:halt(Code).

%% Per-client key for the in-memory submission cap: the IPv4 address, or
%% the /64 for IPv6 (one subscriber usually owns a whole /64).
ip_key(Text) ->
    case inet:parse_address(binary_to_list(Text)) of
        {ok, {_, _, _, _} = V4} ->
            list_to_binary(inet:ntoa(V4));
        {ok, {A, B, C, D, _, _, _, _}} ->
            list_to_binary(io_lib:format("~.16b:~.16b:~.16b:~.16b::/64", [A, B, C, D]));
        _ ->
            <<"unknown">>
    end.

put_global(Key, Value) ->
    persistent_term:put({cube_leaderboard, Key}, Value),
    nil.

get_global(Key) -> persistent_term:get({cube_leaderboard, Key}).

%% inets httpd on loopback; nginx terminates TLS and proxies /api/ here.
%% Every request goes to cube_leaderboard_httpd:do/1 (no file serving).
start_httpd(Port, Root) ->
    {ok, _} = application:ensure_all_started(crypto),
    {ok, _} = application:ensure_all_started(inets),
    Options = [
        {port, Port},
        {bind_address, {127, 0, 0, 1}},
        {ipfamily, inet},
        {server_name, "cube-leaderboard"},
        {server_root, binary_to_list(Root)},
        {document_root, binary_to_list(Root)},
        {modules, [cube_leaderboard_httpd]},
        {server_tokens, none},
        {max_body_size, 16384},
        {max_header_size, 8192},
        {max_uri_size, 512},
        {max_clients, 200},
        {keep_alive, true},
        {keep_alive_timeout, 30},
        {max_keep_alive_request, 1000},
        {minimum_bytes_per_second, 128}
    ],
    case inets:start(httpd, Options) of
        {ok, Pid} -> {ok, Pid};
        {error, Reason} -> {error, reason(Reason)}
    end.

reason(Reason) when is_atom(Reason) -> atom_to_binary(Reason);
reason(Reason) -> unicode:characters_to_binary(io_lib:format("~0p", [Reason])).
