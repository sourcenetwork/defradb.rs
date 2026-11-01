#!/usr/bin/env bun
// Counts testo scenario outcomes per binary for one client, and checks the
// known-failure lists.
//
//     bun testo-report.ts shard <junit.xml> <client> <known-failures.txt> <out.json>
//     bun testo-report.ts summary <dir of shard json files> <known-failures dir>
//
// A test whose failure output says `unsupported op:` stopped at an action the
// client does not implement yet (`Op::run`'s default, or an explicit
// `Failure::unsupported`). It is counted as unsupported, not as a failure.
//
// Each known-failures line is `<binary> <test> #<issue>`; a line starting with
// `#` is a comment. A shard fails when a test fails that is not listed, or when
// a listed test passes ("fixed: remove from the list"), so a list only shrinks.
// `summary` fails on a line without an issue or whose issue is closed, compares
// each list's size with the base branch's copy, and warns on every entry the
// pull request adds.

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
  binaries: Record<string, Counts>;
  unsupported_ops: Counts;
  unexpected: string[];
  fixed: string[];
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

function writeTable(binaries: Record<string, Counts>) {
  write(`| binary | ${COLUMNS.join(" | ")} |\n`);
  write(`|---|${"---:|".repeat(COLUMNS.length)}\n`);
  const total: Counts = {};
  for (const binary of Object.keys(binaries).sort()) {
    const row = binaries[binary];
    for (const [name, count] of Object.entries(row)) bump(total, name, count);
    write(`| ${binary} | ${COLUMNS.map((c) => row[c] ?? 0).join(" | ")} |\n`);
  }
  write(`| **total** | ${COLUMNS.map((c) => `**${total[c] ?? 0}**`).join(" | ")} |\n`);
}

function shard(junit: string, client: string, knownPath: string, out: string): number {
  const { known, bad } = readKnown(knownPath);
  for (const line of bad) console.log(`::error::${line}`);
  const binaries: Record<string, Counts> = {};
  const ops: Counts = {};
  const unexpected: string[] = [];
  const fixed: string[] = [];
  let knownSeen = 0;
  for (const testCase of testCases(readFileSync(junit, "utf8"))) {
    const binary = testCase.classname.split("::").at(-1) ?? "";
    const id = key(binary, testCase.name);
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
    binaries[binary] ??= {};
    bump(binaries[binary], result);
  }
  const report: Report = { client, binaries, unsupported_ops: ops, unexpected, fixed };
  writeFileSync(out, JSON.stringify(report, null, 1));
  write(`### ${client}\n\n`);
  writeTable(binaries);
  for (const [title, tests] of [
    ["Failing, not in the known-failures list", unexpected],
    ["fixed: remove from the list", fixed],
  ] as const) {
    if (tests.length) {
      write(`\n**${title}** (${tests.length})\n\n`);
      for (const test of [...tests].sort().slice(0, 200)) write(`- \`${test}\`\n`);
    }
  }
  flushSummary();
  for (const test of unexpected) console.log(`::error::${client}: ${test} failed and is not in ${knownPath}`);
  for (const test of fixed) console.log(`::error::${client}: ${test} passed. fixed: remove from the list ${knownPath}`);
  console.log(`${client}: ${unexpected.length} unexpected failures, ${knownSeen} known, ${fixed.length} fixed`);
  return unexpected.length || fixed.length || bad.length ? 1 : 0;
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

function checkLists(knownDir: string): number {
  let errors = 0;
  const notes: string[] = [];
  let ref = "main";
  write("## Known-failure lists\n\n| list | entries | base | change |\n|---|---:|---:|---:|\n");
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
    const change = known.size - base.known.size;
    write(`| ${client} | ${known.size} | ${base.known.size} | ${change >= 0 ? "+" : ""}${change} |\n`);
    for (const [id, issue] of entries) {
      if (!base.known.has(id)) {
        const note = `${pullRequest()} adds ${id} #${issue} to ${path}`;
        console.log(`::warning::${note}`);
        notes.push(note);
      }
    }
  }
  if (notes.length) {
    write(`\n**Added against ${ref}** (${notes.length})\n\n`);
    for (const note of notes.slice(0, 300)) write(`- ${note}\n`);
  }
  write("\n");
  return errors;
}

function* jsonFiles(directory: string): Generator<string> {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) yield* jsonFiles(path);
    else if (name.endsWith(".json")) yield path;
  }
}

function summary(directory: string, knownDir: string): number {
  const clients = new Map<string, { binaries: Record<string, Counts>; ops: Counts }>();
  for (const path of jsonFiles(directory)) {
    const report = JSON.parse(readFileSync(path, "utf8")) as Report;
    if (typeof report !== "object" || Array.isArray(report) || !report.client) continue;
    if (!clients.has(report.client)) clients.set(report.client, { binaries: {}, ops: {} });
    const merged = clients.get(report.client)!;
    for (const [binary, counts] of Object.entries(report.binaries)) {
      merged.binaries[binary] ??= {};
      for (const [name, count] of Object.entries(counts)) bump(merged.binaries[binary], name, count);
    }
    for (const [op, count] of Object.entries(report.unsupported_ops)) bump(merged.ops, op, count);
  }
  let errors = checkLists(knownDir);
  write("## Outcomes per client\n\n");
  write(
    "`fail` is unexpected, `known` is listed in `.github/testo/known-failures/<client>.txt`, " +
      "`unsupported` stopped at an action the client does not implement.\n\n",
  );
  for (const client of CLIENTS) {
    const merged = clients.get(client);
    if (!merged) {
      write(`### ${client}\n\nno report\n\n`);
      errors++;
      continue;
    }
    write(`### ${client}\n\n`);
    writeTable(merged.binaries);
    const ops = Object.entries(merged.ops)
      .sort(([, a], [, b]) => b - a)
      .slice(0, 15);
    if (ops.length) {
      write("\nMost common unsupported actions:\n\n");
      for (const [op, count] of ops) write(`- ${count} × \`${op}\`\n`);
    }
    write("\n");
  }
  flushSummary();
  return errors ? 1 : 0;
}

const [mode, ...args] = process.argv.slice(2);
if (mode === "shard") process.exit(shard(args[0], args[1], args[2], args[3]));
if (mode === "summary") process.exit(summary(args[0], args[1]));
console.error(`unknown mode ${mode}`);
process.exit(2);
