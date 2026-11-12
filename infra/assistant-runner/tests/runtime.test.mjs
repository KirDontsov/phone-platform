import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { createRunner } from "../src/server.mjs";
import {
  fixture,
  fakeRpc,
  loginBody,
  turnBody,
  token,
  ids,
} from "./helpers.mjs";
async function setup(t, { mode = "normal", approved = true, loginTtl } = {}) {
  const f = await fixture(t, { approved });
  const app = await createRunner({
    token,
    apiUrl: f.apiUrl,
    authRoot: path.join(f.directory, "auth"),
    tempRoot: path.join(f.directory, "temp"),
    rpcFactory: fakeRpc(mode),
    loginTtl,
  });
  t.after(() => app.close());
  await new Promise((r) => app.server.listen(0, "127.0.0.1", r));
  return {
    ...f,
    ...app,
    base: "http://127.0.0.1:" + app.server.address().port,
  };
}
async function post(app, url, body, credential = token) {
  const response = await fetch(app.base + url, {
    method: "POST",
    headers: {
      Authorization: "Bearer " + credential,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return { status: response.status, text: await response.text() };
}
test("HTTP controls require shared token and status is explicit unconfigured", async (t) => {
  const app = await setup(t);
  const org = ids().organization_id;
  assert.equal(
    (await post(app, "/auth/status", { organization_id: org }, "wrong")).status,
    401,
  );
  const status = await post(app, "/auth/status", { organization_id: org });
  const data = JSON.parse(status.text);
  assert.equal(data.authenticated, false);
  assert.equal(data.generation, 0);
  assert.equal(data.codex_version, "0.167.2");
  assert.equal(
    (await post(app, "/rpc", { method: "command/exec" })).status,
    404,
  );
});
test("API approval gates auth promotion and generations fence logout/other tenants", async (t) => {
  const app = await setup(t);
  const body = loginBody();
  let response = await post(app, "/auth/start", body);
  assert.equal(response.status, 200, response.text);
  assert.equal(JSON.parse(response.text).authenticated, true);
  assert.equal(app.calls[0].path, "/api/assistant/auth/complete");
  const raw = await fs.readFile(
    path.join(app.directory, "auth", body.organization_id, "auth.json"),
    "utf8",
  );
  assert.ok(raw.includes(body.api_key));
  assert.equal(
    (
      await fs.stat(
        path.join(app.directory, "auth", body.organization_id, "auth.json"),
      )
    ).mode & 0o777,
    0o600,
  );
  assert.ok(!response.text.includes(body.api_key));
  assert.equal(
    JSON.parse(
      (
        await post(app, "/auth/status", {
          organization_id: ids().organization_id,
        })
      ).text,
    ).authenticated,
    false,
  );
  response = await post(app, "/auth/logout", {
    organization_id: body.organization_id,
    generation: 2,
  });
  assert.equal(JSON.parse(response.text).generation, 2);
  assert.equal((await post(app, "/auth/start", body)).status, 409);
  await assert.rejects(
    fs.stat(
      path.join(app.directory, "auth", body.organization_id, "auth.json"),
    ),
  );
});
test("2xx denied authority never promotes staged auth", async (t) => {
  const app = await setup(t, { approved: false });
  const body = loginBody();
  const response = await post(app, "/auth/start", body);
  assert.equal(response.status, 403, response.text);
  assert.ok(!response.text.includes(body.api_key));
  await assert.rejects(
    fs.stat(
      path.join(app.directory, "auth", body.organization_id, "auth.json"),
    ),
  );
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});
test("device code pending TTL and cancellation physically clean staged homes", async (t) => {
  const app = await setup(t, { loginTtl: 60 });
  const body = { ...ids(), type: "chatgptDeviceCode" };
  const response = await post(app, "/auth/start", body);
  assert.equal(response.status, 200, response.text);
  assert.equal(
    JSON.parse(response.text).pending_login.verification_url,
    "https://auth.openai.com/codex/device",
  );
  await new Promise((r) => setTimeout(r, 100));
  const status = JSON.parse(
    (await post(app, "/auth/status", { organization_id: body.organization_id }))
      .text,
  );
  assert.equal(status.pending_login, null);
  assert.equal(status.error_code, "assistant_login_expired");
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});
test("fresh turn has exactly one terminal, Unicode text and no diagnostics/secrets", async (t) => {
  const app = await setup(t);
  const auth = loginBody();
  await post(app, "/auth/start", auth);
  const response = await post(app, "/turn", turnBody(auth));
  assert.equal(response.status, 200, response.text);
  const events = response.text.trim().split("\n").map(JSON.parse);
  assert.equal(
    events.filter((v) => v.type === "completed" || v.type === "error").length,
    1,
  );
  assert.equal(events.at(-1).text, "Готово");
  assert.ok(!response.text.includes("APIKEY-MUST-NOT-ECHO"));
  assert.ok(!response.text.includes(token));
  assert.ok(!response.text.includes(auth.api_key));
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});
test("authenticated startup informational notifications before initialize/config responses retain a completed turn", async (t) => {
  const app = await setup(t, { mode: "startup" });
  const auth = loginBody();
  assert.equal((await post(app, "/auth/start", auth)).status, 200);
  const response = await post(app, "/turn", turnBody(auth));
  assert.equal(response.status, 200);
  const events = response.text.trim().split("\n").map(JSON.parse);
  assert.deepEqual(events.at(-1), {
    type: "completed",
    text: "Готово",
    model: "fake-model",
  });
  assert.equal(
    events.filter((v) => ["completed", "error"].includes(v.type)).length,
    1,
  );
  assert.ok(!response.text.includes("remoteControl"));
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});
for (const [mode, code] of [
  ["eof", "codex_stream_interrupted"],
  ["forbidden", "forbidden_codex_item"],
  ["unknown", "unexpected_codex_event"],
  ["request", "unsupported_runtime_request"],
  ["startup-forbidden", "forbidden_codex_item"],
  ["startup-unknown", "unexpected_codex_event"],
])
  test("fail closed: " + mode, async (t) => {
    const app = await setup(t, { mode });
    const auth = loginBody();
    await post(app, "/auth/start", auth);
    const response = await post(app, "/turn", turnBody(auth));
    const events = response.text.trim().split("\n").map(JSON.parse);
    assert.equal(
      events.filter((v) => v.type === "completed" || v.type === "error").length,
      1,
      response.text,
    );
    assert.equal(events.at(-1).code, code);
    assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
  });
test("cancel kills pending process and stale generation cannot start a turn", async (t) => {
  const app = await setup(t, { mode: "slow" });
  const auth = loginBody();
  await post(app, "/auth/start", auth);
  const body = turnBody(auth);
  const stream = post(app, "/turn", body);
  for (
    let i = 0;
    i < 200 && !app.turns.active.get(body.run_id)?.rpc?.child.pid;
    i++
  )
    await new Promise((r) => setTimeout(r, 5));
  const process = app.turns.active.get(body.run_id)?.rpc;
  assert.ok(process?.child.pid);
  await post(app, "/cancel", {
    organization_id: body.organization_id,
    run_id: body.run_id,
  });
  const response = await stream;
  assert.equal(
    JSON.parse(response.text.trim().split("\n").at(-1)).code,
    "assistant_run_cancelled",
  );
  assert.equal(process.closed, true);
  await post(app, "/auth/logout", {
    organization_id: auth.organization_id,
    generation: 2,
  });
  assert.equal((await post(app, "/turn", turnBody(auth))).status, 409);
});

test("device completion requires authority callback and old generation cannot delete promoted credentials", async (t) => {
  const app = await setup(t, { mode: "device" });
  const body = { ...ids(), type: "chatgptDeviceCode" };
  await post(app, "/auth/start", body);
  let status;
  for (let i = 0; i < 50; i++) {
    status = JSON.parse(
      (
        await post(app, "/auth/status", {
          organization_id: body.organization_id,
        })
      ).text,
    );
    if (status.authenticated) break;
    await new Promise((r) => setTimeout(r, 10));
  }
  assert.equal(status.authenticated, true);
  assert.equal(status.auth_mode, "chatgpt");
  assert.equal(status.pending_login, null);
  assert.equal(
    app.calls.filter((c) => c.path === "/api/assistant/auth/complete").length,
    2,
  );
  assert.equal(
    (
      await post(app, "/auth/logout", {
        organization_id: body.organization_id,
        generation: 0,
      })
    ).status,
    409,
  );
  assert.ok(await app.storage.auth(body.organization_id));
});
test("device logout fences late callback and stages are removed", async (t) => {
  const app = await setup(t, { mode: "device" });
  const body = { ...ids(), type: "chatgptDeviceCode" };
  await post(app, "/auth/start", body);
  await post(app, "/auth/logout", {
    organization_id: body.organization_id,
    generation: 2,
  });
  await new Promise((r) => setTimeout(r, 120));
  const status = JSON.parse(
    (await post(app, "/auth/status", { organization_id: body.organization_id }))
      .text,
  );
  assert.equal(status.generation, 2);
  assert.equal(status.authenticated, false);
  assert.equal(app.calls.length, 0);
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});

test("HTTP disconnect disposes process tree and temporary context", async (t) => {
  const app = await setup(t, { mode: "slow" });
  const auth = loginBody();
  await post(app, "/auth/start", auth);
  const body = turnBody(auth);
  const response = await fetch(app.base + "/turn", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + token,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  for (
    let i = 0;
    i < 200 && !app.turns.active.get(body.run_id)?.rpc?.child.pid;
    i++
  )
    await new Promise((r) => setTimeout(r, 5));
  const rpc = app.turns.active.get(body.run_id)?.rpc;
  assert.ok(rpc?.child.pid);
  await response.body.cancel();
  for (let i = 0; i < 200 && app.turns.active.has(body.run_id); i++)
    await new Promise((r) => setTimeout(r, 5));
  assert.equal(rpc.closed, true);
  assert.equal(app.turns.active.size, 0);
  assert.deepEqual(await fs.readdir(path.join(app.directory, "temp")), []);
});

test("failed reauthorization preserves confirmed credential, generation and old turns; logout revokes active and pending", async (t) => {
  const app = await setup(t);
  const auth = loginBody();
  await post(app, "/auth/start", auth);
  const file = path.join(app.directory, "auth", auth.organization_id, "auth.json");
  const previous = await fs.readFile(file);
  app.auth.rpcFactory = fakeRpc("login-fail");
  const rejected = await post(app, "/auth/start", {...auth,auth_attempt_id:ids().auth_attempt_id,generation:2,api_key:"sk-rejected-test-only-key"});
  assert.equal(rejected.status,422);
  const status = JSON.parse((await post(app,"/auth/status",{organization_id:auth.organization_id})).text);
  assert.equal(status.authenticated,true);
  assert.equal(status.generation,auth.generation);
  assert.deepEqual(await fs.readFile(file),previous);
  const oldTurn=await post(app,"/turn",turnBody(auth));
  assert.equal(JSON.parse(oldTurn.text.trim().split("\n").at(-1)).type,"completed");
  app.auth.rpcFactory = fakeRpc("normal");
  const pending = await post(app,"/auth/start",{...ids(),organization_id:auth.organization_id,generation:3,type:"chatgptDeviceCode"});
  assert.equal(pending.status,200);
  assert.equal(JSON.parse(pending.text).authenticated,true);
  assert.equal(JSON.parse(pending.text).pending_generation,3);
  assert.ok(JSON.parse(pending.text).pending_login);
  await post(app,"/auth/logout",{organization_id:auth.organization_id,generation:4});
  const revoked=JSON.parse((await post(app,"/auth/status",{organization_id:auth.organization_id})).text);
  assert.equal(revoked.authenticated,false);assert.equal(revoked.pending_login,null);
  await assert.rejects(fs.stat(file));
  assert.equal((await post(app,"/turn",turnBody(auth))).status,409);
});

test("lost commit response is resolved by an idempotent receipt without restoring old credentials", async t => {
  const http=await import('node:http');
  const app=await setup(t);const auth=loginBody();await post(app,'/auth/start',auth);
  const phases=[];
  const server=http.createServer(async(req,res)=>{let text='';for await(const chunk of req)text+=chunk;const body=JSON.parse(text);phases.push(body.phase);if(body.phase==='commit'){req.socket.destroy();return;}res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({data:{approved:true,generation:body.generation,committed:body.phase==='status',active_generation:body.generation}}));});
  await new Promise(r=>server.listen(0,'127.0.0.1',r));t.after(()=>new Promise(r=>{server.closeAllConnections();server.close(r)}));
  app.auth.apiUrl='http://127.0.0.1:'+server.address().port;
  const result=await post(app,'/auth/start',{...auth,auth_attempt_id:ids().auth_attempt_id,generation:2,api_key:'sk-new-confirmed-test-only-key'});
  assert.equal(result.status,200,result.text);assert.equal(JSON.parse(result.text).authenticated,true);assert.equal(JSON.parse(result.text).generation,2);
  assert.deepEqual(phases,['prepare','commit','commit','commit','status']);
  const raw=await fs.readFile(path.join(app.directory,'auth',auth.organization_id,'auth.json'),'utf8');assert.ok(raw.includes('sk-new-confirmed-test-only-key'));
  await assert.rejects(fs.stat(path.join(app.directory,'auth',auth.organization_id,'auth.previous.json')));
});

test("unresolved commit uncertainty blocks turns and retains both private credentials until explicit logout",async t=>{
  const http=await import('node:http');const app=await setup(t);const auth=loginBody();await post(app,'/auth/start',auth);
  const file=path.join(app.directory,'auth',auth.organization_id,'auth.json');const old=await fs.readFile(file);
  const server=http.createServer(async(req,res)=>{let text='';for await(const chunk of req)text+=chunk;const body=JSON.parse(text);if(body.phase!=='prepare'){req.socket.destroy();return;}res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({data:{approved:true,generation:body.generation}}));});
  await new Promise(r=>server.listen(0,'127.0.0.1',r));t.after(()=>new Promise(r=>{server.closeAllConnections();server.close(r)}));app.auth.apiUrl='http://127.0.0.1:'+server.address().port;
  const result=await post(app,'/auth/start',{...auth,auth_attempt_id:ids().auth_attempt_id,generation:2,api_key:'sk-uncertain-test-only-key'});assert.equal(result.status,503,result.text);
  const state=JSON.parse((await post(app,'/auth/status',{organization_id:auth.organization_id})).text);assert.equal(state.authenticated,false);assert.equal(state.error_code,'assistant_auth_commit_uncertain');
  assert.deepEqual(await fs.readFile(path.join(app.directory,'auth',auth.organization_id,'auth.previous.json')),old);
  assert.ok((await fs.readFile(file,'utf8')).includes('sk-uncertain-test-only-key'));assert.equal((await post(app,'/turn',turnBody(auth))).status,409);
  await post(app,'/auth/logout',{organization_id:auth.organization_id,generation:3});await assert.rejects(fs.stat(file));await assert.rejects(fs.stat(path.join(app.directory,'auth',auth.organization_id,'auth.previous.json')));
});

for (const crashStage of ["after_promotion","after_commit"]) {
  test(`restart after ${crashStage} cannot authenticate new credential under previous authority`,async t=>{
    const app=await setup(t);const auth=loginBody();await post(app,"/auth/start",auth);
    const originalPromote=app.storage.promote.bind(app.storage);
    app.storage.promote=async(org,home)=>{
      const durable=await app.storage.read(org);
      assert.equal(durable.authenticated,false,"Promotion must be fenced before the file swap");
      assert.equal(durable.commit_uncertain,true);
      await originalPromote(org,home);
      if(crashStage==="after_promotion")throw Error("synthetic_crash_after_promote");
    };
    const originalWrite=app.storage.write.bind(app.storage);
    if(crashStage==="after_commit")app.storage.write=async(org,state)=>{
      if(state.generation===2&&state.authenticated===true)throw Error("synthetic_crash_after_commit");
      return originalWrite(org,state);
    };
    const response=await post(app,"/auth/start",{...auth,auth_attempt_id:ids().auth_attempt_id,generation:2,api_key:"sk-new-account-test-only"});
    assert.equal(response.status,500,response.text);
    const restarted=await createRunner({token,apiUrl:app.apiUrl,authRoot:path.join(app.directory,"auth"),tempRoot:path.join(app.directory,"temp"),rpcFactory:fakeRpc("normal")});
    t.after(()=>restarted.close());await new Promise(r=>restarted.server.listen(0,"127.0.0.1",r));
    const target={base:"http://127.0.0.1:"+restarted.server.address().port};
    const status=JSON.parse((await post(target,"/auth/status",{organization_id:auth.organization_id})).text);
    assert.equal(status.authenticated,false);assert.equal(status.generation,1);
    const file=path.join(app.directory,"auth",auth.organization_id,"auth.json");assert.ok((await fs.readFile(file,"utf8")).includes("sk-new-account-test-only"));
    assert.equal((await post(target,"/turn",turnBody(auth))).status,409);
    assert.equal((await post(target,"/turn",{...turnBody(auth),auth_generation:2})).status,409);
    await post(target,"/auth/logout",{organization_id:auth.organization_id,generation:3});
    await assert.rejects(fs.stat(file));
  });
}
