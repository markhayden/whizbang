//! The examples of the config's record are read by the reader they describe (plan 1B,
//! "Rules every task's tests follow", 5): a block fenced as `toml` is read, and a block
//! fenced as `toml refused` is refused with exactly the sentence in the `text` block
//! that follows it.

use whizbang_core::config::Config;
use whizbang_core::contract::Secrets;

/// The fenced blocks of a Markdown page, each as the word after its fence and its lines.
fn blocks(page: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(String, String)> = None;
    for line in page.lines() {
        match (&mut open, line.strip_prefix("```")) {
            (None, Some(info)) => open = Some((info.trim().to_owned(), String::new())),
            (Some(_), Some(_)) => blocks.extend(open.take()),
            (Some((_, body)), None) => {
                body.push_str(line);
                body.push('\n');
            }
            (None, None) => {}
        }
    }
    assert!(open.is_none(), "a block is not closed");
    blocks
}

/// Reads every example of a page, and says how many were read and how many refused.
fn read_the_examples_of(page: &str) -> (usize, usize) {
    let none = Secrets::none();
    let blocks = blocks(page);
    let (mut read, mut refused) = (0, 0);
    for (index, (info, body)) in blocks.iter().enumerate() {
        match info.as_str() {
            "toml" => {
                if let Err(why) = Config::parse(body.as_bytes(), &none) {
                    panic!("an example is refused: {why}\n{body}");
                }
                read += 1;
            }
            "toml refused" => {
                let Err(why) = Config::parse(body.as_bytes(), &none) else {
                    panic!("an example that the page says is refused is read:\n{body}");
                };
                let (next, sentence) = blocks.get(index + 1).expect("a block follows");
                assert_eq!(next, "text", "the sentence follows the example:\n{body}");
                // The page breaks a long sentence over lines.
                let sentence = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
                assert_eq!(why.to_string(), sentence, "{body}");
                refused += 1;
            }
            _ => {}
        }
    }
    (read, refused)
}

#[test]
fn every_example_in_the_record_is_read() {
    let record = include_str!("../../../../../docs/adr/0011-the-config-file.md");
    // The count is held, so that an example whose fence was mistyped, and which this
    // test would then pass over, is missed here.
    assert_eq!(read_the_examples_of(record), (3, 0));
}
