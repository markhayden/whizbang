# The MCP server over stdio

`whizbang serve` is the door for a local agent (`SPEC.md` section 1): an MCP server on its
standard input and output. Each action is a tool; `check` takes the use case, the pictures
by their identities in the store with their roles, and the brief. It ends when its input
does.

```text
whizbang serve --store <dir> [--config <file> [--env-file <file>]] [--models-dir <dir>]
               [--wait-s <seconds>]
```

- **The same checks as the command line.** A call runs through preparation, admission and
  spend, and what it sends a judge goes through the gate on what leaves the machine. A
  config that names a key is not served without its env file.
- **Pictures by identity.** Bring a picture into the store with `whizbang store import
  --picture <file> --store <dir>`, and name it by the identity that prints. A picture
  given inline is not taken.
- **Progress, cancellation and a stated time.** A call made with a progress token is told
  each event of its run as it happens. A call the client cancels ends its run as
  cancelled and is not answered. A run still going after `--wait-s` seconds (900 by
  default) is cancelled, and its call answered that the server gave up on it.
- **The end of the input** cancels every run still going, waits for each up to ten
  seconds, and stops the worker of the Python pack.
- **Only the protocol is written to the standard output**, one message to a line; a
  complaint before the server starts goes to the error output, and the exit is 1.
- **Bounds.** A message is at most 1 MiB and nests at most 16 deep; at most four calls run
  at once.

With `[cache]` in the config, a check asked again unchanged is answered from the run that
answered it first ([the config file](config.md)). The decisions behind the server are in
ADR 0031.

The same tools over HTTP, for a hosted app, are `whizbang serve --http`: [the engine API](engine-api.md).
