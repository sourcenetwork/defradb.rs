#!/usr/bin/env python3
"""Compare two run documents and decide what actually regressed.

The hard part is not computing a percentage, it is refusing to report one that
means nothing. A run recorded while the host was busy can shift a figure by a
quarter with no code change, so a comparison that treats every delta as signal
produces alerts nobody trusts, and an alert nobody trusts is worse than none.

Three rules keep a verdict honest:

* **Both sides must be trusted.** If either run marked a family
  ``contaminated`` or ``absent``, the comparison is reported as ``unverified``
  and never as a pass or a regression.
* **Ranges must separate.** A row carries the ``min`` and ``max`` seen across
  repetitions. A delta counts only when the two ranges do not overlap: if the
  new ``max`` still reaches the old ``min``, the runs did not measure a
  difference, whatever their medians say. A row without a range skips this
  test, which is correct for a deterministic count and is why a bench that
  repeats a timing should always report one.
* **A floor on top of that.** Non-overlapping ranges can still be a fraction of
  a percent apart. A change within plus or minus ``--threshold`` percent is
  acceptable and reported as noise regardless, so at the default only a move of
  more than 5% either way is called a regression or an improvement.

Each side may be given more than once, and should be. A benchmark's own
confidence interval describes the spread *within* one measurement, which on a
shared runner is a fraction of the spread *between* two of them: the first
measurement in a job runs on a cold, idle machine and the last runs on a hot,
contended one, which is a systematic drift in one direction rather than noise
that averages out. Repeating each side and interleaving the passes is what makes
the recorded range mean "what this metric does on this runner", and only then
does a separation between two ranges mean the code changed.

Comparisons are per platform. A metric measured on Linux is not a baseline for
the same metric measured in a browser, and pretending otherwise would produce a
regression report on every run.
"""

import argparse
import json
import pathlib
import sys

PASS, REGRESSED, IMPROVED, NOISE, UNVERIFIED = (
    "pass",
    "regressed",
    "improved",
    "noise",
    "unverified",
)
TRUSTED = "clean"


def rows_of(doc, platform):
    """Every comparable row on one platform, keyed the way the dashboard keys
    them, carrying everything a verdict needs."""
    out = {}
    families = ((doc.get("platforms") or {}).get(platform) or {}).get("families") or {}
    for fname, fam in families.items():
        trust = (fam or {}).get("trust", "absent")
        for group in (fam or {}).get("groups") or []:
            unit = group.get("unit", "")
            lower = bool(group.get("lower_is_better"))
            for row in group.get("rows") or []:
                value = row.get("value")
                if not isinstance(value, (int, float)):
                    continue
                key = f"{fname}/{group.get('name')}/{row.get('name')}"
                out[key] = {
                    "value": float(value),
                    "min": row.get("min"),
                    "max": row.get("max"),
                    "unit": unit,
                    "lower": lower,
                    "trust": trust,
                    "family": fname,
                }
    return out


def fold(docs, platform):
    """One row set from repeated measurements of the same commit.

    The point estimate is the median across passes, and the range spans every
    pass: both the spread between them and each pass's own interval. Wider is
    the honest direction here, because every percent of range that is left out
    becomes a delta reported as real.
    """
    passes = [rows_of(doc, platform) for doc in docs]
    keys = set().union(*(set(p) for p in passes)) if passes else set()
    out = {}
    for key in keys:
        seen = [p[key] for p in passes if key in p]
        if not seen:
            continue
        values = sorted(r["value"] for r in seen)
        mid = len(values) // 2
        median = values[mid] if len(values) % 2 else (values[mid - 1] + values[mid]) / 2
        lows = [r["min"] if isinstance(r["min"], (int, float)) else r["value"] for r in seen]
        highs = [r["max"] if isinstance(r["max"], (int, float)) else r["value"] for r in seen]
        worst = max((r["trust"] for r in seen), key=lambda t: {"clean": 0}.get(t, 1))
        out[key] = {
            "value": median,
            "min": min(lows + values),
            "max": max(highs + values),
            "unit": seen[0]["unit"],
            "lower": seen[0]["lower"],
            "trust": worst,
            "family": seen[0]["family"],
            "passes": len(seen),
        }
    return out


