#!/usr/bin/env bun
// Counts testo scenario outcomes per area for one client, and checks the
// known-failure lists.
//
//     bun testo-report.ts shard <junit.xml> <list.json> <client> <shard>/<shards> <known-failures.txt> <out.json>
//     bun testo-report.ts summary <dir of shard json files> <known-failures dir>
//
// `<list.json>` is `cargo nextest list --message-format json` for the shard's
// selection: the tests it was meant to run, which JUnit alone cannot say,
// since it leaves out skipped tests.
//
// A test whose failure output says `unsupported op:` stopped at an action the
// client does not implement yet (`Op::run`'s default, or an explicit
// `Failure::unsupported`). It is counted as unsupported, not as a failure.
//
// Each known-failures line is `<binary> <test> #<issue>`; a line starting with
// `#` is a comment. A shard fails when a test fails that is not listed, when a
// listed test passes ("fixed: remove from the list"), so a list only shrinks,
// or when a listed test it selected left no result. `summary` fails on a line
// without an issue or whose issue is closed, on a test a shard reported as an
// unexpected failure or as selected but left no result, on a listed test no
// shard selected (renamed, removed or filtered out: "missing"), and on a
// client with a shard that never reported. It warns on a listed test that
// passed ("fixed"), compares each list's size with the base branch's copy,
// and warns on every entry the pull request adds.

import { appendFileSync, existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const UNSUPPORTED = /unsupported op: ([^\n\r]*)/;
const ENTRY = /^(\S+)\s+(\S+)\s+#(\d+)\s*$/;
const CLIENTS = ["in-process", "rust-http", "go-http", "cli", "legacy"];
const COLUMNS = ["pass", "fail", "known", "unsupported"];

type Counts = Record<string, number>;
type Known = Map<string, number>;
interface Report {
  client: string;
  shard: number;
  shards: number;
  binaries: Record<string, Counts>;
  unsupported_ops: Counts;
  unexpected: string[];
  fixed: string[];
  missing: string[];
  /// The listed tests this shard selected, so `summary` can find the ones no shard did.
  selected_known: string[];
}

/// The `<binary> <test>` ids a `cargo nextest list --message-format json`
/// output selects.
function selectedTests(listJson: string): Set<string> {
  const listing = JSON.parse(listJson) as {
    "rust-suites": Record<string, { "binary-id": string; testcases: Record<string, { "filter-match": { status: string } }> }>;
  };
  const selected = new Set<string>();
  for (const suite of Object.values(listing["rust-suites"] ?? {})) {
    const binary = suite["binary-id"].split("::").at(-1) ?? "";
    for (const [name, testCase] of Object.entries(suite.testcases ?? {})) {
      if (testCase["filter-match"]?.status === "matches") selected.add(key(binary, name));
    }
  }
  return selected;
}

const summaryLines: string[] = [];
const write = (text: string) => summaryLines.push(text);
const flushSummary = () => {
  const path = process.env.GITHUB_STEP_SUMMARY;
  if (path) appendFileSync(path, summaryLines.join(""));
};

const key = (binary: string, test: string) => `${binary} ${test}`;

function parseKnown(text: string, source: string): { known: Known; bad: string[] } {
  const known: Known = new Map();
  const bad: string[] = [];
  text.split("\n").forEach((raw, index) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const match = ENTRY.exec(line);
    if (match) known.set(key(match[1], match[2]), Number(match[3]));
    else bad.push(`${source}:${index + 1}: expected \`<binary> <test> #<issue>\`, got \`${line}\``);
  });
  return { known, bad };
}

function readKnown(path: string) {
  return parseKnown(existsSync(path) ? readFileSync(path, "utf8") : "", path);
}

function unescapeXml(text: string): string {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#(\d+);/g, (_, code) => String.fromCodePoint(Number(code)))
    .replace(/&#x([0-9a-f]+);/gi, (_, code) => String.fromCodePoint(parseInt(code, 16)))
    .replace(/&amp;/g, "&");
}

interface TestCase {
  classname: string;
  name: string;
  body: string;
}

function* testCases(xml: string): Generator<TestCase> {
  const pattern = /<testcase\b([^>]*?)(?:\/>|>([\s\S]*?)<\/testcase>)/g;
  for (const match of xml.matchAll(pattern)) {
    const attributes: Record<string, string> = {};
    for (const [, name, value] of match[1].matchAll(/([\w:-]+)="([^"]*)"/g)) {
      attributes[name] = unescapeXml(value);
    }
    yield { classname: attributes.classname ?? "", name: attributes.name ?? "", body: match[2] ?? "" };
  }
}

