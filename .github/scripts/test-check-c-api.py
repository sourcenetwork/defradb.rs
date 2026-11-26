#!/usr/bin/env python3
"""check-c-api.py against small headers; needs clang, nothing else."""
import contextlib
import importlib.util
import io
from pathlib import Path
import re
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("check_c_api", Path(__file__).with_name("check-c-api.py"))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)

BASE = """
#ifndef DEFRA_H
#define DEFRA_H
#include <stdint.h>
#define DEFRA_SIGN_RETRYABLE 2
typedef struct DefraResult {
  int status;
  uint32_t _reserved0;
  char *error;
} DefraResult;
typedef struct DefraThingOptions {
  uintptr_t identity;
  int flag;
  uint32_t _reserved0;
} DefraThingOptions;
typedef enum DefraKind { DEFRA_KIND_A, DEFRA_KIND_B = 4, DEFRA_KIND_C } DefraKind;
typedef void (*DefraResultCallback)(uintptr_t context, struct DefraResult result);
#ifdef __cplusplus
extern "C" {
#endif
struct DefraResult defra_thing_sized(uintptr_t target, const DefraThingOptions *opts, uint32_t opts_size);
#ifdef __cplusplus
}
#endif
static inline struct DefraResult defra_thing(uintptr_t target, DefraThingOptions opts) {
    return defra_thing_sized(target, &opts, (uint32_t)sizeof opts);
}
#endif
"""


