# The playground

`whizbang play` is the door for a person trying tools in a browser (`SPEC.md` section 1): a
page in Google Chrome, served by the `whizbang` process on your own machine. This page covers
starting it, the address and its token, reopening the page, stopping, and then each screen in
turn: Home, a kit, the workspace where a task is drafted, a run going, its result, History, a
run's report, a tool's record, and Models.

```text
whizbang play --store <dir> [--config <file> [--env-file <file>]] [--models-dir <dir>]
              [--port <port>] [--no-open] [--stdin-quit] [--run-limit-s <seconds>]
```

## How this page is held

What the page says is checked by three tiers, and each part below names the cases that hold
it:

- **The fast tier** (`just check`): Rust tests against the host on a real loopback socket,
  with the test kit's fakes for every model, named here as `play::…`, `play_result::…`,
  `play_models::…` and `play_guard::…`; and the interface's own unit tests. Nothing is sent
  anywhere.
- **The Chrome tier** (`just e2e`, and the CI jobs `e2e (<runner>)`): cases in `ui/e2e/`, run
  in Google Chrome against the fixture host, the host and the interface as they ship with a
  fake segmenter, a scripted fill and a scripted judge in place of real models. Each is named
  here as `<file>: <title>`. Nothing is sent anywhere.
- **By hand**: the loop's three cases and a quit during a paid fill, run live in the
  playground with real models on the owner's Mac (plan m5, task E1). Their run ids and
  settled spend are in `docs/results/m5-playground.md`, aggregates only.

`guides::the_playgrounds_guide_names_only_cases_that_exist` fails when a case named here is
not in the tree.

## Starting it

- **It opens Chrome for you** at `http://127.0.0.1:7341/`, and prints that address. Chrome
  is the only browser the playground supports.
- **The store** is made where there is none, as `whizbang store import --create` makes one:
  its last folder alone, for you only. A store whose folder is another user's, or that its
  group or anyone may write, is refused (`chmod go-w` on the folder mends the second).
- **A config that names a key, and no env file,** still opens: the backend whose key is
  missing is shown as not ready (no key), and work that needs no key runs. Nothing on the
  page asks for a key.
- **The port** is 7341 unless `--port` says otherwise. It stays the same from one launch to
  the next, so that the page keeps what it saved for you (its drafts). A port in use is
  refused, and the sentence names `--port`. The playground listens on `127.0.0.1` only;
  there is no flag that would make it listen anywhere else.

Held by `play::hello_answers_the_version_and_the_store_behind_the_guard`,
`play::a_config_naming_a_key_with_no_env_file_opens_and_says_no_key` and
`play_guard::the_host_listens_on_loopback_only` (fast tier).

## The address and its token

When it starts, the playground makes a token: 32 random bytes, new at every launch. Chrome is
given a small page in the store's folder, `play-open.html`, readable by you only, which sends
it on to the address with the token after `#k=`, so the token is never on a command line; the
page keeps it for that tab and takes it out of the address bar at once. The small page is
removed when the playground stops. Every call the page makes carries the token. A call
without it, or with the token of an earlier launch, is refused, and the page says to run
`whizbang play` again.

With `--no-open`, or where Chrome cannot be started, nothing is opened: the address is
printed with its token, for you to open. Treat that line as you would a password for as long
as the playground runs. The token is never written to a log, a run's record or an error.

Held by `play::the_token_is_never_on_the_command_line_that_opens_chrome` and
`play_guard::the_token_is_in_no_log_event_record_or_error` (fast tier);
`shell.spec.ts: the page loads with the token, removes it from the address and shows Home`
and `shell.spec.ts: a request from the page without the token is refused and the page says
to run whizbang play again` (Chrome tier).

## Reopening the page

Closing the tab cancels nothing: a run started from the page belongs to the playground's
process, not to the page. Open the address again, or run `whizbang play` again with the same
store: a second `whizbang play` finds the one already serving that store and opens its
address instead of failing. Home then lists the runs still going under Continue, and each
opens and is followed to its end.

A store held by anything else (another command that writes it) is refused, naming the
holder, as every command that writes a store is.

