//! The CRDT types one node has: the native ones, and the custom ones
//! registered on its builder.
//!
//! The set is built once, when the node is built, and never changes, so it
//! is a plain map behind an `Arc` that every holder (the store's merge
//! operator, the database, the query engine) shares by value.

use std::ops::Deref;
use std::sync::Arc;

use defra_core::block::CrdtDelta;
use rapidhash::{HashMapExt, RapidHashMap};
use schema::{CType, CrdtName};

use super::unregistered::UnregisteredField;
use super::TaggedCrdt;
use super::{native, native_delta, native_part, CausalCrdt, FieldCrdt, FieldCrdtError};
use crate::stored::ValueKind;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{ctype} is a native CRDT type; a registered type's ctype() is CType::Custom")]
    NotCustom { ctype: CType },
    #[error("the CRDT type '{name}' is registered twice")]
    Duplicate { name: CrdtName },
}

/// A custom CRDT type, ready to register.
pub struct Registration {
    crdt: Arc<dyn FieldCrdt>,
}

impl Registration {
    pub fn new<T>(crdt: T) -> Self
    where
        T: FieldCrdt + 'static,
    {
        Self {
            crdt: Arc::new(crdt),
        }
    }

    /// The name the type registers under, or why it cannot register.
    pub fn name(&self) -> Result<CrdtName, RegistryError> {
        match self.crdt.ctype() {
            CType::Custom(name) => Ok(name),
            ctype => Err(RegistryError::NotCustom { ctype }),
        }
    }
}

/// The CRDT types of one node. Cloning shares the same set.
#[derive(Clone, Default)]
pub struct Crdts {
    // A plain map rather than a `HopscotchMap`: it is built once and never
    // written again, so threads only ever read it through the shared `Arc`.
    custom: Arc<RapidHashMap<CrdtName, Arc<dyn FieldCrdt>>>,
}

impl std::fmt::Debug for Crdts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<_> = self.custom.keys().map(CrdtName::as_str).collect();
        names.sort_unstable();
        f.debug_struct("Crdts").field("custom", &names).finish()
    }
}

impl Crdts {
    /// The native types and `registrations`. A name registered twice, or a
    /// registration whose type is native, is refused.
    pub fn new<I>(registrations: I) -> Result<Self, RegistryError>
    where
        I: IntoIterator<Item = Registration>,
    {
        let mut custom = RapidHashMap::new();
        for registration in registrations {
            let name = registration.name()?;
            if custom.insert(name, registration.crdt).is_some() {
                return Err(RegistryError::Duplicate { name });
            }
        }
        Ok(Self {
            custom: Arc::new(custom),
        })
    }

    /// The names of the custom types this node has.
    pub fn custom_names(&self) -> impl Iterator<Item = CrdtName> + '_ {
        self.custom.keys().copied()
    }

    /// The registered type of `name`, if this node has it.
    pub fn registered(&self, name: CrdtName) -> Option<&dyn FieldCrdt> {
        self.custom.get(&name).map(Arc::as_ref)
    }

    /// The CRDT type a field declares, or `None` for a type with no field
    /// blocks (`None`, `Object`, `Composite`, unknown). A custom type this
    /// node has not registered is a stand-in that refuses every write,
    /// decode and merge with [`FieldCrdtError::Unregistered`], so no caller
    /// treats its field as another type.
    pub fn field(&self, ctype: CType) -> Option<CrdtRef<'_>> {
        match ctype {
            CType::Custom(name) => Some(self.custom(name)),
            ctype => native(ctype).map(CrdtRef::known),
        }
    }

    /// The CRDT type that wrote a field block, by its wire variant; a
    /// counter block takes the field's declared counter, positive unless it
    /// allows decrements. A block of another family than the declared type
    /// is refused, as Go refuses it: merging it would sum into a register or
    /// overwrite a counter. `CType::None` declares nothing, so any field
    /// block is read as its variant says. A custom block is read by the type
    /// it names, which must be the declared one and registered here
    /// ([`FieldCrdtError::Unregistered`] otherwise).
    pub fn delta(
        &self,
        delta: &CrdtDelta,
        declared: CType,
    ) -> Result<&dyn FieldCrdt, FieldCrdtError> {
        match native_delta(delta, declared)? {
            Ok(crdt) => Ok(crdt),
            Err(name) => self
                .registered(name)
                .ok_or(FieldCrdtError::Unregistered { crdt: name }),
        }
    }

    /// The CRDT type a stored part was written by, from its kind.
    pub fn part(&self, kind: ValueKind) -> CrdtRef<'_> {
        match (native_part(kind), kind) {
            (Some(native), _) => CrdtRef::known(native),
            (None, ValueKind::Custom(name)) => self.custom(name),
            (None, _) => unreachable!("every native kind has a native type"),
        }
    }

    /// Whether `ctype` is a custom type this node has not registered: its
    /// field has no state here, so a document without a value for it stores
    /// no part for it.
    pub fn is_unregistered(&self, ctype: CType) -> bool {
        matches!(ctype, CType::Custom(name) if self.registered(name).is_none())
    }

    fn custom(&self, name: CrdtName) -> CrdtRef<'_> {
        match self.registered(name) {
            Some(crdt) => CrdtRef::known(crdt),
            None => CrdtRef(Found::Unregistered(UnregisteredField::new(name))),
        }
    }
}

/// Finds the CRDT type of a stored part by its kind: what the document value
/// merges and reads its parts through.
pub trait PartCrdts {
    fn part_crdt(&self, kind: ValueKind) -> CrdtRef<'_>;
}

impl PartCrdts for Crdts {
    fn part_crdt(&self, kind: ValueKind) -> CrdtRef<'_> {
        self.part(kind)
    }
}

/// A CRDT type a lookup found: one the node has, or the stand-in for a
/// custom type it lacks. Used as a `&dyn FieldCrdt`.
#[derive(Clone, Copy)]
pub struct CrdtRef<'crdts>(Found<'crdts>);

#[derive(Clone, Copy)]
enum Found<'crdts> {
    Known(&'crdts dyn FieldCrdt),
    Unregistered(UnregisteredField),
}

impl<'crdts> CrdtRef<'crdts> {
    pub fn known(crdt: &'crdts dyn FieldCrdt) -> Self {
        Self(Found::Known(crdt))
    }

    /// The type, unless it is the stand-in for one the node lacks.
    pub fn registered(self) -> Option<&'crdts dyn FieldCrdt> {
        match self.0 {
            Found::Known(crdt) => Some(crdt),
            Found::Unregistered(_) => None,
        }
    }

    /// [`FieldCrdt::causal`], borrowed for as long as the set it was found in.
    pub fn causal(self) -> Option<&'crdts dyn CausalCrdt> {
        self.registered()?.causal()
    }

    /// [`FieldCrdt::tagged`], borrowed for as long as the set it was found in.
    pub fn tagged(self) -> Option<&'crdts dyn TaggedCrdt> {
        self.registered()?.tagged()
    }
}

impl<'crdts> Deref for CrdtRef<'crdts> {
    type Target = dyn FieldCrdt + 'crdts;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Found::Known(crdt) => *crdt,
            Found::Unregistered(stand_in) => stand_in,
        }
    }
}
