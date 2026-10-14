use super::spellbook::Spellbook;
use super::*;
use std::collections::HashMap;

const ROOM_DIR: &str = "C:/rooms/kintsu";
const HOUSE: &str = "C:/rooms/house/familiars/spellbook.json";
const ROOM: &str = "C:/rooms/kintsu/familiars/spellbook.json";
const SCOUT: &str = r#"{"id":"scout","name":"Pip","aliases":["pipkin"],"lane":"smol-scout","description":"Finds things.","temperament":"curious"}"#;

fn house_page() -> String {
    format!(
        r#"{{"version":1,"collective":"familiars","collectiveAliases":["kittens"],"guide":"Organs.","familiars":[{SCOUT}]}}"#
    )
}

/// Reads the way the Host does: parse each present file as one page.
fn load(files: &[(&str, &str)]) -> LoadedSpellbook {
    let files = files.iter().copied().collect::<HashMap<_, _>>();
    load_spellbook(ROOM_DIR, |path| match files.get(path) {
        None => SpellbookRead::Missing,
        Some(text) => match serde_json::from_str(text) {
            Ok(page) => SpellbookRead::Parsed(page),
            Err(error) => SpellbookRead::Malformed(error.to_string()),
        },
    })
}

fn effective(loaded: LoadedSpellbook) -> Spellbook {
    assert_eq!(loaded.status.errors, Vec::<String>::new());
    loaded.status.spellbook.expect("effective spellbook")
}

fn layers(loaded: &LoadedSpellbook) -> Vec<(&str, &str)> {
    loaded
        .sources
        .iter()
        .map(|source| (source.layer, source.path.as_str()))
        .collect()
}

fn summon(familiar: &str, files: &[(&str, &str)]) -> HouseDispatchReceipt {
    let request = DispatchRequest {
        familiar: familiar.into(),
        ..Default::default()
    };
    house_dispatch(request, || load(files))
}

#[test]
fn a_house_page_alone_is_the_effective_book() {
    let house = house_page();
    let loaded = load(&[(HOUSE, &house)]);
    assert_eq!(layers(&loaded), [("house", HOUSE)]);
    let book = effective(loaded);
    assert_eq!(book.guide, "Organs.");
    assert_eq!(book.familiars[0].id, "scout");
}

#[test]
fn a_room_page_alone_needs_no_house() {
    let room = format!(r#"{{"version":1,"collective":"kittens","familiars":[{SCOUT}]}}"#);
    let loaded = load(&[(ROOM, &room)]);
    assert_eq!(layers(&loaded), [("room", ROOM)]);
    assert_eq!(effective(loaded).collective, "kittens");
}

#[test]
fn a_room_entry_replaces_the_house_entry_whole() {
    // A deep merge would keep the House temperament and the `pipkin` alias.
    let house = house_page();
    let room = r#"{"familiars":[{"id":"scout","name":"Moss","lane":"smol-scout","description":"Room scout."}]}"#;
    let book = effective(load(&[(HOUSE, &house), (ROOM, room)]));
    let scout = &book.familiars[0];
    assert_eq!(book.familiars.len(), 1);
    assert_eq!(
        (scout.name.as_str(), scout.temperament.as_deref()),
        ("Moss", None)
    );
    assert!(scout.aliases.is_empty());
    assert_eq!(book.collective, "familiars");
}

#[test]
fn explicit_empty_values_override_instead_of_inheriting() {
    let house = house_page();
    let room = r#"{"guide":"","collectiveAliases":[],"familiars":[]}"#;
    let book = effective(load(&[(HOUSE, &house), (ROOM, room)]));
    assert!(book.guide.is_empty());
    assert!(book.collective_aliases.is_empty());
    assert!(book.familiars.is_empty());
}

#[test]
fn a_null_room_guide_refuses_instead_of_inheriting() {
    let house = house_page();
    let loaded = load(&[(HOUSE, &house), (ROOM, r#"{"guide":null}"#)]);
    assert!(!loaded.status.ok);
    assert!(loaded.status.spellbook.is_none());
    assert!(
        loaded
            .status
            .errors
            .iter()
            .any(|error| error.contains(ROOM))
    );
}

#[test]
fn a_cleared_chapter_leaves_house_familiars_unknown() {
    let house = house_page();
    let receipt = summon("pipkin", &[(HOUSE, &house), (ROOM, r#"{"familiars":[]}"#)]);
    assert_eq!(receipt.dispatch.errors, ["Unknown familiar: pipkin"]);
    assert!(receipt.familiar.is_none());
}

#[test]
fn aliases_resolve_against_the_effective_book() {
    let house = house_page();
    let room = r#"{"familiars":[{"id":"keeper","name":"Wick","aliases":["lantern"],"lane":"smol-scout","description":"Keeps the light."}]}"#;
    for (alias, id) in [("pipkin", "scout"), ("lantern", "keeper")] {
        let receipt = summon(alias, &[(HOUSE, &house), (ROOM, room)]);
        assert_eq!(
            receipt.familiar.map(|familiar| familiar.id).as_deref(),
            Some(id)
        );
    }
}

#[test]
fn a_room_alias_cannot_take_a_house_familiars_key() {
    let house = house_page();
    let room = r#"{"familiars":[{"id":"keeper","name":"Wick","aliases":["pipkin"],"lane":"smol-scout","description":"Keeps the light."}]}"#;
    let loaded = load(&[(HOUSE, &house), (ROOM, room)]);
    assert_eq!(
        loaded.status.errors,
        ["Familiar lookup key 'pipkin' is already owned by 'scout'."]
    );
}

#[test]
fn duplicate_room_ids_cannot_hide_behind_house_replacement() {
    let house = house_page();
    let room = format!(r#"{{"familiars":[{SCOUT},{SCOUT}]}}"#);
    let loaded = load(&[(HOUSE, &house), (ROOM, &room)]);
    assert!(!loaded.status.ok);
    assert!(loaded.status.spellbook.is_none());
    assert!(
        loaded
            .status
            .errors
            .iter()
            .any(|error| error.contains(ROOM))
    );
}

#[test]
fn a_malformed_house_page_refuses_with_its_own_path() {
    let room = format!(r#"{{"version":1,"collective":"kittens","familiars":[{SCOUT}]}}"#);
    let loaded = load(&[(HOUSE, "{"), (ROOM, &room)]);
    assert!(loaded.status.spellbook.is_none());
    assert_eq!(layers(&loaded), [("house", HOUSE)]);
    assert!(
        loaded.status.errors[0]
            .starts_with(&format!("Familiar spellbook '{HOUSE}' is not valid JSON"))
    );
}

#[test]
fn an_undeclared_category_refuses_instead_of_riding_along() {
    let loaded = load(&[(ROOM, r#"{"version":1,"wards":[]}"#)]);
    assert!(loaded.status.spellbook.is_none());
    assert!(loaded.status.errors[0].contains("unknown field `wards`"));
}

#[test]
fn no_page_in_either_layer_names_every_path_tried() {
    let loaded = load(&[]);
    assert!(loaded.sources.is_empty());
    assert!(loaded.status.errors[0].contains(HOUSE));
    assert!(loaded.status.errors[0].contains(ROOM));
}

#[test]
fn the_house_page_sits_beside_the_room_never_above_a_bare_room() {
    assert_eq!(
        house_dir("C:\\rooms\\kintsu\\").as_deref(),
        Some("C:\\rooms\\house")
    );
    assert_eq!(
        house_dir("/srv/rooms/kintsu").as_deref(),
        Some("/srv/rooms/house")
    );
    assert_eq!(house_dir("configured-room"), None);
}
