//! The ways out (`SPEC.md` section 10, "Egress"; ADR 0012): a recipient, the four
//! classes of data, and the three interfaces through which anything leaves. Nothing is
//! enforced here: the gate is the egress strand's.

use std::sync::Arc;

use std::sync::Mutex;

use whizbang_core::capability::{
    Bounds, Declaration, Fault, FaultKind, FillHandle, FillRequest, Filler, Painted, Price, Roster,
    Support,
};

use crate::artifact::support::{HEIGHT, WIDTH};
use crate::capability::fill::picture;
use whizbang_core::contract::{
    Backends, Cancellation, Capability, Estimate, ModelIdentity, Secrets,
};
use whizbang_core::egress::{
    ACCOUNT_LIMIT, ADDRESS_MAX_BYTES, Account, Address, Classes, Credential, DataClass,
    DownloadRequest, Downloaded, Downloader, NotAllowed, Origin, Outgoing, PIECES_LIMIT, Piece,
    ProbeRequest, Probed, Prober, REPLY_MAX_BYTES, Recipient, Reply, Sender, UPSTREAM_LIMIT,
};

/// A value no test would write by accident, told to the constructors as a secret.
const MARKER: &str = "sk-marker-7f3a9c";

fn secrets() -> Secrets {
    Secrets::new([MARKER.to_owned()]).unwrap()
}

fn origin(address: &str) -> Origin {
    Origin::of(address, &Secrets::none()).unwrap()
}

fn address(text: &str) -> Address {
    Address::of(text, &Secrets::none()).unwrap()
}

/// An address whose host has this written before it as a user, a password or both. It
/// is put together here, and written whole nowhere in this file: the repo's scan for
/// secrets takes an address with a user in it for a credential, as it should.
fn with_user(user: &str, rest: &str) -> String {
    ["https://", user, "@", rest].concat()
}

fn account(text: &str) -> Account {
    Account::new(text, &Secrets::none()).unwrap()
}

// --- an origin, and a recipient -------------------------------------------------------------

#[test]
fn an_origin_is_a_scheme_a_host_and_a_port() {
    let plain = origin("https://api.example.com");
    assert_eq!(
        (plain.scheme(), plain.host(), plain.port()),
        ("https", "api.example.com", 443)
    );
    assert_eq!(plain.to_string(), "https://api.example.com");
    let ported = origin("http://gpu-host-7.internal:8443/whizbang-sam3");
    assert_eq!(
        (ported.scheme(), ported.host(), ported.port()),
        ("http", "gpu-host-7.internal", 8443)
    );
    assert_eq!(ported.to_string(), "http://gpu-host-7.internal:8443");
    // The port a scheme implies and the same port written out are one origin; a host
    // is one host in either case of its letters; and a path is no part of an origin.
    assert_eq!(origin("https://api.example.com:443"), plain);
    assert_eq!(origin("HTTPS://API.Example.COM/v1/fill"), plain);
    assert_eq!(origin("https://api.example.com/"), plain);
    assert_eq!(origin("http://localhost").port(), 80);
    assert_eq!(origin("http://10.0.3.17:9000").host(), "10.0.3.17");
}

#[test]
fn two_origins_that_differ_in_scheme_host_or_port_are_two_recipients() {
    let base = Recipient::new(origin("https://api.example.com"));
    let others = [
        "http://api.example.com",
        "https://api.example.org",
        "https://www.api.example.com",
        "https://api.example.com:8443",
        "http://api.example.com:443",
    ];
    for other in others {
        assert_ne!(Recipient::new(origin(other)), base, "{other}");
        assert_ne!(origin(other), origin("https://api.example.com"), "{other}");
    }
    assert_eq!(
        Recipient::new(origin("https://api.example.com:443/x")),
        base
    );
}

#[test]
fn a_recipient_cannot_be_made_from_an_address_with_a_user_a_password_a_query_or_a_fragment() {
    let refused = [
        (with_user("user", "api.example.com"), NotAllowed::UserInfo),
        (
            with_user("user:pass", "api.example.com/v1"),
            NotAllowed::UserInfo,
        ),
        (with_user(":pass", "api.example.com"), NotAllowed::UserInfo),
        (
            "https://api.example.com/v1@x".to_owned(),
            NotAllowed::UserInfo,
        ),
        (
            "https://api.example.com?key=1".to_owned(),
            NotAllowed::Query,
        ),
        (
            "https://api.example.com/v1?key=1".to_owned(),
            NotAllowed::Query,
        ),
        ("https://api.example.com/v1?".to_owned(), NotAllowed::Query),
        (
            "https://api.example.com#top".to_owned(),
            NotAllowed::Fragment,
        ),
        (
            "https://api.example.com/v1#".to_owned(),
            NotAllowed::Fragment,
        ),
    ];
    for (text, why) in &refused {
        let (text, why) = (text.as_str(), *why);
        assert_eq!(Origin::of(text, &Secrets::none()), Err(why), "{text}");
        assert_eq!(Recipient::at(text, &Secrets::none()), Err(why), "{text}");
    }
    let made = Recipient::at("https://api.example.com/v1", &Secrets::none()).unwrap();
    assert_eq!(made, Recipient::new(origin("https://api.example.com")));
}

