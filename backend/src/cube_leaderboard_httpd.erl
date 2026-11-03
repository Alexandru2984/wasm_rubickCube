%% inets httpd callback: turns the request into plain binaries and hands it
%% to cube_leaderboard/web.gleam. Only what the API uses crosses over, and
%% anything that isn't valid UTF-8 arrives as an empty string.
-module(cube_leaderboard_httpd).
-export([do/1]).

-include_lib("inets/include/httpd.hrl").

do(#mod{method = Method, request_uri = Uri, parsed_header = Headers, entity_body = Body}) ->
    {Status, Json} =
        try
            'cube_leaderboard@web':handle_raw(
                text(Method),
                text(Uri),
                header("content-type", Headers),
                header("origin", Headers),
                header("x-real-ip", Headers),
                bytes(Body)
            )
        catch
            Class:Reason:Stack ->
                logger:error("cube-leaderboard: ~p:~0p ~0p", [Class, Reason, Stack]),
                {500, <<"{\"error\":\"internal\"}">>}
        end,
    Head = [
        {code, Status},
        {content_type, "application/json; charset=utf-8"},
        {content_length, integer_to_list(byte_size(Json))},
        {cache_control, "no-store"}
    ],
    {proceed, [{response, {response, Head, [Json]}}]}.

header(Name, Headers) ->
    case lists:keyfind(Name, 1, Headers) of
        {_, Value} -> text(Value);
        false -> <<>>
    end.

bytes(Value) when is_binary(Value) -> Value;
bytes(Value) ->
    try list_to_binary(Value)
    catch _:_ -> <<>>
    end.

text(Value) ->
    case unicode:characters_to_binary(bytes(Value)) of
        Bin when is_binary(Bin) -> Bin;
        _ -> <<>>
    end.
