# Install: from a release archive

Whizbang is released as one archive for each of its two platforms. An archive holds the
`whizbang` binary, its licence, the licences of the code it is built from, and this page.
Everything else whizbang needs is inside the binary: the Python pack, the recipes, the
rules and the catalogue of models.

| Platform | Archive |
|---|---|
| macOS 15 or later, on Apple silicon | `whizbang-<version>-aarch64-apple-darwin.tar.gz` |
| Ubuntu 22.04, on x86-64 | `whizbang-<version>-x86_64-unknown-linux-gnu.tar.gz` |

`<version>` below is the version you install, such as `0.1.0`, and `<target>` is
`aarch64-apple-darwin` or `x86_64-unknown-linux-gnu`.

## Download, check, unpack

The archives and the file `SHA256SUMS` are on the release's page,
`https://github.com/markhayden/whizbang/releases`. Download both with `curl`, in an empty
folder:

```sh
curl -fLO https://github.com/markhayden/whizbang/releases/download/v<version>/whizbang-<version>-<target>.tar.gz
curl -fLO https://github.com/markhayden/whizbang/releases/download/v<version>/SHA256SUMS
```

Check that the archive is the one that was released, before you unpack it. On a Mac:

```sh
grep -F "whizbang-<version>-<target>.tar.gz" SHA256SUMS | shasum -a 256 -c -
```

On Linux:

```sh
grep -F "whizbang-<version>-<target>.tar.gz" SHA256SUMS | sha256sum -c -
```

Either prints the archive's name and `OK`. If it does not, do not use the archive.

Unpack it, and put the binary in a folder that is on your `PATH`. `/usr/local/bin` is on
it on both platforms; any other folder on it does as well, and one of your own needs no
`sudo`:

```sh
tar -xzf whizbang-<version>-<target>.tar.gz
sudo mkdir -p /usr/local/bin
sudo install -m 755 whizbang-<version>-<target>/whizbang /usr/local/bin/whizbang
whizbang --version
```

The last command prints the version and the commit the binary was built from:
`whizbang <version> (commit <40 hexadecimal digits>)`. `whizbang describe` lists what
the build can do.

### On a Mac: download with `curl`, not with a browser

The binary is not signed with a developer's identity and is not notarised by Apple. A
browser marks every file it downloads, and macOS refuses to start an unsigned program
that carries the mark. `curl` sets no mark, so an archive downloaded with the commands
above unpacks to a binary that runs. If you downloaded the archive with a browser, download
it again with `curl`.

## What the Python pack needs first

The binary runs by itself. The Python pack, the worker whizbang starts for a local model,
is installed by `whizbang setup`, which downloads no Python and needs two things to be
there first:

1. **`uv`, at version 0.12.23.** Take it from its maker's releases on GitHub, at
   `https://github.com/astral-sh/uv/releases/tag/0.12.23`, check it against the SHA-256
   published beside it there, and put the `uv` binary on your `PATH`:

   ```sh
   curl -fLO https://github.com/astral-sh/uv/releases/download/0.12.23/uv-<target>.tar.gz
   curl -fLO https://github.com/astral-sh/uv/releases/download/0.12.23/uv-<target>.tar.gz.sha256
   shasum -a 256 -c uv-<target>.tar.gz.sha256
   tar -xzf uv-<target>.tar.gz
   sudo install -m 755 uv-<target>/uv /usr/local/bin/uv
   uv --version
   ```

   On Linux the check is `sha256sum -c uv-<target>.tar.gz.sha256`. Either prints the
   archive's name and `OK`. `whizbang setup` refuses another version of `uv`; `--uv` names
   one by an absolute path.
2. **Python 3.12.15, installed by that `uv`,** from GitHub's releases and from nowhere
   else:

   ```sh
   uv python install --no-config --no-bin --mirror https://github.com/astral-sh/python-build-standalone/releases/download 3.12.15
   ```

   `uv` keeps it in your home folder, or in the folder `UV_PYTHON_INSTALL_DIR` names when
   that variable is set; `whizbang setup` looks in the same place.

Then:

```sh
whizbang setup
```

It contacts nothing, installs the pack's core, starts its worker once and prints what the
worker said of itself. The pack, where it is installed and how to remove it are in the
setup guide, `https://github.com/markhayden/whizbang/blob/main/docs/guides/setup.md`.

## What each platform can do

| | macOS on Apple silicon | Linux on x86-64 |
|---|---|---|
| Every operation and recipe that uses no model; the store, runs, spend, export and import; `whizbang serve` over stdio; the evaluation and lab commands | Yes | Yes |
| Fills through fal; the judges of the three providers; `whizbang squint check`; `remove` and the repairs given a location | Yes, with your keys | Yes, with your keys |
| The Python pack's core and its worker | Yes | Yes |
| A local model: `segment` on SAM 3, and so `remove-prompt` | Yes, after `whizbang setup --extra torch` and `whizbang models get sam3` | No |

**On Linux no local model runs.** `whizbang setup --extra torch` is refused there before
anything is downloaded, and a command that needs a local model says that the model does
not run on this platform, and runs nothing. Everything in the first three rows works.

**Keys.** A fill and a judge are a provider's service, paid with your own key, which
lives in an env file you name; nothing is sent anywhere until a config approves the
recipient. The config guide,
`https://github.com/markhayden/whizbang/blob/main/docs/guides/config.md`, says how.

## A local model, on a Mac

```sh
whizbang setup --extra torch
whizbang models get sam3
```

The first installs the runtime local models run on, about 760 MB, from the Python package
index. The second downloads SAM 3, about 3.4 GB, **from its maker, on Hugging Face, with
your own sign-in**: whizbang does not host the model's files, and SAM 3's are behind a
sign-in that you make and whose terms you accept there. The models guide,
`https://github.com/markhayden/whizbang/blob/main/docs/guides/models.md`, says where the
sign-in is read from and how to name it in the config.

## Removing it

Delete the binary. The pack and the models are in the models directory (the models guide
says where); a store is the folder you named with `--store`.
