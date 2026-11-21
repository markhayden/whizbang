//! The fixture host of the playground's Chrome cases (plan m5, task U1, item 5): the
//! playground's host as it ships (`whizbang_serve::play::Host`, task H1), over the
//! binary's registry and wiring and the door the binary hands it
//! (`whizbang::playing::Playground`), started by Playwright's global setup in a new store.
//! It is an example target, so the binary never holds it.
//!
//! It prints one line first, the page's address with the launch token, as `whizbang play
//! --no-open --stdin-quit --port 0` does, so the test that started it can open the page:
//!
//! ```text
//! http://127.0.0.1:<port>/#k=<64 hex>
//! ```
//!
//! It is wired under no config: no backend, so nothing a case does reaches a model or
//! leaves the machine. The test kit's fake segmenter and the scripted fill and judge,
//! chosen by a header the fixture alone reads (`x-fixture-script`), are added with the
//! first Chrome cases that run a task (task W1); the seven cases of U1 need none.
//!
//! Options: `--store <folder>` (made if missing, its last folder alone), `--port <n>` (0,
//! the default, takes a free port), `--stdin-quit` (quit when the standard input ends, so
//! a test that dies leaves no host behind).

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use whizbang::playing::{Playground, address};
use whizbang::wiring::{self, Make, Over};
use whizbang_serve::play::{Host, Settings, Token, threads};

fn fail(message: &str) -> ! {
    eprintln!("play_fixture: {message}");
    std::process::exit(2)
}

fn main() {
    let mut store = std::env::temp_dir().join("whizbang-play-fixture");
    let mut port: u16 = 0;
    let mut stdin_quit = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--store" => store = args.next().map(PathBuf::from).unwrap_or(store),
            "--port" => port = args.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            "--stdin-quit" => stdin_quit = true,
            other => fail(&format!("unknown argument {other}")),
        }
    }
    let over = Over::nothing();
    let opening = wiring::opening(&store, Make::TheRoot, wiring::recovery())
        .unwrap_or_else(|error| fail(&wiring::not_opened(&error)));
    let writer = opening
        .writer()
        .unwrap_or_else(|why| fail(&why.to_string()));
    let registry = wiring::registry_over(&over, false).unwrap_or_else(|why| fail(&why));
    let descriptors = registry.descriptors().cloned().collect();
    let executor = wiring::runner(registry, writer, &over).unwrap_or_else(|why| fail(&why));
    let play = Arc::new(Playground::new(descriptors, Arc::new(over)));
    let token = Token::new().unwrap_or_else(|error| fail(&error.to_string()));
    let mut settings = Settings::new(store);
    settings.port = port;
    let host = Host::start(Arc::new(executor), play, settings, token.clone())
        .unwrap_or_else(|error| fail(&format!("cannot listen: {error}")));
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", address(host.address().port(), Some(&token)));
    let _ = out.flush();
    if stdin_quit {
        let stopper = host.stopper();
        threads::spawn(
            "input",
            Box::new(move || {
                let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
                stopper.quit();
            }),
        )
        .unwrap_or_else(|error| fail(&error.to_string()));
    }
    host.wait();
}
