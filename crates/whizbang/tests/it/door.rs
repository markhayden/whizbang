//! `door::…`: the call every door makes of a use case's recipe (plan m4, task A1, item 3).

use whizbang::door::{Details, Measuring, check_call};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::Value;
use whizbang_core::identity::Digest;
use whizbang_squint::job::Job;

/// The overrides a call states, one a line, in order.
fn set_of(measuring: &Measuring<'_>, use_case: &str) -> Vec<String> {
    let job = Job::shorthand(use_case, false, 0, None);
    let images = [ArtifactId(Digest::of_bytes(b"candidate"))];
    let call = check_call(&job, &images, None, measuring);
    assert_eq!(call.action(), use_case);
    match call.arguments().get(whizbang_core::recipe::SET) {
        Some(Value::Text(set)) => set.lines().map(str::to_owned).collect(),
        other => panic!("every call states its settings: {other:?}"),
    }
}

#[test]
fn the_call_of_a_check_states_all_four_settings_whatever_the_recipe_says() {
    use whizbang_core::recipe::{Recipe, Written};
    // The control arm, which is what every shipped recipe says itself, and arm F, which is
    // what each said until plan m4's task A1: both state all four settings, so neither
    // call leans on the recipe's file, and `--details first` cannot fall silent because
    // it happens to equal what a recipe says.
    let control = Measuring::stated(None, None).unwrap();
    let first = Measuring::stated(Some("first"), None).unwrap();
    for (name, file) in whizbang_squint::recipes::SHIPPED {
        assert_eq!(
            set_of(&control, name),
            [
                "details.details=none",
                "judge.tools=view,align",
                "judge.projection=visual_only",
                "judge.second_look=off"
            ],
            "{name}"
        );
        assert_eq!(
            set_of(&first, name),
            [
                "details.details=first",
                "judge.tools=view,align,resize",
                "judge.projection=full",
                "judge.second_look=off"
            ],
            "{name}"
        );
        // The control arm's call states each setting although the recipe's own step says
        // the same: this is the case the door used to leave out.
        let recipe = Recipe::read(file).unwrap();
        let text = |value: &str| Written::Text(value.to_owned());
        let (details, judge) = (&recipe.steps[0], &recipe.steps[1]);
        assert_eq!(details.params["details"], text("none"), "{name}");
        assert_eq!(judge.params["tools"], text("view,align"), "{name}");
        assert_eq!(judge.params["projection"], text("visual_only"), "{name}");
        assert_eq!(judge.params["second_look"], text("off"), "{name}");
    }
    // Every other combination a door can ask is stated as asked: a projection apart from
    // its arm, a second look, the tools measured first, and a tool file offered beside
    // the arm's own.
    let tools = ["mine".to_owned()];
    let asked = Measuring {
        details: Details::None,
        detail: Some("align,change_map"),
        projection: "full",
        tools: &tools,
        second_look: "strict",
    };
    assert_eq!(
        set_of(&asked, "source-fidelity"),
        [
            "details.details=none",
            "details.detail=align,change_map",
            "judge.tools=view,align,mine",
            "judge.projection=full",
            "judge.second_look=strict"
        ]
    );
    // The names a door takes, the default first.
    assert_eq!(Details::NAMES, ["none", "first"]);
    assert_eq!(Details::DEFAULT, Details::None);
    assert_eq!(
        (Details::None.projection(), Details::First.projection()),
        ("visual_only", "full")
    );
}
