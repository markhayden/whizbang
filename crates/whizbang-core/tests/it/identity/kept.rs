//! What is decoded, kept while a run asks for it (plan mE, task L2, item 4): a file read
//! again under the same limits is not decoded again while a keep is held, and is once
//! the last is let go; at most four files are kept, the oldest let go first.

use whizbang_core::identity::{DecodeLimits, Digest, decodes_of, identify, keep_decoded};

use crate::artifact::support::png;

/// A file of its own for each test, so that no other test's decodes are counted.
fn file(seed: u8) -> Vec<u8> {
    png(move |x, y| [seed, x as u8, y as u8, 255])
}

#[test]
fn a_file_identified_again_while_decodes_are_kept_is_not_decoded_again() {
    let limits = DecodeLimits::default();
    let picture = file(201);
    let digest = Digest::of_bytes(&picture);
    let first = identify(&picture, &limits).unwrap();
    identify(&picture, &limits).unwrap();
    assert_eq!(
        decodes_of(digest),
        2,
        "nothing is kept while no keep is held"
    );
    {
        let _outer = keep_decoded();
        let inner = keep_decoded();
        assert_eq!(identify(&picture, &limits).unwrap(), first);
        for _ in 0..5 {
            assert_eq!(identify(&picture, &limits).unwrap(), first);
        }
        assert_eq!(decodes_of(digest), 3);
        // Under other limits it is decoded again.
        let other = DecodeLimits {
            max_bytes: limits.max_bytes - 1,
            ..limits
        };
        identify(&picture, &other).unwrap();
        assert_eq!(decodes_of(digest), 4);
        // Four other files read after it let it go.
        drop(inner);
        for seed in 202..206 {
            identify(&file(seed), &limits).unwrap();
        }
        identify(&picture, &limits).unwrap();
        assert_eq!(decodes_of(digest), 5);
    }
    // Once the last keep is let go, nothing is kept.
    identify(&picture, &limits).unwrap();
    assert_eq!(decodes_of(digest), 6);
}
