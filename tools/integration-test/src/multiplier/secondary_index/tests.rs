use testo::multiplier::SkipReason;
use testo::prelude::*;

use super::{add_indexes, has_index_directive, SecondaryIndex};
use crate::actions::{Actions, AddCollection, DeleteIndex, ListIndexes, NewIndex, Request};
use crate::multiplier::TestMultiplier;

fn add(sdl: &str) -> Actions {
    Actions::AddCollection(AddCollection {
        sdl: sdl.into(),
        ..Default::default()
    })
}

fn request(text: &str) -> Actions {
    Actions::Request(Request {
        request: text.into(),
        ..Default::default()
    })
}

fn instances(actions: Vec<Actions>) -> Vec<ActionInstance<Actions>> {
    actions.into_iter().map(Into::into).collect()
}

fn skips(actions: Vec<Actions>) -> bool {
    matches!(
        SecondaryIndex.should_skip(&instances(actions)),
        SkipReason::Skip(_)
    )
}

fn sdl_of(instance: &ActionInstance<Actions>) -> String {
    match instance {
        ActionInstance::Sync(Actions::AddCollection(add)) => add.sdl.as_str().to_owned(),
        other => panic!("not an AddCollection: {other:?}"),
    }
}

#[track_caller]
fn assert_has(sdl: &str, part: &str) {
    assert!(sdl.contains(part), "{part:?} not in:\n{sdl}");
}

#[track_caller]
fn assert_lacks(sdl: &str, part: &str) {
    assert!(!sdl.contains(part), "{part:?} in:\n{sdl}");
}

#[test]
fn index_actions_skip() {
    let user = || add("type User { name: String }");
    assert!(skips(vec![user(), Actions::NewIndex(NewIndex::default())]));
    assert!(skips(vec![
        user(),
        Actions::DeleteIndex(DeleteIndex::default())
    ]));
    assert!(skips(vec![
        user(),
        Actions::ListIndexes(ListIndexes::default())
    ]));
    assert!(!skips(vec![user()]));
}

#[test]
fn index_directives_are_detected() {
    assert!(has_index_directive("type User { name: String @index }"));
    assert!(has_index_directive(
        "type User { email: String @index(unique: true) }"
    ));
    assert!(has_index_directive(
        "type User { embedding: [Float32!] @vectorIndex(dimensions: 3) }"
    ));
    assert!(!has_index_directive("type User { name: String }"));
}

#[test]
fn simple_field() {
    assert_eq!(
        add_indexes("type User {\n\tname: String\n}"),
        "type User {\n\tname: String @index\n}"
    );
}

#[test]
fn multiple_fields() {
    assert_eq!(
        add_indexes("type User {\n\tname: String\n\tage: Int\n\tactive: Boolean\n}"),
        "type User {\n\tname: String @index\n\tage: Int @index\n\tactive: Boolean @index\n}"
    );
}

#[test]
fn all_scalar_types() {
    let fields = [
        ("name", "String"),
        ("age", "Int"),
        ("score", "Float"),
        ("points", "Float32"),
        ("points2", "Float64"),
        ("active", "Boolean"),
        ("created", "DateTime"),
        ("docID", "ID"),
        ("custom", "JSON"),
    ];
    let body = |suffix: &str| -> String {
        fields
            .iter()
            .map(|(name, ty)| format!("\t{name}: {ty}{suffix}\n"))
            .collect()
    };
    assert_eq!(
        add_indexes(&format!("type User {{\n{}}}", body(""))),
        format!("type User {{\n{}}}", body(" @index"))
    );
}

#[test]
fn index_goes_before_other_directives() {
    let result = add_indexes(
        "type User {
	name: String @crdt(type: lww)
	points: Float @crdt(type: pcounter)
	active: Boolean @default(value: true)
}",
    );
    assert_has(&result, "name: String @index @crdt(type: lww)");
    assert_has(&result, "points: Float @index @crdt(type: pcounter)");
    assert_has(&result, "active: Boolean @index @default(value: true)");
}

#[test]
fn non_null_fields() {
    let result = add_indexes(
        "type User {
	name: String!
	age: Int! @default(value: 0)
	score: Float!
}",
    );
    assert_has(&result, "name: String! @index");
    assert_has(&result, "age: Int! @index @default(value: 0)");
    assert_has(&result, "score: Float! @index");
}

