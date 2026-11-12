#!/usr/bin/env node
// Unified, machine-readable API compatibility report of the Blitz probe.
//
// Combines the declared implementation state in manifest.json with the
// ROVES_PROBE_RESULT evidence of one CI run (fixture, template, story, window
// and WebGPU diagnostic logs). No dependencies: Node >= 18 built-ins only.
//
// Usage:
//   node report.mjs --logs <dir> [--out <file>] [--mode normal|jitless]
//                   [--template-sha <sha>] [--github]
//   node report.mjs --validate [--sources <probe src dir>] [--template <template root>]
//
// Status of an API (first rule that applies):
//   missing             implementation "missing" (declared absent from the probe)
//   failed              an evidence check ran and did not pass
//   unavailable         evidence not produced in this run (log absent, check not
//                       run, no adapter) or implementation "unknown"
//   present_incomplete  implementation "present_incomplete"; its evidence, if any, passed
//   verified            implementation "present" and every evidence check passed
// A template path is "missing" when a required API or path is missing, then
// failed/unavailable from its own end-to-end evidence (none = unavailable,
// "no end-to-end harness"), else verified.
// The report never changes a CI gate: it only fails on an invalid manifest.

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const IMPLEMENTATIONS = new Set(["present", "present_incomplete", "missing", "unknown"]);
const NOT_RUN = new Set(["not_run", "no_webgpu", "no_adapter", "skipped"]);
const STATUSES = ["verified", "present_incomplete", "missing", "unavailable", "failed"];
const PRIORITY_ORDER = ["P0", "P1", "P2", "P3", "done"];
const RESULT_PREFIX = "ROVES_PROBE_RESULT ";

function parseArgs(argv) {
  const args = { github: false, validate: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) throw new Error(`${arg} needs a value`);
      return argv[++i];
    };
    if (arg === "--logs") args.logs = value();
    else if (arg === "--out") args.out = value();
    else if (arg === "--mode") args.mode = value();
    else if (arg === "--template-sha") args.templateSha = value();
    else if (arg === "--manifest") args.manifest = value();
    else if (arg === "--sources") args.sources = value();
    else if (arg === "--template") args.template = value();
    else if (arg === "--github") args.github = true;
    else if (arg === "--validate") args.validate = true;
    else throw new Error(`unknown argument ${arg}`);
  }
  return args;
}

// Template files the audit cites that no longer exist in a template checkout:
// a warning (the CI builds the template's moving main), never an error.
export function templateWarnings(manifest, templateRoot) {
  const warnings = [];
  for (const path of manifest.template_paths ?? []) {
    for (const source of path.sources ?? []) {
      if (!existsSync(join(templateRoot, source))) warnings.push(`path ${path.id}: template file ${source} not found (audit may be stale)`);
    }
  }
  return warnings;
}

// Structural checks; every error is fatal (the manifest is ours, not CI output).
export function validateManifest(manifest, { sourceChecks } = {}) {
  const errors = [];
  const logs = manifest.logs ?? {};
  const evidence = manifest.evidence ?? {};
  for (const [key, spec] of Object.entries(evidence)) {
    if (!logs[spec.log]) errors.push(`evidence ${key}: unknown log "${spec.log}"`);
    if (typeof spec.check !== "string") errors.push(`evidence ${key}: check id missing`);
    else if (sourceChecks && !sourceChecks.has(spec.check)) {
      errors.push(`evidence ${key}: check "${spec.check}" is not printed by any probe source`);
    }
  }
  const apiIds = new Set();
  for (const api of manifest.apis ?? []) {
    if (apiIds.has(api.id)) errors.push(`api ${api.id}: duplicate id`);
    apiIds.add(api.id);
    if (!IMPLEMENTATIONS.has(api.implementation)) {
      errors.push(`api ${api.id}: implementation "${api.implementation}" not in ${[...IMPLEMENTATIONS]}`);
    }
    for (const key of api.evidence ?? []) {
      if (!evidence[key]) errors.push(`api ${api.id}: unknown evidence "${key}"`);
    }
    if (api.implementation === "present" && !(api.evidence ?? []).length) {
      errors.push(`api ${api.id}: "present" needs at least one evidence check`);
    }
    if (api.implementation === "missing" && (api.evidence ?? []).length) {
      errors.push(`api ${api.id}: "missing" cannot list evidence`);
    }
  }
  const pathIds = new Set((manifest.template_paths ?? []).map((path) => path.id));
  for (const path of manifest.template_paths ?? []) {
    if (!PRIORITY_ORDER.includes(path.priority)) errors.push(`path ${path.id}: priority "${path.priority}"`);
    for (const id of [...(path.requires ?? []), ...(path.optional ?? [])]) {
      if (!apiIds.has(id) && !pathIds.has(id)) errors.push(`path ${path.id}: unknown requirement "${id}"`);
    }
    for (const key of path.evidence ?? []) {
      if (!evidence[key]) errors.push(`path ${path.id}: unknown evidence "${key}"`);
    }
  }
  return errors;
}

