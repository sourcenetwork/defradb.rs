use defra_node::{EmbeddedNode, QueryRequest};

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const SENDER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";
const POLICY: &str = r#"
name: Target bound creation
resources:
  - name: agents
    relations:
      - name: invoker
      - name: parent
        types: [agents]
    permissions:
      - name: invoke
        expr: invoker + parent->invoke
      - name: read
      - name: update
      - name: delete
  - name: requests
    relations:
      - name: working_agent
      - name: parent
        types: [agents]
    permissions:
      - name: read
        expr: working_agent + parent->read
      - name: update
        expr: working_agent
      - name: delete
"#;

async fn execute(node: &EmbeddedNode, actor: &str, query: &str) -> defra_node::QueryResponse {
    node.execute_request(
        QueryRequest::new(query).with_identity(Some(identity::Did::new(actor).unwrap())),
    )
    .await
}

#[track_caller]
fn created(response: &defra_node::QueryResponse, name: &str) -> String {
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.as_ref().unwrap();
    data.get(format!("create_{name}"))
        .or_else(|| data.get(format!("add_{name}")))
        .and_then(|rows| rows.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row["_docID"].as_str())
        .unwrap()
        .to_owned()
}

#[test]
fn local_policy_commitment_survives_creation_and_schema_patch() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        let policy = node.add_dac_policy(OWNER, POLICY).await.unwrap();
        node.add_collection(&format!(r#"type Agent @policy(id: "{policy}", resource: "agents") {{ did: String @index(unique: true) @immutable }}"#)).await.unwrap();
        let sdl = format!(r#"type Requests @policy(id: "{policy}", resource: "requests", createField: "agent_did", createTarget: "Agent", createTargetField: "did", createPermission: "invoke") {{ agent_did: String @immutable }}"#);
        let parsed = query::sdl_parse::parse_sdl(&sdl).unwrap().remove(0);
        node.add_collection(&sdl).await.unwrap();
        let original = node.get_collection("Requests").unwrap().unwrap();
        assert_eq!(original.collection_id, parsed.collection_id);
        assert_eq!(original.version_id, parsed.version_id);
        assert_ne!(original.collection_id, original.version_id);
        let patched = node.patch_collection("Requests", r#"[{"op":"add","path":"/Requests/Fields/-","value":{"Name":"message","Kind":"String"}}]"#).await.unwrap();
        assert_eq!(patched.collection_id, original.collection_id);
        assert_ne!(patched.version_id, original.version_id);
        assert_eq!(patched.policy, original.policy);
        assert!(patched.policy.as_ref().unwrap().create_rule.is_some());
    }).unwrap()
}

#[test]
fn target_bound_create_and_revoke_are_transactional() {
    workers::block_on(async {
        let node = EmbeddedNode::builder().build().await.unwrap();
        let policy = node.add_dac_policy(OWNER, POLICY).await.unwrap();
        let other_policy = node.add_dac_policy(OWNER, &POLICY.replace("Target bound creation", "Other policy")).await.unwrap();
        node.add_collection(&format!(r#"
            type Agent @policy(id: "{policy}", resource: "agents") {{
                did: String @index(unique: true) @immutable
                nonce: String @immutable
            }}
            type Requests @policy(id: "{policy}", resource: "requests",
                createField: "agent_did", createTarget: "Agent", createTargetField: "did", createPermission: "invoke", createRelation: "working_agent", createActorField: "did") {{
                agent_did: String @immutable
                request_key: String @index(unique: true) @immutable
                message: String
            }}
            type GoalCommand @policy(id: "{policy}", resource: "requests",
                createField: "target_doc", createTarget: "Agent", createTargetField: "_docID", createPermission: "invoke", createMatchField: "selected_did", createTargetMatchField: "did", createParentRelation: "parent") {{
                target_doc: String @immutable
                selected_did: String @immutable
            }}
            type BadCommand @policy(id: "{policy}", resource: "requests",
                createField: "target_doc", createTarget: "Agent", createTargetField: "_docID", createPermission: "invoke", createParentRelation: "undeclared") {{
                target_doc: String @immutable
            }}
            type OtherAgent @policy(id: "{other_policy}", resource: "agents") {{ did: String @immutable }}
            type CrossCommand @policy(id: "{policy}", resource: "requests",
                createField: "target_doc", createTarget: "OtherAgent", createTargetField: "_docID", createPermission: "invoke", createParentRelation: "parent") {{
                target_doc: String @immutable
            }}"#)).await.unwrap();
        let agent = created(&execute(&node, OWNER, &format!(r#"mutation {{ create_Agent(input: {{did: "{OWNER}", nonce: "first"}}) {{_docID}} }}"#)).await, "Agent");
        let other = created(&execute(&node, OWNER, &format!(r#"mutation {{ create_Agent(input: {{did: "{SENDER}", nonce: "first"}}) {{_docID}} }}"#)).await, "Agent");
        node.add_dac_actor_relationship(OWNER, "Agent", &agent, "invoker", SENDER).await.unwrap();
        let mismatched = execute(&node, SENDER, &format!(r#"mutation {{ create_GoalCommand(input: {{target_doc: "{agent}", selected_did: "{SENDER}"}}) {{_docID}} }}"#)).await;
        assert!(mismatched.has_errors(), "command selected a different agent");
        let denied_target = execute(&node, SENDER, &format!(r#"mutation {{ create_GoalCommand(input: {{target_doc: "{other}", selected_did: "{SENDER}"}}) {{_docID}} }}"#)).await;
        let missing_target = execute(&node, SENDER, &format!(r#"mutation {{ create_GoalCommand(input: {{target_doc: "absent", selected_did: "{SENDER}"}}) {{_docID}} }}"#)).await;
        assert!(denied_target.has_errors() && missing_target.has_errors());
        assert_eq!(mismatched.errors[0].message, denied_target.errors[0].message);
        assert_eq!(mismatched.errors[0].message, missing_target.errors[0].message);
        let bound = execute(&node, SENDER, &format!(r#"mutation {{ create_GoalCommand(input: {{target_doc: "{agent}", selected_did: "{OWNER}"}}) {{_docID}} }}"#)).await;
        let bound = created(&bound, "GoalCommand");
        let inherited_read = execute(&node, OWNER, &format!(r#"{{ GoalCommand(filter: {{_docID: {{_eq: "{bound}"}}}}) {{_docID}} }}"#)).await;
        assert_eq!(inherited_read.data.unwrap()["GoalCommand"].as_array().unwrap().len(), 1, "target parent must grant inherited read");
        let bad_parent = execute(&node, SENDER, &format!(r#"mutation {{ create_BadCommand(input: {{target_doc: "{agent}"}}) {{_docID}} }}"#)).await;
        assert!(bad_parent.has_errors(), "undeclared parent relation was accepted");
        let bad_rows = execute(&node, SENDER, "{ BadCommand {_docID} }").await;
        assert!(bad_rows.data.unwrap()["BadCommand"].as_array().unwrap().is_empty(), "failed parent grant did not roll back creation");
        let foreign = created(&execute(&node, OWNER, &format!(r#"mutation {{ create_OtherAgent(input: {{did: "{OWNER}"}}) {{_docID}} }}"#)).await, "OtherAgent");
        node.add_dac_actor_relationship(OWNER, "OtherAgent", &foreign, "invoker", SENDER).await.unwrap();
        let cross = execute(&node, SENDER, &format!(r#"mutation {{ create_CrossCommand(input: {{target_doc: "{foreign}"}}) {{_docID}} }}"#)).await;
        assert!(cross.has_errors(), "cross-policy parent was accepted");
        let cross_rows = execute(&node, SENDER, "{ CrossCommand {_docID} }").await;
        assert!(cross_rows.data.unwrap()["CrossCommand"].as_array().unwrap().is_empty());
        let create = format!(r#"mutation {{ create_Requests(input: {{agent_did: "{OWNER}", request_key: "one"}}) {{_docID}} }}"#);
        let tx = node.begin_transaction(false).await.unwrap();
        let request = node.execute_request_in_txn(QueryRequest::new(&create).with_identity(Some(identity::Did::new(SENDER).unwrap())), &tx).await;
        let doc_id = created(&request, "Requests");
        node.commit_transaction(&tx).await.unwrap();
        let visible = execute(&node, OWNER, &format!(r#"{{ Requests(filter: {{_docID: {{_eq: "{doc_id}"}}}}) {{_docID}} }}"#)).await;
        assert_eq!(visible.data.unwrap()["Requests"].as_array().unwrap().len(), 1);

        let denied = execute(&node, SENDER, &format!(r#"mutation {{ create_Requests(input: {{agent_did: "{SENDER}", request_key: "denied"}}) {{_docID}} }}"#)).await;
        assert!(denied.has_errors(), "ungranted sibling target was created");
        let mixed = execute(&node, SENDER, &format!(r#"mutation {{ create_Requests(input: [{{agent_did: "{OWNER}", request_key: "mixed-allowed"}}, {{agent_did: "{SENDER}", request_key: "mixed-denied"}}]) {{_docID}} }}"#)).await;
        assert!(mixed.has_errors(), "mixed batch created a denied target");
        let visible = execute(&node, SENDER, r#"{ Requests(filter: {request_key: {_eq: "mixed-allowed"}}) {_docID} }"#).await;
        assert!(visible.data.unwrap()["Requests"].as_array().unwrap().is_empty());
        let upsert = execute(&node, SENDER, &format!(r#"mutation {{ upsert_Requests(filter: {{request_key: {{_eq: "missing"}}}}, add: {{agent_did: "{SENDER}", request_key: "missing"}}, update: {{message: "changed"}}) {{_docID}} }}"#)).await;
        assert!(upsert.has_errors(), "upsert created for an ungranted target");
        let automatic = execute(&node, SENDER, &format!(r#"mutation {{ create_Requests(input: {{agent_did: "{OWNER}", request_key: "autocommit"}}) {{_docID}} }}"#)).await;
        let automatic = created(&automatic, "Requests");
        let visible = execute(&node, OWNER, &format!(r#"{{ Requests(filter: {{_docID: {{_eq: "{automatic}"}}}}) {{_docID}} }}"#)).await;
        assert_eq!(visible.data.unwrap()["Requests"].as_array().unwrap().len(), 1);

        let stale = node.begin_transaction(false).await.unwrap();
        let response = node.execute_request_in_txn(QueryRequest::new(format!(r#"mutation {{ create_Requests(input: {{agent_did: "{OWNER}", request_key: "stale"}}) {{_docID}} }}"#))
            .with_identity(Some(identity::Did::new(SENDER).unwrap())), &stale).await;
        created(&response, "Requests");
        node.delete_dac_actor_relationship(OWNER, "Agent", &agent, "invoker", SENDER).await.unwrap();
        node.add_dac_actor_relationship(OWNER, "Agent", &agent, "invoker", SENDER).await.unwrap();
        assert!(node.commit_transaction(&stale).await.is_err(), "stale authorization committed after revoke/regrant");

        let parent = acp::Subject::entity_set("agents", &agent, "");
        let relation_tx = node.begin_transaction(false).await.unwrap();
        node.add_dac_relationship_in_txn(&relation_tx, identity::Did::new(OWNER).unwrap(), "Agent", &other, "parent", parent.clone()).await.unwrap();
        node.commit_transaction(&relation_tx).await.unwrap();
        let inherited = format!(r#"mutation {{ create_Requests(input: {{agent_did: "{SENDER}", request_key: "inherited"}}) {{_docID}} }}"#);
        let parent_race = node.begin_transaction(false).await.unwrap();
        created(&node.execute_request_in_txn(QueryRequest::new(&inherited).with_identity(Some(identity::Did::new(SENDER).unwrap())), &parent_race).await, "Requests");
        let acp = node.document_acp();
        let owner_did = identity::Did::new(OWNER).unwrap();
        acp.delete_relationship(&owner_did, parent.clone(), &policy, "agents", &other, "parent", &[]).await.unwrap();
        acp.add_relationship(&owner_did, parent, &policy, "agents", &other, "parent", &[]).await.unwrap();
        assert!(node.commit_transaction(&parent_race).await.is_err(), "parent relationship ABA was not detected");

        let absent_race = node.begin_transaction(false).await.unwrap();
        created(&node.execute_request_in_txn(QueryRequest::new(&inherited).with_identity(Some(identity::Did::new(SENDER).unwrap())), &absent_race).await, "Requests");
        node.add_dac_actor_relationship(OWNER, "Agent", &other, "invoker", SENDER).await.unwrap();
        node.delete_dac_actor_relationship(OWNER, "Agent", &other, "invoker", SENDER).await.unwrap();
        assert!(node.commit_transaction(&absent_race).await.is_err(), "absent authorization relation ABA was not detected");

        let target_race = node.begin_transaction(false).await.unwrap();
        let recreate = format!(r#"mutation {{ create_Requests(input: {{agent_did: "{OWNER}", request_key: "target-race"}}) {{_docID}} }}"#);
        created(&node.execute_request_in_txn(QueryRequest::new(&recreate).with_identity(Some(identity::Did::new(SENDER).unwrap())), &target_race).await, "Requests");
        let deleted = execute(&node, OWNER, &format!(r#"mutation {{ delete_Agent(docID: "{agent}") {{_docID}} }}"#)).await;
        assert!(!deleted.has_errors(), "{:?}", deleted.errors);
        let fresh = created(&execute(&node, OWNER, &format!(r#"mutation {{ create_Agent(input: {{did: "{OWNER}", nonce: "second"}}) {{_docID}} }}"#)).await, "Agent");
        node.add_dac_actor_relationship(OWNER, "Agent", &fresh, "invoker", SENDER).await.unwrap();
        assert!(node.commit_transaction(&target_race).await.is_err(), "target replacement was not detected");
        node.shutdown().await;
    }).unwrap();
}
