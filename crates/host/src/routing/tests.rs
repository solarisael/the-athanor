use super::spellbook::Spellbook;
use super::*;
use serde_json::{Value, json};

const ROOM: &str = "kintsu";

fn scout() -> Value {
    json!({"id":"scout","name":"Pip","aliases":["pipkin"],"lane":"smol-scout","description":"Finds things.","temperament":"curious"})
}

fn house_page() -> Value {
    json!({"version":1,"collective":"familiars","collectiveAliases":["kittens"],"guide":"Organs.","familiars":[scout()]})
}

fn familiar(id: &str, name: &str, aliases: &[&str]) -> Value {
    json!({"id":id,"name":name,"aliases":aliases,"lane":"smol-scout","description":"Works."})
}

fn room_page(familiars: Vec<Value>) -> Value {
    json!({"familiars": familiars})
}

fn pages(pages: &[(&str, Value)]) -> Vec<StoredPage> {
    pages
        .iter()
        .map(|(scope, page)| ((*scope).to_owned(), page.clone()))
        .collect()
}

fn load(stored: &[(&str, Value)]) -> LoadedSpellbook {
    effective_spellbook(ROOM, &pages(stored))
}

fn effective(loaded: LoadedSpellbook) -> Spellbook {
    assert_eq!(loaded.status.errors, Vec::<String>::new());
    loaded.status.spellbook.expect("effective spellbook")
}

fn layers(loaded: &LoadedSpellbook) -> Vec<(&str, &str)> {
    loaded
        .sources
        .iter()
        .map(|source| (source.layer, source.scope.as_str()))
        .collect()
}

fn summon(familiar: &str, stored: &[(&str, Value)]) -> HouseDispatchReceipt {
    let request = DispatchRequest {
        familiar: familiar.into(),
        ..Default::default()
    };
    house_dispatch(request, || load(stored))
}

/// The familiar a selector resolves to. The bare request lacks a task, so the
/// packet itself is refused; resolution is what these tests judge.
fn summoned(familiar: &str, stored: &[(&str, Value)]) -> Familiar {
    let receipt = summon(familiar, stored);
    receipt
        .familiar
        .unwrap_or_else(|| panic!("no familiar matched: {:?}", receipt.dispatch.errors))
}

#[test]
fn a_house_page_alone_is_the_effective_book() {
    let loaded = load(&[("house", house_page())]);
    assert_eq!(layers(&loaded), [("house", "house")]);
    let book = effective(loaded);
    assert_eq!(book.guide, "Organs.");
    assert_eq!(book.familiars[0].id, "scout");
    assert_eq!(book.familiars[0].origin, "house");
}

#[test]
fn the_seeded_starter_house_page_is_a_valid_house_page() {
    let page: Value = serde_json::from_str(akasha::migrations::STARTER_HOUSE_SPELLBOOK).unwrap();
    serde_json::from_value::<super::spellbook::SpellbookPage>(page.clone()).unwrap();
    let loaded = load(&[("house", page)]);
    assert!(loaded.status.ok, "{:?}", loaded.status.errors);
}

#[test]
fn a_room_page_alone_needs_no_house() {
    let room = json!({"version":1,"collective":"kittens","familiars":[scout()]});
    let loaded = load(&[(ROOM, room)]);
    assert_eq!(layers(&loaded), [("room", ROOM)]);
    assert_eq!(effective(loaded).collective, "kittens");
}

#[test]
fn a_room_entry_replaces_the_house_entry_whole() {
    // A deep merge would keep the House temperament and the `pipkin` alias.
    let room = room_page(vec![familiar("scout", "Moss", &[])]);
    let loaded = load(&[("house", house_page()), (ROOM, room)]);
    assert_eq!(layers(&loaded), [("room", ROOM), ("house", "house")]);
    let book = effective(loaded);
    let scout = &book.familiars[0];
    assert_eq!(book.familiars.len(), 1);
    assert_eq!(
        (scout.name.as_str(), scout.temperament.as_deref()),
        ("Moss", None)
    );
    assert!(scout.aliases.is_empty());
    assert_eq!(scout.origin, ROOM);
    assert_eq!(book.collective, "familiars");
}

#[test]
fn explicit_empty_values_override_instead_of_inheriting() {
    let room = json!({"guide":"","collectiveAliases":[],"familiars":[]});
    let book = effective(load(&[("house", house_page()), (ROOM, room)]));
    assert!(book.guide.is_empty());
    assert!(book.collective_aliases.is_empty());
    assert!(book.familiars.is_empty());
}