def classify(before, after, threshold):
    """Verdict, percentage and reason for one pair of measurements."""
    if before["trust"] != TRUSTED:
        return UNVERIFIED, None, f"the baseline's {before['family']} is {before['trust']}"
    if after["trust"] != TRUSTED:
        return UNVERIFIED, None, f"this run's {after['family']} is {after['trust']}"
    if before["value"] == 0:
        return UNVERIFIED, None, "the baseline measured zero, so there is no ratio to take"

    pct = (after["value"] - before["value"]) / abs(before["value"]) * 100.0
    if before["lower"]:
        pct = -pct

    if abs(pct) <= threshold:
        return NOISE, pct, f"within the {threshold:g}% band, which is acceptable"

    have_ranges = all(
        isinstance(side[k], (int, float)) for side in (before, after) for k in ("min", "max")
    )
    if have_ranges and not (after["max"] < before["min"] or after["min"] > before["max"]):
        return (
            NOISE,
            pct,
            "the two runs' measured ranges overlap, so they did not measure a difference",
        )

    return (REGRESSED if pct < 0 else IMPROVED), pct, ""


def platforms_of(docs):
    seen = set()
    for d in docs:
        seen |= set(d.get("platforms") or {})
    return seen


def compare(bases, curs, threshold):
    platforms = sorted(platforms_of(bases) & platforms_of(curs))
    only_cur = sorted(platforms_of(curs) - platforms_of(bases))
    deltas = []
    for platform in platforms:
        b, c = fold(bases, platform), fold(curs, platform)
        for key in sorted(set(b) | set(c)):
            if key not in b or key not in c:
                deltas.append(
                    {
                        "platform": platform,
                        "key": key,
                        "verdict": UNVERIFIED,
                        "pct": None,
                        "reason": "only one of the two runs measured this",
                        "before": (b.get(key) or {}).get("value"),
                        "after": (c.get(key) or {}).get("value"),
                        "unit": (b.get(key) or c.get(key) or {}).get("unit", ""),
                    }
                )
                continue
            verdict, pct, reason = classify(b[key], c[key], threshold)
            deltas.append(
                {
                    "platform": platform,
                    "key": key,
                    "verdict": verdict,
                    "pct": pct,
                    "reason": reason,
                    "before": b[key]["value"],
                    "after": c[key]["value"],
                    "unit": c[key]["unit"],
                }
            )
    return deltas, platforms, only_cur


def fmt(v, unit):
    if not isinstance(v, (int, float)):
        return "n/a"
    if unit in ("B", "bytes"):
        for limit, suffix in ((1 << 30, "GiB"), (1 << 20, "MiB"), (1024, "KiB")):
            if abs(v) >= limit:
                return f"{v / limit:.2f} {suffix}"
        return f"{v:.0f} B"
    a = abs(v)
    if a >= 1e6:
        return f"{v / 1e6:.2f}M"
    if a >= 1e4:
        return f"{v / 1e3:.1f}k"
    if a >= 1:
        return f"{v:,.2f}".rstrip("0").rstrip(".")
    if a == 0:
        return "0"
    # Two decimals turn every sub-second timing into "0.01", which is the same
    # string for a value and the value it regressed from. Significant figures
    # keep the two distinguishable.
    return f"{v:.4g}"


