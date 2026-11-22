# The playground

`whizbang play` is the door for a person trying tools in a browser (`SPEC.md` section 1): a
page in Google Chrome, served by the `whizbang` process on your own machine. This first part
covers starting it, the address and its token, reopening the page, stopping, and what the
playground holds while it runs. The screens are described as they land (milestone 5).

```text
whizbang play --store <dir> [--config <file> [--env-file <file>]] [--models-dir <dir>]
              [--port <port>] [--no-open] [--stdin-quit] [--run-limit-s <seconds>]
```

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
  the next, so that the page keeps what it saved for you. A port in use is refused, and the
  sentence names `--port`. The playground listens on `127.0.0.1` only; there is no flag that
  would make it listen anywhere else.

## The address and its token

When it starts, the playground makes a token: 32 random bytes, new at every launch. Chrome is
given a small page in the store's folder, `play-open.html`, readable by you only, which sends
it on to the address with the token after `#k=`, so the token is never on a command line; the
page keeps it for that address and takes it out of the address bar at once. The small page is
removed when the playground stops. Every call the page makes carries it. A call
without it, or with the token of an earlier launch, is refused, and the page says to run
`whizbang play` again.

With `--no-open`, nothing is opened: the address is printed with its token, for you to open.
Treat that line as you would a password for as long as the playground runs. The token is
never written to a log, a run's record or an error.

## Reopening the page

Closing the tab cancels nothing: a run started from the page belongs to the playground's
process, not to the page. Open the address again, or run `whizbang play` again with the same
store: a second `whizbang play` finds the one already serving that store and opens its
address instead of failing. The page then lists the runs still going and follows them.

A store held by anything else (another command that writes it) is refused, naming the
holder, as every command that writes a store is.

## Stopping it

Three ways, which do the same:

- **Quit** in the page;
- **Ctrl-C** in the terminal (or a hang-up, or `SIGTERM`);
- under `--stdin-quit`, the end of the standard input, for a script that starts it.

The playground stops taking requests, cancels every run it started, and waits up to ten
seconds for each to end, so that each run's record says it was cancelled and what it spent
is accounted for. Then it lets the store go and exits 0. A second Ctrl-C during those ten
seconds ends the wait at once. A run that ignored its cancellation and has not ended is left
to be ended as interrupted by the next command that opens the store to write, and the last
line says how many there were.

## A run's stated time

Each run started from the page is cancelled after an hour, or after `--run-limit-s` seconds.
Cancelling a run cancels the runs it called. A run whose work ignores the cancellation is
shown as stopping and keeps its place among the four that may run at once until it ends;
Quit is the way out.

## A result

When a run ends, its screen is the result: its verdict or how it ended; the picture it was
given and the one it made, side by side, swiped or as their difference, at one zoom; the
verdict's reasons and the places they cite; a loop's attempts, each with its outcome and the
picture it made on the way (marked as such: it is not the result); and the cost of the run
and every run it called, by capability and backend, local work shown as free.

- **Save picture** (⌘S) downloads the picture it made.
- **Send to** opens a new draft of another task with the result's outputs filled in: a field
  is filled when it is the only one of that task that takes the output's type, and left for
  you to fill when two or more do. A removal sent to "Check an edit" gives its picture as
  the source, its result as the edit and, when it had words, the job the loop writes from
  them, requirements and all. The draft says where it came from and leads back to the run.
- **Run again** opens a new draft of the same task with the run's inputs and the backends it
  used written out. Changing it changes nothing of the run.
- **Copy command** gives the `whizbang` command that makes the same call: every input by its
  identity in the store; the store, config, env file and models directory by their paths;
  each backend the run took from the config's default named explicitly; every word quoted
  for a POSIX shell. When the config has changed since the run, a comment line above the
  command says so, and the command runs under the config as it is now. Run it once the
  playground has quit: the store takes one writer.

## What the playground holds while it runs

- **The store, to write.** While `whizbang play` runs, other commands that write the store
  are refused until it exits; commands that only read it work as always.
- **`play.json`** in the store's folder: its port, its process and its token, readable by
  you only. A second `whizbang play` trusts it only while the folder and the file are yours
  alone. It is removed when the playground stops; one left by a playground that was
  killed is replaced at the next launch.
- **Threads.** The host makes 19 threads; one process holds at most 42 made by playground
  hosts.
- **Nothing on another machine.** The playground sends nothing off the machine itself; the
  runs it starts go only where your config already approves, as from the command line.

A store on a network file system is not supported: the store's locks may not hold there,
and the playground holds a store for as long as it runs.

The decisions behind the playground's host are in ADR 0041.