#[test]
fn an_address_that_is_not_one_plain_origin_is_refused() {
    let refused = [
        ("", NotAllowed::Scheme),
        ("api.example.com", NotAllowed::Scheme),
        ("ftp://api.example.com", NotAllowed::Scheme),
        ("file:///etc/hosts", NotAllowed::Scheme),
        ("https:/api.example.com", NotAllowed::Scheme),
        ("https://", NotAllowed::Host),
        ("https:///v1", NotAllowed::Host),
        ("https://api_example.com", NotAllowed::Host),
        ("https://api..example.com", NotAllowed::Host),
        ("https://.example.com", NotAllowed::Host),
        ("https://-api.example.com", NotAllowed::Host),
        ("https://api-.example.com", NotAllowed::Host),
        ("https://api.example-", NotAllowed::Host),
        ("https://api.example.com.", NotAllowed::Host),
        ("https://ex\u{e4}mple.com", NotAllowed::Character),
        ("https://[::1]:8080", NotAllowed::Host),
        (
            "https://api.example.com\\@evil.example",
            NotAllowed::Character,
        ),
        ("https://api.example.com:", NotAllowed::Port),
        ("https://api.example.com:0", NotAllowed::Port),
        ("https://api.example.com:65536", NotAllowed::Port),
        ("https://api.example.com:0443", NotAllowed::Port),
        ("https://api.example.com:44a", NotAllowed::Port),
        ("https://api.example.com:+443", NotAllowed::Port),
        ("https://api.example.com/a b", NotAllowed::Character),
        ("https://api.example.com/a\nb", NotAllowed::Character),
        (" https://api.example.com", NotAllowed::Character),
    ];
    for (text, why) in refused {
        assert_eq!(Origin::of(text, &Secrets::none()), Err(why), "{text:?}");
    }
    let long = format!("https://api.example.com/{}", "a".repeat(ADDRESS_MAX_BYTES));
    assert_eq!(
        Origin::of(&long, &Secrets::none()),
        Err(NotAllowed::TooLong)
    );
    // A host of 253 characters is one; of 254 it is not, and a label is 63 at most.
    let label = "a".repeat(63);
    let host = format!("{label}.{label}.{label}.{}", "b".repeat(61));
    assert_eq!(host.len(), 253);
    assert_eq!(origin(&format!("https://{host}")).host(), host);
    let longer = format!("https://{host}c");
    assert_eq!(Origin::of(&longer, &Secrets::none()), Err(NotAllowed::Host));
    let wide = format!("https://{}.com", "a".repeat(64));
    assert_eq!(Origin::of(&wide, &Secrets::none()), Err(NotAllowed::Host));
}

#[test]
fn a_refusal_of_an_address_shows_nothing_of_the_address() {
    for why in [
        NotAllowed::Secret,
        NotAllowed::TooLong,
        NotAllowed::Character,
        NotAllowed::Scheme,
        NotAllowed::UserInfo,
        NotAllowed::Query,
        NotAllowed::Fragment,
        NotAllowed::Host,
        NotAllowed::Port,
        NotAllowed::Name,
        NotAllowed::TooMany,
        NotAllowed::Twice,
        NotAllowed::Elsewhere,
        NotAllowed::Empty,
    ] {
        let sentence = why.to_string();
        assert!(!sentence.is_empty() && sentence.is_ascii(), "{sentence}");
        assert!(!sentence.contains('/') && !sentence.contains("example"));
    }
    let written = with_user("user:hunter2hunter2", "api.example.com");
    let refused = Origin::of(&written, &Secrets::none());
    let shown = format!("{refused:?} {}", refused.clone().unwrap_err());
    assert!(
        !shown.contains("hunter2") && !shown.contains("example"),
        "{shown}"
    );
}

#[test]
fn an_account_is_a_bounded_name_and_is_shown_cut() {
    let whole = account("acct_4f8a1c9e77");
    assert_eq!(whole.shown(), "acct...");
    assert_eq!(format!("{whole:?}"), "Account(acct...)");
    assert_eq!(account("abcd").shown(), "abcd");
    assert_eq!(account("abcde").shown(), "abcd...");
    assert_eq!(whole, account("acct_4f8a1c9e77"));
    // Two accounts that are cut alike are still two accounts.
    assert_ne!(whole, account("acct_0000000000"));
    assert!(Account::new(&"a".repeat(ACCOUNT_LIMIT), &Secrets::none()).is_ok());
    let refused = [
        String::new(),
        "a".repeat(ACCOUNT_LIMIT + 1),
        "two words".to_owned(),
        "quo\"te".to_owned(),
        "back\\slash".to_owned(),
        "line\nbreak".to_owned(),
        "caf\u{e9}".to_owned(),
        "tab\there".to_owned(),
    ];
    for text in refused {
        let made = Account::new(&text, &Secrets::none());
        assert_eq!(made, Err(NotAllowed::Name), "{text:?}");
    }
}