#[test]
fn a_null_room_guide_refuses_instead_of_inheriting() {
    let loaded = load(&[("house", house_page()), (ROOM, json!({"guide":null}))]);
    assert!(!loaded.status.ok);
    assert!(loaded.status.spellbook.is_none());
    assert!(loaded.status.errors[0].starts_with("Spellbook page 'kintsu'"));
}

#[test]
fn a_cleared_chapter_leaves_house_familiars_unknown() {
    let receipt = summon(
        "pipkin",
        &[("house", house_page()), (ROOM, json!({"familiars":[]}))],
    );
    assert!(receipt.dispatch.errors[0].contains("Unknown familiar: pipkin"));
    assert!(receipt.dispatch.errors[0].contains("Valid familiar IDs: <none>"));
    assert!(receipt.familiar.is_none());
}

#[test]
fn aliases_resolve_against_the_effective_book() {
    let room = room_page(vec![familiar("keeper", "Wick", &["lantern"])]);
    for (alias, id) in [("pipkin", "scout"), ("lantern", "keeper")] {
        let found = summoned(alias, &[("house", house_page()), (ROOM, room.clone())]);
        assert_eq!(found.id, id);
    }
}

#[test]
fn a_room_alias_cannot_take_a_house_familiars_key() {
    let room = room_page(vec![familiar("keeper", "Wick", &["pipkin"])]);
    let loaded = load(&[("house", house_page()), (ROOM, room)]);
    assert_eq!(
        loaded.status.errors,
        ["Familiar lookup key 'pipkin' is already owned by 'scout'."]
    );
}

#[test]
fn duplicate_room_ids_cannot_hide_behind_house_replacement() {
    let room = room_page(vec![scout(), scout()]);
    let loaded = load(&[("house", house_page()), (ROOM, room)]);
    assert!(loaded.status.spellbook.is_none());
    assert_eq!(
        loaded.status.errors,
        ["Spellbook page 'kintsu' repeats id 'scout'."]
    );
}

#[test]
fn a_malformed_house_page_refuses_with_its_own_scope() {
    let room = json!({"version":1,"collective":"kittens","familiars":[scout()]});
    let loaded = load(&[("house", json!(["not", "a", "page"])), (ROOM, room)]);
    assert!(loaded.status.spellbook.is_none());
    assert!(
        loaded.status.errors[0].starts_with("Spellbook page 'house' is not a valid page"),
        "{:?}",
        loaded.status.errors
    );
}

#[test]
fn an_undeclared_category_refuses_instead_of_riding_along() {
    let loaded = load(&[(ROOM, json!({"version":1,"wards":[]}))]);
    assert!(loaded.status.spellbook.is_none());
    assert!(loaded.status.errors[0].contains("unknown field `wards`"));
}

#[test]
fn a_page_that_names_an_origin_is_refused() {
    let mut forged = familiar("keeper", "Wick", &[]);
    forged["origin"] = json!("house");
    let loaded = load(&[("house", house_page()), (ROOM, room_page(vec![forged]))]);
    assert!(loaded.status.spellbook.is_none());
    assert!(
        loaded.status.errors[0].contains("unknown field `origin`"),
        "{:?}",
        loaded.status.errors
    );
}

#[test]
fn no_page_for_the_room_or_the_house_names_both() {
    let loaded = load(&[("kodo", house_page())]);
    assert!(loaded.sources.is_empty());
    assert!(loaded.status.errors[0].contains("('kintsu') or the house"));
}

#[test]
fn an_unreadable_book_says_spellbook_unavailable() {
    let loaded = load_spellbook(ROOM, &Err("connection refused".into()));
    assert_eq!(
        loaded.status.errors,
        ["Spellbook unavailable: connection refused"]
    );
}

#[test]
fn another_rooms_familiar_is_visible_from_this_room() {
    let kodo = room_page(vec![familiar("lavra", "Lavra", &["plough"])]);
    let stored = [("house", house_page()), ("kodo", kodo)];
    let loaded = load(&stored);
    assert_eq!(layers(&loaded), [("house", "house"), ("other", "kodo")]);
    let found = summoned("plough", &stored);
    assert_eq!(
        (found.id.as_str(), found.origin.as_str()),
        ("lavra", "kodo")
    );
}

#[test]
fn the_room_wins_over_the_house_and_every_other_room() {
    let room = room_page(vec![familiar("scout", "Moss", &[])]);
    let kodo = room_page(vec![familiar("scout", "Ash", &[])]);
    let found = summoned(
        "scout",
        &[("house", house_page()), (ROOM, room), ("kodo", kodo)],
    );
    assert_eq!((found.name.as_str(), found.origin.as_str()), ("Moss", ROOM));
}

