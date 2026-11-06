import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createContext, runInContext} from 'node:vm';
import {createHash, randomUUID} from 'node:crypto';

const source = readFileSync(new URL('../google-apps-script/Code.gs', import.meta.url), 'utf8');
function app() {
  let now = 1800000000000;
  const values = new Map(Object.entries({
    GOOGLE_CLIENT_ID:'test.apps.googleusercontent.com', GOOGLE_CLIENT_SECRET:'server-secret',
    WALLTA_WEB_APP_URL:'https://script.google.com/macros/s/test-deployment/exec',
  }));
  const requests = [], responses = [], states = [];
  const propertyStore = {getProperty:key=>values.get(key) ?? null, setProperty:(key,value)=>values.set(key,value),
    deleteProperty:key=>values.delete(key), getProperties:()=>Object.fromEntries(values)};
  const ctx = createContext({
    Date:class extends Date { static now(){return now;} },
    PropertiesService:{getScriptProperties:()=>propertyStore},
    Utilities:{getUuid:randomUUID, DigestAlgorithm:{SHA_256:'sha256'}, Charset:{UTF_8:'utf8'},
      computeDigest:(_,value)=>[...createHash('sha256').update(value).digest()],
      base64EncodeWebSafe:value=>Buffer.from(value).toString('base64url')},
    LockService:{getScriptLock:()=>({waitLock(){},releaseLock(){}})},
    ScriptApp:{getScriptId:()=> 'test-script-id', newStateToken:()=>{
      const state={}; const builder={withMethod:v=>(state.method=v,builder),withArgument:(k,v)=>(state[k]=v,builder),
        withTimeout:v=>(state.timeout=v,builder),createToken:()=>(states.push(state),'signed-platform-state')}; return builder;
    },getOAuthToken:()=>{throw Error('Must never use deployment owner token');}},
    ContentService:{MimeType:{JSON:'json'},createTextOutput:text=>({text,setMimeType(){return this;}})},
    HtmlService:{createHtmlOutput:text=>({text,setTitle(){return this;},addMetaTag(){return this;}})},
    UrlFetchApp:{fetch:(url,options)=>{
      requests.push({url,options}); const next=responses.shift();
      if (typeof next==='function') return next();
      if (!next) throw Error('Unexpected network request');
      return {getResponseCode:()=>next.status ?? 200,getContentText:()=>JSON.stringify(next.body)};
    }},
  });
  runInContext(source,ctx);
  const post = body=>JSON.parse(ctx.doPost({postData:{type:'application/json',contents:JSON.stringify(body)}}).text);
  const pair=(kind='drive')=>post({action:'pair',kind,displayName:'My phone'}).data;
  const start=p=>{ctx.doGet({parameter:{action:'connect',code:new URL(p.verificationUrl).searchParams.get('code')}});return states.at(-1);};
  const grant=(p,kind='drive')=>{
    const state=start(p);
    responses.push({body:{access_token:'visitor-access',refresh_token:'visitor-refresh',expires_in:3600,
      scope:kind==='drive'?'https://www.googleapis.com/auth/drive.readonly':'https://www.googleapis.com/auth/photospicker.mediaitems.readonly'}},
      {body:{email:'visitor@example.com'}});
    return ctx.walltaOAuthCallback({parameter:{...state,code:'one-use-google-code'}});
  };
  return {ctx,values,requests,responses,states,post,pair,start,grant,advance:seconds=>{now+=seconds*1000;}};
}