function outcome(body: string): [string, string | null] {
  if (/<skipped\b/.test(body)) return ["skipped", null];
  if (!/<(failure|error)\b/.test(body)) return ["pass", null];
  const match = UNSUPPORTED.exec(unescapeXml(body));
  if (match) return ["unsupported", match[1].split(" {")[0].trim().slice(0, 100)];
  return ["fail", null];
}

const bump = (counts: Counts, name: string, by = 1) => {
  counts[name] = (counts[name] ?? 0) + by;
};

function totals(binaries: Record<string, Counts>): Counts {
  const total: Counts = {};
  for (const row of Object.values(binaries)) for (const [name, count] of Object.entries(row)) bump(total, name, count);
  return total;
}

function writeTable(binaries: Record<string, Counts>) {
  write(`| area | ${COLUMNS.join(" | ")} |\n`);
  write(`|---|${"---:|".repeat(COLUMNS.length)}\n`);
  for (const binary of Object.keys(binaries).sort()) {
    const row = binaries[binary];
    write(`| ${binary} | ${COLUMNS.map((c) => row[c] ?? 0).join(" | ")} |\n`);
  }
  const total = totals(binaries);
  write(`| **total** | ${COLUMNS.map((c) => `**${total[c] ?? 0}**`).join(" | ")} |\n`);
}

/// A collapsed section; GitHub needs the blank line after `<summary>` to render the markdown inside.
function writeDetails(title: string, open: boolean, body: () => void) {
  write(`<details${open ? " open" : ""}><summary>${title}</summary>\n\n`);
  body();
  write("\n</details>\n\n");
}

function shard(
  junit: string,
  list: string,
  client: string,
  part: string,
  knownPath: string,
  out: string,
): number {
  const { known, bad } = readKnown(knownPath);
  for (const line of bad) console.log(`::error::${line}`);
  const [shardIndex, shards] = part.split("/").map(Number);
  const selected = selectedTests(readFileSync(list, "utf8"));
  const binaries: Record<string, Counts> = {};
  const ops: Counts = {};
  const unexpected: string[] = [];
  const fixed: string[] = [];
  const reported = new Set<string>();
  let knownSeen = 0;
  for (const testCase of testCases(readFileSync(junit, "utf8"))) {
    const binary = testCase.classname.split("::").at(-1) ?? "";
    const id = key(binary, testCase.name);
    reported.add(id);
    let [result, op] = outcome(testCase.body);
    const listed = known.has(id);
    if (op) bump(ops, op);
    if (result === "fail" && listed) {
      result = "known";
      knownSeen++;
    } else if (result === "fail") {
      unexpected.push(id);
    } else if (listed && result === "pass") {
      fixed.push(`${id} #${known.get(id)}`);
    }
    const area = `${binary === "testo" ? "" : `${binary} `}${testCase.name.split("::")[0]}`;
    binaries[area] ??= {};
    bump(binaries[area], result);
  }
  const selectedKnown = [...known.keys()].filter((id) => selected.has(id));
  const missing = selectedKnown.filter((id) => !reported.has(id)).map((id) => `${id} #${known.get(id)}`);
  const report: Report = {
    client,
    shard: shardIndex,
    shards,
    binaries,
    unsupported_ops: ops,
    unexpected,
    fixed,
    missing,
    selected_known: selectedKnown,
  };
  writeFileSync(out, JSON.stringify(report, null, 1));
  write(`### ${client}, shard ${part}\n\n`);
  writeTable(binaries);
  for (const [title, tests] of [
    ["Failing, not in the known-failures list", unexpected],
    ["fixed: remove from the list", fixed],
    ["missing: listed and selected, but no result", missing],
  ] as const) {
    if (tests.length) {
      write(`\n**${title}** (${tests.length})\n\n`);
      for (const test of [...tests].sort().slice(0, 200)) write(`- \`${test}\`\n`);
    }
  }
  flushSummary();
  for (const test of unexpected) console.log(`::error::${client}: ${test} failed and is not in ${knownPath}`);
  for (const test of fixed) console.log(`::error::${client}: ${test} passed. fixed: remove from the list ${knownPath}`);
  for (const test of missing) console.log(`::error::${client}: ${test} was selected but left no result. missing`);
  console.log(
    `${client} ${part}: ${unexpected.length} unexpected failures, ${knownSeen} known, ` +
      `${fixed.length} fixed, ${missing.length} missing`,
  );
  return unexpected.length || fixed.length || missing.length || bad.length ? 1 : 0;
}