#[test]
fn the_house_wins_over_other_rooms_by_id_and_by_key() {
    // kodo redefines the House id and claims the House alias for its own familiar.
    let kodo = room_page(vec![
        familiar("scout", "Ash", &[]),
        familiar("lavra", "Lavra", &["pipkin"]),
    ]);
    let stored = [("house", house_page()), ("kodo", kodo)];
    let loaded = load(&stored);
    let book = effective(loaded);
    assert_eq!(book.familiars.len(), 2);
    let found = summoned("scout", &stored);
    assert_eq!(
        (found.name.as_str(), found.origin.as_str()),
        ("Pip", "house")
    );
    assert_eq!(summoned("pipkin", &stored).id, "scout");
    assert_eq!(summoned("lavra", &stored).origin, "kodo");
}

#[test]
fn an_id_two_other_rooms_define_is_contested_and_named() {
    let stored = [
        ("house", house_page()),
        ("kodo", room_page(vec![familiar("lavra", "Lavra", &[])])),
        ("tuner", room_page(vec![familiar("lavra", "Plough", &[])])),
    ];
    let loaded = load(&stored);
    let book = effective(loaded.clone());
    assert!(book.familiars.iter().all(|familiar| familiar.id != "lavra"));
    assert_eq!(loaded.status.contested[0].key, "lavra");

    let receipt = summon("lavra", &stored);
    assert!(receipt.familiar.is_none());
    assert_eq!(
        receipt.dispatch.errors,
        [
            "Familiar key 'lavra' is contested: lavra (kodo), lavra (tuner). Define it in this room or in the house to choose."
        ]
    );
}

#[test]
fn defining_a_contested_id_in_this_room_chooses_it() {
    let stored = [
        ("house", house_page()),
        (ROOM, room_page(vec![familiar("lavra", "Mine", &[])])),
        ("kodo", room_page(vec![familiar("lavra", "Lavra", &[])])),
        ("tuner", room_page(vec![familiar("lavra", "Plough", &[])])),
    ];
    assert!(load(&stored).status.contested.is_empty());
    assert_eq!(summoned("lavra", &stored).name, "Mine");
}

#[test]
fn a_contested_alias_is_refused_while_the_other_keys_work() {
    let stored = [
        ("house", house_page()),
        (
            "kodo",
            room_page(vec![familiar("lavra", "Lavra", &["smith"])]),
        ),
        (
            "tuner",
            room_page(vec![familiar("forja", "Forja", &["smith"])]),
        ),
    ];
    let receipt = summon("Smith", &stored);
    assert!(receipt.familiar.is_none());
    assert_eq!(
        receipt.dispatch.errors,
        [
            "Familiar key 'smith' is contested: lavra (kodo), forja (tuner). Define it in this room or in the house to choose."
        ]
    );
    assert_eq!(summoned("lavra", &stored).origin, "kodo");
    assert_eq!(summoned("Forja", &stored).origin, "tuner");
}

#[test]
fn an_invalid_foreign_familiar_is_left_out_without_refusing_the_room() {
    let broken = json!({"id":"lavra","name":"Lavra","lane":"no-such-lane","description":"Works."});
    let stored = [
        ("house", house_page()),
        (
            "kodo",
            room_page(vec![broken, familiar("forja", "Forja", &[])]),
        ),
        ("tuner", json!({"familiars":[{"id":"x","wards":true}]})),
    ];
    let loaded = load(&stored);
    assert!(loaded.status.ok);
    let ids = loaded
        .status
        .spellbook
        .as_ref()
        .expect("the room book stands")
        .familiars
        .iter()
        .map(|familiar| familiar.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["scout", "forja"]);
    let warnings = loaded.status.warnings.join("\n");
    assert!(warnings.contains("Spellbook page 'kodo' familiar excluded: Familiar 'lavra' uses unknown worker lane 'no-such-lane'."));
    assert!(warnings.contains("Spellbook page 'tuner' is not a valid page"));
}

#[test]
fn the_dispatch_identity_comes_from_the_familiars_origin_page() {
    let room = json!({"collective":"wardens","spellbookAliases":["kintsu-book"]});
    let kodo = json!({"collective":"dragonlings","familiars":[familiar("lavra", "Lavra", &[])]});
    let stored = [("house", house_page()), (ROOM, room), ("kodo", kodo)];

    let identity = summon("lavra", &stored).spellbook.expect("identity");
    assert_eq!(identity.collective, "dragonlings");
    assert_eq!(identity.collective_aliases, ["kittens"]);
    assert!(identity.spellbook_aliases.is_empty());

    let identity = summon("scout", &stored).spellbook.expect("identity");
    assert_eq!(identity.collective, "familiars");
}

