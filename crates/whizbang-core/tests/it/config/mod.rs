//! The config (`SPEC.md` section 4; plan 1B, tasks C1 to C5). So far this holds one
//! thing: the TOML reader the workspace takes (task B0b), shown to do the three things
//! a strict reader of a config rests on. The config's own type and its tests are C1's.

/// The example of `SPEC.md` section 4, as the spec has it: its one fenced block of TOML.
fn the_specs_example() -> &'static str {
    let spec = include_str!("../../../../../SPEC.md");
    let open = "```toml\n";
    let start = spec.find(open).expect("the spec has a block of TOML") + open.len();
    let rest = &spec[start..];
    assert!(!rest.contains(open), "the spec has a second block of TOML");
    &rest[..rest.find("```").expect("the block is closed")]
}

#[test]
fn the_toml_reader_reads_the_specs_example_as_tables_of_strings() {
    let config: toml::Table = the_specs_example().parse().expect("the example is TOML");
    assert_eq!(config.keys().collect::<Vec<_>>(), ["backends"]);
    let backends = config["backends"].as_table().expect("a table");
    assert_eq!(backends.keys().collect::<Vec<_>>(), ["fill", "segment"]);
    assert_eq!(backends["segment"]["kind"].as_str(), Some("remote"));
    assert_eq!(
        backends["segment"]["token_env"].as_str(),
        Some("SEGMENT_TOKEN")
    );
    assert_eq!(backends["fill"]["provider"].as_str(), Some("fal"));
    assert_eq!(backends["fill"]["key_env"].as_str(), Some("FAL_KEY"));
}

#[test]
fn the_toml_reader_refuses_a_key_given_twice_and_a_table_given_twice() {
    for twice in [
        "a = 1\na = 2\n",
        "[backends.fill]\nkind = \"service\"\nkind = \"remote\"\n",
        "[backends.fill]\nkind = \"service\"\n[backends.fill]\nmodel = \"m\"\n",
        "backends.fill.kind = \"service\"\n[backends.fill]\nkind = \"remote\"\n",
    ] {
        assert!(twice.parse::<toml::Table>().is_err(), "{twice}");
    }
}

#[test]
fn the_toml_reader_tells_the_kinds_of_value_apart() {
    let read: toml::Table = "text = \"1\"\nwhole = 1\nreal = 1.0\nflag = true\nlist = [1]\n"
        .parse()
        .expect("TOML");
    assert!(read["text"].is_str() && !read["text"].is_integer());
    assert!(read["whole"].is_integer() && !read["whole"].is_float());
    assert!(read["real"].is_float() && !read["real"].is_integer());
    assert!(read["flag"].is_bool());
    assert!(read["list"].is_array());
}

#[test]
fn the_toml_reader_refuses_nesting_past_a_depth() {
    // A list inside a list, 1,000 deep: refused, and not read by going 1,000 calls deep.
    let deep = format!("a = {}{}\n", "[".repeat(1000), "]".repeat(1000));
    assert!(deep.parse::<toml::Table>().is_err());
    let shallow = format!("a = {}{}\n", "[".repeat(8), "]".repeat(8));
    assert!(shallow.parse::<toml::Table>().is_ok());
}