function gh(...args: string[]) {
  const result = Bun.spawnSync(["gh", ...args], { stdout: "pipe", stderr: "pipe" });
  return { ok: result.exitCode === 0, stdout: result.stdout.toString(), stderr: result.stderr.toString() };
}

const repository = () => process.env.GITHUB_REPOSITORY ?? "sourcenetwork/defradb.rs";

const states = new Map<number, string>();
function issueState(issue: number): string {
  if (!states.has(issue)) {
    const result = gh("issue", "view", String(issue), "--repo", repository(), "--json", "state", "--jq", ".state");
    states.set(issue, result.ok ? result.stdout.trim() : `unreadable (${result.stderr.trim()})`);
  }
  return states.get(issue)!;
}

function baseKnown(path: string): { known: Known; ref: string } {
  const ref = process.env.GITHUB_BASE_REF || "main";
  const result = gh(
    "api",
    "-H",
    "Accept: application/vnd.github.raw",
    `repos/${repository()}/contents/${path}?ref=${ref}`,
  );
  return { known: result.ok ? parseKnown(result.stdout, `${ref}:${path}`).known : new Map(), ref };
}

function pullRequest(): string {
  const match = /^refs\/pull\/(\d+)\//.exec(process.env.GITHUB_REF ?? "");
  return match ? `#${match[1]}` : process.env.GITHUB_REF_NAME ?? "this change";
}

interface Lists {
  errors: number;
  /// Per client: entries in its list here and on the base branch.
  sizes: Map<string, { entries: number; base: number }>;
  notes: string[];
  ref: string;
}

function checkLists(knownDir: string): Lists {
  let errors = 0;
  const notes: string[] = [];
  const sizes = new Map<string, { entries: number; base: number }>();
  let ref = "main";
  for (const client of CLIENTS) {
    const path = join(knownDir, `${client}.txt`);
    const { known, bad } = readKnown(path);
    for (const line of bad) {
      console.log(`::error::${line}`);
      errors++;
    }
    const entries = [...known.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    for (const [id, issue] of entries) {
      const state = issueState(issue);
      if (state !== "OPEN") {
        console.log(`::error::${path}: ${id} names #${issue}, which is ${state.toLowerCase()}`);
        errors++;
      }
    }
    const base = baseKnown(path);
    ref = base.ref;
    sizes.set(client, { entries: known.size, base: base.known.size });
    for (const [id, issue] of entries) {
      if (!base.known.has(id)) {
        const note = `${pullRequest()} adds ${id} #${issue} to ${path}`;
        console.log(`::warning::${note}`);
        notes.push(note);
      }
    }
  }
  return { errors, sizes, notes, ref };
}

function* jsonFiles(directory: string): Generator<string> {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) yield* jsonFiles(path);
    else if (name.endsWith(".json")) yield path;
  }
}

interface Merged {
  binaries: Record<string, Counts>;
  ops: Counts;
  shards: number;
  reported: Set<number>;
  selectedKnown: Set<string>;
  /// Failing tests no known-failures list names, as `<binary> <test>`.
  unexpected: string[];
  /// Listed tests a shard selected but got no result for, and listed tests
  /// that passed, as `<binary> <test> #<issue>`.
  missing: string[];
  fixed: string[];
}

/// Why a client's totals are partial, or `null` when every shard reported.
function incomplete(merged: Merged): string | null {
  const absent = Array.from({ length: merged.shards }, (_, i) => i + 1).filter((i) => !merged.reported.has(i));
  return absent.length ? `incomplete: shard ${absent.join(", ")} of ${merged.shards} never reported` : null;
}

/// Listed tests that no shard of a fully reported client selected: renamed,
/// removed or filtered out, so the entry checks nothing.
function checkMissing(clients: Map<string, Merged>, knownDir: string): number {
  let errors = 0;
  for (const [client, merged] of clients) {
    if (incomplete(merged)) continue;
    const path = join(knownDir, `${client}.txt`);
    for (const [id, issue] of readKnown(path).known) {
      if (!merged.selectedKnown.has(id)) {
        console.log(`::error::${path}: ${id} #${issue} matches no selected test. missing: remove or rename it`);
        errors++;
      }
    }
  }
  return errors;
}

