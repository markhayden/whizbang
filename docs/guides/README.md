# Guides

One page per door and per "how to add" task (`SPEC.md` section 12), and one for the config
file. A guide ships with the milestone that makes its subject work, and is listed here until
then. Milestones are those of `SPEC.md` section 16.

## Written

| Guide | Covers | Since |
|---|---|---|
| [The config file](config.md) | Writing the one TOML file that chooses backends, caps a call's spend and approves recipients, and the env file that holds the keys | Checkpoint 1B |
| [Models](models.md) | `whizbang models list`, `get` and `remove`: what the catalogue holds, installing a model, removing one, and where the models directory is | Checkpoint 1C |
| [Backends](backends.md) | `whizbang backends status`: whether each backend the config offers is ready, and `--probe` | Checkpoint 1C |
| [Setup](setup.md) | `whizbang setup`: installing the Python pack and its runtime from the lock file, where it goes, and every place it contacts | Checkpoint 1C |
| [The MCP server over stdio](serve.md) | `whizbang serve` for a local agent: the tools, progress, cancellation, the stated time of a run and the end of the input | 2 |
| [Install](install.md) | Installing from a release archive: download, the SHA-256 check, what the Python pack needs first, what each platform can do, and why a Mac downloads with `curl`. The archive carries this page as its `README.md` | 4 |
| [The loop: remove, check, retry](loop.md) | `whizbang rascal remove-checked`: what an attempt is, when it tries again, the plan said before anything runs, what three attempts reserve with each judge, what it prints and hands back when every attempt fails, and its exit codes | 4 |
| [The playground](playground.md) | `whizbang play`: starting it, the address and its token, reopening the page, stopping, a run's stated time, what it holds while it runs, and every screen: Home, a kit, the workspace, a run going, its result, History, a run's report, a tool's record, and Models | 5 |
| [The engine API](engine-api.md) | `whizbang serve --http` for a hosted app: starting it, the token, off loopback behind a proxy, the routes, calling a tool, reconnecting, cancelling, the uploads it keeps, stopping, and user story 1 end to end in `curl` | 6 |
| [A GPU host of your own](gpu-host.md) | SAM 3 on a GPU host you deploy with your own account and tokens, for the kind `remote`: the front and the worker, a machine of your own, Modal's deployment, the config, the price and what it does not bound, the spend limit, SAM 3's files and its licence, the token | 6 |

## Doors

| Guide | Covers | Milestone |
|---|---|---|
| The command line | `whizbang` for a person and for scripts | Checkpoint 1A |

## How to add

| Guide | Covers | Milestone |
|---|---|---|
| Adding a model | An adapter, its catalogue entry and its conformance fixtures | Checkpoint 1D |
| Adding a tool | Writing an experimental tool, trying and scoring it, and taking it up the ladder of `SPEC.md` section 8. Says that an experimental tool and a user's adapter are trusted code | 2 |
| Running an evaluation | Freezing a gate, running it, writing up the result | 2 |
| Writing a recipe | A named, typed sequence of steps over primitives and other recipes, including a user's own recipe file | 3 |
| Adding a kit | A kit beside rascal and squint | When Pixel starts |

## Rules for a guide

- An example is marked with the test tier that runs it. In the fast tier it uses offline
  fixtures; a documentation check never spends.
- A guide does not repeat the reference. What a build can do is in `docs/reference/`, which
  is generated.
- The procedures agents follow (starting a task, porting a module, reviewing and merging,
  cutting a release) are skills under `.claude/skills/`, not guides.