#[test]
fn array_fields() {
    let result = add_indexes(
        "type User {
	names: [String]
	numbers: [Int!]
	scores: [Float]!
	flags: [Boolean!]!
	tags: [String] @crdt(type: lww)
	more: [Int!] @default(value: [])
}",
    );
    assert_has(&result, "names: [String] @index");
    assert_has(&result, "numbers: [Int!] @index");
    assert_has(&result, "scores: [Float]! @index");
    assert_has(&result, "flags: [Boolean!]! @index");
    assert_has(&result, "tags: [String] @index @crdt(type: lww)");
    assert_has(&result, "more: [Int!] @index @default(value: [])");
}

#[test]
fn one_to_many_indexes_the_single_side() {
    for owner in ["User", "User!"] {
        let result = add_indexes(&format!(
            "type User {{
	name: String
	devices: [Device]
}}

type Device {{
	model: String
	owner: {owner}
}}"
        ));
        assert_has(&result, "name: String @index");
        assert_has(&result, "model: String @index");
        assert_has(&result, "devices: [Device]\n");
        assert_has(&result, &format!("owner: {owner} @index"));
    }
}

#[test]
fn one_to_one_is_not_indexed() {
    for bang in ["", "!"] {
        let result = add_indexes(&format!(
            "type User {{
	name: String
	address: Address{bang}
}}

type Address {{
	city: String
	user: User{bang} @primary
}}"
        ));
        assert_has(&result, "name: String @index");
        assert_has(&result, "city: String @index");
        assert_has(&result, &format!("user: User{bang} @primary"));
        assert_lacks(&result, &format!("user: User{bang} @index"));
        assert_lacks(&result, &format!("address: Address{bang} @index"));
    }
}

#[test]
fn one_to_one_id_field_is_not_indexed() {
    let result = add_indexes(
        "type Book {
	name: String
	_authorID: Int
	author: Author @primary
}

type Author {
	name: String
	published: Book
}",
    );
    assert_has(&result, "name: String @index");
    assert_has(&result, "_authorID: Int\n");
    assert_has(&result, "author: Author @primary");
    assert_lacks(&result, "author: Author @index");
    assert_lacks(&result, "published: Book @index");
}

#[test]
fn a_same_named_field_on_another_type_is_indexed() {
    let result = add_indexes(
        "type Book {
	author: Author @primary
}

type Author {
	published: Book
}

type Audit {
	_authorID: Int
}",
    );
    assert_has(&result, "_authorID: Int @index");
}

#[test]
fn one_to_many_id_field_is_indexed() {
    let result = add_indexes(
        "type User {
	name: String
	devices: [Device]
}

type Device {
	model: String
	_ownerID: String
	owner: User
}",
    );
    assert_has(&result, "_ownerID: String @index");
    assert_has(&result, "owner: User @index");
}

#[test]
fn every_single_side_of_several_relations() {
    let result = add_indexes(
        "type User {
	name: String
	devices: [Device]
}

type Device {
	model: String
	owner: User
	manufacturer: Manufacturer
}

type Manufacturer {
	name: String
	devices: [Device]
}",
    );
    assert_lacks(&result, "[Device] @index");
    assert_has(&result, "owner: User @index");
    assert_has(&result, "manufacturer: Manufacturer @index");
}

#[test]
fn single_self_reference_is_indexed() {
    let result = add_indexes("type User {\n\tname: String\n\tboss: User\n}");
    assert_has(&result, "name: String @index");
    assert_has(&result, "boss: User @index");
}

#[test]
fn one_to_one_self_reference_is_not_indexed() {
    let result = add_indexes(
        "type User {
	name: String
	boss: User @primary
	underling: User
}",
    );
    assert_has(&result, "name: String @index");
    assert_has(&result, "boss: User @primary");
    assert_lacks(&result, "boss: User @index");
    assert_lacks(&result, "underling: User @index");
}