Held by `play::a_second_play_on_a_held_store_opens_the_running_hosts_address` (fast tier);
`reconnect.spec.ts: a reloaded page shows the running run and follows it to its end` and
`reconnect.spec.ts: a run started in a tab that is closed ends completed, and a new tab finds
it in Continue and opens its result` (Chrome tier).

## Stopping it

Three ways, which do the same:

- **Quit** in the top bar;
- **Ctrl-C** in the terminal (or a hang-up, or `SIGTERM`);
- under `--stdin-quit`, the end of the standard input, for a script that starts it.

**Quit** asks first ("Quit the playground?"). It says how many runs are going and that each
is cancelled; that each cancelled run's record says cancelled, with every request it sent
accounted for in the ledger, so nothing stays reserved; that a run not ended within ten
seconds is ended as interrupted the next time the store is opened; and that your pictures,
history and drafts stay. While it is open nothing else on the page can be reached: Tab
stays inside it, the page's keys do nothing, and Escape or **Keep playing** closes it; **Quit** quits, and the page then says
the playground has quit and asks nothing more until you run `whizbang play` again. Closing
the tab is not quitting: the playground keeps running.

The playground stops taking requests, cancels every run it started, and waits up to ten
seconds for each to end, so that each run's record says it was cancelled and what it spent
is accounted for. Then it lets the store go and exits 0. A second Ctrl-C during those ten
seconds ends the wait at once. A run that ignored its cancellation and has not ended is left
to be ended as interrupted by the next command that opens the store to write, and the last
line says how many there were.

Held by `play::a_quit_cancels_every_run_waits_and_releases_the_store`,
`play::two_interrupts_and_an_interrupt_with_every_worker_stalled_each_quit`,
`play::a_second_quit_ends_the_grace_at_once` and
`play::a_run_that_outlives_the_grace_is_counted_and_ended_as_interrupted_by_the_next_writer`
(fast tier); `quit.spec.ts: quit with a run going cancels it, its record says cancelled, the
ledger has no open reservation, and the store takes a writer at once` and `quit.spec.ts: an
interrupt from the terminal does the same` (Chrome tier); a quit during a paid fill, by hand
(`docs/results/m5-playground.md`).

## A run's stated time

Each run started from the page is cancelled after an hour, or after `--run-limit-s` seconds.
Cancelling a run cancels the runs it called. A run whose work ignores the cancellation is
shown as stopping and keeps its place among the four that may run at once until it ends;
stopping the playground is the way out.

Held by `play::a_run_past_its_stated_time_is_cancelled_with_its_children` and
`play::a_run_that_ignores_its_cancellation_keeps_its_place_and_quit_still_exits_after_the_grace`
(fast tier).

## Around every screen

- **The top bar** holds the wordmark (back to Home), where you are (Home, or a kit and its
  task), search, **History**, **Models**, what is left of the store's allowance beside the
  allowance (as `hello` read it when the page opened), and **Quit**. Clicking the kit's name in a
  task's breadcrumb opens the kit switcher.
- **The status bar** at the foot says whether the host answered, the store's folder, each
  backend's readiness in a word or two (no key, not installed), and the version. When the
  host cannot be reached or refused the page, it says why there instead.