class CheckCApi(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.dir.cleanup)

    def header(self, name, text):
        path = Path(self.dir.name) / name
        path.write_text(text, encoding="utf-8")
        return checker.api(path)

    def breaks(self, old, new):
        return checker.breaks(self.header("old.h", old), self.header("new.h", new))

    def changed(self, before, after):
        self.assertIn(before, BASE)
        return self.breaks(BASE, BASE.replace(before, after))

    def test_reads_only_the_header_not_what_it_includes(self):
        model = self.header("defra.h", BASE)
        self.assertEqual(set(model["function"]), {"defra_thing_sized", "defra_thing"})
        self.assertEqual(set(model["struct"]), {"DefraResult", "DefraThingOptions"})
        self.assertEqual(set(model["enum"]), {"DefraKind"})
        self.assertEqual(model["enum constant"], {"DEFRA_KIND_A": 0, "DEFRA_KIND_B": 4, "DEFRA_KIND_C": 5})
        self.assertEqual(model["macro"]["DEFRA_SIGN_RETRYABLE"], "2")

    def test_an_identical_header_is_compatible(self):
        self.assertEqual(self.breaks(BASE, BASE), [])

    def test_additions_are_compatible(self):
        grown = BASE.replace(
            "  uint32_t _reserved0;\n} DefraThingOptions;",
            "  uint32_t _reserved0;\n  const char *name;\n} DefraThingOptions;",
        ).replace(
            "uint32_t opts_size);\n",
            "uint32_t opts_size);\nvoid defra_new_call(uintptr_t target);\n",
        ).replace(
            "typedef void (*DefraResultCallback)",
            "#define DEFRA_NEW 1\ntypedef struct DefraOtherOptions { uintptr_t identity; } DefraOtherOptions;\n"
            "typedef void (*DefraResultCallback)",
        ).replace("DEFRA_KIND_C }", "DEFRA_KIND_C, DEFRA_KIND_D }")
        self.assertEqual(self.breaks(BASE, grown), [])

    def test_removing_an_options_field_breaks(self):
        found = self.changed("  int flag;\n  uint32_t _reserved0;\n} DefraThingOptions;", "  int flag;\n} DefraThingOptions;")
        self.assertEqual(found, ["struct DefraThingOptions lost field 2 `uint32_t _reserved0`"])

    def test_replacing_a_reserved_field_breaks(self):
        found = self.changed("  int flag;\n  uint32_t _reserved0;\n} DefraThingOptions;", "  int flag;\n  uint32_t limit;\n} DefraThingOptions;")
        self.assertEqual(found, ["struct DefraThingOptions field 2 `uint32_t _reserved0` was replaced by `uint32_t limit`"])

    def test_retyping_or_reordering_options_fields_breaks(self):
        self.assertTrue(self.changed("  int flag;\n  uint32_t _reserved0;\n} DefraThingOptions;", "  uint32_t flag;\n  uint32_t _reserved0;\n} DefraThingOptions;"))
        self.assertTrue(self.changed("  int flag;\n  uint32_t _reserved0;\n} DefraThingOptions;", "  uint32_t _reserved0;\n  int flag;\n} DefraThingOptions;"))

    def test_inserting_an_options_field_before_the_end_breaks(self):
        found = self.changed("  uintptr_t identity;\n  int flag;", "  uintptr_t identity;\n  const char *name;\n  int flag;")
        self.assertIn("struct DefraThingOptions field 1 `int flag` was replaced by `const char * name`", found)

    def test_growing_a_result_struct_breaks(self):
        found = self.changed("  char *error;\n} DefraResult;", "  char *error;\n  char *value;\n} DefraResult;")
        self.assertEqual(len(found), 1)
        self.assertIn("struct DefraResult gained `char * value`", found[0])

    def test_changing_or_removing_a_function_breaks(self):
        self.assertEqual(
            self.changed("uint32_t opts_size);", "uintptr_t opts_size);"),
            ["function defra_thing_sized changed from `extern struct DefraResult (uintptr_t, const DefraThingOptions *, uint32_t)` "
             "to `extern struct DefraResult (uintptr_t, const DefraThingOptions *, uintptr_t)`"],
        )
        found = self.changed("static inline struct DefraResult defra_thing(", "static inline struct DefraResult defra_thing_renamed(")
        self.assertEqual(found, ["function defra_thing was removed"])

    def test_renaming_a_parameter_is_compatible(self):
        self.assertEqual(self.changed("defra_thing_sized(uintptr_t target,", "defra_thing_sized(uintptr_t handle,"), [])

    def test_changing_a_callback_typedef_breaks(self):
        found = self.changed("(uintptr_t context, struct DefraResult result);", "(struct DefraResult result);")
        self.assertEqual(found, ["typedef DefraResultCallback changed from `void (*)(uintptr_t, struct DefraResult)` to `void (*)(struct DefraResult)`"])

    def test_reads_the_cxx_linkage_of_exported_functions(self):
        self.assertEqual(self.header("defra.h", BASE)["linkage"], {"defra_thing_sized": "C"})

    def test_dropping_the_extern_c_guards_breaks(self):
        unguarded = re.sub(r'#ifdef __cplusplus\n(extern "C" \{|\})\n#endif\n', "", BASE)
        self.assertNotIn("extern", unguarded)
        found = self.breaks(BASE, unguarded)
        self.assertEqual(len(found), 1)
        self.assertTrue(found[0].startswith("function defra_thing_sized has C++ linkage in C++"))

    def test_adding_a_function_outside_the_extern_c_guards_breaks(self):
        found = self.changed("static inline struct DefraResult defra_thing(", "void defra_new_call(void);\nstatic inline struct DefraResult defra_thing(")
        self.assertEqual(len(found), 1)
        self.assertTrue(found[0].startswith("function defra_new_call has C++ linkage in C++"))

    def test_respelling_a_macro_with_the_same_value_is_compatible(self):
        for spelling in ("(2)", "((2))", "( 2 )", "(1 + 1)", "(1 << 1)", "0x2", "((int32_t)2)"):
            with self.subTest(spelling=spelling):
                self.assertEqual(self.changed("#define DEFRA_SIGN_RETRYABLE 2", f"#define DEFRA_SIGN_RETRYABLE {spelling}"), [])

    def test_a_macro_with_a_new_value_or_no_value_breaks(self):
        for spelling in ("(3)", "(1) + (2)", '"2"', "2.0"):
            with self.subTest(spelling=spelling):
                self.assertEqual(
                    self.changed("#define DEFRA_SIGN_RETRYABLE 2", f"#define DEFRA_SIGN_RETRYABLE {spelling}"),
                    [f"macro DEFRA_SIGN_RETRYABLE changed from `2` to `{spelling}`"],
                )

    def test_a_non_integer_macro_compares_its_tokens(self):
        old = BASE.replace("#define DEFRA_SIGN_RETRYABLE 2", '#define DEFRA_NAME "defra"')
        self.assertEqual(self.breaks(old, old.replace('"defra"', '( "defra" )')), [])
        self.assertTrue(self.breaks(old, old.replace('"defra"', '"defradb"')))

    def test_spaces_inside_a_string_literal_are_part_of_the_macro(self):
        old = BASE.replace("#define DEFRA_SIGN_RETRYABLE 2", '#define DEFRA_NAME "defra db"')
        self.assertEqual(self.breaks(old, old.replace('"defra db"', '"defradb"')), ['macro DEFRA_NAME changed from `"defra db"` to `"defradb"`'])
        self.assertEqual(self.breaks(old, old.replace('"defra db"', "'d'")), ["macro DEFRA_NAME changed from `\"defra db\"` to `'d'`"])
        char = BASE.replace("#define DEFRA_SIGN_RETRYABLE 2", "#define DEFRA_SEP ' '")
        self.assertTrue(self.breaks(char, char.replace("' '", "'_'")))

    def test_a_multiline_macro_is_read_whole(self):
        old = BASE.replace("#define DEFRA_SIGN_RETRYABLE 2", "#define DEFRA_LIMIT (1 \\\n  + 1)")
        self.assertEqual(self.header("defra.h", old)["macro"]["DEFRA_LIMIT"], "(1   + 1)")
        self.assertEqual(self.breaks(old, old), [])
        self.assertEqual(self.breaks(old, old.replace("(1 \\\n  + 1)", "(1 + \\\n 1)")), [])
        self.assertEqual(self.breaks(old, old.replace("+ 1)", "+ 2)")), ["macro DEFRA_LIMIT changed from `(1   + 1)` to `(1   + 2)`"])

    def test_anonymous_enum_constants_are_compared(self):
        old = BASE.replace("typedef void (*DefraResultCallback)", "enum { DEFRA_FLAG = 4 };\ntypedef void (*DefraResultCallback)")
        self.assertEqual(self.header("defra.h", old)["enum constant"]["DEFRA_FLAG"], 4)
        self.assertEqual(self.breaks(old, old), [])
        self.assertEqual(self.breaks(old, old.replace("DEFRA_FLAG = 4", "DEFRA_FLAG = 5")), ["enum constant DEFRA_FLAG changed from `4` to `5`"])
        self.assertEqual(self.breaks(old, old.replace("enum { DEFRA_FLAG = 4 };\n", "")), ["enum constant DEFRA_FLAG was removed"])

    def test_changing_an_enum_or_a_macro_breaks(self):
        self.assertEqual(
            self.changed("DEFRA_KIND_B = 4", "DEFRA_KIND_B = 3"),
            ["enum constant DEFRA_KIND_B changed from `4` to `3`", "enum constant DEFRA_KIND_C changed from `5` to `4`"],
        )
        self.assertEqual(self.changed("#define DEFRA_SIGN_RETRYABLE 2", "#define DEFRA_SIGN_RETRYABLE 3"), ["macro DEFRA_SIGN_RETRYABLE changed from `2` to `3`"])

    def test_an_accepted_break_passes_and_still_names_each_break(self):
        old = Path(self.dir.name) / "old.h"
        new = Path(self.dir.name) / "new.h"
        old.write_text(BASE, encoding="utf-8")
        new.write_text(BASE.replace("  int flag;\n  uint32_t _reserved0;\n} DefraThingOptions;", "  int flag;\n} DefraThingOptions;"), encoding="utf-8")
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            refused = checker.main([str(old), str(new)])
            accepted = checker.main([str(old), str(new), "--accept-break"])
        self.assertEqual((refused, accepted), (1, 0))
        self.assertEqual(out.getvalue().count("lost field 2 `uint32_t _reserved0`"), 2)


if __name__ == "__main__":
    unittest.main()