// Check ids printed by the probe sources (`\"check\":\"<id>\"` in Rust, `check: "<id>"` in JS).
export function sourceCheckIds(dir) {
  const ids = new Set();
  for (const name of readdirSync(dir)) {
    if (!/\.(rs|js)$/.test(name)) continue;
    const text = readFileSync(join(dir, name), "utf8");
    for (const match of text.matchAll(/\\"check\\":\\"([a-z0-9_]+)\\"|"check":"([a-z0-9_]+)"|check: "([a-z0-9_]+)"/g)) {
      ids.add(match[1] ?? match[2] ?? match[3]);
    }
  }
  return ids;
}

// All ROVES_PROBE_RESULT objects of one log, in order; unparsable lines counted.
export function parseResults(text) {
  const results = [];
  let unparsable = 0;
  for (const line of text.split(/\r?\n/)) {
    const at = line.indexOf(RESULT_PREFIX);
    if (at < 0) continue;
    try {
      results.push(JSON.parse(line.slice(at + RESULT_PREFIX.length)));
    } catch {
      unparsable++;
    }
  }
  return { results, unparsable, panicked: /panicked/.test(text), stages: stagesOf(text) };
}

function stagesOf(text) {
  return [...text.matchAll(/ROVES_PROBE_STAGE (\S+)/g)].map((match) => match[1]);
}

function valueAt(object, path) {
  return path.split(".").reduce((value, key) => (value == null ? undefined : value[key]), object);
}

function sameValue(a, b) {
  return JSON.stringify(a) === JSON.stringify(b);
}

// One evidence item against the parsed logs: passed / failed / unavailable.
export function evaluateEvidence(key, spec, logs) {
  const log = logs[spec.log];
  const base = { key, check: spec.check, log: spec.log };
  if (spec.stage) base.stage = spec.stage;
  if (!log) return { ...base, status: "unavailable", reason: "log not produced in this run" };
  const lines = log.results.filter((result) => result.check === spec.check);
  if (!lines.length) {
    if (log.panicked) {
      const last = log.stages.at(-1);
      return { ...base, status: "failed", reason: `no result line; the run panicked${last ? ` (last stage ${last})` : ""}` };
    }
    return { ...base, status: "unavailable", reason: "no result line (check not run in this log)" };
  }
  // A check may be printed by more than one process in a log; the last one wins.
  const result = lines.at(-1);
  if (NOT_RUN.has(result.status)) {
    return { ...base, status: "unavailable", reason: `reported ${result.status}${result.reason ? `: ${result.reason}` : ""}` };
  }
  const expect = spec.expect ?? { status: "passed" };
  const mismatches = Object.entries(expect)
    .filter(([path, expected]) => !sameValue(valueAt(result, path), expected))
    .map(([path, expected]) => `${path}=${JSON.stringify(valueAt(result, path))} (expected ${JSON.stringify(expected)})`);
  if (spec.stage && !spec.expect && result.status === "passed") {
    // Stage tests have no result line of their own: they panic on failure, and
    // the summary line is printed only after every stage of the fixture ran.
    if (!log.stages.includes(spec.stage)) {
      return { ...base, status: "unavailable", reason: `stage ${spec.stage} was not entered in this log` };
    }
    return { ...base, status: "passed", reason: `stage ${spec.stage} completed before ${spec.check}` };
  }
  return mismatches.length
    ? { ...base, status: "failed", reason: mismatches.join("; ").slice(0, 600) }
    : { ...base, status: "passed" };
}

