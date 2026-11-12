//! The rule for the text a description prints.
//!
//! The reference (`docs/reference/`) prints an action's name and purpose as they are, so
//! neither may be text that Markdown or HTML reads as markup. `scripts/docs.sh` refuses
//! such a description when the reference is generated; these functions are the same rule,
//! held where a descriptor is made, so that no action that breaks it can be registered.
//! The cases of `scripts/tests/test-docs.sh` are run against both.

/// The longest name, in characters.
pub const NAME_LIMIT: usize = 64;

/// The longest purpose, in characters.
pub const PURPOSE_LIMIT: usize = 200;

/// The marks a purpose may hold beside ASCII letters, digits and spaces.
const PURPOSE_MARKS: &str = ".,;:'\"()/%+-";

/// An action's name: a lower-case letter, then lower-case letters, digits, `_` or `-`;
/// 64 characters at most. A choice an argument offers is held to the same rule.
pub fn action_name(name: &str) -> Result<(), String> {
    slug(name, true)
}

/// The name of an argument or a port: an action's name without `-`, so that it is a name
/// in every language a caller is written in.
pub fn field_name(name: &str) -> Result<(), String> {
    slug(name, false)
}

fn slug(name: &str, dash: bool) -> Result<(), String> {
    let allowed = if dash {
        "a lower-case letter, then lower-case letters, digits, _ or -"
    } else {
        "a lower-case letter, then lower-case letters, digits or _"
    };
    let first = name.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    let rest = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || (dash && c == '-'));
    if !first || !rest {
        return Err(format!("the name {name:?} must be {allowed}"));
    }
    if name.len() > NAME_LIMIT {
        return Err(format!(
            "the name {name:?} is longer than {NAME_LIMIT} characters"
        ));
    }
    Ok(())
}

/// A purpose: one line of 1 to 200 characters that starts with a letter, does not end
/// with a space, holds only ASCII letters, digits, spaces and `. , ; : ' " ( ) / % + -`,
/// and holds neither `://` nor `www.` in any case.
pub fn purpose(purpose: &str) -> Result<(), String> {
    plain_line("purpose", purpose)
}

/// The purpose rule, for any text a description prints as it is: `what` says which text
/// it is, in the refusal. The refusal shows the start of a long text, not the whole.
pub fn plain_line(what: &str, line: &str) -> Result<(), String> {
    let refuse = |why: &str| {
        let start: String = line.chars().take(60).collect();
        let cut = if start.len() < line.len() { "..." } else { "" };
        Err(format!("the {what} {start:?}{cut} {why}"))
    };
    if !line.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return refuse("must start with a letter");
    }
    if line.ends_with(' ') {
        return refuse("must not end with a space");
    }
    if let Some(found) = line
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == ' ' || PURPOSE_MARKS.contains(*c)))
    {
        return refuse(&format!(
            "holds {found:?}: only letters, digits, spaces and {PURPOSE_MARKS} are printed as they are"
        ));
    }
    // Every character is ASCII from here on, so bytes are characters.
    if line.len() > PURPOSE_LIMIT {
        return refuse(&format!("is longer than {PURPOSE_LIMIT} characters"));
    }
    if line.contains("://") || line.to_ascii_lowercase().contains("www.") {
        return refuse("holds what some renderers turn into a link");
    }
    Ok(())
}

/// An action's version: three numbers with dots between them, each written the one way a
/// number is written: no sign, no leading zero, nine digits at most.
pub fn version(version: &str) -> Result<(), String> {
    let parts: Vec<&str> = version.split('.').collect();
    let numbers = parts.iter().all(|part| {
        !part.is_empty()
            && part.len() <= 9
            && part.bytes().all(|b| b.is_ascii_digit())
            && (*part == "0" || !part.starts_with('0'))
    });
    if parts.len() != 3 || !numbers {
        return Err(format!(
            "the version {version:?} must be three numbers with dots between them"
        ));
    }
    Ok(())
}
