#!/usr/bin/env node
// End-to-end smoke test against a running Atlas. Node 18+, no dependencies.
// Usage: ATLAS_URL=http://localhost:8080 node scripts/smoke.mjs
const BASE = (process.env.ATLAS_URL || "http://localhost:8080").replace(/\/$/, "");
const COLLECTION = "halcyon";
const MULTI_HOP = "Who is the on-call lead for the team that owns the navigation stack?";
const POLL_MS = 2000;
const TIMEOUT_MS = 15 * 60 * 1000;

let key = "";

async function call(method, path, body, auth = true) {
  const headers = {};
  if (body !== undefined) headers["Content-Type"] = "application/json";
  if (auth) headers["Authorization"] = `Bearer ${key}`;
  const res = await fetch(BASE + path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    /* keep text */
  }
  if (!res.ok) {
    throw new Error(`${method} ${path} -> ${res.status} ${text.slice(0, 300)}`);
  }
  return json;
}

const results = [];
async function step(name, fn) {
  const t0 = Date.now();
  try {
    const detail = await fn();
    results.push({ name, ok: true, detail: detail || "", ms: Date.now() - t0 });
  } catch (e) {
    results.push({ name, ok: false, detail: String(e.message || e), ms: Date.now() - t0 });
  }
}

function printTable() {
  console.log("\n" + "check".padEnd(34) + "result  time     detail");
  for (const r of results) {
    console.log(
      r.name.padEnd(34) +
        (r.ok ? "PASS" : "FAIL").padEnd(8) +
        `${(r.ms / 1000).toFixed(1)}s`.padEnd(9) +
        r.detail
    );
  }
}

function finish() {
  printTable();
  const failed = results.filter((r) => !r.ok).length;
  console.log(failed === 0 ? "\nAll checks passed." : `\n${failed} check(s) failed.`);
  process.exit(failed === 0 ? 0 : 1);
}

async function waitForOperations(operations) {
  const pending = new Map(operations.map((o) => [o.operation_id, o.source_uri]));
  const failures = [];
  const deadline = Date.now() + TIMEOUT_MS;
  while (pending.size > 0) {
    if (Date.now() > deadline) throw new Error(`timed out with ${pending.size} operation(s) pending`);
    for (const [id, uri] of [...pending]) {
      const op = await call("GET", `/api/v1/ingestion-operations/${id}`);
      if (op.status === "Succeeded") pending.delete(id);
      else if (op.status === "Failed") {
        pending.delete(id);
        failures.push(uri);
      }
    }
    process.stdout.write(".");
    if (pending.size > 0) await new Promise((r) => setTimeout(r, POLL_MS));
  }
  process.stdout.write("\n");
  if (failures.length) throw new Error(`failed: ${failures.join(", ")}`);
}

async function main() {
  console.log(`Atlas smoke test against ${BASE}`);
  try {
    key = (await call("GET", "/demo/bootstrap", undefined, false)).api_key;
  } catch (e) {
    console.error(`Cannot reach Atlas at ${BASE}: ${e.message}`);
    process.exit(2);
  }

  const health = await call("GET", "/demo/health");
  if (!health.ready) {
    console.error("Atlas is not ready:");
    for (const c of health.checks.filter((c) => !c.ok)) {
      console.error(`  ${c.label}: ${c.detail}`);
      if (c.fix) console.error(`    fix: ${c.fix}`);
    }
    process.exit(2);
  }
  results.push({ name: "health", ok: true, detail: `${health.checks.length} checks ready`, ms: 0 });

  await step("load samples and ingest", async () => {
    const { operations } = await call("POST", "/demo/samples/load", {});
    process.stdout.write(`Ingesting ${operations.length} documents `);
    await waitForOperations(operations);
    return `${operations.length} documents`;
  });

  let chunkId = null;
  await step("search", async () => {
    const r = await call("POST", "/api/v1/search", {
      query: "HX-2 battery runtime",
      collection_id: COLLECTION,
      top_k: 5,
    });
    if (!r.chunks?.length) throw new Error("no chunks returned");
    chunkId = r.chunks[0].indexed_chunk.chunk.id;
    return `${r.chunks.length} chunks`;
  });

  await step("context", async () => {
    const r = await call("POST", "/api/v1/context", {
      collection_id: COLLECTION,
      query: MULTI_HOP,
      render: "numbered",
    });
    if (!r.passages?.length) throw new Error("no passages");
    return `${r.passages.length} passages, ${r.usage.used}/${r.usage.budget} tokens`;
  });

  await step("generate with verify", async () => {
    const r = await call("POST", "/api/v1/generate", {
      collection_id: COLLECTION,
      query: MULTI_HOP,
      verify: true,
    });
    if (r.status !== "ok") throw new Error(`status ${r.status}`);
    if (!r.answer) throw new Error("empty answer");
    if (r.verification?.status !== "ok") {
      throw new Error(`verification ${r.verification?.status}: ${r.verification?.code || "missing"}`);
    }
    return `verdict ${r.verification.verdict}, ${r.citations.length} citations`;
  });

  await step("evidence for a chunk", async () => {
    if (!chunkId) throw new Error("no chunk id from search");
    const r = await call("GET", `/evidence/chunk/${chunkId}`);
    return `keys: ${Object.keys(r).slice(0, 4).join(", ")}`;
  });

  await step("retrieval evaluation", async () => {
    const r = await call("POST", "/demo/eval", {});
    return `hit@k ${r.report.hit_rate_at_k.toFixed(2)}, mrr ${r.report.mrr.toFixed(2)}, ${r.report.num_queries} queries`;
  });

  await step("verify flawed answer", async () => {
    const samples = await call("GET", "/demo/samples");
    const flawed = samples.flawed_answers.find((a) => a.id === "flawed");
    if (!flawed) throw new Error("no flawed sample");
    const ctx = await call("POST", "/api/v1/context", {
      collection_id: COLLECTION,
      query: flawed.question,
    });
    const passages = ctx.passages.map((p) => ({ ref_id: p.ref_id, chunk_ids: p.chunk_ids }));
    const r = await call("POST", "/api/v1/verify", {
      collection_id: COLLECTION,
      answer: flawed.answer,
      passages,
    });
    const unsupported = r.counts.unsupported;
    if (unsupported < 1) throw new Error(`expected at least one unsupported, got ${JSON.stringify(r.counts)}`);
    return `${unsupported} unsupported, verdict ${r.verdict}`;
  });

  finish();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