#[test]
fn named_one_to_one_relations_are_not_indexed() {
    let result = add_indexes(
        r#"type User {
	hosts: Dog @primary @relation(name:"hosts")
	walks: Dog @relation(name:"walkies")
}

type Dog {
	host: User @relation(name:"hosts")
	walker: User @primary @relation(name:"walkies")
}"#,
    );
    assert_has(&result, r#"hosts: Dog @primary @relation(name:"hosts")"#);
    assert_has(
        &result,
        r#"walker: User @primary @relation(name:"walkies")"#,
    );
    assert_has(&result, r#"walks: Dog @relation(name:"walkies")"#);
    assert_has(&result, r#"host: User @relation(name:"hosts")"#);
    assert_lacks(&result, "@index");
}

#[test]
fn circular_one_to_one_is_not_indexed() {
    let result = add_indexes(
        r#"type User {
	toleratedBy: Cat @relation(name:"tolerates")
}

type Cat {
	loves: Mouse @primary @relation(name:"loves")
	tolerates: User @primary @relation(name:"tolerates")
}

type Mouse {
	lovedBy: Cat @relation(name:"loves")
}"#,
    );
    assert_lacks(&result, "@index");
}

#[test]
fn join_type_relations_are_indexed() {
    let result = add_indexes(
        r#"type Student {
	name: String
}

type Course {
	name: String
}

type Enrollment {
	student: Student @relation(name: "student_enrollments")
	course: Course @relation(name: "course_enrollments")
}"#,
    );
    assert_has(&result, "name: String @index");
    assert_has(&result, "student: Student @index");
    assert_has(&result, "course: Course @index");
}

#[test]
fn relation_without_back_reference_is_indexed() {
    let result = add_indexes(
        "type Author {
	name: String
}

type Book {
	title: String
	author: Author
}",
    );
    assert_has(&result, "title: String @index");
    assert_has(&result, "author: Author @index");
}

#[test]
fn whitespace_is_kept() {
    let result = add_indexes("type User {\n\tname:    String\n\tage:Int\n}");
    assert_has(&result, "name:    String @index");
    assert_has(&result, "age:Int @index");
}

#[test]
fn types_with_directives_and_one_line_bodies_are_indexed() {
    assert_eq!(
        add_indexes(r#"type User @policy(id: "p", resource: "u") { name: String, age: Int }"#),
        r#"type User @policy(id: "p", resource: "u") { name: String @index, age: Int @index }"#
    );
}

#[test]
fn descriptions_comments_and_blobs_are_left_alone() {
    let result = add_indexes(
        r#""""A user: String"""
type User {
	# not: String
	"the name: String"
	name: String
	data: Blob
}"#,
    );
    assert_has(&result, "# not: String\n");
    assert_has(&result, "\"the name: String\"\n");
    assert_has(&result, "name: String @index");
    assert_has(&result, "data: Blob\n");
}

#[test]
fn sdl_that_does_not_parse_is_unchanged() {
    let sdl = "type User { name: String";
    assert_eq!(add_indexes(sdl), sdl);
}

#[test]
fn apply_rewrites_even_when_the_test_manages_indexes() {
    let new_index = Actions::NewIndex(NewIndex::default());
    let result = SecondaryIndex.apply(&instances(vec![
        add("type User { name: String }"),
        new_index.clone(),
    ]));
    assert_has(&sdl_of(&result[0]), "@index");
    assert!(matches!(&result[1], ActionInstance::Sync(action) if *action == new_index));
}

#[test]
fn apply_leaves_an_indexed_sdl_unchanged() {
    let actions = instances(vec![add("type User { name: String @index }")]);
    assert_eq!(
        sdl_of(&SecondaryIndex.apply(&actions)[0]),
        "type User { name: String @index }"
    );
}

#[test]
fn apply_indexes_every_field() {
    let result = SecondaryIndex.apply(&instances(vec![add(
        "type User {\n\tname: String\n\tage: Int\n}",
    )]));
    let sdl = sdl_of(&result[0]);
    assert_has(&sdl, "name: String @index");
    assert_has(&sdl, "age: Int @index");
}

#[test]
fn named_secondary_index() {
    assert_eq!(SecondaryIndex.name(), "secondary-index");
    let named = TestMultiplier::named(" signed-docs , secondary-index ");
    assert_eq!(named.len(), 1);
    assert_eq!(named[0].name(), "secondary-index");
    assert!(TestMultiplier::named("signed-docs").is_empty());
    assert!(TestMultiplier::named("").is_empty());
}

#[test]
fn sdl_with_indexes_skips() {
    assert!(skips(vec![add("type User { name: String @index }")]));
    assert!(skips(vec![add(
        "type User { email: String @index(unique: true) }"
    )]));
    assert!(skips(vec![add(
        "type User { embedding: [Float32!] @vectorIndex(dimensions: 3) }"
    )]));
}

#[test]
fn explain_requests_skip() {
    let user = || add("type User { name: String }");
    assert!(skips(vec![
        user(),
        request("query @explain(type: debug) { User { name } }")
    ]));
    assert!(skips(vec![
        user(),
        request("query @explain(type: simple) { User { name } }")
    ]));
    assert!(!skips(vec![user(), request("query { User { name } }")]));
}
