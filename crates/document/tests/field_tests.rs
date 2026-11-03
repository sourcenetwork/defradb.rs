//! Integration tests for Field

use document::{Error, Field};
use schema::CType;

#[test]
fn test_new_field() {
    let field = Field::new("name", CType::LwwRegister).unwrap();
    assert_eq!(field.name(), "name");
    assert_eq!(field.crdt_type(), CType::LwwRegister);
}

#[test]
fn test_lww_field() {
    let field = Field::new("title", CType::LwwRegister).unwrap();
    assert_eq!(field.name(), "title");
    assert_eq!(field.crdt_type(), CType::LwwRegister);
}

#[test]
fn test_counter_field() {
    let field = Field::new("views", CType::PnCounter).unwrap();
    assert_eq!(field.name(), "views");
    assert_eq!(field.crdt_type(), CType::PnCounter);
}

#[test]
fn test_display() {
    let field = Field::new("email", CType::LwwRegister).unwrap();
    assert_eq!(field.to_string(), "email");
}

#[test]
fn test_equality() {
    let f1 = Field::new("name", CType::LwwRegister).unwrap();
    let f2 = Field::new("name", CType::LwwRegister).unwrap();
    let f3 = Field::new("other", CType::LwwRegister).unwrap();
    let f4 = Field::new("name", CType::PnCounter).unwrap();

    assert_eq!(f1, f2);
    assert_ne!(f1, f3);
    assert_ne!(f1, f4); // Same name, different CRDT type
}

#[test]
fn test_empty_name_rejected() {
    let result = Field::new("", CType::LwwRegister);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::EmptyFieldName));
}

#[test]
fn test_empty_name_rejected_lww() {
    let result = Field::new("", CType::LwwRegister);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::EmptyFieldName));
}

#[test]
fn test_empty_name_rejected_counter() {
    let result = Field::new("", CType::PnCounter);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), Error::EmptyFieldName));
}
