import test from 'node:test';
import assert from 'node:assert/strict';
import {childEnvironment} from '../src/config.mjs';
test('Codex receives only its fixed private paired proxy and direct internal callback bypass',()=>{
 const source={MODEL_RUNNER_URL:'http://model-runner:4096',MODEL_RUNNER_PASSWORD:'fixed-pair-password-123456789',HTTPS_PROXY:'http://evil.test',OPENAI_API_KEY:'do-not-inherit',DATABASE_URL:'do-not-inherit'};
 const proxy=childEnvironment('/tmp/own-test-home',{runtime:source});const expected='http://opencode:fixed-pair-password-123456789@model-runner:4096';
 assert.equal(proxy.HTTPS_PROXY,expected);assert.equal(proxy.HTTP_PROXY,expected);assert.equal(proxy.https_proxy,expected);assert.equal(proxy.http_proxy,expected);assert.equal(proxy.NO_PROXY,'api,assistant-runner,localhost,127.0.0.1,::1');
 const child=childEnvironment('/tmp/own-test-home',{runtime:source});assert.equal(child.HTTPS_PROXY,expected);assert.equal(child.OPENAI_API_KEY,undefined);assert.equal(child.DATABASE_URL,undefined);
 assert.equal(childEnvironment('/tmp/own-test-home',{runtime:{HTTPS_PROXY:'http://evil.test'}}).HTTPS_PROXY,undefined);
 for(const MODEL_RUNNER_URL of ['http://evil.test:4096','http://127.0.0.1:4096','https://model-runner:4096','http://model-runner:4096/path','http://model-runner:4096?x=1'])assert.throws(()=>childEnvironment('/tmp/own-test-home',{runtime:{...source,MODEL_RUNNER_URL}}));
 assert.throws(()=>childEnvironment('/tmp/own-test-home',{runtime:{...source,MODEL_RUNNER_PASSWORD:'short'}}));
});
