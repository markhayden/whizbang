# The engine API

`whizbang serve --http` is the door for a hosted app that uses whizbang as its engine
(`SPEC.md` section 1): the MCP server of [the stdio guide](serve.md), spoken over HTTP, with
plain routes beside it for pictures in and out and for a run by its id. The app's server
calls it; it holds no model code, and needs no GPU and no Python unless a backend it uses
runs a model locally. The decisions behind it are in ADR 0045.

This is the guide's first part: starting the server, its token, off loopback, the routes,
calling a tool, reconnecting, cancelling and stopping. Each example names the test that
holds it, and its tier: **the fast tier** (`just check`), Rust tests on a real loopback
socket with no client library, against the engine's own code and the binary's wiring over
the test kit's fakes, or against the shipped binary where only a process shows it.

## Starting it

```text
whizbang serve --http <addr> (--token-env <NAME> | --token-file <file>) --store <dir>
               [--config <file> [--env-file <file>]] [--models-dir <dir>] [--wait-s <seconds>]
               [--off-loopback --host-name <name>...] [--allow-origin <origin>...]
               [--calls <n>] [--pixel-limit <pixels>] [--uploads-kept-h <hours>]
```

- **Where it listens.** `--http 127.0.0.1:8080` listens on loopback, and needs nothing more.
  Any other address is refused at start unless `--off-loopback` is given with at least one
  `--host-name` (`serve_http::an_address_off_loopback_is_refused_without_off_loopback_and_a_host_name`,
  fast tier).
- **The store** is made where there is none, its last folder alone, and is held as the one
  writer until the server stops: no other command writes it meanwhile.
- **Every backend in use.** As over stdio, the executor is built over the config, its env
  file and the models directory, every backend the config offers in use; a config that
  names a key is not served without its env file.
- **The start line.** One line to the error output: where it listens, its host names, and
  whether off loopback was stated. Never the token.

## The token

One bearer token, on loopback too: `--token-env <NAME>` names a variable of the server's
environment that holds it, or `--token-file <file>` a file that does. Exactly one of the two
is given (`serve_http::with_neither_token_flag_or_with_both_the_start_is_refused`, fast
tier). A token is at least 32 characters of printable ASCII, with no spaces; a file that its
group or anyone may read or write, or that is another user's, is refused at start
(`serve_http::a_token_file_others_can_read_or_a_short_token_is_refused_at_start`, fast
tier). Make one with, for example, `openssl rand -hex 32`, and keep the file `chmod 600`.

The token is read once: a changed token needs a restart. Every request sends it as
`Authorization: Bearer <token>`; without it, or with another, the answer is 401 and names
nothing (`engine_guard::a_call_without_the_token_is_refused_401_and_names_nothing`, fast
tier). It is in no answer, log, event, record or error
(`engine_guard::the_token_is_in_no_answer_log_event_record_or_error`, fast tier).

## Off loopback

The server speaks plain HTTP. Off loopback it stands behind a reverse proxy that ends TLS;
built-in TLS is deferred (`SPEC.md` section 10). The server cannot see the proxy, so
`--off-loopback` is your statement that one is there. Give the names the proxy forwards in
`Host` as `--host-name`; a request that names any other is refused 421
(`engine_guard::a_request_whose_host_is_not_one_of_the_servers_names_is_refused_421`, fast
tier).

A request that sends an `Origin` is refused 403 unless that origin is listed with
`--allow-origin`; none is listed by default, since the caller is a server, which sends none
(`engine_guard::a_request_from_an_origin_not_listed_is_refused_403_with_no_allow_origin_header`,
`engine_guard::a_listed_origin_is_answered_and_a_request_with_no_origin_is_taken`, fast
tier).

## The routes

| Route | What it does |
|---|---|
| `POST /mcp` | One JSON-RPC message of MCP, revision 2025-06-18 |
| `POST /v1/uploads` | A picture, `image/png` or `image/jpeg`, imported; answers `{"image": "sha256:…"}` |
| `POST /v1/uploads?mask_of=<image>` | A mask for that image, a grey PNG of its size; answers `{"mask": "sha256:…"}` |
| `GET /v1/artifacts/<identity>` | A picture as PNG, a mask as its grey PNG |
| `GET /v1/runs/<run>` | The run's state, and once it has ended its outcome, its outputs and its record |
| `POST /v1/runs/<run>/cancel` | Cancels a run this server started: 202 and `stopping` |

