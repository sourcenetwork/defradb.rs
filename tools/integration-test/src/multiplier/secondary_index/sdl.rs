//! Adds `@index` to every indexable field of an SDL, as Go's `addIndexesToSDL`.

use rapidhash::{HashSetExt, RapidHashSet};

/// The built-in types an index can cover, as Go's `scalarTypes`.
const SCALARS: [&str; 9] = [
    "String", "Int", "Float", "Float32", "Float64", "Boolean", "DateTime", "ID", "JSON",
];

/// Whether `sdl` already declares an index, so the multiplier leaves it alone.
pub fn has_index_directive(sdl: &str) -> bool {
    sdl.contains("@index") || sdl.contains("@vectorIndex")
}

/// `sdl` with `@index` after the type of each scalar field and of each
/// relation field that holds the foreign key. One-to-one relations and their
/// `_<name>ID` fields stay as they are: the node already gives them a unique
/// index. An SDL that does not parse is returned unchanged.
pub fn add_indexes(sdl: &str) -> String {
    if graphql_parser::schema::parse_schema::<String>(sdl).is_err() {
        return sdl.to_owned();
    }
    let objects = objects(sdl);
    let defined: RapidHashSet<&str> = objects.iter().map(|o| o.name).collect();
    let one_to_one_ids = one_to_one_ids(&objects);

    let mut inserts: Vec<usize> = Vec::new();
    for object in &objects {
        for field in &object.fields {
            let indexed = if SCALARS.contains(&field.base) {
                !one_to_one_ids.contains(field.name)
            } else {
                !field.list
                    && defined.contains(field.base)
                    && !one_to_one(&objects, object.name, field.base)
            };
            if indexed {
                inserts.push(field.type_end);
            }
        }
    }

    let mut result = sdl.to_owned();
    inserts.sort_unstable();
    for at in inserts.into_iter().rev() {
        result.insert_str(at, " @index");
    }
    result
}

struct Object<'a> {
    name: &'a str,
    fields: Vec<Field<'a>>,
}

struct Field<'a> {
    name: &'a str,
    /// The named type under any list and non-null wrappers.
    base: &'a str,
    list: bool,
    /// The byte offset just past the field's type.
    type_end: usize,
}

impl Object<'_> {
    fn single_relations_to(&self, target: &str) -> usize {
        self.fields
            .iter()
            .filter(|f| !f.list && f.base == target)
            .count()
    }
}

/// Both sides hold one of the other. A type is one-to-one with itself when it
/// has exactly two single fields of its own type.
fn one_to_one(objects: &[Object<'_>], a: &str, b: &str) -> bool {
    let count = |from: &str, to: &str| {
        objects
            .iter()
            .find(|o| o.name == from)
            .map_or(0, |o| o.single_relations_to(to))
    };
    if a == b {
        return count(a, a) == 2;
    }
    count(a, b) > 0 && count(b, a) > 0
}

/// The `_<name>ID` field names of every one-to-one relation field.
fn one_to_one_ids(objects: &[Object<'_>]) -> RapidHashSet<String> {
    let mut ids = RapidHashSet::new();
    for object in objects {
        for field in &object.fields {
            if !field.list && one_to_one(objects, object.name, field.base) {
                ids.insert(format!("_{}ID", field.name));
            }
        }
    }
    ids
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Name,
    Punct(char),
    Str,
    Other,
}

#[derive(Clone, Copy)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Lexes `sdl` into tokens with byte spans, which `graphql_parser` does not
/// keep. Whitespace, commas and comments are dropped.
fn tokens(sdl: &str) -> Vec<Token> {
    let bytes = sdl.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let c = bytes[i];
        let kind = match c {
            b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                i += 1;
                continue;
            }
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'"' if sdl[i..].starts_with("\"\"\"") => {
                i += 3;
                while i < bytes.len() && !sdl[i..].starts_with("\"\"\"") {
                    i += if sdl[i..].starts_with("\\\"\"\"") {
                        4
                    } else {
                        1
                    };
                }
                i = (i + 3).min(bytes.len());
                Kind::Str
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                Kind::Str
            }
            b'_' | b'a'..=b'z' | b'A'..=b'Z' => {
                while i < bytes.len() && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                    i += 1;
                }
                Kind::Name
            }
            b'{' | b'}' | b'(' | b')' | b'[' | b']' | b':' | b'!' | b'@' => {
                i += 1;
                Kind::Punct(c as char)
            }
            _ => {
                i += sdl[i..].chars().next().map_or(1, char::len_utf8);
                Kind::Other
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: i,
        });
    }
    tokens
}

