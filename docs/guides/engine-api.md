# The engine API

`whizbang serve --http` is the door for a hosted app that uses whizbang as its engine
(`SPEC.md` section 1): the MCP server of [the stdio guide](serve.md), spoken over HTTP, with
plain routes beside it for pictures in and out and for a run by its id. The app's server
calls it; it holds no model code, and needs no GPU and no Python unless a backend it uses
runs a model locally. The decisions behind it are in ADR 0045.

It covers starting the server, its token, off loopback, the routes, calling a tool,
reconnecting, cancelling, the uploads it keeps and stopping, then user story 1 end to end as
a worked example. Each example names the test that holds it, and its tier: **the fast
tier** (`just check`), Rust tests on a real loopback socket with no client library, against
the engine's own code and the binary's wiring over fakes, or against the shipped binary
where only a process shows it; or **live**, against real providers, which spends.

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
from its header before it is decoded; a type other than PNG or JPEG, or bytes that are not of the type sent, are refused 415
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

At most four runs started here go at once (`--calls` lowers it; each run has a handler of its own, so it is 1 to 4); a fifth new one is answered 429, and
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

## The uploads it keeps

An upload that no run took is let go once it is older than `--uploads-kept-h` hours (24 by
default): the server collects such uploads when it starts and once an hour after, through
the store's collection, and one a run took stays
(`serve_http::an_upload_no_run_took_is_collected_after_its_age_and_one_a_run_took_stays`,
fast tier). An upload sent again is marked again at most once an hour. What is let go is no
longer named by the store, and answers 404. Each collection ends in the store's own prune,
run by the server, so the bytes of an upload go from the disk with its name; a run that is
going keeps every file it reads until it ends (ADR 0048; the test above checks that the
uploaded picture's file is gone).

## Stopping

`SIGTERM`, `SIGINT` (Ctrl-C) or `SIGHUP` stops the server: it takes nothing new, cancels every
run and waits for each up to ten seconds, which accounts its spend, and lets the store go; a
second signal ends the wait at once
(`serve_http::a_signal_cancels_every_run_accounts_the_ledger_and_releases_the_store`, fast
tier). A run that outlived the wait is ended as interrupted the next time the store is
opened to write.

## User story 1, end to end

A hosted editor lets people upload a picture, name or click a thing to remove, and download
the result. Its server runs `whizbang serve --http` with no GPU and no Python; segmentation
runs on SAM 3 on [a GPU host of your own](gpu-host.md), the fill on Bria through fal. The app
calls two or three actions and writes no model code.

**The config** names the remote segmenter and fal's filler, each origin approved for what it
is sent; the env file holds `SEGMENT_TOKEN` and fal's key:

```toml
[backends.segment]
kind = "remote"
url = "https://<the front's endpoint>"
token_env = "SEGMENT_TOKEN"
model = "sam3"

[backends.segment.settings]
price_microusd_per_second = 300
host_limit_s = 600

[backends.fill]
kind = "service"
provider = "fal"
model = "bria-eraser"
key_env = "FAL_KEY"

[egress.recipients.gpu]
origin = "https://<the front's endpoint>"
classes = ["source_images", "prompts"]
probe = true

[egress.recipients.fal]
origin = "https://fal.run"
classes = ["source_images", "derived_images"]
```

**The calls**, in `curl` (live); `$E` is the engine's address, `$T` its token, and every
call but an upload and a download names `MCP-Protocol-Version: 2025-06-18`:

```text
# Named: an upload, remove_prompt with the words, a download.
curl -s -H "Authorization: Bearer $T" -H "Content-Type: image/png" \
     --data-binary @photo.png "$E/v1/uploads"
# -> {"image": "sha256:…"}
curl -sN -H "Authorization: Bearer $T" -H "MCP-Protocol-Version: 2025-06-18" \
     -H "Content-Type: application/json" "$E/mcp" -d '{"jsonrpc": "2.0", "id": 1,
     "method": "tools/call", "params": {"name": "remove_prompt", "arguments":
     {"image": "sha256:…", "prompt": "the lamp", "request_id": "edit-1"}}}'
# -> an event stream; the result's text names the line "image sha256:…"
curl -s -H "Authorization: Bearer $T" -o result.png "$E/v1/artifacts/sha256:…"

# Clicked: segment the point for one instance, fill with its mask, a download.
curl -sN … "$E/mcp" -d '{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params":
     {"name": "segment", "arguments": {"image": "sha256:…", "kind": "point",
     "query": "412,230", "max_instances": 1, "request_id": "edit-2"}}}'
# -> its text names the line "mask sha256:…"
curl -sN … "$E/mcp" -d '{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params":
     {"name": "fill", "arguments": {"image": "sha256:…", "mask": "sha256:…",
     "request_id": "edit-3"}}}'
curl -s -H "Authorization: Bearer $T" -o result.png "$E/v1/artifacts/sha256:…"
```

The same exchange runs in the fast tier, the engine's segmenter of the kind `remote` at the
pack's own front on loopback with a fake model behind it and its filler at a server that
answers as fal does
(`story::a_hosted_editor_uploads_names_and_clicks_and_downloads_with_no_local_worker`, fast
tier): whizbang starts no pack worker; each run's record names the front for `segment` and
fal for `fill`, with the classes each was sent; the ledger books each segment at its bound,
0.36 dollars with the example, and each fill at fal's price. `fill` paints the mask as
`segment` found it; `remove_prompt` grows and feathers its mask first.

Live, in the live round of plan m6 (task L1), the same exchange ran from a short client
against SAM 3 on Modal and Bria through fal: three pictures, each named and each clicked, nine
runs, every one completed with no local worker (`docs/results/m6-user-story-1.md`). The
clicked removals there left a faint fringe at the thing's edge, since `fill` paints the mask
unshaped.
