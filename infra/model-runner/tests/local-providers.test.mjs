import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import { once } from 'node:events';
import { createGateway } from '../gateway.mjs';
import { parseLocalSettings, localCatalog, apinexTranscript, redactNative } from '../local-providers.mjs';
const org = '11111111-1111-4111-8111-111111111111', foreign = '22222222-2222-4222-8222-222222222222';
const key = 'synthetic-local-provider-secret-12345', password = 'synthetic-local-transport-password-123456';
const authorization = 'Basic ' + Buffer.from('opencode:' + password).toString('base64');
const config = JSON.parse(fs.readFileSync(new URL('../opencode.json', import.meta.url), 'utf8'));
function settings() { return parseLocalSettings({version:1,organization_id:org,providers:{apinex:{name:'APInex',options:{baseURL:'https://api.apinex.bond/v1',apiKey:key},models:{'free/test':{name:'Existing model'}}}}}); }
async function listen(server) { server.listen(0,'127.0.0.1'); await once(server,'listening'); return 'http://127.0.0.1:'+server.address().port; }
async function setup(t, options={}) {
 const nativeCalls=[], providerCalls=[];
 const native = http.createServer(async(req,res)=>{ const chunks=[];for await(const c of req)chunks.push(c);nativeCalls.push({path:req.url,method:req.method,body:Buffer.concat(chunks).toString()});res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(req.url==='/session'?{id:'ses_owned'}:req.url==='/config'?{permission:{'*':'deny'},provider:{apinex:{options:{apiKey:key,headers:{Authorization:'Bearer '+key}}}},echo:'Bearer '+key}:{ok:true})); });
 const nativeBase=await listen(native);
 const gateway=createGateway({password,config,localSettings:settings(),nativeBase,fetchImpl:async(url,init)=>{providerCalls.push({url,init});return new Response(JSON.stringify(url.endsWith('/models')?{data:[{id:'free/test'}]}:{metadata:{duration:2},results:{channels:[{alternatives:[{transcript:'Синтетическая речь',words:[{start:0,end:1,word:'Речь',speaker:0}]}]}]},usage:{cost_tokens:800,units:2}}),{status:200});},...options});
 const base=await listen(gateway.server);
 t.after(async()=>{await gateway.close();await new Promise(resolve=>{native.close(resolve);native.closeAllConnections();});});
 return {nativeCalls,providerCalls,call(path,body,tenant=org,method){return fetch(base+path,{method:method||(body===undefined?'GET':'POST'),headers:{Authorization:authorization,...(tenant?{'X-Phone-Organization':tenant}:{}),...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body)});}};
}
test('imported credentials are scoped and every native config/provider/session path fails closed',async t=>{
 const h=await setup(t);
 for(const tenant of [foreign,null]){
  assert.deepEqual(await(await h.call('/local/providers',undefined,tenant)).json(),{available:false,providers:[],asr_models:[]});
  for(const path of ['/config','/provider','/config/providers','/experimental/tool/ids','/session/ses_other'])assert.equal((await h.call(path,undefined,tenant)).status,403);
  assert.equal((await h.call('/session',{},tenant)).status,403);
  assert.equal((await h.call('/session/ses_other/message',{model:{providerID:'apinex',modelID:'free/test'}},tenant)).status,403);
 }
 assert.equal(h.nativeCalls.length,0);assert.equal(h.providerCalls.length,0);
 const catalog=await(await h.call('/local/providers')).json();assert.equal(catalog.providers[0].models[0].id,'apinex/free/test');assert.ok(!JSON.stringify(catalog).includes(key));
 const nativeConfig=await(await h.call('/config')).text();assert.ok(!nativeConfig.includes(key));assert.match(nativeConfig,/<configured>/);
});
test('sessions belong to the importing organization, approved model is required and capacity is reserved before create',async t=>{
 const h=await setup(t,{maxSessions:1});
 assert.equal((await h.call('/session',{})).status,200);
 assert.equal((await h.call('/session',{})).status,429);assert.equal(h.nativeCalls.filter(c=>c.path==='/session').length,1);
 assert.equal((await h.call('/session/ses_owned',undefined,foreign)).status,403);
 assert.equal((await h.call('/session/ses_missing/message',{model:{providerID:'apinex',modelID:'free/test'}})).status,403);
 assert.equal((await h.call('/session/ses_owned/message',{model:{providerID:'apinex',modelID:'undeclared-paid'}})).status,422);
 assert.equal((await h.call('/session/ses_owned/message',{model:{providerID:'apinex',modelID:'free/test'}})).status,200);
 assert.equal((await h.call('/session/ses_owned',undefined,org,'DELETE')).status,200);
 assert.equal((await h.call('/session',{})).status,200);
});
test('existing provider probes never require user API key or submit audio; STT uses exact local provider and honest model',async t=>{
 const h=await setup(t);
 assert.equal((await h.call('/local/test',{model:'apinex/free/test'})).status,200);
 assert.equal((await h.call('/asr/test',{model:'apinex/voice-stt'})).status,200);
 assert.equal(h.providerCalls.length,2);assert.ok(h.providerCalls.every(c=>c.init.method==='GET'));
 assert.equal(h.providerCalls[0].init.headers.Authorization,'Bearer '+key);
 const payload={model:'apinex/voice-stt',input_audio:{data:Buffer.from('synthetic audio').toString('base64'),format:'wav'},response_format:'verbose_json'};
 assert.equal((await h.call('/asr/transcriptions',payload,foreign)).status,403);assert.equal(h.providerCalls.length,2);
 const result=await(await h.call('/asr/transcriptions',payload)).json();assert.equal(result.text,'Синтетическая речь');assert.equal(result.segments[0].role,undefined);assert.equal(result.usage.cost,undefined);
 assert.match(h.providerCalls[2].url,/^https:\/\/api\.apinex\.bond\/v1\/tools\/voice\/stt\?/);assert.equal(h.providerCalls[2].init.headers['Content-Type'],'audio/wav');assert.equal(h.providerCalls[2].init.body.toString(),'synthetic audio');
 assert.equal((await h.call('/asr/test',{model:'moondream/parakeet-ultra'})).status,422);
});
test('only provider definitions are trusted and credentials never appear in nested response fields',()=>{
 const local=settings();assert.equal(local.providers.apinex.npm,'@ai-sdk/openai-compatible');
 assert.ok(!JSON.stringify(redactNative({arbitrary:key,nested:{api_key:key,headers:{Authorization:key}}},local)).includes(key));
 const bad=JSON.parse(JSON.stringify(local));bad.providers.apinex.options.baseURL='https://user:pass@other.example/v1';assert.throws(()=>parseLocalSettings(bad));
 assert.equal(localCatalog(local,foreign).available,false);
 assert.throws(()=>apinexTranscript({results:{channels:[{alternatives:[{transcript:'test',words:[{start:-1,end:2,word:'x'}]}]}]}}));
});
