# The playground

`whizbang play` is the door for a person trying tools in a browser (`SPEC.md` section 1): a
page in Google Chrome, served by the `whizbang` process on your own machine. This page covers
starting it, the address and its token, reopening the page, stopping, and then each screen in
turn: Home, a kit, the workspace where a task is drafted, a run going, its result, two runs
compared, saved examples, History, a run's report, a tool's record, and Models.

```text
whizbang play --store <dir> [--config <file> [--env-file <file>]] [--models-dir <dir>]
              [--port <port>] [--no-open] [--stdin-quit] [--run-limit-s <seconds>]
```

## How this page is held

What the page says is checked by three tiers, and each part below names the cases that hold
it:

- **The fast tier** (`just check`): Rust tests against the host on a real loopback socket,
  with the test kit's fakes for every model, named here as `play::…`, `play_result::…`,
  `play_models::…`, `play_compare::…`, `play_examples::…`, `play_library::…` and
  `play_guard::…`; and the interface's own unit tests. Nothing is sent anywhere.
- **The Chrome tier** (`just e2e`): cases in `ui/e2e/`, run in Google Chrome against the
  fixture host, the host and the interface as they ship with a fake segmenter, a scripted
  fill and a scripted judge in place of real models, held to 240 seconds of test time in
  all. CI runs it in two shards on each runner, `e2e (<runner>, shard <k>)`, each held to 120
  seconds. Each case is named here as `<file>: <title>`. Nothing is sent anywhere.
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
  missing is shown as not ready (no key), with the variable to set named ("the filler needs
  FAL_KEY in your env file"; a name that does not look like one, short upper-case words joined by
  underscores, is not said back), and work that needs no key runs. Nothing on the page asks
  for a key.
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
`home.spec.ts: the page loads with the token, removes it from the address and shows Home`
and `home.spec.ts: a request from the page without the token is refused and the page says
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
  task), search, **History**, **Models**, the store's allowance ("$X spent · $Y held of $Z",
  "Models and keys" below), and **Quit**. Clicking the kit's name in a
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
binary. Held by `home.spec.ts: the page loads nothing from any origin but its host`,
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
- **Examples**: the examples saved in this store (below, "Saved examples"), with how many
  of the 1,000 a store keeps.
- **Pictures**: every picture the store holds as an input of a run or an import, whatever
  gave it: the playground, the command line, the engine API or a bundle. They are newest
  used first, 24 at a time with **Show more**, under four chips: **All**, **Named** (a
  picture given a name in the playground), **Inputs of runs** and **Imported**, each with
  its count. A picture no person named is shown by its identity, shortened. Each tile says
  how many runs took it, or that it was imported and not run yet. A run records nothing
  of the door that started it, so the library says what the store knows instead. You can
  drop up to eight pictures at once.
  Clicking a picture opens its card: its name or "No name given", its size and bytes,
  when it was first used (the earliest run that took it, or its import), and the runs that
  took it, each a link to its result. Beside them is every task that takes a picture, and
  picking one opens its draft with the picture given. An unrun draft of that task is
  replaced only once you confirm. A picture the store no longer holds is said to be
  missing, and no task is offered.

**Drafts** are a task with its inputs given and not yet run. They are kept in Chrome's own
storage for the playground's address, one a task and the twelve newest in all, and are a
convenience: losing them loses no run. Running a draft removes it.

Held by `play_library::the_library_lists_inputs_of_runs_and_imports_by_pages_with_each_cards_runs`
(fast tier); `home.spec.ts: home lists the pictures of a store filled from the command line`,
`home.spec.ts: a third kit of fake actions shows on home, in the switcher and in
search` and `reconnect.spec.ts: a run started in a tab that is closed ends completed, and a
new tab finds it in Continue and opens its result` (Chrome tier).

## A kit

A kit's address (`#/k/rascal`, `#/k/squint`), from "+n tasks" or the kit switcher, lists all
of its tasks the same way; each opens its workspace. Below them are the kit's saved
examples.

## The workspace: a draft

A task's address (`#/k/<kit>/<task>`) is its draft: three columns, the inputs and Run on the
left, the canvas in the middle, and the task's runs and tools on the right.