#[test]
fn a_recipient_is_an_origin_an_account_and_the_upstreams_it_may_forward_to() {
    let alone = Recipient::new(origin("https://api.example.com"));
    assert_eq!(alone.origin(), &origin("https://api.example.com"));
    assert_eq!(alone.account(), None);
    assert!(alone.upstreams().is_empty());
    let upstreams = vec![
        origin("https://one.example.net"),
        origin("https://two.example.net"),
    ];
    let full = alone
        .clone()
        .with_account(account("acct_4f8a1c9e77"))
        .forwarding_to(upstreams.clone())
        .unwrap();
    assert_eq!(full.account(), Some(&account("acct_4f8a1c9e77")));
    assert_eq!(full.upstreams(), &upstreams[..]);
    // The same endpoint under another account, or with other upstreams, is another
    // recipient: an approval is of all three.
    assert_ne!(full, alone);
    assert_ne!(
        alone.clone().with_account(account("acct_4f8a1c9e77")),
        alone.clone().with_account(account("acct_0000000000"))
    );
    assert_ne!(
        full,
        alone
            .clone()
            .with_account(account("acct_4f8a1c9e77"))
            .forwarding_to(vec![origin("https://one.example.net")])
            .unwrap()
    );
    // What it may reach: itself and its upstreams, and nothing else.
    assert!(full.reaches(&origin("https://api.example.com")));
    assert!(full.reaches(&origin("https://two.example.net")));
    assert!(!full.reaches(&origin("https://three.example.net")));
    assert!(!full.reaches(&origin("http://two.example.net")));
    assert!(!alone.reaches(&origin("https://two.example.net")));
}

#[test]
fn a_recipients_upstreams_are_bounded_other_than_itself_and_named_once() {
    let base = || Recipient::new(origin("https://api.example.com"));
    let numbered = |count: usize| -> Vec<Origin> {
        (0..count)
            .map(|at| origin(&format!("https://up{at}.example.net")))
            .collect()
    };
    assert!(base().forwarding_to(numbered(UPSTREAM_LIMIT)).is_ok());
    assert_eq!(
        base().forwarding_to(numbered(UPSTREAM_LIMIT + 1)),
        Err(NotAllowed::TooMany)
    );
    let twice = vec![
        origin("https://up.example.net"),
        origin("https://up.example.net:443"),
    ];
    assert_eq!(base().forwarding_to(twice), Err(NotAllowed::Twice));
    let itself = vec![origin("https://api.example.com")];
    assert_eq!(base().forwarding_to(itself), Err(NotAllowed::Twice));
}

#[test]
fn what_is_printed_of_a_recipient_is_its_origin_and_a_cut_account_and_nothing_else() {
    let alone = Recipient::new(origin("https://api.example.com"));
    assert_eq!(format!("{alone:?}"), "Recipient(https://api.example.com)");
    let full = alone
        .with_account(account("acct_4f8a1c9e77"))
        .forwarding_to(vec![origin("https://one.example.net")])
        .unwrap();
    assert_eq!(
        format!("{full:?}"),
        "Recipient(https://api.example.com as acct...)"
    );
    assert_eq!(
        format!("{:?}", origin("http://h:81")),
        "Origin(http://h:81)"
    );
}

#[test]
fn an_account_or_an_origin_that_holds_a_secret_is_refused_whole_and_not_shown() {
    let secrets = secrets();
    let holding = [
        format!("https://{MARKER}.example.com"),
        format!("https://api.example.com/{MARKER}"),
        format!("https://api.example.com/v1/{MARKER}/fill"),
        // Past where an address is cut, and in an address that is wrong besides.
        format!(
            "https://api.example.com/{}{MARKER}",
            "a".repeat(ADDRESS_MAX_BYTES)
        ),
        with_user(&format!("user:{MARKER}"), "api.example.com"),
        format!("nonsense {MARKER}"),
    ];
    for text in &holding {
        for refused in [
            Origin::of(text, &secrets).map(|_| ()),
            Address::of(text, &secrets).map(|_| ()),
            Recipient::at(text, &secrets).map(|_| ()),
        ] {
            assert_eq!(refused, Err(NotAllowed::Secret), "{text}");
            let shown = format!("{refused:?} {}", refused.unwrap_err());
            assert!(!shown.contains("marker"), "{shown}");
        }
    }
    let accounts = [
        MARKER.to_owned(),
        format!("acct_{MARKER}"),
        // Past the bound on an account, and with a character no account holds.
        format!("{}{MARKER}", "a".repeat(ACCOUNT_LIMIT)),
        format!("two words {MARKER}"),
    ];
    for text in &accounts {
        let refused = Account::new(text, &secrets);
        assert_eq!(refused, Err(NotAllowed::Secret), "{text}");
        assert!(!format!("{refused:?}").contains("marker"));
    }
    // The same texts, to a constructor that was told of no secret, are judged as any.
    assert!(Account::new(MARKER, &Secrets::none()).is_ok());
    assert!(Origin::of(&holding[1], &Secrets::none()).is_ok());
}

// --- an address, and a credential -----------------------------------------------------------