/// The object types of `sdl`, which must parse as a schema.
fn objects(sdl: &str) -> Vec<Object<'_>> {
    let mut parser = Parser {
        sdl,
        tokens: tokens(sdl),
        at: 0,
    };
    let mut objects = Vec::new();
    while let Some(token) = parser.peek() {
        let extended = parser.at > 0 && parser.text_at(parser.at - 1) == "extend";
        if token.kind == Kind::Name && parser.text(token) == "type" && !extended {
            parser.at += 1;
            if let Some(object) = parser.object() {
                objects.push(object);
            }
        } else if token.kind == Kind::Punct('{') {
            parser.skip_balanced('{', '}');
        } else {
            parser.at += 1;
        }
    }
    objects
}

struct Parser<'a> {
    sdl: &'a str,
    tokens: Vec<Token>,
    at: usize,
}

const DEFINITION_KEYWORDS: [&str; 9] = [
    "type",
    "input",
    "interface",
    "enum",
    "scalar",
    "union",
    "schema",
    "directive",
    "extend",
];

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).copied()
    }

    fn text(&self, token: Token) -> &'a str {
        &self.sdl[token.start..token.end]
    }

    fn text_at(&self, at: usize) -> &'a str {
        self.text(self.tokens[at])
    }

    fn is(&self, kind: Kind) -> bool {
        self.peek().is_some_and(|t| t.kind == kind)
    }

    fn skip_balanced(&mut self, open: char, close: char) {
        let mut depth = 0usize;
        while let Some(token) = self.peek() {
            self.at += 1;
            if token.kind == Kind::Punct(open) {
                depth += 1;
            } else if token.kind == Kind::Punct(close) {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
        }
    }

    /// `Name ... { fields }` after `type`; a type without a body has no fields.
    fn object(&mut self) -> Option<Object<'a>> {
        let name = self.peek().filter(|t| t.kind == Kind::Name)?;
        self.at += 1;
        let name = self.text(name);
        loop {
            let token = self.peek()?;
            match token.kind {
                Kind::Punct('{') => break,
                Kind::Punct('(') => self.skip_balanced('(', ')'),
                Kind::Name if DEFINITION_KEYWORDS.contains(&self.text(token)) => {
                    let after_at = self.text_at(self.at - 1) == "@";
                    if !after_at {
                        return Some(Object {
                            name,
                            fields: Vec::new(),
                        });
                    }
                    self.at += 1;
                }
                _ => self.at += 1,
            }
        }
        self.at += 1;
        let mut fields = Vec::new();
        while !self.is(Kind::Punct('}')) {
            fields.push(self.field()?);
        }
        self.at += 1;
        Some(Object { name, fields })
    }

    /// `["description"] name [(args)] : Type @directive(...)*`
    fn field(&mut self) -> Option<Field<'a>> {
        if self.is(Kind::Str) {
            self.at += 1;
        }
        let name = self.peek().filter(|t| t.kind == Kind::Name)?;
        self.at += 1;
        if self.is(Kind::Punct('(')) {
            self.skip_balanced('(', ')');
        }
        if !self.is(Kind::Punct(':')) {
            return None;
        }
        self.at += 1;
        let (base, list) = self.field_type()?;
        let type_end = self.tokens[self.at - 1].end;
        while self.is(Kind::Punct('@')) {
            self.at += 2;
            if self.is(Kind::Punct('(')) {
                self.skip_balanced('(', ')');
            }
        }
        Some(Field {
            name: self.text(name),
            base,
            list,
            type_end,
        })
    }

    /// `Name`, `[Type]`, each optionally `!`: the base name, and whether it is a list.
    fn field_type(&mut self) -> Option<(&'a str, bool)> {
        let token = self.peek()?;
        self.at += 1;
        let parsed = match token.kind {
            Kind::Name => (self.text(token), false),
            Kind::Punct('[') => {
                let (base, _) = self.field_type()?;
                if !self.is(Kind::Punct(']')) {
                    return None;
                }
                self.at += 1;
                (base, true)
            }
            _ => return None,
        };
        if self.is(Kind::Punct('!')) {
            self.at += 1;
        }
        Some(parsed)
    }
}