**The inputs** are built from what the task describes, each with its words of help:

- text in a text field; a number or an integer with its bounds, an integer with few values
  as buttons; yes or no; a choice among the task's values;
- **a picture**, dropped on its field or chosen from the library: the pictures Home lists,
  whatever door gave them (the playground, `store import`, a run from the command line, a
  bundle), a page at a time with **More pictures**. A picture is sent to the store as bytes and kept there as a snapshot: moving or changing
  the file afterwards changes nothing of the run. Its metadata (EXIF, profiles) is not kept;
  its name is;
- **a list of pictures**, in order, held to the count the task takes: drop or choose more,
  move each up or down, remove one;
- **a location drawn on the canvas** (a box, a polygon, a line or a stroke, as the task takes
  them), committed to the store when it is drawn, with Clear to take it off; for `segment`, a
  click on the canvas is its point and a drag its box;
- **another artifact** (a mask, a location, a profile) from the outputs of that type in the
  store's history, newest first, by pages, each row with its thumbnail, what its run was
  asked and its task, so two runs of one task can be told apart. The list is the store's, so it is the same after
  a reload or a restart, and it is read again when a run of the task ends;
- **a backend**, where the task lets you choose one: each choice the config offers with its
  state, READY, NO KEY, GET · its size (a model not installed), UPDATE, DAMAGED or NOT READY.
  A choice that is not ready can still be picked, and Run then says what is missing. A
  choice the task refuses for itself is NOT READY with the refusal's words; while the call
  lacks something of its own (a picture not given yet), each choice shows its backend's own
  state instead.

Squint's tasks take their pictures and brief as `whizbang squint check` does: the picture to
judge, the source where the use case takes one, references in order, and the brief. The host
writes the job from them. A job file is not taken from the page.

**The plan** under the inputs is the call prepared again at each change: each need by its
capability, with the backend and model that will answer it, how many requests it may send
and the most each may cost ("3 × ≤ $0.040"; local work says "local"; a need whose backend is
not ready says "not ready" and why, and is neither local nor free); a loop's attempts as
asked, or how many of them the allowance lets run; and what is held while it runs, "up to
$X", "free", or "not ready" when the call cannot run as prepared. Beside Run the
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
`play::a_choice_refused_for_itself_is_told_apart_from_a_missing_input`,
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
masks after a reload, and a task shows its earlier runs with their mask and result` and
`home.spec.ts: home lists the pictures of a store filled from the command line`, which
picks one of them in a field's library too (Chrome tier).

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
  given and the one it made, side by side, swiped or as their difference, at one zoom,
  which by default fits both pictures to the space there is;
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
the record gives, once; among its inputs the judge it named is marked "not used", with "no key" or "not ready".
A run whose prompt found nothing is tagged with "nothing found" and says so plainly
('Nothing in the picture matched "the thing". Name what to remove more exactly.'), with
the record's own words beside it, smaller.

Held by `play_result::the_record_route_splits_cost_by_capability_and_backend_and_names_free_local_work`
and `play_result::the_record_of_a_run_that_has_not_ended_is_refused_409` (fast tier);
`compare.spec.ts: side by side, swipe and difference show the source and the result at one
zoom`, `run.spec.ts: a rascal task runs from a picture and words to a result` and
`run.spec.ts: a squint task runs from two pictures and a brief to a verdict` (Chrome tier);
the loop's cases by hand (a fail the loop stopped, a clean pass, a vague prompt that found
nothing, and a run with no judge ready left unchecked; `docs/results/m5-playground.md`).

### Save picture, Send to, Run again, More

- **Save picture** (⌘S) downloads the picture it made, as `<run id>-result.png`.
- **Send to** opens a new draft of another task with the result's outputs filled in: a field
  is filled when it is the only one of that task that takes the output's type, and left for
  you to fill when two or more do. A removal sent to "Check an edit" gives its picture as
  the source, its result as the edit and, when it had words, the job the loop writes from
  them, requirements and all. The draft says where it came from ("Sent from …", the run and
  when), tags each field it filled FROM <KIT> until you change it, and leads back to the run.
  A result no task takes says so.
- **Run again** opens a new draft of the same task with the run's inputs and the backends it
  used written out. Changing it changes nothing of the run. Left as it is, it makes the call
  the run made: a check keeps the run's job, requirements and all, though its form shows
  the brief alone, until you change the brief; and what the run was asked that no field
  shows (a recipe's `set`, `from` or `until`, from the command line) is kept and sent as it
  was, which the draft names. A run from a step (`from` and the inputs it was given at that
  step) is let go of as soon as you change a picture or another input, since those inputs
  were made from the pictures given then; settings (`set`) and `until` stay.
- **More** holds **Pin as baseline** and **Compare with baseline** (below, "Comparing two
  runs"), **Save as example…** (below, "Saved examples"), **Replay…** (below, "Replaying a
  run"), **Bundle this run…** (below, "Bundles") and **Copy command**. The arrow keys move
  through it, and Escape closes it.
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
`play_result::run_again_left_as_it_is_makes_the_recorded_call_with_what_no_field_shows`,
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

## Comparing two runs

Pin a run as the baseline (a result's **More**, **Pin as baseline**, or a History row's
**Pin as baseline**, shown when the pointer is over the row). Then open another run of the
same task and press **Compare with baseline** (in its More menu, or on its History row).
The comparison is at `#/compare/<baseline>/<attempt>`, and the pin is kept there and in
Chrome's storage for the playground's address, so a reload keeps it; the store keeps no
pin. With only a baseline picked (`#/compare/<baseline>`) the page says to pick an attempt.