#[test]
fn familiar_status_shows_the_raw_house_and_room_pages() {
    let room = room_page(vec![familiar("keeper", "Wick", &[])]);
    let stored = pages(&[("house", house_page()), (ROOM, room.clone())]);
    let receipt = familiar_status(ROOM, &Ok(stored));
    assert_eq!(receipt.pages.house, Some(house_page()));
    assert_eq!(receipt.pages.room, Some(room));
    let receipt = familiar_status("kodo", &Ok(pages(&[("house", house_page())])));
    assert_eq!(receipt.pages.room, None);
}

#[test]
fn a_room_may_write_only_its_own_page_or_the_house_page() {
    let stored = pages(&[("house", house_page())]);
    let page = room_page(vec![familiar("keeper", "Wick", &[])]);
    assert!(spellbook_write_errors(ROOM, ROOM, &page, &stored).is_empty());
    assert!(spellbook_write_errors(ROOM, "house", &house_page(), &stored).is_empty());
    assert_eq!(
        spellbook_write_errors(ROOM, "kodo", &page, &stored),
        ["Spellbook write refused: scope 'kodo' is neither this room ('kintsu') nor 'house'."]
    );
}

#[test]
fn a_house_write_that_breaks_a_room_is_refused_and_names_it() {
    // kodo's page has no version of its own; it leans on the House for it.
    let stored = pages(&[
        ("house", house_page()),
        ("kodo", room_page(vec![familiar("lavra", "Lavra", &[])])),
    ]);
    let versionless = json!({"collective":"familiars","familiars":[scout()]});
    let errors = spellbook_write_errors(ROOM, "house", &versionless, &stored);
    assert!(errors.contains(&"Familiar spellbook version must be 1.".to_owned()));
    assert!(
        errors
            .iter()
            .any(|error| error.starts_with("Room 'kodo' would break:")),
        "{errors:?}"
    );
}

#[test]
fn repeated_dispatches_compose_with_valid_distinct_names_and_preserved_routes() {
    let room = json!({"version":1,"collective":"familiars","familiars":[{"id":"long-familiar-identity-for-bounded-task-names","name":"Moss","lane":"smol-scout","ompAgent":"scout-kitten","modelRole":"@scout","description":"Finds things."}]});
    let request = DispatchRequest {
        familiar: "long-familiar-identity-for-bounded-task-names".into(),
        task: "Read the exact target.".into(),
        lesson_bodies: vec!["Read exact sources before acting.".into()],
        ..Default::default()
    };
    let mut tasks = Vec::new();

    for _ in 0..2 {
        let receipt = house_dispatch(request.clone(), || load(&[(ROOM, room.clone())]));
        assert!(receipt.dispatch.ok, "{:?}", receipt.dispatch.errors);
        assert_eq!(receipt.dispatch.omp_agent.as_deref(), Some("scout-kitten"));
        assert_eq!(receipt.dispatch.model_role.as_deref(), Some("@scout"));
        tasks.extend(
            receipt
                .dispatch
                .spawn_packet
                .expect("accepted packet")
                .args
                .tasks,
        );
    }

    assert_eq!(tasks.len(), 2);
    assert_ne!(tasks[0].name, tasks[1].name);

    for task in tasks {
        assert!(task.name.len() <= 32);
        assert!(task.name.starts_with("Long"));
        assert!(task.name.as_bytes()[0].is_ascii_uppercase());
        assert!(task.name.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_eq!(task.agent.as_deref(), Some("scout-kitten"));
    }
}

#[test]
fn unknown_familiar_lists_effective_ids_without_accepting_agent_selectors() {
    let room = json!({"familiars":[{"id":"scout","name":"Moss","lane":"smol-scout","ompAgent":"scout-kitten","modelRole":"@scout","description":"Room scout."},familiar("keeper", "Wick", &["lantern"])]});

    for selector in ["missing", "pipkin", "scout-kitten"] {
        let receipt = summon(selector, &[("house", house_page()), (ROOM, room.clone())]);
        assert!(!receipt.dispatch.ok);
        assert!(receipt.dispatch.spawn_packet.is_none());
        assert!(receipt.familiar.is_none());
        let refusal = &receipt.dispatch.errors[0];

        assert!(refusal.contains(&format!("Unknown familiar: {selector}")));
        assert!(refusal.contains("Valid familiar IDs: scout, keeper."));
        assert!(!refusal.contains("Valid familiar IDs: scout-kitten"));
    }
}
