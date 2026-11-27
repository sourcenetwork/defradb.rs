#!/usr/bin/env python3
"""Fails when a new defra.h breaks code built against an older one.

Reads both headers through clang's AST and compares what a caller depends on:

- every function keeps its name, storage and signature;
- an options struct (a name ending in `Options`) only grows at its end: each
  older field keeps its place, name and type, `_reserved` fields included,
  since a reserved field never becomes a field;
- every other struct is returned or passed by value, so its size is part of
  the ABI and its fields stay exactly as they were;
- every typedef, enum constant and macro keeps its meaning; a macro may be
  respelled, as long as it evaluates to the same integer or has the same
  tokens once comments and enclosing parentheses are dropped; an enum
  constant counts whether or not its enum has a name;
- every exported function has C linkage when C++ includes the header, so a
  C++ caller links against the symbol the library exports, not a mangled one.

Anything added is allowed. Padding is the header compile's job: `just
check-ffi-header` builds it with -Wpadded.

Usage: check-c-api.py OLD_HEADER NEW_HEADER
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

OPTIONS_SUFFIX = "Options"
MACRO_RE = re.compile(r"^[ \t]*#[ \t]*define[ \t]+(\w+(?:\([^)]*\))?)[ \t]*(.*?)[ \t]*$", re.MULTILINE)
# One preprocessor token, or the whitespace or comment between two. A string or
# character literal is one token, its spaces included.
TOKEN_RE = re.compile(
    r"""\s+ | /\*.*?\*/ | //[^\n]*
    | (?:u8|u|U|L)?"(?:\\.|[^"\\\n])*"
    | (?:u8|u|U|L)?'(?:\\.|[^'\\\n])*'
    | \.?\d(?:[eEpP][+-]|[\w.])*
    | [A-Za-z_]\w*
    | <<= | >>= | \.\.\. | -> | \+\+ | -- | << | >> | <= | >= | == | != | && | \|\| | [-+*/%&|^]= | \#\#
    | .""",
    re.VERBOSE | re.DOTALL,
)
SKIP_RE = re.compile(r"\s+|/\*.*\*/|//.*", re.DOTALL)
KINDS = ("function", "struct", "typedef", "enum", "enum constant", "macro")


def clang_ast(header, language="c", standard="c11"):
    clang = os.environ.get("CLANG", "clang")
    command = [clang, "-x", language, f"-std={standard}", "-fsyntax-only", "-Xclang", "-ast-dump=json", str(header)]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"clang could not parse {header}:\n{done.stderr}")
    return json.loads(done.stdout)


def track_file(value, state):
    """Follows clang's JSON locations, which name a file only when it changes."""
    if isinstance(value, dict):
        if "file" in value:
            state["file"] = value["file"]
        for key, inner in value.items():
            if key != "includedFrom":
                track_file(inner, state)
    elif isinstance(value, list):
        for inner in value:
            track_file(inner, state)


def header_decls(ast, header):
    """The top-level declarations spelled in `header`, not in what it includes."""
    target = os.path.realpath(header)
    state = {"file": None}
    for node in ast.get("inner", []):
        track_file(node.get("loc", {}), state)
        if state["file"] and os.path.realpath(state["file"]) == target:
            yield node
        track_file({key: value for key, value in node.items() if key != "loc"}, state)


def enum_constants(node):
    constants = []
    value = -1
    for constant in node.get("inner", []):
        if constant.get("kind") != "EnumConstantDecl":
            continue
        explicit = next((inner["value"] for inner in constant.get("inner", []) if "value" in inner), None)
        value = int(explicit, 0) if explicit is not None else value + 1
        constants.append((constant["name"], value))
    return constants


def function_signature(node):
    storage = node.get("storageClass", "extern")
    inline = "inline " if node.get("inline") else ""
    return f"{storage} {inline}{node['type']['qualType']}"


def cxx_linkage(header):
    """The language linkage each exported function has when C++ includes `header`."""
    linkage = {}
    for node in header_decls(clang_ast(header, "c++", "c++17"), header):
        if node.get("kind") == "LinkageSpecDecl":
            functions, language = node.get("inner", []), node.get("language", "C++")
        else:
            functions, language = [node], "C++"
        for function in functions:
            if function.get("kind") == "FunctionDecl" and function.get("storageClass") != "static":
                linkage[function["name"]] = language
    return linkage