- **On the left**: the baseline (with **Clear**) and the attempt (with **Swap**), each with
  its picture, its id and how it ended, when it ran, how long it took and its cost; and
  **Open baseline** and **Open attempt**.
- **In the middle**: the two runs' pictures, labelled Baseline and Attempt, side by side at
  one zoom, or swiped, or as their difference, when both are of one size; pictures of two
  sizes are shown side by side alone. How each ended is under its picture.
- **What differs**, on the right: what each was asked, what each capability resolved to
  (the config's name of its backend, and its model), the settings it ran under, its inputs
  and its outputs, by name or port, each row that differs marked; **Show all** shows the
  rows that are the same too, marked "same". Then the outcome, the time, the cost of each
  whole run (every run it called included, with what no answer settled said beside it, and
  "not all read" for a family the store could not read whole) and its models.

The comparison claims no winner and computes no score. Two runs of two tasks, or a run
another run called, are refused, and the page offers to pin the other run instead; a run
still going ends first.

Held by `play_compare::two_runs_of_one_task_compare_by_settings_inputs_outputs_time_and_cost`,
`play_compare::two_paid_loops_compare_cost_and_models_over_their_families_with_unresolved_spend_said`
and `play_compare::two_tasks_are_refused_422_and_a_run_not_ended_409` (fast tier);
`compare.spec.ts: a pinned baseline and an attempt of the same task show their pictures, settings and cost side by side, and the pin survives a reload`
(Chrome tier).

## Saved examples

An example is a run's task, inputs and settings kept as a named starting point. **Save as
example…** in a result's More menu asks for a name (at most 80 characters) and optional
notes (at most 2,000), and says what it keeps: the task, its settings, the backends it used
and its pictures. The store keeps it (`examples/<id>`), with each of its pictures, so it
outlives its run: deleting the run, and pruning, leaves the example whole. A store keeps at
most 1,000. No example ships with the binary.

Examples are listed on Home and in their kit's page. Each row's menu (⋯) opens it as a
draft, renames it, opens its run, or deletes it; deleting asks once, and the run and its
pictures stay. An example whose task this build no longer has says so, and can be read and
deleted, not opened.

**Opening one** fills a draft of its task and runs nothing. The draft says which example it
came from. Its choices of backend are the example's where this config offers them now: a
backend the config no longer names, or one that is not ready (a key not set, a model not
installed), is left to the config's default, and the draft says which and why. Readiness
and the plan are asked again as for any draft, and a run from it is an ordinary run under
the config as it is now. A picture the store let go of with its run is given back from the
example as an import when it is opened. Left as it is, the draft makes the example's call:
an example of a check keeps its job, requirements and all, read from the example itself
(so after its run is gone), until you change the brief.

From the command line, `whizbang store list` lists the examples after the runs, when there are any, each by its
id, task and name, and `whizbang store delete --example <id>` deletes one.

Held by `play_examples::an_example_keeps_its_inputs_after_its_run_is_deleted_and_pruned`,
`play_examples::opening_an_example_starts_no_run`,
`play_examples::an_example_of_a_check_opened_unchanged_makes_its_recorded_call_after_its_run_is_gone`,
`play_examples::an_example_whose_backend_is_not_offered_now_names_it_and_falls_back`,
`play_examples::a_name_or_notes_holding_markup_are_shown_as_text`,
`play_examples::past_1000_examples_a_new_one_is_refused_naming_the_bound` and
`play_examples::store_list_names_an_example_and_store_delete_example_lets_it_go` (fast tier);
`examples.spec.ts: an example saved from a result opens as a draft that runs nothing, after a restart and after its run was deleted`
(Chrome tier).

## Bundles

A bundle carries a run, or a chain of runs, out of one store as one file that another store
takes in (`docs/guides/bundles-and-replay.md` says what it holds and how it is checked).

- **Bundle this run…**, in a result's More menu, says what the bundle holds and never holds
  (no key, token, endpoint, path or model weights) and its size: about the sum of what the
  store holds of the run before it is written, and exactly once it is. **Download bundle**
  saves `<run>.whizbang-bundle`.
- **A chain** is made in History: **Make a chain**, then **Pick** runs in the order they ran,
  up to 16. Each pick is checked by the host, which names each link (a later run's input
  that is an earlier pick's output) or why it refuses the chain (an input two picked outputs
  could be, or a run another run called). **Bundle chain** saves the file,
  `chain-<first run>.whizbang-bundle`, and History says so under its filters: the file's
  name, its steps and its size.
- **Bundles** (`#/bundles`, from History) lists the bundles the store holds. A bundle file
  dropped on its frame, or chosen, is sent to the host, read whole and checked before
  anything of it is kept, and shown (`#/bundles/<digest>`): its steps and their links,
  whether each step replays offline as this build decides it from the step's records (never
  from what the bundle says of itself), what it holds and what it withholds, each with why.
  A file that is not a bundle this build can read is refused, naming its first fault, and
  nothing is kept. **Delete bundle** asks once, then lets it go: what a run of this store
  took from it stays, and so do the pictures a step's draft was given from it, which became
  imports of this store when the step was opened.

