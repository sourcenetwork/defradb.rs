#!/usr/bin/env python3
"""Fails when a new defra.h breaks code built against an older one.

Reads both headers through clang's AST and compares what a caller depends on:

- every function keeps its name, storage and signature;
- an options struct (a name ending in `Options`) only grows at its end: each
  older field keeps its place, name and type, `_reserved` fields included,
  since a reserved field never becomes a field;
- every other struct is returned or passed by value, so its size is part of
  the ABI and its fields stay exactly as they were;
- every typedef, enum constant and macro keeps its meaning.

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

OPTIONS_SUFFIX = "Options"
MACRO_RE = re.compile(r"^[ \t]*#[ \t]*define[ \t]+(\w+(?:\([^)]*\))?)[ \t]*(.*?)[ \t]*$", re.MULTILINE)


def clang_ast(header):
    clang = os.environ.get("CLANG", "clang")
    command = [clang, "-x", "c", "-std=c11", "-fsyntax-only", "-Xclang", "-ast-dump=json", str(header)]
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


def api(header):
    """What a caller of `header` depends on, by kind and name."""
    model = {"function": {}, "struct": {}, "typedef": {}, "enum": {}, "macro": {}}
    for node in header_decls(clang_ast(header), header):
        kind, name = node.get("kind"), node.get("name")
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
            model["enum"][name] = enum_constants(node)
    text = Path(header).read_text(encoding="utf-8")
    model["macro"] = {name: value for name, value in MACRO_RE.findall(text)}
    return model


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


def enum_breaks(name, old, new):
    values = dict(new)
    breaks = []
    for constant, value in old:
        if constant not in values:
            breaks.append(f"enum {name} lost {constant}")
        elif values[constant] != value:
            breaks.append(f"enum {name} {constant} changed from {value} to {values[constant]}")
    return breaks


def breaks(old, new):
    """Every change from `old` to `new` that breaks a caller built against `old`."""
    found = []
    for kind in ("function", "struct", "typedef", "enum", "macro"):
        for name, before in old[kind].items():
            if name not in new[kind]:
                found.append(f"{kind} {name} was removed")
                continue
            after = new[kind][name]
            if kind == "struct":
                found += struct_breaks(name, before, after)
            elif kind == "enum":
                found += enum_breaks(name, before, after)
            elif before != after:
                found.append(f"{kind} {name} changed from `{before}` to `{after}`")
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