function apiStatus(api, evidence) {
  const items = (api.evidence ?? []).map((key) => evidence[key]);
  if (api.implementation === "missing") return { status: "missing", items };
  if (items.some((item) => item.status === "failed")) return { status: "failed", items };
  if (api.implementation === "unknown") return { status: "unavailable", items, reason: "engine behaviour not verified" };
  if (items.some((item) => item.status === "unavailable")) return { status: "unavailable", items };
  if (api.implementation === "present_incomplete") return { status: "present_incomplete", items };
  return { status: "verified", items };
}

export function buildReport(manifest, logs, meta = {}) {
  const evidence = {};
  for (const [key, spec] of Object.entries(manifest.evidence)) evidence[key] = evaluateEvidence(key, spec, logs);

  const apis = manifest.apis.map((api) => {
    const { status, items, reason } = apiStatus(api, evidence);
    return {
      id: api.id,
      area: api.area,
      surface: api.surface,
      implementation: api.implementation,
      status,
      ...(reason ? { reason } : {}),
      ...(api.gaps ? { gaps: api.gaps } : {}),
      ...(api.template_use ? { template_use: api.template_use } : {}),
      evidence: items,
    };
  });
  const apiById = new Map(apis.map((api) => [api.id, api]));

  // Paths may require other paths; resolve in dependency order (no cycles allowed).
  const pathSpecs = new Map(manifest.template_paths.map((path) => [path.id, path]));
  const paths = new Map();
  const resolving = new Set();
  function resolvePath(id) {
    if (paths.has(id)) return paths.get(id);
    if (resolving.has(id)) throw new Error(`template path cycle at ${id}`);
    resolving.add(id);
    const spec = pathSpecs.get(id);
    const requirement = (reqId) => apiById.get(reqId) ?? resolvePath(reqId);
    const blockers = (spec.requires ?? [])
      .map((reqId) => ({ id: reqId, status: requirement(reqId).status }))
      .filter((req) => req.status === "missing");
    const incomplete = (spec.requires ?? [])
      .map((reqId) => ({ id: reqId, status: requirement(reqId).status }))
      .filter((req) => req.status !== "verified" && req.status !== "missing");
    const optional = (spec.optional ?? []).map((reqId) => ({ id: reqId, status: requirement(reqId).status }));
    const items = (spec.evidence ?? []).map((key) => evidence[key]);
    let status;
    let reason;
    if (blockers.length) {
      status = "missing";
      reason = `blocked by ${blockers.map((req) => req.id).join(", ")}`;
    } else if (items.some((item) => item.status === "failed")) {
      status = "failed";
    } else if (!items.length) {
      status = "unavailable";
      reason = "no end-to-end harness";
    } else if (items.some((item) => item.status === "unavailable")) {
      status = "unavailable";
    } else {
      status = "verified";
    }
    const path = {
      id,
      description: spec.description,
      priority: spec.priority,
      status,
      ...(reason ? { reason } : {}),
      sources: spec.sources ?? [],
      blockers,
      not_verified_requirements: incomplete,
      optional,
      evidence: items,
      ...(spec.next ? { next: spec.next } : {}),
    };
    resolving.delete(id);
    paths.set(id, path);
    return path;
  }
  for (const id of pathSpecs.keys()) resolvePath(id);
  const templatePaths = [...pathSpecs.keys()].map((id) => paths.get(id));

  const count = (items) => Object.fromEntries(STATUSES.map((status) => [status, items.filter((item) => item.status === status).length]));
  // Remaining coverage: open template paths by priority, then the missing APIs
  // they need (each listed once, at the highest priority that needs it).
  const open = templatePaths
    .filter((path) => path.status !== "verified")
    .sort((a, b) => PRIORITY_ORDER.indexOf(a.priority) - PRIORITY_ORDER.indexOf(b.priority));
  const neededApis = new Map();
  for (const path of open) {
    for (const reqId of path.blockers.map((req) => req.id)) {
      if (apiById.has(reqId) && !neededApis.has(reqId)) neededApis.set(reqId, path.priority);
    }
  }
  return {
    schema: 1,
    generated_at: new Date().toISOString(),
    run: {
      mode: meta.mode ?? null,
      commit: meta.commit ?? null,
      run_id: meta.runId ?? null,
      template_sha: meta.templateSha ?? null,
    },
    logs: Object.fromEntries(
      Object.entries(manifest.logs).map(([name, file]) => [
        name,
        logs[name]
          ? { file, present: true, results: logs[name].results.length, unparsable: logs[name].unparsable, panicked: logs[name].panicked }
          : { file, present: false },
      ]),
    ),
    summary: { apis: count(apis), template_paths: count(templatePaths) },
    remaining: {
      template_paths: open.map((path) => ({ id: path.id, priority: path.priority, status: path.status, reason: path.reason ?? null })),
      missing_apis: [...neededApis].map(([id, priority]) => ({ id, priority })),
    },
    apis,
    template_paths: templatePaths,
    evidence,
  };
}