def markdown(deltas, platforms, only_cur, base, cur, threshold, note="", passes=(1, 1)):
    regressed = [d for d in deltas if d["verdict"] == REGRESSED]
    improved = [d for d in deltas if d["verdict"] == IMPROVED]
    unverified = [d for d in deltas if d["verdict"] == UNVERIFIED]
    compared = [d for d in deltas if d["verdict"] != UNVERIFIED]

    out = ["## Performance", ""]
    out.append(
        f"`{(cur.get('commit') or '')[:12]}` ({cur.get('label') or 'this run'}) "
        f"against `{(base.get('commit') or '')[:12]}` ({base.get('label') or 'baseline'}). "
        f"Anything within plus or minus {threshold:g}% is acceptable; only a move past that "
        f"is reported, and only when the two sides' measured ranges do not overlap."
    )
    if max(passes) > 1:
        out.append("")
        out.append(
            f"Measured over {passes[1]} pass(es) of this run and {passes[0]} of the baseline, "
            "interleaved so that a runner getting slower as the job proceeds cannot read as a "
            "change in the code. Each side's range spans its passes."
        )
    if note:
        out.append("")
        out.append(note)
    out.append("")
    if not platforms:
        out.append(
            "The two runs share no platform, so nothing was compared. This is a gap in "
            "collection, not a result."
        )
        return "\n".join(out)
    out.append(f"Platforms compared: {', '.join(f'`{p}`' for p in platforms)}.")
    if only_cur:
        out.append(
            f"Measured only in this run, so not compared: {', '.join(f'`{p}`' for p in only_cur)}."
        )
    out.append("")

    def table(title, rows):
        if not rows:
            return []
        body = [f"### {title}", "", "| platform | metric | baseline | this run | change |", "|---|---|---:|---:|---:|"]
        for d in sorted(rows, key=lambda r: abs(r["pct"] or 0), reverse=True):
            body.append(
                f"| `{d['platform']}` | {d['key']} | {fmt(d['before'], d['unit'])} | "
                f"{fmt(d['after'], d['unit'])} | {d['pct']:+.1f}% |"
            )
        body.append("")
        return body

    out += table(f"Regressed ({len(regressed)})", regressed)
    out += table(f"Improved ({len(improved)})", improved)

    out.append(
        f"{len(compared)} metric(s) compared, {len(unverified)} not comparable."
    )
    if unverified:
        reasons = {}
        for d in unverified:
            reasons[d["reason"]] = reasons.get(d["reason"], 0) + 1
        out.append("")
        out.append("Not compared, and why:")
        for reason, n in sorted(reasons.items(), key=lambda kv: -kv[1]):
            out.append(f"- {n} × {reason}")
    out.append("")
    if regressed:
        out.append(f"**Verdict: regressed** on {len(regressed)} metric(s).")
    elif not compared:
        out.append(
            "**Verdict: unverified.** Nothing was comparable between these two runs, so this "
            "is not a pass."
        )
    else:
        out.append("**Verdict: no regression.**")
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--baseline",
        required=True,
        action="append",
        help="a run document for the baseline; repeat it once per measurement pass",
    )
    ap.add_argument(
        "--current",
        required=True,
        action="append",
        help="a run document for this run; repeat it once per measurement pass",
    )
    ap.add_argument(
        "--threshold",
        type=float,
        default=5.0,
        help="a change within plus or minus this percent is acceptable (default 5)",
    )
    ap.add_argument("--markdown", help="write the report here as well as to stdout")
    ap.add_argument(
        "--note",
        default="",
        help="what this comparison did and did not measure, printed with the report",
    )
    ap.add_argument("--fail-on-regression", action="store_true")
    args = ap.parse_args()

    bases = [json.loads(pathlib.Path(p).read_text()) for p in args.baseline]
    curs = [json.loads(pathlib.Path(p).read_text()) for p in args.current]
    deltas, platforms, only_cur = compare(bases, curs, args.threshold)
    report = markdown(
        deltas, platforms, only_cur, bases[0], curs[0], args.threshold, args.note,
        passes=(len(bases), len(curs)),
    )
    print(report)
    if args.markdown:
        pathlib.Path(args.markdown).write_text(report + "\n")

    regressed = [d for d in deltas if d["verdict"] == REGRESSED]
    if args.fail_on_regression and regressed:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
