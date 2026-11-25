"""The subjects a job names, for the tool files that compare them (plan mE, task T1, item 3):
`pack/tools/outline.py` reads them here, and so will `regions`. The rule is per-subject
comparison's (`pack/tools/per_subject.py`, `subjects_of`), moved here whole so that it is
shared and not copied: each requirement of kind `must_hold` whose text is "The ... keeps
...", the phrase before " keeps ", its leading article dropped ("The rectangular swimming
pool keeps its identity, ..." names "rectangular swimming pool"); a requirement not of that
form is not applicable, by its id; a label is never read. `per_subject.py` keeps its own
copy until its next measured run, because its record's rung 2 names the digest of the file
as it was measured; `test_outline.Subjects` holds the two to one rule.

A tool file reaches it as `from whizbang_pack.subjects import subjects_of`: the tool worker
runs from the pack this module is part of. The standard library only."""

KEEPS = " keeps "
ARTICLES = ("the ", "a ", "an ")


def subjects_of(job):
    """The subjects a job names, each a phrase, and the ids of its requirements that name
    none: only requirements of kind must_hold are read, and only their text."""
    named, not_applicable = [], []
    for requirement in job.get("requirements", []):
        if requirement.get("kind") != "must_hold":
            continue
        text = str(requirement.get("text", ""))
        before, found, _ = text.partition(KEEPS)
        phrase = before.strip()
        for article in ARTICLES:
            if phrase.lower().startswith(article):
                phrase = phrase[len(article) :].strip()
                break
        if found and phrase:
            named.append(phrase)
        else:
            not_applicable.append(str(requirement.get("id", "a requirement")))
    return named, not_applicable
