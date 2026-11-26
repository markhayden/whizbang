//! `decode::…`: the decode's protocol version 2, where `neither` is a loss of the recipe and
//! `not_worse` can be reached (plan mE, task P0, item 3), and version 1 as it was.

use whizbang_eval::lab::decode::{
    self, Assignment, Counts, Outcome, Pair, Pick, Picked, Picks, Shown, Side,
};
use whizbang_eval::lab::protocol::Protocol;

use Picked::{Left as W, Neither as N, Right as L, Same as S};

/// A protocol of one group `g` of eight images, under this version and `min_decided`.
fn protocol(version: u32, min_decided: u32) -> (Protocol, String) {
    let text = format!(
        "version = {version}\nname = \"t\"\nseed = 7\narms = [\"recipe\", \"reference\"]\n\
         min_decided = {min_decided}\n[[group]]\nname = \"g\"\n\
         images = [\"i1\", \"i2\", \"i3\", \"i4\", \"i5\", \"i6\", \"i7\", \"i8\"]\n"
    );
    Protocol::parse(text.as_bytes()).unwrap()
}

/// The group's decode of these picks, arm `a` on the left of every pair (so `W`, left, is
/// a win and `L`, right, a loss).
fn decoded(version: u32, min_decided: u32, picked: [Picked; 8]) -> decode::Decode {
    let (protocol, digest) = protocol(version, min_decided);
    let shown = |cell: String| Shown { cell, attempt: 1 };
    let assignment = Assignment {
        assignment_version: 1,
        lab: "t".to_owned(),
        protocol_digest: digest.clone(),
        arms: protocol.arms.clone(),
        seed: 7,
        subset: None,
        pair: (1..=8)
            .map(|i| Pair {
                id: format!("i{i}"),
                group: "g".to_owned(),
                left: shown(format!("a-{i}")),
                right: shown(format!("b-{i}")),
                a_side: Side::Left,
            })
            .collect(),
    };
    let picks = Picks {
        picks_version: 1,
        assignment_digest: "sha256:assignment".to_owned(),
        pick: (1..=8)
            .zip(picked)
            .map(|(i, pick)| Pick {
                pair: format!("i{i}"),
                pick,
            })
            .collect(),
    };
    decode::decode(&protocol, &digest, &assignment, "sha256:assignment", &picks).unwrap()
}

fn outcome(version: u32, min_decided: u32, picked: [Picked; 8]) -> Outcome {
    decoded(version, min_decided, picked).groups[0].outcome
}

#[test]
fn not_worse_is_reached_when_same_picks_keep_losses_under_a_third() {
    // Of 8 pairs picked, 3 wins, 2 losses and 3 `same`: wins 3 of 5 decided, under two
    // thirds; losses 2 of 8 picked, under a third.
    assert_eq!(outcome(2, 8, [W, W, W, L, L, S, S, S]), Outcome::NotWorse);
    // Every pair `same`: the recipe alike to the reference.
    assert_eq!(outcome(2, 8, [S; 8]), Outcome::NotWorse);
    // Losses at a third of the pairs picked are not under it.
    let counts = Counts {
        wins: 3,
        losses: 3,
        same: 3,
        neither: 0,
    };
    assert_eq!(decode::outcome(&protocol(2, 1).0, counts), Outcome::Worse);
    // Under the default fractions it is reached at any size of round, by `same` picks
    // alone, and never under version 1.
    for picked in 1..=30_u32 {
        let same = Counts {
            same: picked,
            ..Counts::default()
        };
        assert_eq!(decode::outcome(&protocol(2, 1).0, same), Outcome::NotWorse);
        assert_eq!(
            decode::outcome(&protocol(1, 1).0, same),
            Outcome::Inconclusive
        );
    }
}

#[test]
fn a_same_pick_does_not_make_a_version_2_round_inconclusive() {
    // Version 2 counts the minimum over pairs picked: eight picked of a minimum of eight.
    assert_eq!(outcome(2, 8, [W, W, W, W, S, S, S, S]), Outcome::Preferred);
    assert_eq!(outcome(2, 8, [S; 8]), Outcome::NotWorse);
    // Version 1 counts it over decided pairs, which `same` is not.
    assert_eq!(
        outcome(1, 8, [W, W, W, W, S, S, S, S]),
        Outcome::Inconclusive
    );
    // A group of fewer pairs than the minimum is inconclusive under either.
    assert_eq!(outcome(2, 9, [W; 8]), Outcome::Inconclusive);
}