#[test]
fn an_address_is_an_origin_and_the_rest_and_shows_only_its_origin() {
    let hop = address("https://uploads.example.net/put/9f2c?sig=abc123&exp=17");
    assert_eq!(hop.origin(), &origin("https://uploads.example.net"));
    assert_eq!(hop.rest(), "/put/9f2c?sig=abc123&exp=17");
    assert_eq!(
        format!("{hop:?}"),
        "Address(https://uploads.example.net, 27 more bytes)"
    );
    assert_eq!(address("https://api.example.com").rest(), "");
    assert_eq!(address("https://api.example.com/").rest(), "/");
    // A hop may have a query, which a recipient may not; nothing has a user, a
    // password or a fragment.
    let refused = [
        (
            with_user("user:pass", "uploads.example.net/put"),
            NotAllowed::UserInfo,
        ),
        (
            "https://uploads.example.net/put#part".to_owned(),
            NotAllowed::Fragment,
        ),
        (
            "https://uploads.example.net/put?a=1#part".to_owned(),
            NotAllowed::Fragment,
        ),
        (
            "https://uploads.example.net/put?to=a@b".to_owned(),
            NotAllowed::UserInfo,
        ),
        ("uploads.example.net/put".to_owned(), NotAllowed::Scheme),
    ];
    for (text, why) in &refused {
        let (text, why) = (text.as_str(), *why);
        assert_eq!(Address::of(text, &Secrets::none()), Err(why), "{text}");
    }
    let most = format!("https://h/{}", "a".repeat(ADDRESS_MAX_BYTES - 10));
    assert_eq!(most.len(), ADDRESS_MAX_BYTES);
    assert!(Address::of(&most, &Secrets::none()).is_ok());
    let over = format!("{most}a");
    assert_eq!(
        Address::of(&over, &Secrets::none()),
        Err(NotAllowed::TooLong)
    );
}