test('health exposes protocol/configuration, never client values',()=>{
  const a=app(); const health=JSON.parse(a.ctx.doGet({parameter:{}}).text);
  assert.equal(health.service,'wallta-google-auth'); assert.equal(health.protocol,1); assert.equal(health.configured,true);
  assert.equal(JSON.stringify(health).includes('server-secret'),false);
  assert.equal(health.redirectUri,'https://script.google.com/macros/d/test-script-id/usercallback');
});
test('pair separates browser verification from device secrets and needs no folder',()=>{
  const a=app(),p=a.pair(); assert.match(p.deviceCode,/^[a-f0-9]{64}$/);
  const url=new URL(p.verificationUrl),code=url.searchParams.get('code');
  assert.match(code,/^[a-f0-9]{64}$/); assert.notEqual(code,p.deviceCode);
  assert.equal(a.post({action:'poll',deviceCode:code}).ok,false);
  const html=a.ctx.doGet({parameter:{action:'connect',code}}).text;
  assert.equal(html.includes(p.deviceCode),false); assert.equal(html.includes('server-secret'),false);
  assert.equal(a.states[0].method,'walltaOAuthCallback');
  const auth=new URL(html.match(/href="(https:\/\/accounts[^\"]+)"/)[1].replaceAll('&amp;','&'));
  assert.equal(auth.searchParams.get('redirect_uri'),'https://script.google.com/macros/d/test-script-id/usercallback');
  assert.equal(auth.searchParams.get('code_challenge_method'),'S256');
  assert.equal(auth.searchParams.get('access_type'),'offline');
});
test('each user authorizes their own account; polling never returns refresh token or client secret',()=>{
  const a=app(),p=a.pair(); a.grant(p);
  const r=a.post({action:'poll',deviceCode:p.deviceCode});
  assert.equal(r.ok,true); assert.equal(r.data.phase,'ready'); assert.equal(r.data.account,'visitor@example.com');
  assert.match(r.data.connectionToken,/^[a-f0-9]{64}$/);
  assert.equal(JSON.stringify(r).includes('visitor-refresh'),false); assert.equal(JSON.stringify(r).includes('server-secret'),false);
  const token=a.post({action:'token',connectionToken:r.data.connectionToken});
  assert.equal(token.data.accessToken,'visitor-access'); assert.equal(a.requests.length,2);
  assert.equal(a.requests[0].options.payload.client_secret,'server-secret');
});
test('Photos uses Picker permission, not full-library or Ambient access',()=>{
  const a=app(),p=a.pair('photos'); a.grant(p,'photos');
  const r=a.post({action:'poll',deviceCode:p.deviceCode}); assert.equal(r.data.kind,'photos');
  assert.equal(a.requests[0].options.payload.code,'one-use-google-code');
});
test('expired pairs cannot start or finish authorization',()=>{
  const a=app(),p=a.pair(),state=a.start(p); a.advance(601);
  assert.equal(a.post({action:'poll',deviceCode:p.deviceCode}).error.code,'expired');
  const result=a.ctx.walltaOAuthCallback({parameter:{...state,code:'late-code'}});
  assert.equal(result.text.includes('Connected'),false); assert.equal(a.requests.length,0);
});
test('invalid callback nonce and denied Drive scope cannot create a usable grant',()=>{
  const a=app(),p=a.pair(),state=a.start(p);
  a.ctx.walltaOAuthCallback({parameter:{...state,nonce:'wrong',code:'fake-code'}}); assert.equal(a.requests.length,0);
  a.responses.push({body:{access_token:'not-enough',refresh_token:'refresh',expires_in:3600,scope:'openid'}});
  a.ctx.walltaOAuthCallback({parameter:{...state,code:'real-code'}});
  const r=a.post({action:'poll',deviceCode:p.deviceCode}); assert.equal(r.data.phase,'error');
  assert.equal(a.post({action:'token',connectionToken:p.deviceCode}).ok,false);
});
test('cancel during token exchange prevents late callback from publishing credentials',()=>{
  const a=app(),p=a.pair(),state=a.start(p);
  a.responses.push(()=>{
    a.post({action:'cancel',deviceCode:p.deviceCode});
    return {getResponseCode:()=>200,getContentText:()=>JSON.stringify({access_token:'late-access',refresh_token:'late-refresh',expires_in:3600,scope:'https://www.googleapis.com/auth/drive.readonly'})};
  },{body:{email:'visitor@example.com'}});
  a.ctx.walltaOAuthCallback({parameter:{...state,code:'real-code'}});
  assert.equal(a.post({action:'poll',deviceCode:p.deviceCode}).ok,false);
  assert.equal([...a.values.values()].some(value=>value.includes('late-refresh')),false);
});
test('callback replay cannot exchange a Google code twice',()=>{
  const a=app(),p=a.pair(),state=a.start(p); a.grant(p);
  const count=a.requests.length;
  a.ctx.walltaOAuthCallback({parameter:{...state,code:'replayed-code'}});
  assert.equal(a.requests.length,count);
});
test('access-token refresh keeps refresh tokens server-side; disconnect affects only this connection',()=>{
  const a=app(),p=a.pair(); a.grant(p); const connection=a.post({action:'poll',deviceCode:p.deviceCode}).data.connectionToken;
  a.advance(3600); a.responses.push({body:{access_token:'renewed-access',expires_in:3600}});
  const r=a.post({action:'token',connectionToken:connection}); assert.equal(r.data.accessToken,'renewed-access');
  assert.equal(a.requests.at(-1).options.payload.refresh_token,'visitor-refresh');
  assert.equal(a.post({action:'disconnect',connectionToken:connection}).ok,true);
  assert.equal(a.post({action:'token',connectionToken:connection}).ok,false);
  assert.equal(a.requests.some(r=>r.url.includes('/revoke')),false);
});
test('refresh rejection disables the grant without exposing provider response bodies',()=>{
  const a=app(),p=a.pair(); a.grant(p); const connection=a.post({action:'poll',deviceCode:p.deviceCode}).data.connectionToken;
  a.advance(3600); a.responses.push({status:400,body:{error:'invalid_grant',detail:'private upstream response'}});
  const r=a.post({action:'token',connectionToken:connection}); assert.equal(r.ok,false);
  assert.equal(JSON.stringify(r).includes('private upstream response'),false);
  assert.equal(JSON.stringify(r).includes('visitor-refresh'),false);
});
test('disconnect during refresh cannot recreate a removed connection',()=>{
  const a=app(),p=a.pair(); a.grant(p); const connection=a.post({action:'poll',deviceCode:p.deviceCode}).data.connectionToken;
  a.advance(3600); a.responses.push(()=>{
    a.post({action:'disconnect',connectionToken:connection});
    return {getResponseCode:()=>200,getContentText:()=>JSON.stringify({access_token:'late-renewal',expires_in:3600})};
  });
  assert.equal(a.post({action:'token',connectionToken:connection}).ok,false);
  assert.equal([...a.values.values()].some(value=>value.includes('late-renewal')),false);
});
test('a temporary refresh failure retains the grant and releases the refresh claim',()=>{
  const a=app(),p=a.pair(); a.grant(p); const connection=a.post({action:'poll',deviceCode:p.deviceCode}).data.connectionToken;
  a.advance(3600); a.responses.push({status:503,body:{error:'temporary'}});
  assert.equal(a.post({action:'token',connectionToken:connection}).error.code,'provider_error');
  a.responses.push({body:{access_token:'recovered',expires_in:3600}});
  assert.equal(a.post({action:'token',connectionToken:connection}).data.accessToken,'recovered');
});
test('browser-controlled device names are escaped and cannot inject markup',()=>{
  const a=app(); const p=a.post({action:'pair',kind:'drive',displayName:'<script>steal()</script>'}).data;
  const html=a.ctx.doGet({parameter:{action:'connect',code:new URL(p.verificationUrl).searchParams.get('code')}}).text;
  assert.equal(html.includes('<script>steal()'),false); assert.ok(html.includes('&lt;script&gt;'));
});
test('unconfigured service, malformed bodies, GET secrets and unexpected actions fail closed',()=>{
  const a=app(); a.values.delete('GOOGLE_CLIENT_SECRET'); assert.equal(a.post({action:'pair',kind:'drive'}).ok,false);
  assert.equal(JSON.parse(a.ctx.doPost({postData:{type:'application/json',contents:'bad json'}}).text).ok,false);
  assert.equal(JSON.parse(a.ctx.doPost({postData:{type:'text/plain',contents:'{}'}}).text).ok,false);
  assert.equal(a.post({action:'pair',kind:'arbitrary-api'}).ok,false);
  assert.equal(a.post({action:'admin'}).ok,false);
  assert.equal(JSON.parse(a.ctx.doGet({parameter:{action:'token',connectionToken:'secret'}}).text).ok,false);
});
test('pairing capacity is bounded and expired pairs are cleaned before adding new ones',()=>{
  const a=app(); for(let n=0;n<50;n++) assert.ok(a.pair());
  assert.equal(a.post({action:'pair',kind:'drive'}).error.code,'busy');
  a.advance(601); assert.equal(a.post({action:'pair',kind:'drive'}).ok,true);
});
test('pending callbacks cannot overfill connection capacity after pairing was created',()=>{
  const a=app();
  for(let n=0;n<99;n++) a.values.set('WT1:C:'+String(n).padStart(64,'0'),JSON.stringify({expiresAt:1800100000}));
  const first=a.pair(),second=a.pair();
  a.grant(first); a.grant(second);
  assert.equal([...a.values.keys()].filter(key=>key.startsWith('WT1:C:')).length,100);
  assert.equal(a.post({action:'poll',deviceCode:second.deviceCode}).data.phase,'error');
});
test('unsaved cancellation after pairing cleanup disconnects by the delivered capability',()=>{
  const a=app(),p=a.pair(); a.grant(p);
  const connection=a.post({action:'poll',deviceCode:p.deviceCode}).data.connectionToken;
  a.advance(601); a.pair(); // The short-lived pairing is gone; the delivered grant has its own lifecycle.
  assert.equal(a.post({action:'token',connectionToken:connection}).ok,true);
  a.post({action:'disconnect',connectionToken:connection});
  assert.equal(a.post({action:'token',connectionToken:connection}).ok,false);
});