function readLogs(manifest, dir) {
  const logs = {};
  for (const [name, file] of Object.entries(manifest.logs)) {
    const path = join(dir, file);
    if (existsSync(path)) logs[name] = parseResults(readFileSync(path, "utf8"));
  }
  return logs;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const here = dirname(fileURLToPath(import.meta.url));
  const manifestPath = resolve(args.manifest ?? join(here, "manifest.json"));
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const errors = validateManifest(manifest, {
    sourceChecks: args.sources ? sourceCheckIds(args.sources) : undefined,
  });
  if (errors.length) {
    for (const error of errors) console.error(`${args.github ? "::error::" : ""}compat manifest: ${error}`);
    process.exit(2);
  }
  const warnings = args.template ? templateWarnings(manifest, args.template) : [];
  for (const warning of warnings) console.log(`${args.github ? "::warning::" : ""}compat manifest: ${warning}`);
  if (args.validate && !args.logs) {
    console.log(`compat manifest valid: ${manifest.apis.length} APIs, ${manifest.template_paths.length} template paths, ${Object.keys(manifest.evidence).length} evidence checks`);
    return;
  }
  if (!args.logs) throw new Error("--logs <dir> is required (or --validate)");
  const report = buildReport(manifest, readLogs(manifest, args.logs), {
    mode: args.mode,
    templateSha: args.templateSha,
    commit: process.env.GITHUB_SHA,
    runId: process.env.GITHUB_RUN_ID,
  });
  report.template_audit_warnings = warnings;
  const out = args.out ?? join(args.logs, "blitz-compat-report.json");
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
  const fmt = (counts) => STATUSES.map((status) => `${status} ${counts[status]}`).join(", ");
  const prefix = args.github ? "::notice::" : "";
  console.log(`${prefix}API compatibility (${args.mode ?? "?"}): APIs ${fmt(report.summary.apis)} | template paths ${fmt(report.summary.template_paths)}`);
  const open = report.remaining.template_paths.map((path) => `${path.id} ${path.priority} ${path.status}`).join("; ");
  console.log(`${prefix}open template paths: ${open || "none"}`.slice(0, 900));
  const failed = [...report.apis, ...report.template_paths].filter((item) => item.status === "failed");
  if (failed.length) {
    const detail = failed
      .map((item) => `${item.id}: ${item.evidence.filter((e) => e.status === "failed").map((e) => `${e.key} ${e.reason ?? ""}`).join(", ")}`)
      .join(" | ");
    console.log(`${prefix}failed evidence: ${detail}`.slice(0, 900));
  }
  console.log(`report written to ${out}`);
}

// Run as a script (not when imported); case-insensitive for Windows drive letters.
if (process.argv[1] && resolve(process.argv[1]).toLowerCase() === fileURLToPath(import.meta.url).toLowerCase()) main();