- **Search** (⌘K, or the top bar's search) finds tasks, tools, the pictures the playground
  imported, and each task's and tool's record, by every word typed. A picture picked there
  starts a task from it, as on Home.
- **Keys.** ⌘K search, ⌘↵ Run, ⌘S Save picture, ⌘1 to ⌘9 the kits in their order, ⌘0 Home.
  On Linux, Control does what ⌘ does.
- **Every text a run hands the page** (a reason, an event's message, a name) is shown as
  those characters and never as markup, and every control character is shown escaped.

The page loads nothing from anywhere but its own host: its fonts and its art are in the
binary. Held by `shell.spec.ts: the page loads nothing from any origin but its host`,
`home.spec.ts: a third kit of fake actions shows on home, in the switcher and in search`
(Chrome tier) and `play::an_event_message_with_an_escape_sequence_is_answered_escaped`
(fast tier).

## Home

Home (`#/`) is where the page opens.

- **The intro**: the wordmark, the line "Edit pictures and check the result, with models on
  your Mac or in the cloud", and three steps: pick a kit, give it a picture, send it on.
- **Continue** lists the runs still going (asked again every two seconds, each tagged
  RUNNING, or RUNNING · STOPPING) and your drafts (tagged DRAFT). A run opens its running
  screen; a draft opens its task with what you gave it.
- **Kits**: each kit the build describes, with its line, its count of tasks and tools, and
  its first four tasks, each with what it takes and gives in words, and the kit another kit
  uses when it does ("uses squint"); the rest are behind "+n tasks". A kit added to the build
  shows here with no change to the page.
- **Pictures**: every picture the playground has imported into the store, newest first, by
  pages of 48, under the name it came with; and a place to drop up to eight at once.
  Clicking a picture opens a panel of every task that takes a picture; picking one opens its
  draft with the picture given. An unrun draft of that task is replaced only once you
  confirm. A picture the store no longer holds is said to be missing, and no task is
  offered.

**Drafts** are a task with its inputs given and not yet run. They are kept in Chrome's own
storage for the playground's address, one a task and the twelve newest in all, and are a
convenience: losing them loses no run. Running a draft removes it.

Held by `home.spec.ts: a third kit of fake actions shows on home, in the switcher and in
search` and `reconnect.spec.ts: a run started in a tab that is closed ends completed, and a
new tab finds it in Continue and opens its result` (Chrome tier).

## A kit

A kit's address (`#/k/rascal`, `#/k/squint`), from "+n tasks" or the kit switcher, lists all
of its tasks the same way; each opens its workspace.

## The workspace: a draft

A task's address (`#/k/<kit>/<task>`) is its draft: three columns, the inputs and Run on the
left, the canvas in the middle, and the task's runs and tools on the right.

**The inputs** are built from what the task describes, each with its words of help:

- text in a text field; a number or an integer with its bounds, an integer with few values
  as buttons; yes or no; a choice among the task's values;
- **a picture**, dropped on its field or chosen from the library (the pictures Home lists).
  A picture is sent to the store as bytes and kept there as a snapshot: moving or changing
  the file afterwards changes nothing of the run. Its metadata (EXIF, profiles) is not kept;
  its name is;
- **a list of pictures**, in order, held to the count the task takes: drop or choose more,
  move each up or down, remove one;
- **a location drawn on the canvas** (a box, a polygon, a line or a stroke, as the task takes
  them), committed to the store when it is drawn, with Clear to take it off; for `segment`, a
  click on the canvas is its point and a drag its box;
- **another artifact** (a mask, a location, a profile) from the outputs of that type in the
  store's history, newest first, by pages. The list is the store's, so it is the same after
  a reload or a restart, and it is read again when a run of the task ends;
- **a backend**, where the task lets you choose one: each choice the config offers with its
  state, READY, NO KEY, GET · its size (a model not installed), UPDATE, DAMAGED or NOT READY.
  A choice that is not ready can still be picked, and Run then says what is missing.

Squint's tasks take their pictures and brief as `whizbang squint check` does: the picture to
judge, the source where the use case takes one, references in order, and the brief. The host
writes the job from them. A job file is not taken from the page.

**The plan** under the inputs is the call prepared again at each change: each need by its
capability, with the backend and model that will answer it, how many requests it may send
and the most each may cost ("3 × ≤ $0.040"; local work says "local"); a loop's attempts as
asked and as allowed; and what is held while it runs, "up to $X", or "free". Beside Run the
same bound is shown before you press it.

**Run** (⌘↵) is offered only on a call prepared and ready; otherwise it says why (an input
to give, "Preparing…", or what each need is missing). A call that has changed since it was
prepared is refused as stale: the page prepares it again and asks you to look at the plan and
press Run once more. Every run goes through the same preparation, admission, spend and gate
as from the command line.

**The canvas** shows the task's picture at fit or at a zoom you choose, with what you drew
on it, under three tabs where the task has them: **Original**, the picture given; **Mask**,
the draft's own mask, or else the last run's mask over that run's picture; and **Result**,
the last run's picture. **The right column** lists the runs of this task still going and,
under "Earlier runs", its runs from history, ten at a time, read again when a run ends (when
history cannot be read, it says why); the tools it uses (each a link
to its record); and "Read this task's record".

A draft has no command of its own: a command is made from a saved run, so use Copy command
on a result.

Held by `play::a_start_without_a_matching_fresh_preparation_is_refused_as_stale`,
`play::a_start_sent_twice_with_one_request_id_answers_one_run`,
`play::each_choice_is_prepared_and_answered_with_its_readiness_kind`,
`play::the_first_import_into_a_new_store_keeps_the_picture_and_its_name_in_one_publication`,
`play::a_picture_that_fails_to_decode_leaves_nothing_in_the_store` and
`play::a_drawn_shape_is_committed_as_a_location_bound_to_its_picture` (fast tier);
`inputs.spec.ts: a variadic port takes, orders and removes pictures within its count`,
`inputs.spec.ts: an artifact port binds a picture from the library and from a drop`,
`inputs.spec.ts: a box and a stroke drawn on the canvas become the location the task is
given, and a click and a drag become segment's point and box`,
`inputs.spec.ts: a picture changed on disk after it was sent leaves the run's input and
result as they were`,
`readiness.spec.ts: each choice shows ready, no key or get with its size, and a choice that
is not ready can be picked and Run says what is missing`,
`readiness.spec.ts: a paid run shows each need's bound and the total before Run, and Run is
refused once the preparation is stale` and
`run.spec.ts: each of the five squint use cases runs from its form` and
`inputs.spec.ts: a picture picked in search starts a task, whose mask port lists the store's
masks after a reload, and a task shows its earlier runs with their mask and result` (Chrome
tier).

A picture is held to 64 MiB and to 50,000,000 pixels, and at most two are sent at once; a
picture over either limit is refused with the limit named
(`play_guard::a_picture_over_the_pixel_limit_is_refused_413_naming_the_limit`, fast tier;
`security.spec.ts: an upload of a picture over the pixel limit is refused, naming the
limit`, Chrome tier).

## Running

A run's address (`#/run/<id>`) shows it while it goes, asked again four times a second:

- its task, RUNNING (or STOPPING), and its id;
- **the inputs**, locked: the words and values as given and the pictures by their thumbnails;
- **spent so far**, settled against what was reserved for it, with a bar; or "nothing
  reserved: free";
- **the steps**: each step of the task as it waits, runs (with its time and a bar that says
  it is going, not how far: a request reports no fraction done), ends or fails, with the
  child run that made it; a loop's attempts each under a divider, and "attempt n" above.
  Messages past what a run keeps are counted as not kept;
- **the canvas**: the original, and the mask and result once there are any; while a loop
  goes on, the places its last check cited, on the picture that check looked at;
- **Stop run**, which asks the run to stop and says "Stopping…" until it has. A stopped run
  ends cancelled and commits nothing.

When the run ends, the same address becomes its result.

Held by `play::a_run_started_by_the_host_is_found_by_its_id_after_its_caller_is_gone`,
`play::a_paid_run_says_what_the_ledger_settled_of_what_it_reserved` and
`play::an_ended_run_past_its_kept_messages_is_polled_with_the_count_skipped` (fast tier);
`run.spec.ts: progress shows each step as it starts and ends`,
`run.spec.ts: the running step list shows a step running and an attempt started before the
run ends` and `run.spec.ts: stop ends the run as cancelled and nothing is committed`
(Chrome tier).

## A result

When a run ends, its screen is the result:

- **its tag**: PASS, FAIL or UNSURE for a check's verdict; otherwise DONE, CANCELLED or
  ERROR; with its id, how long it took, and what it spent of what it reserved, or "free";
- **the inputs** it was given, and **Report**, the link to its report (below);
- **the canvas**, with views as the run has them: Original; Compare, the picture it was
  given and the one it made, side by side, swiped or as their difference, at one zoom;
  Result; Mask, the mask over the result; and, for a loop, each attempt's picture, marked as
  a picture made on the way and not the result. A check's places are drawn on its picture;
- **the verdict card**: the verdict in words ("Check passed", "The check failed", "Not
  sure"), the attempt it is of, and its reasons;
- **How it got there**, a loop's attempts, each with what changed, its verdict or why it was
  not checked, and which one was kept; and how the loop ended;
- **Cost**: each capability with how often it was asked and its backend, local work shown
  as free, and the total of the run and every run it called;
- **the steps**, as the run said them, and **Recorded**, the run's whole id.

The verdict card, How it got there, Cost, the steps and Recorded are in the right column,
under **Summary**.

A run whose judge had no key is shown done, with each attempt "not checked" and the reason
the record gives.

Held by `play_result::the_record_route_splits_cost_by_capability_and_backend_and_names_free_local_work`
and `play_result::the_record_of_a_run_that_has_not_ended_is_refused_409` (fast tier);
`compare.spec.ts: side by side, swipe and difference show the source and the result at one
zoom`, `run.spec.ts: a rascal task runs from a picture and words to a result` and
`run.spec.ts: a squint task runs from two pictures and a brief to a verdict` (Chrome tier);
the loop's three cases (a clean pass or a retry then a pass, a vague prompt showing its
tries and its flag, an unset key leaving the run unchecked), by hand
(`docs/results/m5-playground.md`).

### Save picture, Send to, Run again, Copy command

- **Save picture** (⌘S) downloads the picture it made, as `<run id>-result.png`.
- **Send to** opens a new draft of another task with the result's outputs filled in: a field
  is filled when it is the only one of that task that takes the output's type, and left for
  you to fill when two or more do. A removal sent to "Check an edit" gives its picture as
  the source, its result as the edit and, when it had words, the job the loop writes from
  them, requirements and all. The draft says where it came from ("Sent from …", the run and
  when), tags each field it filled FROM <KIT> until you change it, and leads back to the run.
  A result no task takes says so.
- **Run again** opens a new draft of the same task with the run's inputs and the backends it
  used written out. Changing it changes nothing of the run.
- **Copy command** gives the `whizbang` command that makes the same call, shows it, and puts
  it on the clipboard: every input by its identity in the store; the store, config, env
  file, models directory and a recipe file by their absolute paths; each backend the run
  took from the config's default named explicitly; each flag with its value as one word
  (`--prompt=-a hose`), so a value that begins with `-` is never read as a flag; every word
  quoted for a POSIX shell. When the config, or a recipe file the run ran, has changed since
  the run, a line above the command says so (`: '…'`, which a shell runs as nothing), and
  the command runs what is there now. A run whose words hold a control or format character
  other than a new line or a tab gives no command: no pasted line says it safely. Run it
  once the playground has quit: the store takes one writer.

Send to and Run again replace a draft of the same task you have not run only once you say
so ("Replace it", or "Keep my draft").

Held by `play_result::send_to_offers_each_task_whose_ports_a_results_outputs_fill`,
`play_result::send_to_leaves_a_port_empty_when_two_ports_take_the_same_type`,
`play_result::the_command_quotes_every_argument_for_a_posix_shell`,
`play_result::words_holding_an_escape_sequence_a_nul_or_a_format_character_are_refused_422`
and `play_result::the_command_copied_from_a_mask_rect_run_makes_the_same_call_with_the_shipped_binary`
(fast tier); `send.spec.ts: send to squint opens check an edit with the source, the edit and
the words tagged from rascal`, `send.spec.ts: back to the run returns to the sending result`,
`send.spec.ts: run again opens a draft whose changes do not touch the result it came from`,
`send.spec.ts: a result's mask sent to another rascal task binds as its mask`,
`send.spec.ts: a removal sent to check an edit carries the job the loop writes`,
`command.spec.ts: copy command gives a command that, run in a shell against the same store,
makes the same call` and `command.spec.ts: the command of a check sent from a removal
carries its whole job` (Chrome tier).

## History

History (the top bar's **History**, or `#/history`) lists every run of the store, newest
first, fifty at a time with "More runs", and keeps them across restarts: the store holds
them, not the page. Each row has a thumbnail, the task, what was asked (its words and the
names your pictures came with), its result, the models, how long it took, its cost and when.
Opening a row opens its result.

- **Filter** by kit, by result (pass, fail, unsure, done, cancelled, error, interrupted) and
  by words, matched without case. The address carries the filter, and every filter of
  `whizbang runs stats` too (`#/history?action=check&since=2026-11-01`), so a filtered list
  can be reopened or shared with yourself.
- **The figures** below the list are the ones `whizbang runs stats` prints for the same
  filter: runs (child runs included), how they ended, verdicts, a loop's attempts and
  retries, retries and repairs, time as median and 95th percentile, cost settled and
  unresolved, and cache hits.

Held by `play::history_filters_by_kit_task_result_and_words` and
`play::stats_answers_what_runs_stats_answers_for_the_same_filter` (fast tier);
`history.spec.ts: runs are listed after the host is stopped and started again, with their
result, cost and time`, `history.spec.ts: filters by kit, result and words narrow the list
and the figures` and `history.spec.ts: a finished run's evidence (steps, findings, places,
pictures) is the same after a restart` (Chrome tier).

## A run's report, and Export

**Report**, on a run that has ended (`#/run/<id>/report`), is one HTML document the host
makes from the run's record: what was asked, the pictures given and made, each mask over its
picture, the steps, a loop's attempts, a check's verdict, reasons, checklist, findings with
their places drawn, each judge turn (a part the store withheld, a missing picture or a failed
turn said as such), and the cost. It is shown in a frame that runs nothing and has an origin
of its own, so it can reach neither the page nor its token.

- **Export** saves it as `<task>-<run id>.html`, one file that opens alone, offline, with its
  pictures inside.
- **Export run** saves the run's record, its report and its outputs as one tar archive,
  `<task>-<run id>.tar`.

Held by `play::a_squint_report_holds_its_verdict_findings_places_and_turns_and_no_script`,
`play::every_text_from_a_model_or_a_person_is_escaped`,
`play::the_document_opens_alone_with_its_pictures_inline` and
`play::a_withheld_part_a_missing_picture_and_a_failed_turn_each_show_as_such` (fast tier);
`report.spec.ts: a rascal result's report shows its steps, masks and cost in place, its
pictures decoded and its styles applied`, `report.spec.ts: a squint result's report shows
its verdict, findings and the places on the picture in place`, `report.spec.ts: both reports
read the same after a restart`, `report.spec.ts: export of a squint report downloads one
document that opens alone, offline, and holds its pictures`, `report.spec.ts: model and tool
text holding markup shows as text and runs nothing` and `report.spec.ts: the report frame
cannot read the page's storage or call the api` (Chrome tier).

## A tool's record

A tool's or a task's record, the page `docs/reference/tools/<name>.md` the binary carries
(`#/record/<name>`), opens from the workspace (Read this task's record, and each tool the
task uses) and from search. It is shown the same way as a report, its pictures inside, with
"Open the task" for a task.

Held by `play::the_embedded_records_are_the_reference_folder` (fast tier) and
`records.spec.ts: a task's record opens from the workspace with its examples and limits, its
pictures decoded and its styles applied` (Chrome tier).

## Models and keys

The Models screen (the top bar's **Models**, or `#/models`) says what runs where, and what a
run may spend.

**On your Mac** lists the models of the catalogue this build ships (the same list as
`whizbang models list`), with each one's size and state. **Get** installs one as `whizbang
models get` does, into the models directory the playground was started with
(`--models-dir`, the config's `[models] dir`, or the platform's), from the origins the
catalogue names and no other. The install runs in the playground's process on a thread of
its own, one at a time: Get on a model that is being installed follows that install, and Get
on another model waits until it ends. Its progress is counted in files, not bytes ("1 of 3
files"). **Cancel** stops it, and whatever was installed before stays as it was; a Get
while it is stopping is refused until it has. An install another whizbang runs shows its
progress too, with no Cancel; a Get follows it, and Cancel then only stops following it.
Stopping the playground cancels an install that is going, the same way. **Remove** asks
first, then removes a model as `whizbang models remove` does: it waits five seconds for the
runs that use it, and if they have not let it go, it says the model is busy and removes
nothing.

A model whose files need a sign-in (SAM 3's do, on Hugging Face) can be got from the page
only when the config names where its token is (`[[models.tokens]]`, `docs/guides/models.md`,
"A model whose files need a token") as a variable of the env file (`key_env`); otherwise its
row says NOT SET UP and **How to add** says what to add. The playground does not read the
Hugging Face command-line sign-in (`sign_in = "huggingface"`): with that in the config, get
the model with `whizbang models get`. The page never asks for a token.

**In the cloud** lists each backend the config offers that sends data somewhere, with KEY
SET when the env file sets the variable the config names for its key, NO KEY when it does
not, NOT SET UP when the backend cannot be used for another reason, and READY when it needs
no key; and its price per request as the backend states it. **The playground never shows a
key, never sends one to the page, and never writes the config or the env file.** To add a
key, name its variable in the config (`key_env = "NAME"`), put `NAME=…` in the env file,
and start the playground again with the same `--config` and `--env-file`
(`docs/guides/config.md`). There is no Probe: readiness needs none, and a backend is not
contacted from this screen.

**The allowance** shows what is counted against the store's allowance since the last reset,
with the reset's date, and what is left; and the config's cap of one call (`[spend]
call_cap_usd`), which the page shows and does not change. **Reset…** asks for the new
allowance in dollars and for the word `reset` typed again, and only then resets it, as
`whizbang spend reset --allowance` does: the books close, what was spent before stops
counting, and the new allowance starts with nothing spent. Runs already admitted keep what
they reserved, under the books they were admitted under. There is no monthly allowance. The
top bar's figure is the one read when the page opened, until it is opened again.

Held by `play_models::models_lists_only_the_catalogues_models_and_the_configs_backends`,
`play_models::a_second_get_attaches_to_the_running_install`,
`play_models::a_quit_during_an_install_cancels_it_and_leaves_the_earlier_install`,
`play_models::a_removal_of_a_model_in_use_answers_busy_and_the_model_stays`,
`play_models::a_reset_from_the_page_needs_the_amount_and_the_word_and_closes_the_books` and
`play_models::no_response_of_any_route_holds_a_key_a_sign_in_or_the_token` (fast tier);
`models.spec.ts: get installs a model with its progress, and a second get attaches to the
same install`, `models.spec.ts: cancel leaves the earlier install in place`,
`models.spec.ts: remove of a model in use answers busy and the model stays`,
`models.spec.ts: no key is shown for a backend without one and no key is ever in the page`
and `models.spec.ts: a reset needs the amount and the word reset, and the allowance is then
what is spent since it` (Chrome tier); a run left unchecked by an unset key, by hand
(`docs/results/m5-playground.md`).

## What the playground holds while it runs

- **The store, to write.** While `whizbang play` runs, other commands that write the store
  are refused until it exits; commands that only read it work as always.
- **`play.json`** in the store's folder: its port, its process and its token, readable by
  you only. A second `whizbang play` trusts it only while the folder and the file are yours
  alone. It is removed when the playground stops; one left by a playground that was
  killed is replaced at the next launch.
- **Four runs at once.** A fifth is refused until one ends, and the page says so; polling,
  Stop and the other screens are answered whatever is running.
- **Threads.** The host makes 19 threads, and one more while it installs a model; one
  process holds at most 44 made by playground hosts.
- **Nothing on another machine.** The playground sends nothing off the machine itself; the
  runs it starts go only where your config already approves, as from the command line.

Held by `play::a_hosting_file_that_others_could_have_written_is_not_trusted` and
`play_guard::a_fifth_running_call_is_answered_429_and_polling_cancel_and_quit_are_still_answered`
(fast tier); the quit's release of the store, by hand (`docs/results/m5-playground.md`).

A store on a network file system is not supported: the store's locks may not hold there,
and the playground holds a store for as long as it runs.

The decisions behind the playground's host are in ADR 0041, and those behind its interface
in ADR 0043.