The page never names a file by its path: a bundle goes to the host as its bytes, at most
256 MiB, one of the host's two uploads at a time, and is read and taken in one at a time.
A bundle is not encrypted and holds the runs' pictures and words.

Held by `play_bundles::a_bundle_uploaded_is_read_whole_before_anything_is_imported`,
`play_bundles::a_bundle_over_256_mib_is_refused_413_before_it_is_read_whole`,
`play_bundles::with_two_bundle_uploads_stalled_a_third_is_refused_429_and_polling_and_stop_are_answered`,
`play_bundles::a_deleted_bundle_leaves_the_artifacts_a_run_names` and
`play_bundles::no_bundle_route_takes_a_path` (fast tier);
`send.spec.ts: bundle this run downloads one file that imports into another playground` and
`security.spec.ts: a bundle holding markup in its words shows them as text` (Chrome tier).

## Replaying a chain

**Start the chain**, on a chain's bundle, opens the chain's page (`#/chain/<digest>`): its
steps, each with its state (ended, to review, waits, stopped, blocked), and **Review step
N**. Reviewing a step opens it as an ordinary draft of its task, saying which step of which
chain it is: its inputs the bundle carries are given to this store as an import, and each
input linked to an earlier step is bound to the output of that step's new run, at the same
port and place, beside the one recorded: "same as recorded", or "differs from recorded"
with both identities. Change anything, then press **Run**: nothing runs until you do, and
nothing runs two steps by itself. The run is an ordinary run under the config as it is now,
its cost reviewed as any run's, and its record names the bundle and the step it makes again.
A step left as it is makes the call recorded, but for its linked inputs: a check's job is
the step's, read from the bundle, requirements and all, though the form shows its brief
alone, until you change the brief; and what the step was asked that no field shows is
kept and sent as it was, which the draft names.