def api(header):
    """What a caller of `header` depends on, by kind and name."""
    model = {kind: {} for kind in KINDS}
    model["linkage"] = cxx_linkage(header)
    for node in header_decls(clang_ast(header), header):
        kind, name = node.get("kind"), node.get("name")
        if kind == "EnumDecl":
            model["enum constant"].update(enum_constants(node))
        if not name:
            continue
        if kind == "FunctionDecl":
            model["function"][name] = function_signature(node)
        elif kind == "RecordDecl" and node.get("completeDefinition"):
            fields = [(f["name"], f["type"]["qualType"]) for f in node.get("inner", []) if f.get("kind") == "FieldDecl"]
            model["struct"][name] = (node.get("tagUsed", "struct"), fields)
        elif kind == "TypedefDecl":
            model["typedef"][name] = node["type"]["qualType"]
        elif kind == "EnumDecl":
            model["enum"][name] = "enum"
    # Line splicing joins a backslash-newline's lines into one logical line, a macro's whole definition.
    text = re.sub(r"\\\r?\n", "", Path(header).read_text(encoding="utf-8"))
    model["macro"] = {name: value for name, value in MACRO_RE.findall(text)}
    model["header"] = str(header)
    return model


def macro_value(header, name):
    """The integer `name` evaluates to as a constant expression, or None when it is not one."""
    with tempfile.TemporaryDirectory() as work:
        probe = Path(work) / "probe.c"
        probe.write_text(f'#include "{os.path.realpath(header)}"\nenum {{ defra_macro_probe = ({name}) }};\n', encoding="utf-8")
        clang = os.environ.get("CLANG", "clang")
        command = [clang, "-x", "c", "-std=c11", "-fsyntax-only", "-Xclang", "-ast-dump=json", str(probe)]
        done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        return None
    enums = [node for node in json.loads(done.stdout).get("inner", []) if node.get("kind") == "EnumDecl"]
    constants = dict(enum_constants(enums[-1])) if enums else {}
    return constants.get("defra_macro_probe")


def encloses(tokens):
    """Whether the first token's parenthesis closes at the last token."""
    depth = 0
    for index, token in enumerate(tokens):
        depth += {"(": 1, ")": -1}.get(token, 0)
        if depth == 0:
            return index == len(tokens) - 1
    return False


def macro_tokens(text):
    """`text`'s preprocessor tokens, without comments or parentheses enclosing all of them."""
    tokens = [match.group() for match in TOKEN_RE.finditer(text) if not SKIP_RE.fullmatch(match.group())]
    while tokens and tokens[0] == "(" and encloses(tokens):
        tokens = tokens[1:-1]
    return tokens


def macro_equivalent(name, old, new):
    """Whether `name` means the same in both headers: the same integer, or the same tokens."""
    before, after = old["macro"][name], new["macro"][name]
    if before == after or macro_tokens(before) == macro_tokens(after):
        return True
    if "(" in name:
        return False
    old_value, new_value = macro_value(old["header"], name), macro_value(new["header"], name)
    return old_value is not None and old_value == new_value


def field(entry):
    return f"`{entry[1]} {entry[0]}`"


def struct_breaks(name, old, new):
    (old_tag, old_fields), (new_tag, new_fields) = old, new
    if old_tag != new_tag:
        return [f"{old_tag} {name} became a {new_tag}"]
    breaks = []
    for index, entry in enumerate(old_fields):
        if index >= len(new_fields):
            breaks.append(f"struct {name} lost field {index} {field(entry)}")
        elif new_fields[index] != entry:
            breaks.append(f"struct {name} field {index} {field(entry)} was replaced by {field(new_fields[index])}")
    if not name.endswith(OPTIONS_SUFFIX) and len(new_fields) > len(old_fields):
        added = ", ".join(field(entry) for entry in new_fields[len(old_fields):])
        breaks.append(f"struct {name} gained {added}: only an options struct may grow, and this one is passed by value")
    return breaks


def breaks(old, new):
    """Every change from `old` to `new` that breaks a caller built against `old`."""
    found = []
    for kind in KINDS:
        for name, before in old[kind].items():
            if name not in new[kind]:
                found.append(f"{kind} {name} was removed")
                continue
            after = new[kind][name]
            if kind == "struct":
                found += struct_breaks(name, before, after)
            elif kind == "macro":
                if not macro_equivalent(name, old, new):
                    found.append(f"macro {name} changed from `{before}` to `{after}`")
            elif before != after:
                found.append(f"{kind} {name} changed from `{before}` to `{after}`")
    for name, language in new["linkage"].items():
        if language != "C":
            found.append(
                f'function {name} has {language} linkage in C++: a C++ caller links against a mangled symbol '
                'the library does not export; declare it inside extern "C" (cbindgen\'s cpp_compat)'
            )
    return found


def main():
    parser = argparse.ArgumentParser(description="Fail when a new defra.h breaks callers of an older one.")
    parser.add_argument("old", help="the header callers were built against")
    parser.add_argument("new", help="the header this change produces")
    args = parser.parse_args()

    found = breaks(api(args.old), api(args.new))
    if not found:
        print("The C API is compatible: nothing removed or changed.")
        return 0
    for message in found:
        if os.environ.get("GITHUB_ACTIONS") == "true":
            print(f"::error title=C API break::{message}")
        else:
            print(f"error: {message}")
    print(f"{len(found)} breaking C API change(s). Append fields to options structs and add new calls instead.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