function summary(directory: string, knownDir: string): number {
  const clients = new Map<string, Merged>();
  for (const path of jsonFiles(directory)) {
    const report = JSON.parse(readFileSync(path, "utf8")) as Report;
    if (typeof report !== "object" || Array.isArray(report) || !report.client) continue;
    if (!clients.has(report.client)) {
      clients.set(report.client, {
        binaries: {},
        ops: {},
        shards: report.shards,
        reported: new Set(),
        selectedKnown: new Set(),
        unexpected: [],
        missing: [],
        fixed: [],
      });
    }
    const merged = clients.get(report.client)!;
    merged.shards = Math.max(merged.shards, report.shards);
    merged.reported.add(report.shard);
    for (const id of report.selected_known) merged.selectedKnown.add(id);
    merged.unexpected.push(...report.unexpected);
    merged.missing.push(...report.missing);
    merged.fixed.push(...report.fixed);
    for (const [binary, counts] of Object.entries(report.binaries)) {
      merged.binaries[binary] ??= {};
      for (const [name, count] of Object.entries(counts)) bump(merged.binaries[binary], name, count);
    }
    for (const [op, count] of Object.entries(report.unsupported_ops)) bump(merged.ops, op, count);
  }
  const lists = checkLists(knownDir);
  let errors = lists.errors + checkMissing(clients, knownDir);
  // A known failure is fine; a failure no list names fails the summary, the
  // one testo check the merge queue requires.
  for (const [client, merged] of clients) {
    for (const test of merged.unexpected) console.log(`::error::${client}: ${test} failed and is not in its known-failures list`);
    for (const test of merged.missing) console.log(`::error::${client}: ${test} was selected but left no result`);
    errors += merged.unexpected.length + merged.missing.length;
    // A known failure that passed fails its shard, but not the summary: a
    // flip must not block the queue. The warning keeps the list shrinking.
    for (const test of merged.fixed) console.log(`::warning::${client}: ${test} passed. fixed: remove it from its known-failures list`);
  }
  write(
    "`fail` is unexpected, `known` is listed in `.github/testo/known-failures/<client>.txt`, " +
      "`unsupported` stopped at an action the client does not implement; `list` is that file's size " +
      `and its change against \`${lists.ref}\`.\n\n`,
  );
  write(`| client | ${COLUMNS.join(" | ")} | list |\n|---|${"---:|".repeat(COLUMNS.length + 1)}\n`);
  // A pull request runs only some clients; a client it did not run is left out.
  const expected = process.env.TESTO_CLIENTS ? process.env.TESTO_CLIENTS.split(",") : CLIENTS;
  for (const client of CLIENTS) {
    if (!expected.includes(client) && !clients.has(client)) continue;
    const size = lists.sizes.get(client);
    const change = size ? size.entries - size.base : 0;
    const list = size ? `${size.entries} (${change >= 0 ? "+" : ""}${change})` : "—";
    const merged = clients.get(client);
    const partial = merged ? incomplete(merged) : "no report";
    if (partial) {
      console.log(`::error::${client}: ${partial}`);
      errors++;
    }
    const total = merged ? totals(merged.binaries) : {};
    const cells = COLUMNS.map((c) => (c === "fail" && total[c] ? `**${total[c]}**` : (total[c] ?? 0)));
    write(`| ${client}${partial ? ` ⚠️ ${partial}` : ""} | ${cells.join(" | ")} | ${list} |\n`);
  }
  write("\n");
  if (lists.notes.length) {
    writeDetails(`Added to the lists against <code>${lists.ref}</code> (${lists.notes.length})`, false, () => {
      for (const note of lists.notes.slice(0, 300)) write(`- ${note}\n`);
    });
  }
  for (const client of CLIENTS) {
    const merged = clients.get(client);
    if (!merged) continue;
    writeDetails(`${client}: per area`, (totals(merged.binaries).fail ?? 0) > 0, () => {
      writeTable(merged.binaries);
      const ops = Object.entries(merged.ops)
        .sort(([, a], [, b]) => b - a)
        .slice(0, 15);
      if (ops.length) {
        write("\nMost common unsupported actions:\n\n");
        for (const [op, count] of ops) write(`- ${count} × \`${op}\`\n`);
      }
    });
  }
  flushSummary();
  return errors ? 1 : 0;
}

const [mode, ...args] = process.argv.slice(2);
if (mode === "shard") process.exit(shard(args[0], args[1], args[2], args[3], args[4], args[5]));
if (mode === "summary") process.exit(summary(args[0], args[1]));
console.error(`unknown mode ${mode}`);
process.exit(2);