On the step's result, a card names the step by its task's title, as the chain's page does,
and **Continue to step N** opens the next step the same way. A link that
finds no output in the new run, or two that could be it, stops the chain there, saying
why. **Stop** stops it; the runs it made stay. The chain's place (which step, which runs)
is kept in the browser, so closing the tab loses nothing, and the chain's page reads it
again from its runs and its bundle. At the end, **Chain done** offers **Compare last step**
(the step's new run beside the run it replays, when this store holds both) and **Bundle the
new chain**.

A chain's runs, like a replay's, are left out of History and its figures unless asked
("With replays", "Replays only").

Held by `play_chain::a_step_runs_only_when_started_and_continue_binds_the_new_output`,
`play_chain::a_link_that_finds_no_output_or_two_stops_the_chain`,
`play_chain::a_run_started_from_a_step_records_the_bundle_and_the_step` and
`play_chain::an_unchanged_step_makes_the_call_it_recorded_requirements_and_all` (fast tier);
`chain.spec.ts: a chain of a removal and its check replays one ordinary run a step, binds the new output and shows its changed identity`
and `chain.spec.ts: stop leaves the chain and a reopened tab finds its runs` (Chrome tier).

## Replaying a run

**Replay…**, in a result's More menu, is always offered. **Offline**, the default, makes the
run again with every model's answer the one it recorded: nothing is called and nothing is
spent; the pixel work runs again. **Live** makes the call again under the config as it is
now; its cost is reviewed first, as any paid run's, and the button says what it may spend.
When the engine refuses the replay (a run another run called, one that was interrupted, a
task this build does not ship at that version, a tool file offline), the dialog says why in
its words.

A replay is a run of the host like any other: followed, stopped and counted among the four
that go at once. Once it has ended, its result opens with the replay's report on top:
**Reproduced**, **Differs** (each difference by the place of its run, with **Compare side
by side**), **Diverged** (the replay asked something the run never asked, and nothing was
called) or **Can't be answered offline** (with what was missing). A live replay lists the
settings that changed since. What a segmenter's backend read of the machine as it answered
(its device, the memory the GPU held) is not compared, so a live replay of an unchanged
segment is **Reproduced** (`docs/guides/bundles-and-replay.md`).

Held by `play_replay::an_offline_replay_from_the_page_spends_nothing_and_shows_reproduced`
and `replay::two_segment_diagnostics_that_differ_only_in_what_the_backend_read_of_its_machine_are_the_same`
(fast tier); `replay.spec.ts: replay of a result shows reproduced, and a changed one shows where it differs`
(Chrome tier).

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
- **Replays** are left out unless asked: "With replays" lists them too, "Replays only" lists
  them alone (`replayed=include|only`, as `whizbang runs stats --replayed` takes it). A run
  of a replayed family, a chain's step among them, is a replay.
- **Make a chain** picks runs for a chain's bundle (above, "Bundles").
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
top bar says the same figure, what is spent since the last reset of the allowance and what
runs still going hold ("$7.07 spent · $5.93 held of $25.00"; "at least" when the books
cannot say exactly; "no allowance" before any reset), and reads it again when a run ends or
the allowance is reset.

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
what is spent since it` (Chrome tier); an empty env file showing NO KEY and Run
refused before any run starts, by hand (`docs/results/m5-playground.md`).

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
and the playground holds a store for as long as it runs. A store on a mount of a network
type (NFS, SMB, AFP, WebDAV, 9p, Ceph, GlusterFS, a FUSE mount such as sshfs) is refused when
it is opened, naming the type, and so is one on a file system that grants its writer's lock
twice.

The pictures you send and the shapes you draw are kept until you let them go:
`whizbang store prune --unused-imports-older-than <hours>` removes each one that no run
took and that was sent longer ago than that, with the names the playground keeps beside it,
and then prunes. One a run took stays as long as the run does.

The decisions behind the playground's host are in ADR 0041, and those behind its interface
in ADR 0043.
