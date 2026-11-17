//! The network (`SPEC.md` section 10, "Egress"; plan 1C, the network and models
//! strand): the download client behind the checked downloader. Its tests against a
//! server on loopback are inside the crate (`src/net/tests/`), where a root of a
//! test's own can be handed to it; these hold what is public.

mod public;
