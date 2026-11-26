//! What `run/8` adds to a run's record (plan mE, task L1, item 1; ADR 0005, "run/8"): why a
//! loop's attempts ended as one word of a closed set, `reason`, in the group `attempts`
//! after the sentence `ended`. It is written only where the loop said it; a record that
//! does not hold it is written as the version before, which holds all of it.
//!
//! The type is part of the record ([`super::record`] names it again), and is read from
//! JSON only as part of a stored record.

use serde::{Deserialize, Serialize};

/// Why a loop's attempts ended, as one word of a closed set, beside the sentence the
/// loop wrote (ADR 0039). The words are those `whizbang eval loop score` and
/// `scripts/loop-evidence.py` read the sentences of older records as, so a count by
/// word and a count by sentence agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptsEnded {
    /// An attempt passed.
    Passed,
    /// The findings of an attempt were cut short, and never enlarge a mask.
    FindingsCutShort,
    /// An attempt changed something that must be preserved.
    DamagedSomethingPreserved,
    /// No finding says with confidence that the named thing is still there.
    NothingSaysItRemains,
    /// The last attempt the plan allows was made.
    LastAttemptOfThePlan,
    /// A retry was called for, and the one wider mask was used already.
    WiderMaskAlreadyUsed,
    /// A retry's mask was the one before it, and an attempt is not made twice.
    MaskIsTheOneBefore,
    /// One unchecked pass, as the plan said.
    OneUncheckedPass,
    /// The check of an attempt could not run.
    CheckCouldNotRun,
    /// The budget ran out after an attempt.
    BudgetRanOut,
    /// The deadline passed after an attempt.
    DeadlinePassed,
    /// A retry could not be made, and the attempts before it stand.
    RetryCouldNotBeMade,
}

impl AttemptsEnded {
    /// Every word, in the order of the type.
    pub const ALL: [AttemptsEnded; 12] = [
        AttemptsEnded::Passed,
        AttemptsEnded::FindingsCutShort,
        AttemptsEnded::DamagedSomethingPreserved,
        AttemptsEnded::NothingSaysItRemains,
        AttemptsEnded::LastAttemptOfThePlan,
        AttemptsEnded::WiderMaskAlreadyUsed,
        AttemptsEnded::MaskIsTheOneBefore,
        AttemptsEnded::OneUncheckedPass,
        AttemptsEnded::CheckCouldNotRun,
        AttemptsEnded::BudgetRanOut,
        AttemptsEnded::DeadlinePassed,
        AttemptsEnded::RetryCouldNotBeMade,
    ];

    /// The word, as a record writes it.
    pub fn word(self) -> &'static str {
        match self {
            AttemptsEnded::Passed => "passed",
            AttemptsEnded::FindingsCutShort => "findings_cut_short",
            AttemptsEnded::DamagedSomethingPreserved => "damaged_something_preserved",
            AttemptsEnded::NothingSaysItRemains => "nothing_says_it_remains",
            AttemptsEnded::LastAttemptOfThePlan => "last_attempt_of_the_plan",
            AttemptsEnded::WiderMaskAlreadyUsed => "wider_mask_already_used",
            AttemptsEnded::MaskIsTheOneBefore => "mask_is_the_one_before",
            AttemptsEnded::OneUncheckedPass => "one_unchecked_pass",
            AttemptsEnded::CheckCouldNotRun => "check_could_not_run",
            AttemptsEnded::BudgetRanOut => "budget_ran_out",
            AttemptsEnded::DeadlinePassed => "deadline_passed",
            AttemptsEnded::RetryCouldNotBeMade => "retry_could_not_be_made",
        }
    }
}
