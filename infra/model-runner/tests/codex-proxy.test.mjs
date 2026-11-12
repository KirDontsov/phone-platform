import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import net from 'node:net';
import {once} from 'node:events';
import {attachCodexProxy,publicAddress} from '../codex-proxy.mjs';
const password='private-test-pairing-password-only';
const authorization='Basic '+Buffer.from('opencode:'+password).toString('base64');
async function setup(t,options={}) {
  const server=http.createServer((_,r)=>r.end());const proxy=attachCodexProxy(server,{password,...options});
  server.listen(0,'127.0.0.1');await once(server,'listening');
  t.after(async()=>{proxy.close();server.closeAllConnections();await new Promise(r=>server.close(r))});
  return {proxy,port:server.address().port};
}
async function open(port,target='auth.openai.com:443',auth=authorization,head=Buffer.alloc(0)) {
  const socket=net.connect(port,'127.0.0.1');socket.on('error',()=>{});await once(socket,'connect');
  socket.write(Buffer.concat([Buffer.from(`CONNECT ${target} HTTP/1.1\r\nHost: ${target}\r\nProxy-Authorization: ${auth}\r\n\r\n`),head]));
  let data=Buffer.alloc(0);while(!data.includes('\r\n\r\n')){const chunk=await Promise.race([once(socket,'data').then(a=>a[0]),once(socket,'close').then(()=>Buffer.alloc(0)),new Promise((_,r)=>setTimeout(()=>r(Error('response_timeout')),1500))]);if(!chunk.length)throw Error('closed_before_response');data=Buffer.concat([data,chunk]);}
  return {socket,data,status:Number(data.toString().split(' ')[1])};
}
test('CONNECT requires correct pairing and exact allowed host:443 before DNS',async t=>{
  let queries=0;const {port}=await setup(t,{lookup:async()=>{queries++;return[]}});
  for(const [target,auth,status] of [['auth.openai.com:443','bad',407],['evil.test:443',authorization,403],['127.0.0.1:443',authorization,403],['auth.openai.com:80',authorization,403],['user@auth.openai.com:443',authorization,403],['auth.openai.com.evil.test:443',authorization,403]]) {
    const r=await open(port,target,auth);assert.equal(r.status,status);r.socket.destroy();
  }
  assert.equal(queries,0);
});
test('public-address guard denies private, mixed and DNS rebinding results',async t=>{
  for(const ip of ['127.0.0.1','10.1.2.3','172.16.1.1','192.168.1.2','169.254.1.1','100.64.0.1','::1','::ffff:127.0.0.1','fc00::1','fe80::1','2001:db8::1'])assert.equal(publicAddress(ip),false,ip);
  assert.equal(publicAddress('1.1.1.1'),true);assert.equal(publicAddress('2606:4700::1111'),true);
  let dialed=0;const {port}=await setup(t,{lookup:async()=>[{address:'1.1.1.1',family:4},{address:'127.0.0.1',family:4}],connect:()=>{dialed++;throw Error('must_not_dial')}});
  const r=await open(port);assert.equal(r.status,403);r.socket.destroy();assert.equal(dialed,0);
});
test('valid CONNECT pins public DNS result and forwards opaque TLS bytes then closes resources',async t=>{
  const echo=net.createServer(socket=>socket.pipe(socket));echo.listen(0,'127.0.0.1');await once(echo,'listening');t.after(()=>new Promise(r=>echo.close(r)));
  const dials=[];const {port,proxy}=await setup(t,{lookup:async()=>[{address:'1.1.1.1',family:4}],connect:options=>{dials.push(options);return net.connect(echo.address().port,'127.0.0.1')}});
  const bytes=Buffer.from([22,3,3,0,5,1,2,3,4,5]);const r=await open(port,'chatgpt.com:443',authorization,bytes);assert.equal(r.status,200);
  const split=r.data.indexOf('\r\n\r\n')+4;let body=r.data.subarray(split);if(!body.length)body=(await once(r.socket,'data'))[0];assert.deepEqual(body,bytes);assert.equal(dials[0].host,'1.1.1.1');assert.equal(dials[0].port,443);
  proxy.close();await once(r.socket,'close');assert.equal(proxy.activeCount,0);
});
test('DNS deadline and global concurrency bound fail closed',async t=>{
  const a=await setup(t,{lookup:()=>new Promise(()=>{}),resolveMs:30});const failure=await open(a.port);assert.equal(failure.status,502);failure.socket.destroy();
  const b=await setup(t,{lookup:()=>new Promise(()=>{}),resolveMs:100,maxConnections:1});const firstPromise=open(b.port);await new Promise(r=>setTimeout(r,15));const second=await open(b.port);assert.equal(second.status,503);second.socket.destroy();const first=await firstPromise;first.socket.destroy();
});