#[test]
fn neither_is_a_loss_of_the_recipe_in_version_2() {
    // 7 `neither` and 1 loss is worse.
    let decode = decoded(2, 8, [N, N, N, N, N, N, N, L]);
    let g = &decode.groups[0];
    assert_eq!(
        (g.outcome, g.decided, g.wins, g.losses, g.same, g.neither),
        (Outcome::Worse, 1, 0, 1, 0, 7)
    );
    // 5 wins and 3 `neither`: wins 5 of 8 decided, under two thirds; losses 3 of 8, not
    // under a third. Version 1 reads the same picks as 5 wins of 5, preferred.
    let picked = [W, W, W, W, W, N, N, N];
    assert_eq!(outcome(2, 3, picked), Outcome::Worse);
    assert_eq!(outcome(1, 3, picked), Outcome::Preferred);
    // A `neither` in place of a `same` takes a round from not_worse to worse.
    assert_eq!(outcome(2, 8, [W, W, W, L, L, S, S, S]), Outcome::NotWorse);
    assert_eq!(outcome(2, 8, [W, W, W, L, L, N, S, S]), Outcome::Worse);
    // The decode says the rules it read.
    assert!(
        decode
            .text()
            .contains("protocol version 2: a neither is a loss")
    );
    // And the losses it counted, so a record written from it cannot report one loss.
    assert_eq!(decode.groups[0].counted_losses, 8);
    assert!(
        decode
            .text()
            .contains("neither 7, losses with neither 8: worse"),
        "{}",
        decode.text()
    );
}

#[test]
fn no_decided_pair_is_never_preferred() {
    // No win, no loss and no `neither`: no share of wins reaches a fraction of nothing.
    assert_ne!(outcome(2, 1, [S; 8]), Outcome::Preferred);
    let parse = |version: u32, rules: &str| {
        let text = format!(
            "version = {version}\nname = \"t\"\nseed = 7\narms = [\"a\", \"b\"]\n\
             min_decided = 1\n[rules]\n{rules}[[group]]\nname = \"g\"\nimages = [\"i1\"]\n"
        );
        Protocol::parse(text.as_bytes()).map(|(p, _)| p)
    };
    // A preferred fraction at a half or under, or a not_worse one over a half, would let
    // one decided pair make a group preferred, or a loss leave it not worse: refused,
    // under either version.
    for version in [1, 2] {
        for rules in [
            "preferred = [0, 1]\nnot_worse = [1, 3]\n",
            "preferred = [1, 2]\nnot_worse = [1, 3]\n",
            "preferred = [2, 3]\nnot_worse = [2, 3]\n",
            "preferred = [2, 3]\nnot_worse = [1, 1]\n",
        ] {
            let refused = parse(version, rules).unwrap_err();
            assert!(refused.contains("half"), "{rules}: {refused}");
        }
    }
    // At the edges that are taken, all losses or all `neither` is never preferred nor
    // not worse, whatever the size of the round.
    let edge = parse(2, "preferred = [51, 100]\nnot_worse = [1, 2]\n").unwrap();
    let v1 = parse(1, "preferred = [51, 100]\nnot_worse = [1, 2]\n").unwrap();
    for n in 1..=20_u32 {
        let lost = Counts {
            losses: n,
            ..Counts::default()
        };
        let neither = Counts {
            neither: n,
            ..Counts::default()
        };
        assert_eq!(decode::outcome(&edge, lost), Outcome::Worse, "{n} losses");
        assert_eq!(
            decode::outcome(&edge, neither),
            Outcome::Worse,
            "{n} neither"
        );
        assert_eq!(
            decode::outcome(&v1, lost),
            Outcome::Worse,
            "{n} losses, version 1"
        );
        let same = Counts {
            same: n,
            ..Counts::default()
        };
        assert_eq!(decode::outcome(&edge, same), Outcome::NotWorse, "{n} same");
    }
    assert_eq!(outcome(2, 1, [N; 8]), Outcome::Worse);
    assert_eq!(outcome(2, 1, [L; 8]), Outcome::Worse);
}

#[test]
fn a_version_1_protocol_decodes_as_before() {
    // Rascal's rules: same and neither are neither side, the minimum is of decided pairs,
    // and losses are read over the decided pairs: 3 wins and 2 losses of 5 is worse.
    let decode = decoded(1, 3, [W, W, W, L, L, S, S, S]);
    assert_eq!(decode.groups[0].outcome, Outcome::Worse);
    assert_eq!(
        decode.text(),
        "arms: recipe against reference\ngroup g: pairs 8, decided 5, wins 3, losses 2, same \
         3, neither 0: worse\n"
    );
    assert_eq!(outcome(1, 3, [W, W, W, W, W, W, L, N]), Outcome::Preferred);
    assert_eq!(
        outcome(1, 3, [W, W, S, S, S, S, S, N]),
        Outcome::Inconclusive
    );
    // Under its default fractions, not_worse is never reached.
    let (v1, _) = protocol(1, 1);
    for wins in 0..=20_u32 {
        for losses in 0..=(20 - wins) {
            let counts = Counts {
                wins,
                losses,
                same: 1,
                neither: 1,
            };
            assert_ne!(decode::outcome(&v1, counts), Outcome::NotWorse);
        }
    }
    // A version this build does not know is refused.
    let text = "version = 3\nname = \"t\"\nseed = 7\narms = [\"a\", \"b\"]\n[[group]]\n\
                name = \"g\"\nimages = [\"i1\"]\n";
    assert!(
        Protocol::parse(text.as_bytes())
            .unwrap_err()
            .contains("not 1 or 2")
    );
}