#[test]
fn a_credential_is_bound_to_one_origin_and_is_never_printed() {
    let credential = Credential::new(
        origin("https://api.example.com"),
        "x-api-key",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    assert_eq!(credential.origin(), &origin("https://api.example.com"));
    assert_eq!(credential.header(), "x-api-key");
    assert_eq!(credential.expose(), MARKER);
    let shown = format!("{credential:?}");
    assert_eq!(shown, "Credential(x-api-key for https://api.example.com)");
    for (header, value) in [
        ("", "v"),
        ("x api", "v"),
        ("x:api", "v"),
        ("x-api-key", ""),
        ("x-api-key", "a\nb"),
    ] {
        let made = Credential::new(
            origin("https://api.example.com"),
            header,
            value,
            &Secrets::none(),
        );
        assert!(made.is_err(), "{header:?} {value:?}");
        assert!(!format!("{made:?}").contains(value) || value.is_empty());
    }
}

// --- the classes of data --------------------------------------------------------------------

#[test]
fn there_are_four_classes_of_data() {
    let names: Vec<&str> = DataClass::ALL.iter().map(|class| class.name()).collect();
    assert_eq!(
        names,
        ["source_images", "derived_images", "prompts", "evidence"]
    );
}

#[test]
fn a_set_of_classes_holds_each_class_or_not() {
    let none = Classes::none();
    let all = Classes::all();
    for class in DataClass::ALL {
        assert!(!none.holds(class) && all.holds(class));
        let one = Classes::of(&[class]);
        assert!(one.holds(class));
        assert_eq!(one.iter().collect::<Vec<_>>(), [class]);
        assert!(all.covers(one) && one.covers(none) && !none.covers(one));
        assert_eq!(none.with(class), one);
    }
    assert_eq!(Classes::of(&DataClass::ALL), all);
    assert_eq!(all.iter().collect::<Vec<_>>(), DataClass::ALL);
    let pictures = Classes::of(&[DataClass::DerivedImages, DataClass::SourceImages]);
    assert!(pictures.covers(Classes::of(&[DataClass::SourceImages])));
    assert!(!pictures.covers(Classes::of(&[DataClass::SourceImages, DataClass::Prompts])));
    assert!(none.is_empty() && !pictures.is_empty());
    assert_eq!(
        format!("{pictures:?}"),
        "Classes(source_images, derived_images)"
    );
    assert_eq!(format!("{none:?}"), "Classes()");
}

#[test]
fn a_part_with_no_stated_class_needs_every_class_approved() {
    let prompt = Piece::of(DataClass::Prompts, b"a red door".to_vec());
    let picture = Piece::of(DataClass::DerivedImages, vec![1, 2, 3]);
    let unstated = Piece::unclassed(vec![9]);
    assert_eq!(prompt.class(), Some(DataClass::Prompts));
    assert_eq!(unstated.class(), None);
    let to = address("https://api.example.com/v1/fill");
    let stated = Outgoing::new(
        to.clone(),
        vec![prompt.clone(), picture.clone(), prompt.clone()],
    )
    .unwrap();
    assert_eq!(
        stated.needs(),
        Classes::of(&[DataClass::Prompts, DataClass::DerivedImages])
    );
    let mixed = Outgoing::new(to.clone(), vec![prompt, unstated]).unwrap();
    assert_eq!(mixed.needs(), Classes::all());
    // Nothing at all needs nothing: an address alone is handed no class of data.
    assert_eq!(
        Outgoing::new(to, Vec::new()).unwrap().needs(),
        Classes::none()
    );
    // The same rule, for whoever holds the classes of a request's parts and no body.
    assert_eq!(Classes::needed_for([None]), Classes::all());
    assert_eq!(
        Classes::needed_for([Some(DataClass::Evidence), Some(DataClass::Evidence)]),
        Classes::of(&[DataClass::Evidence])
    );
    assert_eq!(Classes::needed_for([]), Classes::none());
}

// --- what is sent ---------------------------------------------------------------------------

#[test]
fn what_is_sent_is_an_address_bounded_pieces_and_a_credential_of_its_own_origin() {
    let to = address("https://api.example.com/v1/fill");
    let piece = || Piece::of(DataClass::Prompts, b"a red door".to_vec());
    let outgoing = Outgoing::new(to.clone(), vec![piece()]).unwrap();
    assert_eq!(outgoing.to(), &to);
    assert_eq!(outgoing.pieces().len(), 1);
    assert_eq!(outgoing.pieces()[0].bytes(), b"a red door");
    assert!(outgoing.credential().is_none());
    let most: Vec<Piece> = (0..PIECES_LIMIT).map(|_| piece()).collect();
    assert!(Outgoing::new(to.clone(), most).is_ok());
    let over: Vec<Piece> = (0..=PIECES_LIMIT).map(|_| piece()).collect();
    assert_eq!(
        Outgoing::new(to.clone(), over).map(|_| ()),
        Err(NotAllowed::TooMany)
    );
    // A credential goes only to the origin it is bound to.
    let own = Credential::new(
        origin("https://api.example.com"),
        "x-api-key",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    let carried = outgoing.clone().with_credential(own).unwrap();
    assert_eq!(
        carried.credential().map(Credential::header),
        Some("x-api-key")
    );
    for other in [
        "https://uploads.example.net",
        "http://api.example.com",
        "https://api.example.com:8443",
    ] {
        let elsewhere =
            Credential::new(origin(other), "x-api-key", MARKER, &Secrets::none()).unwrap();
        let refused = outgoing.clone().with_credential(elsewhere);
        assert_eq!(refused.map(|_| ()), Err(NotAllowed::Elsewhere), "{other}");
    }
    // What is printed is the address's origin, each piece's class and size, and
    // whether a credential goes with it: never a byte of a piece or of the credential.
    assert_eq!(
        format!("{carried:?}"),
        "Outgoing { to: Address(https://api.example.com, 8 more bytes), pieces: \
         [Piece(prompts, 10 bytes)], credential: Some(Credential(x-api-key for \
         https://api.example.com)) }"
    );
    assert_eq!(
        format!("{:?}", Piece::unclassed(vec![0; 3])),
        "Piece(no class, 3 bytes)"
    );
}

/// A sender that keeps what it was handed and answers with a redirect.
#[derive(Default)]
struct Kept(Mutex<Vec<Outgoing>>);

impl Sender for Kept {
    fn send(&self, outgoing: &Outgoing, bounds: &Bounds) -> Result<Reply, Fault> {
        assert!(!bounds.is_cancelled());
        self.0.lock().unwrap().push(outgoing.clone());
        Ok(Reply::moved(address("https://uploads.example.net/put/1")))
    }
}

/// A filler that hands its prompt to a sender, under the bounds its handle gave it.
struct Remote {
    declaration: Declaration,
    sender: Arc<dyn Sender>,
    replies: Mutex<Vec<Reply>>,
}

impl Filler for Remote {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn fill(&self, request: &FillRequest, bounds: &Bounds) -> Result<Painted, Fault> {
        let prompt = request.prompt().unwrap_or_default().as_bytes().to_vec();
        let to = address("https://api.example.com/v1/fill");
        let outgoing = Outgoing::new(to, vec![Piece::of(DataClass::Prompts, prompt)]).unwrap();
        let reply = self.sender.send(&outgoing, bounds)?;
        self.replies.lock().unwrap().push(reply);
        Err(Fault::new(FaultKind::Unavailable, "the test goes no further").unresolved())
    }
}

#[test]
fn a_backend_sends_through_the_interface_under_the_bounds_of_its_request() {
    let kept = Arc::new(Kept::default());
    let remote = Arc::new(Remote {
        declaration: Declaration::new(model(), &["prompt"]).unwrap(),
        sender: kept.clone(),
        replies: Mutex::new(Vec::new()),
    });
    let support = Support::from_plane(WIDTH, HEIGHT, vec![255; (WIDTH * HEIGHT) as usize]).unwrap();
    let request = FillRequest::new(picture(), support, Some("a red door")).unwrap();
    let handle = FillHandle::new(remote.clone(), Cancellation::new());
    assert_eq!(
        handle.fill(&request).unwrap_err().kind(),
        FaultKind::Unavailable
    );
    let handed = kept.0.lock().unwrap();
    assert_eq!(handed.len(), 1);
    assert_eq!(handed[0].pieces()[0].bytes(), b"a red door");
    assert_eq!(handed[0].needs(), Classes::of(&[DataClass::Prompts]));
    let replies = remote.replies.lock().unwrap();
    assert_eq!(
        replies[0].moved_to(),
        Some(&address("https://uploads.example.net/put/1"))
    );
}

#[test]
fn a_reply_is_a_status_and_a_bounded_body_or_another_address() {
    let reply = Reply::answered(200, b"{\"ok\":true}".to_vec()).unwrap();
    assert_eq!(
        (reply.status(), reply.body(), reply.moved_to()),
        (Some(200), &b"{\"ok\":true}"[..], None)
    );
    assert_eq!(format!("{reply:?}"), "Reply(status 200, 11 bytes)");
    let moved = Reply::moved(address("https://uploads.example.net/put/1?sig=abc"));
    assert_eq!((moved.status(), moved.body()), (None, &b""[..]));
    assert_eq!(
        format!("{moved:?}"),
        "Reply(moved to Address(https://uploads.example.net, 14 more bytes))"
    );
    assert!(Reply::answered(200, vec![0; REPLY_MAX_BYTES]).is_ok());
    assert_eq!(
        Reply::answered(200, vec![0; REPLY_MAX_BYTES + 1]).map(|_| ()),
        Err(NotAllowed::TooLong)
    );
}

// --- a download and a probe -----------------------------------------------------------------

#[test]
fn a_download_asks_for_an_address_and_a_most_and_a_credential_of_its_own_origin() {
    let from = address("https://weights.example.org/sam3/model.safetensors");
    let request = DownloadRequest::new(from.clone(), 1 << 30).unwrap();
    assert_eq!(request.from(), &from);
    assert_eq!(request.most_bytes(), 1 << 30);
    assert!(request.credential().is_none());
    assert_eq!(
        DownloadRequest::new(from.clone(), 0).map(|_| ()),
        Err(NotAllowed::Empty)
    );
    let own = Credential::new(
        origin("https://weights.example.org"),
        "authorization",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    assert!(request.clone().with_credential(own).is_ok());
    let other = Credential::new(
        origin("https://api.example.com"),
        "authorization",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    assert_eq!(
        request.clone().with_credential(other).map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
    // Its fields, all of them: there is none for a body, a picture or a prompt.
    assert_eq!(
        format!("{request:?}"),
        "DownloadRequest { from: Address(https://weights.example.org, 23 more bytes), \
         most_bytes: 1073741824, credential: None }"
    );
}

#[test]
fn a_probe_asks_for_an_address_and_a_credential_of_its_own_origin_and_nothing_else() {
    let to = address("https://api.example.com/health");
    let request = ProbeRequest::new(to.clone());
    assert_eq!(request.to(), &to);
    assert!(request.credential().is_none());
    let other = Credential::new(
        origin("https://uploads.example.net"),
        "x-api-key",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    assert_eq!(
        request.clone().with_credential(other).map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
    let own = Credential::new(
        origin("https://api.example.com"),
        "x-api-key",
        MARKER,
        &Secrets::none(),
    )
    .unwrap();
    let carried = request.with_credential(own).unwrap();
    assert_eq!(
        format!("{carried:?}"),
        "ProbeRequest { to: Address(https://api.example.com, 7 more bytes), credential: \
         Some(Credential(x-api-key for https://api.example.com)) }"
    );
}

/// A downloader and a prober of the simplest kind: the interfaces can be implemented
/// and held as shared values, and a caller stops a download by its sink.
struct Fixed(Vec<u8>);

impl Downloader for Fixed {
    fn download(
        &self,
        request: &DownloadRequest,
        cancellation: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, whizbang_core::capability::Fault> {
        let mut handed = 0;
        for chunk in self.0.chunks(2) {
            if cancellation.is_cancelled() || handed >= request.most_bytes() {
                break;
            }
            // A piece that was handed over is counted, whatever the sink then says.
            handed += chunk.len() as u64;
            if !sink(chunk) {
                break;
            }
        }
        Ok(Downloaded::Bytes(handed))
    }
}

impl Prober for Fixed {
    fn probe(
        &self,
        request: &ProbeRequest,
        _: &Cancellation,
    ) -> Result<Probed, whizbang_core::capability::Fault> {
        Ok(Probed::Moved(request.to().clone()))
    }
}

#[test]
fn a_download_hands_its_bytes_to_a_sink_that_can_stop_it_and_a_probe_says_what_it_reached() {
    let fixed: Arc<Fixed> = Arc::new(Fixed(vec![1, 2, 3, 4, 5, 6]));
    let downloader: Arc<dyn Downloader> = fixed.clone();
    let from = address("https://weights.example.org/m");
    let request = DownloadRequest::new(from.clone(), 100).unwrap();
    let mut taken = Vec::new();
    let done = downloader
        .download(&request, &Cancellation::new(), &mut |bytes| {
            taken.extend_from_slice(bytes);
            taken.len() < 4
        })
        .unwrap();
    // The sink said no to the second piece, having been handed it: the piece is among
    // what was handed over, the count says so, and nothing follows it.
    assert_eq!(taken, [1, 2, 3, 4]);
    assert_eq!(done, Downloaded::Bytes(4));
    assert_ne!(done, Downloaded::Moved(from.clone()));
    let prober: Arc<dyn Prober> = fixed;
    let probed = prober
        .probe(&ProbeRequest::new(from.clone()), &Cancellation::new())
        .unwrap();
    assert_eq!(probed, Probed::Moved(from));
    assert_ne!(probed, Probed::Reached { status: 200 });
}

// --- a backend's declaration ----------------------------------------------------------------

fn model() -> ModelIdentity {
    ModelIdentity::new("fake", "m", "1")
}

fn declared() -> Declaration {
    Declaration::new(model(), &[]).unwrap()
}

fn recipient() -> Recipient {
    Recipient::new(origin("https://api.example.com")).with_account(account("acct_4f8a1c9e77"))
}

fn bound_of(declaration: Declaration) -> Estimate {
    let roster = Roster::new()
        .offer(Capability::Fill, "one", declaration)
        .unwrap();
    Backends::bound(&roster, Capability::Fill, &model())
}

#[test]
fn a_declaration_with_no_recipient_reads_as_local() {
    let local = declared();
    assert!(local.is_local());
    assert_eq!(local.recipient(), None);
    let remote = declared().sending_to(recipient());
    assert!(!remote.is_local());
    assert_eq!(remote.recipient(), Some(&recipient()));
    assert_ne!(remote, local);
    // A price says nothing of where requests go: local with one, remote with one.
    assert!(declared().priced(Price::free()).is_local());
    assert!(!remote.clone().priced(Price::free()).is_local());
    let shown = format!("{remote:?}");
    assert!(
        shown.ends_with("recipient: Some(Recipient(https://api.example.com as acct...)) }"),
        "{shown}"
    );
}

#[test]
fn a_backend_that_names_a_recipient_and_states_no_price_has_no_bound() {
    assert!(bound_of(declared().sending_to(recipient())).is_unbounded());
    // In either order of saying, and whatever it is free of saying.
    let free = Price::free();
    assert_eq!(
        bound_of(declared().sending_to(recipient()).priced(free)),
        Estimate::of_micros(0)
    );
    assert_eq!(
        bound_of(declared().priced(free).sending_to(recipient())),
        Estimate::of_micros(0)
    );
    let bound = Price::at_most(700.0).unwrap();
    assert_eq!(
        bound_of(declared().sending_to(recipient()).priced(bound)),
        Estimate::of_micros(700)
    );
}

#[test]
fn a_backend_with_no_recipient_and_no_price_is_local_and_free() {
    assert_eq!(bound_of(declared()), Estimate::of_micros(0));
    // A local backend may still state a price, and is then held to it.
    let about = Price::about(5.0).unwrap();
    assert!(bound_of(declared().priced(about)).is_unbounded());
}

#[test]
fn a_remote_backend_with_no_price_beside_a_local_one_of_the_same_model_has_no_bound() {
    let roster = Roster::new()
        .offer(Capability::Fill, "local", declared())
        .unwrap()
        .offer(
            Capability::Fill,
            "remote",
            declared().sending_to(recipient()),
        )
        .unwrap();
    assert!(Backends::bound(&roster, Capability::Fill, &model()).is_unbounded());
    // And a remote one for another capability says nothing of this one.
    let roster = Roster::new()
        .offer(Capability::Fill, "local", declared())
        .unwrap()
        .offer(
            Capability::Judge,
            "remote",
            declared().sending_to(recipient()),
        )
        .unwrap();
    assert_eq!(
        Backends::bound(&roster, Capability::Fill, &model()),
        Estimate::of_micros(0)
    );
}

// --- from the review ------------------------------------------------------------------------

#[test]
fn a_host_that_is_a_number_is_four_plain_parts_or_is_refused() {
    // One machine has many spellings as a number, and a client that reads an address
    // may go to another machine than the text names. A host whose last label is a
    // number is taken only as four decimal parts of 0 to 255, each written plainly.
    for host in [
        "127.0.0.1",
        "10.0.3.17",
        "0.0.0.0",
        "255.255.255.255",
        "192.168.1.1",
    ] {
        assert_eq!(origin(&format!("http://{host}")).host(), host);
    }
    let refused = [
        "127.1",
        "127.0.1",
        "0x7f.1",
        "0177.0.0.1",
        "2130706433",
        "0x7f000001",
        "0X7F000001",
        "017700000001",
        "127.0.0.01",
        "00000127.0.0.1",
        "3232235777",
        "10.1",
        "1.2.3",
        "0",
        "0x",
        "256.0.0.1",
        "1.2.3.256",
        "1.2.3.4.5",
        "host.123",
        "example.0x1f",
        "a.b.c.7",
    ];
    for host in refused {
        let made = Origin::of(&format!("http://{host}:8080/x"), &Secrets::none());
        assert_eq!(made, Err(NotAllowed::Host), "{host}");
        let made = Address::of(&format!("https://{host}"), &Secrets::none());
        assert_eq!(made.map(|_| ()), Err(NotAllowed::Host), "{host}");
    }
    // A name whose last label only starts with a digit, or holds one, is a name.
    for host in [
        "host.1a",
        "3com.example",
        "a.0xg",
        "127.0.0.1.example",
        "x.7z",
    ] {
        assert_eq!(origin(&format!("http://{host}")).host(), host);
    }
}

#[test]
fn a_secret_that_lowering_a_host_makes_is_refused_and_not_shown() {
    // The secret is registered in lower case; the address has it in capitals, so the
    // text as given holds no secret, and the origin made of it would.
    let secret = "tokenabcdef0123";
    let secrets = Secrets::new([secret.to_owned()]).unwrap();
    let shouted = secret.to_ascii_uppercase();
    let texts = [
        format!("https://{shouted}.example.com"),
        format!("https://api.{shouted}.example.com/v1"),
        format!("HTTPS://{shouted}"),
    ];
    for text in &texts {
        assert_eq!(secrets.redact(text), *text, "the text as given holds none");
        for refused in [
            Origin::of(text, &secrets).map(|_| ()),
            Address::of(text, &secrets).map(|_| ()),
            Recipient::at(text, &secrets).map(|_| ()),
        ] {
            assert_eq!(refused, Err(NotAllowed::Secret), "{text}");
        }
    }
    // With no secret told, the same address is an origin, in lower case.
    let plain = Origin::of(&texts[0], &Secrets::none()).unwrap();
    assert_eq!(plain.host(), format!("{secret}.example.com"));
}

#[test]
fn a_secret_that_folding_an_address_makes_across_its_host_and_its_path_is_refused() {
    // The secret stands across the host and the path. The address as given does not
    // hold it: its host is in capitals, or a port stands between the two halves, which
    // is left out when it is the scheme's own. The address as it is written does, and
    // neither the origin alone nor the rest alone holds it.
    let secret = "zqxjkvwpyh/Path9";
    let secrets = Secrets::new([secret.to_owned()]).unwrap();
    for text in [
        "https://ZQXJKVWPYH/Path9",
        "https://zqxjkvwpyh:443/Path9",
        "http://zqxjkvwpyh:80/Path9?q=1",
        "HTTPS://Zqxjkvwpyh:443/Path9/more",
    ] {
        assert_eq!(secrets.redact(text), text, "the text as given holds none");
        assert_eq!(
            Address::of(text, &secrets).map(|_| ()),
            Err(NotAllowed::Secret),
            "{text}"
        );
    }
    // The origin of such an address holds no secret, and is one.
    Origin::of("https://ZQXJKVWPYH/Path9", &secrets).unwrap();
    // A port that is not the scheme's own stays between the halves, and a path of
    // other letters is another text: neither is the secret.
    for text in ["https://zqxjkvwpyh:8443/Path9", "https://ZQXJKVWPYH/path9"] {
        let address = Address::of(text, &secrets).unwrap();
        let written = format!("{}{}", address.origin(), address.rest());
        assert_eq!(secrets.redact(&written), written, "{text}");
    }
}

#[test]
fn a_credential_of_another_scheme_on_the_same_host_and_port_is_refused_by_each_request() {
    let to = address("https://h/x");
    assert_eq!(to.origin().port(), 443);
    let other = || {
        Credential::new(
            origin("http://h:443"),
            "x-api-key",
            MARKER,
            &Secrets::none(),
        )
        .unwrap()
    };
    assert_eq!(other().origin().port(), 443);
    let outgoing = Outgoing::new(to.clone(), Vec::new()).unwrap();
    assert_eq!(
        outgoing.with_credential(other()).map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
    let download = DownloadRequest::new(to.clone(), 1).unwrap();
    assert_eq!(
        download.with_credential(other()).map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
    let probe = ProbeRequest::new(to);
    assert_eq!(
        probe.with_credential(other()).map(|_| ()),
        Err(NotAllowed::Elsewhere)
    );
}

#[test]
fn a_query_follows_a_path_and_never_the_host() {
    for text in [
        "https://h?x",
        "https://h:80?x",
        "https://h?",
        "http://h.example?a=1",
    ] {
        let made = Address::of(text, &Secrets::none());
        assert!(made.is_err(), "{text}: {made:?}");
    }
    assert_eq!(address("https://h/?x").rest(), "/?x");
    assert_eq!(address("https://h:80/p?x").rest(), "/p?x");
}

#[test]
fn a_credential_is_told_the_secrets_and_its_header_holds_none() {
    let secrets = secrets();
    let at = || origin("https://api.example.com");
    // The value is the secret, where one belongs.
    let made = Credential::new(at(), "x-api-key", MARKER, &secrets).unwrap();
    assert_eq!(made.expose(), MARKER);
    // A key written where the header's name belongs is refused, and not shown.
    for header in [MARKER.to_owned(), format!("x-{MARKER}")] {
        let refused = Credential::new(at(), &header, "value", &secrets);
        assert_eq!(
            refused.clone().map(|_| ()),
            Err(NotAllowed::Secret),
            "{header}"
        );
        assert!(!format!("{refused:?}").contains("marker"));
    }
    // And one that is no name besides is refused for the secret first.
    let refused = Credential::new(at(), &format!("x {MARKER}"), "value", &secrets);
    assert_eq!(refused.map(|_| ()), Err(NotAllowed::Secret));
}

#[test]
fn a_recipient_is_the_same_recipient_in_whatever_order_its_upstreams_were_given() {
    let base = || Recipient::new(origin("https://api.example.com"));
    let (one, two) = (
        origin("https://one.example.net"),
        origin("https://two.example.net"),
    );
    let forward = base()
        .forwarding_to(vec![one.clone(), two.clone()])
        .unwrap();
    let backward = base()
        .forwarding_to(vec![two.clone(), one.clone()])
        .unwrap();
    assert_eq!(forward, backward);
    // They are kept in one order, whichever way they came.
    assert_eq!(forward.upstreams(), backward.upstreams());
    assert_eq!(forward.upstreams(), &[one, two][..]);
}

#[test]
fn known_the_rest_of_an_address_is_not_normalised() {
    // Two spellings of one path are two addresses of one origin. Only origins are
    // compared when a hop is checked, so nothing is decided by the difference.
    let dotted = address("https://api.example.com/a/../b");
    let plain = address("https://api.example.com/b");
    assert_ne!(dotted, plain);
    assert_eq!(dotted.origin(), plain.origin());
    assert_eq!(dotted.rest(), "/a/../b");
}
