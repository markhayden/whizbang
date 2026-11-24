//! `checked::…`: what the checked prober hands on of an answer's body (ADR 0046; plan
//! m6, task R1, item 5): at most 4 KiB, held to that bound before it is handed on.

use std::sync::Arc;

use whizbang_core::capability::FaultKind;
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{CheckedProber, PROBE_BODY_MAX_BYTES, ProbeRequest, Probed, Prober};
use whizbang_testkit::sending::FakeProber;

use super::support::{address, approving, policy, recipient};

#[test]
fn a_probes_body_past_4_kib_is_refused_by_the_checked_prober() {
    assert_eq!(PROBE_BODY_MAX_BYTES, 4096);
    let approved = policy(&format!("{}probe = true\n", approving(&[])));
    let at_bound = vec![b'a'; PROBE_BODY_MAX_BYTES];
    let past = vec![b'a'; PROBE_BODY_MAX_BYTES + 1];
    let fake = Arc::new(FakeProber::scripted(vec![
        Ok(Probed::Reached {
            status: 200,
            body: at_bound.clone(),
        }),
        Ok(Probed::Reached {
            status: 200,
            body: past,
        }),
    ]));
    let checked = CheckedProber::new(fake.clone(), approved, recipient());
    let probe = || {
        let request = ProbeRequest::new(address("https://api.example.com/pack/1/health"));
        checked.probe(&request, &Cancellation::new())
    };
    // A body of 4 KiB is handed on whole.
    assert_eq!(
        probe().unwrap(),
        Probed::Reached {
            status: 200,
            body: at_bound
        }
    );
    // One byte more, and the probe is refused, none of its body handed on.
    let refused = probe().unwrap_err();
    assert_eq!(refused.kind(), FaultKind::TooLarge, "{refused:?}");
    assert!(refused.sentence().contains("4 KiB"));
    assert_eq!(fake.requests(), 2);
}
