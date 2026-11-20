#!/usr/bin/env python3
"""check-c-api.py against small headers; needs clang, nothing else."""
import importlib.util
from pathlib import Path
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
struct DefraResult defra_thing_sized(uintptr_t target, const DefraThingOptions *opts, uint32_t opts_size);
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
        self.assertEqual(model["enum"]["DefraKind"], [("DEFRA_KIND_A", 0), ("DEFRA_KIND_B", 4), ("DEFRA_KIND_C", 5)])
        self.assertEqual(model["macro"]["DEFRA_SIGN_RETRYABLE"], "2")

    def test_an_identical_header_is_compatible(self):
        self.assertEqual(self.breaks(BASE, BASE), [])

    def test_additions_are_compatible(self):
        grown = BASE.replace(
            "  uint32_t _reserved0;\n} DefraThingOptions;",
            "  uint32_t _reserved0;\n  const char *name;\n} DefraThingOptions;",
        ).replace(
            "#endif",
            "#define DEFRA_NEW 1\nvoid defra_new_call(uintptr_t target);\n"
            "typedef struct DefraOtherOptions { uintptr_t identity; } DefraOtherOptions;\n#endif",
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

    def test_changing_an_enum_or_a_macro_breaks(self):
        self.assertEqual(self.changed("DEFRA_KIND_B = 4", "DEFRA_KIND_B = 3"), ["enum DefraKind DEFRA_KIND_B changed from 4 to 3", "enum DefraKind DEFRA_KIND_C changed from 5 to 4"])
        self.assertEqual(self.changed("#define DEFRA_SIGN_RETRYABLE 2", "#define DEFRA_SIGN_RETRYABLE 3"), ["macro DEFRA_SIGN_RETRYABLE changed from `2` to `3`"])


if __name__ == "__main__":
    unittest.main()
