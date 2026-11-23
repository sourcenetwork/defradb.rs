use integration_test::{for_each_runtime, TestCluster};
use query::explain_asserter::ExplainAsserter;

/// Test recursive execute explain with 2-level deep nested one-to-one joins.
///
/// Mirrors Go's TestExecuteExplainWithTwoLevelDeepNestedJoins from PR #4471.
/// Schema: Author -> AuthorContact -> ContactAddress (3-level, all one-to-one)
///
/// Expected structure:
/// ```
/// selectTopNode > selectNode > typeIndexJoin > typeJoinOne > {
///   root: { scanNode: {...} },
///   subType: { selectTopNode > selectNode > typeIndexJoin > typeJoinOne > {
///     root: { scanNode: {...} },
///     subType: { selectTopNode > selectNode > scanNode: {...} }
///   }}
/// }
/// ```
async fn explain_nested_execute_test(cluster: TestCluster) {
    let client = cluster.client(0);

    // Schema matching Go's explain test fixture (one-to-one with @primary)
    client
        .schema_add(
            r#"
            type Author {
                name: String
                age: Int
                verified: Boolean
                contact: AuthorContact @primary
            }
            type AuthorContact {
                cell: String
                email: String
                author: Author
                address: ContactAddress @primary
            }
            type ContactAddress {
                city: String
                country: String
                contact: AuthorContact
            }
            "#,
        )
        .expect("schema add failed");

    // Create test data (bottom-up to satisfy FK constraints)

    // 2 ContactAddress documents
    client
        .query(r#"mutation { add_ContactAddress(input: {city: "Waterloo", country: "Canada"}) { _docID } }"#)
        .expect("create address 1");
    let addr2 = client
        .query(r#"mutation { add_ContactAddress(input: {city: "Brampton", country: "Canada"}) { _docID } }"#)
        .expect("create address 2");
    let _ = addr2["add_ContactAddress"][0]["_docID"]
        .as_str()
        .expect("address 2 _docID");

    // 2 AuthorContact documents linked to addresses
    // First query for address IDs
    let addresses = client
        .query(r#"query { ContactAddress { _docID city } }"#)
        .expect("query addresses");
    let addr_arr = addresses["ContactAddress"]
        .as_array()
        .expect("address array");
    let addr_id_0 = addr_arr[0]["_docID"].as_str().expect("addr 0 id");
    let addr_id_1 = addr_arr[1]["_docID"].as_str().expect("addr 1 id");

    client
        .query(&format!(
            r#"mutation {{ add_AuthorContact(input: {{cell: "5197212301", email: "john@example.com", _addressID: "{addr_id_0}"}}) {{ _docID }} }}"#,
        ))
        .expect("create contact 1");
    client
        .query(&format!(
            r#"mutation {{ add_AuthorContact(input: {{cell: "5197212302", email: "cornelia@example.com", _addressID: "{addr_id_1}"}}) {{ _docID }} }}"#,
        ))
        .expect("create contact 2");

    // 2 Author documents linked to contacts
    let contacts = client
        .query(r#"query { AuthorContact { _docID email } }"#)
        .expect("query contacts");
    let contact_arr = contacts["AuthorContact"].as_array().expect("contact array");
    let contact_id_0 = contact_arr[0]["_docID"].as_str().expect("contact 0 id");
    let contact_id_1 = contact_arr[1]["_docID"].as_str().expect("contact 1 id");

    client
        .query(&format!(
            r#"mutation {{ add_Author(input: {{name: "John Grisham", age: 65, verified: true, _contactID: "{contact_id_0}"}}) {{ _docID }} }}"#,
        ))
        .expect("create author 1");
    client
        .query(&format!(
            r#"mutation {{ add_Author(input: {{name: "Cornelia Funke", age: 62, verified: false, _contactID: "{contact_id_1}"}}) {{ _docID }} }}"#,
        ))
        .expect("create author 2");

    // Run execute explain on the 3-level nested query (matches Go test exactly)
    let explain_result = client
        .query(
            r#"query @explain(type: execute) {
                Author {
                    name
                    contact {
                        email
                        address {
                            city
                        }
                    }
                }
            }"#,
        )
        .expect("explain query");

    // The result may be wrapped in an array (explain returns operationNode array)
    let explain = if explain_result.is_array() {
        explain_result[0].clone()
    } else {
        explain_result.clone()
    };

    let root_path = [
        "explain",
        "operationNode",
        "0",
        "selectTopNode",
        "selectNode",
    ];

    // Level 1: Author -> AuthorContact.
    ExplainAsserter::new(&explain)
        .path(&root_path)
        .iterations(3)
        .filter_matches(2)
        .enter("typeIndexJoin")
        .iterations(3)
        .enter("typeJoinOne")
        .enter("root")
        .enter("scanNode")
        .doc_fetches(2)
        .field_fetches(8)
        .index_fetches(0)
        .iterations(3);

    // Level 2: AuthorContact -> ContactAddress.
    ExplainAsserter::new(&explain)
        .path(&root_path)
        .enter("typeIndexJoin")
        .enter("typeJoinOne")
        .enter("subType")
        .enter("selectTopNode")
        .enter("selectNode")
        .iterations(2)
        .filter_matches(2)
        .enter("typeIndexJoin")
        .iterations(2)
        .enter("typeJoinOne")
        .enter("root")
        .enter("scanNode")
        .doc_fetches(2)
        .field_fetches(6)
        .index_fetches(0)
        .iterations(2);

    // ContactAddress is the leaf scan.
    ExplainAsserter::new(&explain)
        .path(&root_path)
        .enter("typeIndexJoin")
        .enter("typeJoinOne")
        .enter("subType")
        .enter("selectTopNode")
        .enter("selectNode")
        .iterations(2)
        .filter_matches(2)
        .enter("typeIndexJoin")
        .enter("typeJoinOne")
        .enter("subType")
        .enter("selectTopNode")
        .enter("selectNode")
        .enter("scanNode")
        .doc_fetches(2)
        .field_fetches(4)
        .index_fetches(0)
        .iterations(2);
}

for_each_runtime!(explain_nested_execute, explain_nested_execute_test);
