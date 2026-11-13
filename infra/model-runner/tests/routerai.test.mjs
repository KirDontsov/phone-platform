import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {once} from 'node:events';
import {createGateway} from '../gateway.mjs';
import {parseLocalSettings,localCatalog,ROUTERAI_ASR_IDS,ROUTERAI_URL} from '../local-providers.mjs';
const org='11111111-1111-4111-8111-111111111111',foreign='22222222-2222-4222-8222-222222222222';
const localKey='synthetic-routerai-key',transport='synthetic-transport-at-least-24-characters',legacyKey='synthetic-legacy-openrouter-key';
const auth='Basic '+Buffer.from('opencode:'+transport).toString('base64');
const baseConfig=JSON.parse(fs.readFileSync(new URL('../opencode.json',import.meta.url),'utf8'));
function settings(){return parseLocalSettings({version:1,organization_id:org,providers:{routerai:{name:'RouterAI',options:{baseURL:ROUTERAI_URL,apiKey:localKey},models:{'vendor/text':{name:'Text'},...Object.fromEntries(ROUTERAI_ASR_IDS.map(id=>[id,{name:id}]))}}}})}
async function setup(t){
 const seen=[],gateway=createGateway({password:transport,config:baseConfig,localSettings:settings(),fetchImpl:async(url,init)=>{
  seen.push({url,init});assert.equal(init.redirect,'error');
  let value=url.endsWith('/models')?{data:ROUTERAI_ASR_IDS.map(id=>({id,architecture:{output_modalities:['transcription']}}))}:url.endsWith('/key')?{data:{label:'synthetic'}}:{text:'Синтетическая речь',language:'ru',segments:[{start:0,end:1,text:'Синтетическая речь',speaker:0}],usage:{seconds:2,cost:url.startsWith(ROUTERAI_URL)?3.25:0.01}};
  return new Response(JSON.stringify(value),{status:200});
 }});
 gateway.server.listen(0,'127.0.0.1');await once(gateway.server,'listening');t.after(()=>gateway.close());
 return{seen,async call(body,{tenant=org,legacy,path='/asr/transcriptions'}={}){return fetch('http://127.0.0.1:'+gateway.server.address().port+path,{method:'POST',headers:{Authorization:auth,'Content-Type':'application/json','X-Phone-Organization':tenant,...(legacy?{'X-OpenRouter-Api-Key':legacy}:{})},body:JSON.stringify(body)})}};
}
const audio=model=>({model,input_audio:{data:Buffer.from('synthetic-isolated-audio').toString('base64'),format:'wav'},response_format:'verbose_json'});
test('RouterAI catalog includes the exact original speech models and excludes them from native text choices',()=>{
 const local=settings(),catalog=localCatalog(local,org);assert.deepEqual(catalog.asr_models.map(m=>m.id),ROUTERAI_ASR_IDS);assert.ok(catalog.asr_models.every(m=>m.provider==='routerai'));assert.deepEqual(catalog.providers[0].models.map(m=>m.id),['routerai/vendor/text']);
 assert.deepEqual(local.providers.routerai.models[ROUTERAI_ASR_IDS[0]].modalities,{input:['audio'],output:['text']});
 const invalid=settings();invalid.providers.routerai.options.baseURL='https://other.example/v1';assert.throws(()=>parseLocalSettings(invalid));
});
test('same speech model IDs route only to the explicitly selected provider and keep RUB outside common cost',async t=>{
 const h=await setup(t);
 for(const model of ROUTERAI_ASR_IDS){
  const response=await h.call({...audio(model),local_provider:'routerai'});assert.equal(response.status,200);const value=await response.json();assert.equal(value.usage.cost,undefined);assert.equal(value.usage.provider_cost,3.25);assert.equal(value.usage.provider_currency,'RUB');
  const request=h.seen.at(-1);assert.equal(request.url,ROUTERAI_URL+'/audio/transcriptions');assert.equal(request.init.headers.Authorization,'Bearer '+localKey);const body=JSON.parse(request.init.body);assert.equal(body.model,model);assert.equal(body.local_provider,undefined);assert.ok(!request.init.body.includes(localKey));
  const legacy=await h.call(audio(model),{legacy:legacyKey});assert.equal(legacy.status,200);assert.equal((await legacy.json()).usage.cost,0.01);assert.equal(h.seen.at(-1).url,'https://openrouter.ai/api/v1/audio/transcriptions');assert.equal(h.seen.at(-1).init.headers.Authorization,'Bearer '+legacyKey);
 }
});
test('RouterAI keyless mode rejects foreign tenants, mixed keys, invalid models and implicit fallback before any provider request',async t=>{
 const h=await setup(t),model=ROUTERAI_ASR_IDS[0];
 assert.equal((await h.call({...audio(model),local_provider:'routerai'},{tenant:foreign})).status,403);
 assert.equal((await h.call({...audio(model),local_provider:'routerai'},{legacy:legacyKey})).status,422);
 assert.equal((await h.call({...audio('vendor/text'),local_provider:'routerai'})).status,422);
 assert.equal((await h.call({...audio(model),local_provider:'apinex'})).status,422);
 assert.equal((await h.call(audio(model))).status,422);
 assert.equal(h.seen.length,0);
});
test('RouterAI connection test authenticates the saved key and checks speech capability without audio or inference',async t=>{
 const h=await setup(t);const response=await h.call({model:ROUTERAI_ASR_IDS[0],local_provider:'routerai'},{path:'/asr/test'});assert.equal(response.status,200);assert.deepEqual(h.seen.map(r=>r.url),[ROUTERAI_URL+'/key',ROUTERAI_URL+'/models']);assert.ok(h.seen.every(r=>r.init.method==='GET'));assert.equal((await response.json()).provider,'routerai');
});
