//! A filter through a relation the query does not select matches only the
//! related documents the reader may read (`defra.acp.relation_filters`).

use integration_test::actions::{Actions, AddCollection, AddDACPolicy, AddDoc, Request};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

const OWNER: u64 = 1;
const OTHER: u64 = 2;

const POLICY: &str = r#"
name: test
resources:
- name: companies
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: employees
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: people
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#;

fn add_policy() -> Actions {
    AddDACPolicy {
        policy: POLICY.into(),
        expected_data: json!({ "policyID": m::nonempty_string() }),
        common: Common {
            identity_seed: Some(OWNER),
            capture: captures([("policy", "/policyID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn collection(name: &str, resource: &str, fields: &str) -> Text {
    Text::concat([
        Text::from(format!("type {name} @policy(id: \"")),
        Text::reference("policy"),
        Text::from(format!("\", resource: \"{resource}\") {{ {fields} }}")),
    ])
}

fn add_collections(employee_fields: &str, company_fields: &str) -> Actions {
    AddCollection {
        sdl: Text::concat([
            collection("Employee", "employees", employee_fields),
            Text::from("\n"),
            collection("Company", "companies", company_fields),
        ]),
        ..Default::default()
    }
    .into()
}

/// A document the anonymous creator leaves public, or one `owner` keeps private.
fn add_doc(collection: &str, doc: Value, owner: Option<u64>, capture: &'static str) -> Actions {
    AddDoc {
        collection: collection.into(),
        doc_map: Some(doc),
        common: Common {
            identity_seed: owner,
            capture: captures([(capture, "/docID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// A public and a private company, each with a public and a private employee.
fn employees_and_companies(employee_fields: &str, company_fields: &str) -> Vec<Actions> {
    vec![
        add_policy(),
        add_collections(employee_fields, company_fields),
        add_doc(
            "Company",
            json!({ "name": "Public Company" }),
            None,
            "public",
        ),
        add_doc(
            "Company",
            json!({ "name": "Private Company" }),
            Some(OWNER),
            "private",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PubEmp in PubCompany", "company": m::r("public") }),
            None,
            "e0",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PubEmp in PrivateCompany", "company": m::r("private") }),
            None,
            "e1",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PrivateEmp in PubCompany", "company": m::r("public") }),
            Some(OWNER),
            "e2",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PrivateEmp in PrivateCompany", "company": m::r("private") }),
            Some(OWNER),
            "e3",
        ),
    ]
}

fn query(reader: Option<u64>, request: &str, expected: Value) -> Actions {
    query_text(reader, request.into(), expected)
}

fn query_text(reader: Option<u64>, request: Text, expected: Value) -> Actions {
    Request {
        request,
        expected_data: expected,
        common: Common {
            identity_seed: reader,
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

fn names(collection: &str, names: &[&str]) -> Value {
    let rows: Vec<Value> = names.iter().map(|name| json!({ "name": name })).collect();
    json!({ collection: m::unordered(Value::Array(rows)) })
}

fn run(id: &'static str, actions: Vec<Actions>) {
    TestCase {
        id,
        actions: actions.into_iter().map(Into::into).collect(),
        ..Default::default()
    }
    .execute();
}

const EMPLOYEE: &str = "name: String company: Company";
const COMPANY: &str = "name: String employees: [Employee]";

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_many_to_one_filter_matches_only_readable_related_documents() {
    let by_company = |company: &str| {
        format!(
            r#"query {{ Employee(filter: {{company: {{name: {{_eq: "{company}"}}}}}}) {{ name }} }}"#
        )
    };
    let by_company_id = || {
        Text::concat([
            Text::from(r#"query { Employee(filter: {company: {_docID: {_eq: ""#),
            Text::reference("private"),
            Text::from(r#""}}}) { name } }"#),
        ])
    };
    let mut actions = employees_and_companies(EMPLOYEE, COMPANY);
    for reader in [None, Some(OTHER)] {
        actions.push(query(
            reader,
            &by_company("Private Company"),
            names("Employee", &[]),
        ));
        actions.push(query(
            reader,
            &by_company("Public Company"),
            names("Employee", &["PubEmp in PubCompany"]),
        ));
        actions.push(query_text(reader, by_company_id(), names("Employee", &[])));
    }
    let private_company_employees = names(
        "Employee",
        &["PubEmp in PrivateCompany", "PrivateEmp in PrivateCompany"],
    );
    actions.push(query(
        Some(OWNER),
        &by_company("Private Company"),
        private_company_employees.clone(),
    ));
    actions.push(query_text(
        Some(OWNER),
        by_company_id(),
        private_company_employees,
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_one_to_many_filter_matches_only_readable_related_documents() {
    let by_employee = |employee: &str| {
        format!(
            r#"query {{ Company(filter: {{employees: {{name: {{_eq: "{employee}"}}}}}}) {{ name }} }}"#
        )
    };
    let mut actions = employees_and_companies(EMPLOYEE, COMPANY);
    for reader in [None, Some(OTHER)] {
        actions.push(query(
            reader,
            &by_employee("PrivateEmp in PubCompany"),
            names("Company", &[]),
        ));
        actions.push(query(
            reader,
            &by_employee("PubEmp in PubCompany"),
            names("Company", &["Public Company"]),
        ));
    }
    actions.push(query(
        Some(OWNER),
        &by_employee("PrivateEmp in PubCompany"),
        names("Company", &["Public Company"]),
    ));
    run(integration_test::scenario_id!(), actions);
}

/// A public company owned by a private person, and a private company owned by
/// a public person, each with a public employee.
fn employees_companies_and_owners() -> Vec<Actions> {
    vec![
        add_policy(),
        AddCollection {
            sdl: Text::concat([
                collection("Employee", "employees", "name: String company: Company"),
                Text::from("\n"),
                collection(
                    "Company",
                    "companies",
                    "name: String owner: Person employees: [Employee]",
                ),
                Text::from("\n"),
                collection("Person", "people", "name: String companies: [Company]"),
            ]),
            ..Default::default()
        }
        .into(),
        add_doc("Person", json!({ "name": "Public Person" }), None, "p0"),
        add_doc(
            "Person",
            json!({ "name": "Private Person" }),
            Some(OWNER),
            "p1",
        ),
        add_doc(
            "Company",
            json!({ "name": "Public Company", "owner": m::r("p1") }),
            None,
            "public",
        ),
        add_doc(
            "Company",
            json!({ "name": "Private Company", "owner": m::r("p0") }),
            Some(OWNER),
            "private",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PubEmp in PubCompany", "company": m::r("public") }),
            None,
            "e0",
        ),
        add_doc(
            "Employee",
            json!({ "name": "PubEmp in PrivateCompany", "company": m::r("private") }),
            None,
            "e1",
        ),
    ]
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_two_hop_filter_matches_only_readable_related_documents() {
    let by_owner = |owner: &str| {
        format!(
            r#"query {{ Employee(filter: {{company: {{owner: {{name: {{_eq: "{owner}"}}}}}}}}) {{ name }} }}"#
        )
    };
    let mut actions = employees_companies_and_owners();
    for reader in [None, Some(OTHER)] {
        actions.push(query(
            reader,
            &by_owner("Private Person"),
            names("Employee", &[]),
        ));
        actions.push(query(
            reader,
            &by_owner("Public Person"),
            names("Employee", &[]),
        ));
    }
    actions.push(query(
        Some(OWNER),
        &by_owner("Private Person"),
        names("Employee", &["PubEmp in PubCompany"]),
    ));
    actions.push(query(
        Some(OWNER),
        &by_owner("Public Person"),
        names("Employee", &["PubEmp in PrivateCompany"]),
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn an_indexed_relation_filter_matches_only_readable_related_documents() {
    let request = r#"query {
        Employee(filter: {company: {name: {_eq: "Private Company"}}}) { name }
    }"#;
    let mut actions = employees_and_companies(
        "name: String company: Company @index",
        "name: String @index employees: [Employee]",
    );
    for reader in [None, Some(OTHER)] {
        actions.push(query(reader, request, names("Employee", &[])));
    }
    actions.push(query(
        Some(OWNER),
        request,
        names(
            "Employee",
            &["PubEmp in PrivateCompany", "PrivateEmp in PrivateCompany"],
        ),
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn an_indexed_one_to_many_filter_matches_only_readable_related_documents() {
    let by_employee = |employee: &str, selection: &str| {
        format!(
            r#"query {{ Company(filter: {{employees: {{name: {{_eq: "{employee}"}}}}}}) {{ {selection} }} }}"#
        )
    };
    let selected = "name employees { name }";
    let mut actions = employees_and_companies("name: String @index company: Company", COMPANY);
    for reader in [None, Some(OTHER)] {
        for selection in ["name", selected] {
            actions.push(query(
                reader,
                &by_employee("PrivateEmp in PubCompany", selection),
                names("Company", &[]),
            ));
        }
        actions.push(query(
            reader,
            &by_employee("PubEmp in PubCompany", selected),
            json!({ "Company": [{
                "name": "Public Company",
                "employees": [{ "name": "PubEmp in PubCompany" }],
            }] }),
        ));
    }
    actions.push(query(
        Some(OWNER),
        &by_employee("PrivateEmp in PubCompany", selected),
        json!({ "Company": [{
            "name": "Public Company",
            "employees": m::unordered(json!([
                { "name": "PubEmp in PubCompany" },
                { "name": "PrivateEmp in PubCompany" },
            ])),
        }] }),
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_mutation_through_a_relation_filter_picks_only_readable_related_documents() {
    let update = |employee: &str, name: &str| {
        format!(
            r#"mutation {{ update_Company(filter: {{employees: {{name: {{_eq: "{employee}"}}}}}}, input: {{name: "{name}"}}) {{ name }} }}"#
        )
    };
    let delete = r#"mutation {
        delete_Company(filter: {employees: {name: {_eq: "PrivateEmp in PubCompany"}}}) { name }
    }"#;
    let mut actions = employees_and_companies(EMPLOYEE, COMPANY);
    for reader in [None, Some(OTHER)] {
        actions.push(query(
            reader,
            &update("PrivateEmp in PubCompany", "Renamed"),
            json!({ "update_Company": [] }),
        ));
        actions.push(query(reader, delete, json!({ "delete_Company": [] })));
    }
    actions.push(query(
        Some(OWNER),
        "query { Company { name } }",
        names("Company", &["Public Company", "Private Company"]),
    ));
    actions.push(query(
        Some(OWNER),
        &update("PubEmp in PrivateCompany", "Renamed Private"),
        json!({ "update_Company": [{ "name": "Renamed Private" }] }),
    ));
    actions.push(query(
        Some(OWNER),
        &update("PrivateEmp in PubCompany", "Renamed"),
        json!({ "update_Company": [{ "name": "Renamed" }] }),
    ));
    run(integration_test::scenario_id!(), actions);
}

const ONE_TO_ONE_POLICY: &str = r#"
name: test
resources:
- name: authors
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
- name: books
  permissions:
  - name: delete
  - expr: reader
    name: read
  - name: update
  relations:
  - name: reader
    types:
    - actor
"#;

/// A public book by a private author, and a private book by a public author.
fn authors_and_books() -> Vec<Actions> {
    vec![
        AddDACPolicy {
            policy: ONE_TO_ONE_POLICY.into(),
            expected_data: json!({ "policyID": m::nonempty_string() }),
            common: Common {
                identity_seed: Some(OWNER),
                capture: captures([("policy", "/policyID")]),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
        AddCollection {
            sdl: Text::concat([
                collection("Book", "books", "name: String author: Author @primary"),
                Text::from("\n"),
                collection("Author", "authors", "name: String published: Book"),
            ]),
            ..Default::default()
        }
        .into(),
        add_doc(
            "Author",
            json!({ "name": "Private Author" }),
            Some(OWNER),
            "private",
        ),
        add_doc("Author", json!({ "name": "Public Author" }), None, "public"),
        add_doc(
            "Book",
            json!({ "name": "Public Book", "author": m::r("private") }),
            None,
            "b0",
        ),
        add_doc(
            "Book",
            json!({ "name": "Private Book", "author": m::r("public") }),
            Some(OWNER),
            "b1",
        ),
    ]
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_one_to_one_filter_matches_only_readable_related_documents() {
    let by_author = r#"query {
        Book(filter: {author: {name: {_eq: "Private Author"}}}) { name }
    }"#;
    let by_book = r#"query {
        Author(filter: {published: {name: {_eq: "Private Book"}}}) { name }
    }"#;
    let mut actions = authors_and_books();
    for reader in [None, Some(OTHER)] {
        actions.push(query(reader, by_author, names("Book", &[])));
        actions.push(query(reader, by_book, names("Author", &[])));
    }
    actions.push(query(
        Some(OWNER),
        by_author,
        names("Book", &["Public Book"]),
    ));
    actions.push(query(
        Some(OWNER),
        by_book,
        names("Author", &["Public Author"]),
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_secondary_relation_id_names_only_a_readable_related_document() {
    let by_book_id = || -> Text {
        Text::concat([
            Text::from(r#"query { Author(filter: {_publishedID: {_eq: ""#),
            Text::reference("b1"),
            Text::from(r#""}}) { name } }"#),
        ])
    };
    let ids = "query { Author { name _publishedID } }";
    let mut actions = authors_and_books();
    for reader in [None, Some(OTHER)] {
        actions.push(
            Request {
                request: by_book_id(),
                expected_data: names("Author", &[]),
                common: Common {
                    identity_seed: reader,
                    ..Default::default()
                },
                ..Default::default()
            }
            .into(),
        );
        actions.push(query(
            reader,
            ids,
            json!({ "Author": [{ "name": "Public Author", "_publishedID": null }] }),
        ));
    }
    actions.push(
        Request {
            request: by_book_id(),
            expected_data: names("Author", &["Public Author"]),
            common: Common {
                identity_seed: Some(OWNER),
                ..Default::default()
            },
            ..Default::default()
        }
        .into(),
    );
    actions.push(query(
        Some(OWNER),
        ids,
        json!({ "Author": m::unordered(json!([
            { "name": "Public Author", "_publishedID": m::r("b1") },
            { "name": "Private Author", "_publishedID": m::r("b0") },
        ])) }),
    ));
    run(integration_test::scenario_id!(), actions);
}

#[test]
#[mantra_macros::req_test("defra.acp.relation_filters")]
fn a_secondary_relation_id_filter_sees_an_unreadable_related_document_as_null() {
    let by_book_id = |condition: &str| {
        format!(r#"query {{ Author(filter: {{_publishedID: {condition}}}) {{ name }} }}"#)
    };
    let mut actions = authors_and_books();
    actions.push(add_doc(
        "Author",
        json!({ "name": "Bookless Author" }),
        None,
        "bookless",
    ));
    for reader in [None, Some(OTHER)] {
        for condition in ["{_eq: null}", r#"{_neq: ""}"#] {
            actions.push(query(
                reader,
                &by_book_id(condition),
                names("Author", &["Public Author", "Bookless Author"]),
            ));
        }
        actions.push(query(
            reader,
            &by_book_id("{_neq: null}"),
            names("Author", &[]),
        ));
    }
    actions.push(query(
        Some(OWNER),
        &by_book_id("{_eq: null}"),
        names("Author", &["Bookless Author"]),
    ));
    actions.push(query(
        Some(OWNER),
        &by_book_id(r#"{_neq: ""}"#),
        names(
            "Author",
            &["Public Author", "Private Author", "Bookless Author"],
        ),
    ));
    actions.push(query(
        Some(OWNER),
        &by_book_id("{_neq: null}"),
        names("Author", &["Public Author", "Private Author"]),
    ));
    run(integration_test::scenario_id!(), actions);
}
