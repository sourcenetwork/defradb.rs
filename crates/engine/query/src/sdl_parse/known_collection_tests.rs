//! A relation to a collection that exists already is the relation it is when
//! both are declared in one SDL, so a collection has one identity however
//! many calls its schema arrives in.

use rapidhash::RapidHashMap;
use schema::{CollectionVersion, FieldKind};

use super::{parse_sdl, parse_sdl_with_known_collections};

const AUTHOR: &str = "type Author { name: String }";
const BOOK: &str = "type Book { title: String author: Author }";

fn named<'a>(collections: &'a [CollectionVersion], name: &str) -> &'a CollectionVersion {
    collections
        .iter()
        .find(|collection| collection.name == name)
        .unwrap_or_else(|| panic!("no collection named {name}"))
}

fn kind_of<'a>(collection: &'a CollectionVersion, field: &str) -> &'a FieldKind {
    &collection
        .fields
        .iter()
        .find(|candidate| candidate.name == field)
        .unwrap_or_else(|| panic!("no field named {field}"))
        .kind
}

/// Parse `declarations` one SDL at a time, each knowing the collections of
/// the ones before it, and return every collection parsed.
fn one_at_a_time(declarations: &[&str]) -> Vec<CollectionVersion> {
    let mut known = RapidHashMap::default();
    let mut parsed = Vec::new();
    for sdl in declarations {
        let collections = parse_sdl_with_known_collections(sdl, known.clone()).unwrap();
        for collection in &collections {
            known.insert(collection.name.clone(), collection.collection_id.clone());
        }
        parsed.extend(collections);
    }
    parsed
}

/// The collections of `declarations` are the same parsed one SDL at a time
/// and parsed as one SDL.
fn assert_same_as_one_sdl(declarations: &[&str]) {
    let together = parse_sdl(&declarations.join("\n")).unwrap();
    let apart = one_at_a_time(declarations);
    assert_eq!(apart.len(), together.len());
    for collection in &together {
        let alone = named(&apart, &collection.name);
        assert_eq!(
            alone.collection_id, collection.collection_id,
            "{}",
            collection.name
        );
        assert_eq!(
            alone.version_id, collection.version_id,
            "{}",
            collection.name
        );
        assert_eq!(alone.fields, collection.fields, "{}", collection.name);
        assert_eq!(alone.indexes, collection.indexes, "{}", collection.name);
    }
}

#[test]
fn a_relation_to_a_known_collection_commits_to_its_id() {
    let together = parse_sdl(&format!("{AUTHOR}\n{BOOK}")).unwrap();
    let author_id = named(&together, "Author").collection_id.clone();

    let known = [("Author".to_owned(), author_id.clone())]
        .into_iter()
        .collect();
    let alone = parse_sdl_with_known_collections(BOOK, known).unwrap();

    assert_eq!(
        kind_of(&alone[0], "author"),
        &FieldKind::relation(author_id, false)
    );
    assert_eq!(
        alone[0].collection_id,
        named(&together, "Book").collection_id
    );
    assert_eq!(alone[0].fields, named(&together, "Book").fields);
}

#[test]
fn the_identity_follows_the_known_target() {
    let id_for = |target: &str| {
        let known = [("Author".to_owned(), target.to_owned())]
            .into_iter()
            .collect();
        parse_sdl_with_known_collections(BOOK, known).unwrap()[0]
            .collection_id
            .clone()
    };
    let author = parse_sdl(AUTHOR).unwrap()[0].collection_id.clone();
    let other = parse_sdl("type Author { alias: String }").unwrap()[0]
        .collection_id
        .clone();

    assert_ne!(id_for(&author), id_for(&other));
}

#[test]
fn a_chain_parsed_in_three_steps() {
    assert_same_as_one_sdl(&[
        "type Country { name: String }",
        "type City { name: String country: Country }",
        "type Street { name: String city: City }",
    ]);
}

#[test]
fn a_self_reference_beside_a_known_target() {
    assert_same_as_one_sdl(&[
        "type City { name: String }",
        "type Person { name: String boss: Person home: City }",
    ]);
}

#[test]
fn a_one_to_many_pair_pointing_at_a_known_collection() {
    assert_same_as_one_sdl(&[
        "type Publisher { name: String }",
        "type Author { name: String publisher: Publisher books: [Book] }
         type Book { title: String author: Author publisher: Publisher }",
    ]);
}

#[test]
fn an_array_relation_to_a_known_collection() {
    assert_same_as_one_sdl(&[
        "type Author { name: String }",
        "type Shelf { label: String authors: [Author] }",
    ]);
    let apart = one_at_a_time(&[AUTHOR, "type Shelf { label: String authors: [Author] }"]);
    assert_eq!(
        kind_of(named(&apart, "Shelf"), "authors"),
        &FieldKind::relation(named(&apart, "Author").collection_id.clone(), true)
    );
}

#[test]
fn an_indexed_relation_to_a_known_collection_indexes_its_id_field() {
    assert_same_as_one_sdl(&[AUTHOR, "type Book { title: String author: Author @index }"]);
    let apart = one_at_a_time(&[AUTHOR, "type Book { title: String author: Author @index }"]);
    let indexed: Vec<&str> = named(&apart, "Book")
        .indexes
        .iter()
        .flat_map(|index| index.fields.iter().map(|field| field.name.as_str()))
        .collect();
    assert_eq!(indexed, ["_authorID"]);
}

#[test]
fn a_type_of_the_sdl_wins_over_a_known_namesake() {
    let known = [("Author".to_owned(), "stored-author".to_owned())]
        .into_iter()
        .collect();
    let both = parse_sdl_with_known_collections(&format!("{AUTHOR}\n{BOOK}"), known).unwrap();

    assert_eq!(
        kind_of(named(&both, "Book"), "author"),
        &FieldKind::relation(named(&both, "Author").collection_id.clone(), false)
    );
}

#[test]
fn an_unknown_type_is_still_refused() {
    let known = [("Author".to_owned(), "stored-author".to_owned())]
        .into_iter()
        .collect();
    let refused =
        parse_sdl_with_known_collections("type Book { editor: Editor }", known).unwrap_err();

    assert_eq!(
        refused.to_string(),
        parse_sdl("type Book { editor: Editor }")
            .unwrap_err()
            .to_string()
    );
}