A call names its pictures by the identity its upload was answered with
(`serve_http::an_upload_is_named_by_a_call_and_its_result_is_downloaded`, fast tier). No
path is taken: any argument that reads as one (`/` or `~` first, a drive letter, `file:`, a
`..` part) is refused 400 (`engine_guard::a_path_in_any_argument_of_any_tool_is_refused_400`,
fast tier). A picture is at most 64 MiB and 25,000,000 pixels (`--pixel-limit`), checked
from its header before it is decoded; a type other than PNG or JPEG is refused 415
(`engine_guard::an_upload_over_64_mib_is_refused_413_before_it_is_read_whole`,
`engine_guard::an_upload_over_the_pixel_limit_is_refused_413_naming_the_limit`,
`engine_guard::an_upload_of_webp_or_another_type_is_refused_415`, fast tier). A picture's
metadata is not kept.

No local control is reachable: no model install, no spend reset, no config, no store
command, as a tool or a route (`engine_guard::no_local_control_is_listed_callable_or_routed`,
fast tier).

## Calling a tool

`initialize` is answered with 2025-06-18 whatever it asks; every message after it names
`MCP-Protocol-Version: 2025-06-18` or is refused 400, and so is a batch
(`serve_http::an_initialize_asking_another_revision_is_answered_with_2025_06_18`,
`engine_guard::a_request_with_no_protocol_version_another_one_or_a_batch_is_refused_400`,
fast tier). There is no session.

A `tools/call` is answered as an event stream. Its header `Whizbang-Run` names the run before
anything else; then each progress message, when the call gave a `progressToken`; then the
result, whose `_meta` names the run again as `whizbang/run`
(`serve_http::a_call_with_a_progress_token_streams_its_progress_and_its_result_and_names_its_run`,
`serve_http::a_call_without_a_progress_token_streams_no_progress_and_still_names_its_run`,
fast tier). For the user story's hosted editor, the calls are: an upload, then
`remove_prompt` with the picture and the words; or an upload, `segment` with `kind` `point`,
the click as `query` and `max_instances` 1, then `fill` with the picture and that mask; then
a download of the result.

At most four runs started here go at once (`--calls`); a fifth new one is answered 429, and
nothing is admitted or reserved for it
(`engine_guard::a_fifth_new_run_is_answered_429_and_a_repeat_run_routes_and_downloads_are_still_answered`,
fast tier).

## Reconnecting

Give every call a `request_id` of your own. A call sent again under the same id is answered
with the run the first one made, and nothing runs again, by the store's record of the id:
after its stream dropped, after a restart, and after the run was deleted
(`serve_http::a_call_repeated_by_request_id_after_its_stream_dropped_follows_the_same_run_and_spends_once`,
`serve_http::a_repeat_after_a_restart_after_the_table_let_it_go_and_after_its_run_was_pruned_dispatches_nothing_more`,
fast tier). A repeat of a run still going takes over its stream; one of a run that has ended
is answered with its result as one body; both are answered at capacity
(`engine_guard::with_four_streams_open_a_repeat_of_one_takes_over_its_stream_at_once_and_dispatches_and_spends_nothing_more`,
`engine_guard::a_repeat_of_an_ended_run_at_capacity_is_answered_as_one_body`, fast tier).
Another call under the same id is a conflict
(`serve_http::the_same_request_id_with_other_arguments_is_a_conflict_after_a_restart_and_after_a_prune`,
fast tier). A dropped stream cancels nothing.

## Cancelling

`POST /v1/runs/<run>/cancel` cancels a run this server started; it never waits, and a run of
another process is answered 409, since one process owns every run it starts
(`serve_http::a_run_whose_stream_dropped_is_read_and_cancelled_by_its_id_and_commits_nothing`,
`engine_guard::a_cancel_of_a_run_this_server_did_not_start_is_answered_409`, fast tier). A
run still going at its stated time (`--wait-s`, 900 seconds by default) is cancelled, and
its call is answered that the server gave up on it: once it has ended, or after a grace of
ten seconds if it has not.

## Stopping

`SIGTERM`, `SIGINT` (Ctrl-C) or `SIGHUP` stops the server: it takes nothing new, cancels every
run and waits for each up to ten seconds, which accounts its spend, and lets the store go; a
second signal ends the wait at once
(`serve_http::a_signal_cancels_every_run_accounts_the_ledger_and_releases_the_store`, fast
tier). A run that outlived the wait is ended as interrupted the next time the store is
opened to write.
