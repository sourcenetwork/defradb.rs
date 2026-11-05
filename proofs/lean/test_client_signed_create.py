#!/usr/bin/env python3
"""Check model regressions; native implementation conformance is not yet bound."""

import copy
import json
import subprocess
from pathlib import Path


def main():
    base = {
        "capsule": {
            "actor": "did:key:alice", "transaction": "tx1", "operation": "op1",
            "session": "session1", "wave": 0, "offered": ["field-a", "field-b"],
            "live": True,
        },
        "intent": {"expected": ["field-a", "field-b"]},
        "reply": {
            "actor": "did:key:alice", "transaction": "tx1", "operation": "op1",
            "session": "session1", "wave": 0, "blocks": ["field-a", "field-b"],
            "keyMatchesActor": True, "signaturesValid": True,
        },
    }
    cases = [(base, True, True)]
    for field, value in [("actor", "did:key:bob"), ("transaction", "tx2"),
                         ("operation", "op2"), ("session", "session2"),
                         ("wave", 1), ("keyMatchesActor", False),
                         ("signaturesValid", False)]:
        case = copy.deepcopy(base)
        case["reply"][field] = value
        cases.append((case, False, True))
    for blocks in [["field-b", "field-a"], ["field-a"],
                   ["field-a", "field-b", "field-c"], ["field-a", "field-a"]]:
        case = copy.deepcopy(base)
        case["reply"]["blocks"] = blocks
        cases.append((case, False, False))
    expired = copy.deepcopy(base)
    expired["capsule"]["live"] = False
    cases.append((expired, False, True))
    malicious = copy.deepcopy(base)
    malicious["capsule"]["offered"] = ["other-field"]
    malicious["reply"]["blocks"] = ["other-field"]
    cases.append((malicious, True, False))
    composite = copy.deepcopy(base)
    composite["capsule"]["wave"] = composite["reply"]["wave"] = 1
    composite["capsule"]["offered"] = composite["intent"]["expected"] = ["composite"]
    composite["reply"]["blocks"] = ["composite"]
    cases.append((composite, True, True))
    result = subprocess.run(
        ["lake", "env", "lean", "--run", "ClientSignedCreate.lean"],
        cwd=Path(__file__).parent,
        input=json.dumps([case for case, _, _ in cases]) + "\n",
        text=True, capture_output=True, check=True,
    )
    observations = json.loads(result.stdout)
    assert len(observations) == len(cases)
    for (case, accepted, signs), observation in zip(cases, observations):
        assert observation["input"] == case
        assert observation["accepted"] is accepted, observation
        assert observation["clientSigns"] is signs, observation


if __name__ == "__main__":
    main()
