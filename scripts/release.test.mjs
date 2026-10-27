import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { copyFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { generateKeyPairSync, sign, createHash } from 'node:crypto';

const release = await import('./release.mjs').catch(() => ({}));
const version = '0.1.2';
const name = `Wallta_${version}_x64-setup.exe`;
function signed(data, signedVersion = version) {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const id = Buffer.from('12345678');
  const key = Buffer.concat([Buffer.from('Ed'), id, publicKey.export({type:'spki',format:'der'}).subarray(-32)]);
  const payload = sign(null, createHash('blake2b512').update(data).digest(), privateKey);
  const comment = `timestamp:1234\tfile:${name}\tversion:${signedVersion}`;
  const global = sign(null, Buffer.concat([payload, Buffer.from(comment)]), privateKey);
  return {
    pubkey: Buffer.from(`untrusted comment: public key\n${key.toString('base64')}\n`).toString('base64'),
    signature: Buffer.from(`untrusted comment: signature\n${Buffer.concat([Buffer.from('ED'),id,payload]).toString('base64')}\ntrusted comment: ${comment}\n${global.toString('base64')}\n`).toString('base64'),
  };
}
async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(),'wallta-release-'));
  t.after(() => rm(root,{recursive:true,force:true}));
  for (const dir of ['src-tauri','android/app','assets']) await mkdir(join(root,dir),{recursive:true});
  await writeFile(join(root,'package.json'),JSON.stringify({name:'wallta',version}));
  await writeFile(join(root,'package-lock.json'),JSON.stringify({name:'wallta',version,packages:{'':{name:'wallta',version}}}));
  await writeFile(join(root,'src-tauri/Cargo.toml'),`[package]\nname = "wallta"\nversion = "${version}"\n`);
  await writeFile(join(root,'src-tauri/Cargo.lock'),`[[package]]\nname = "dependency"\nversion = "9.9.9"\n\n[[package]]\nname = "wallta"\nversion = "${version}"\n`);
  const exe = Buffer.from('installer');
  const keys = signed(exe);
  await writeFile(join(root,'src-tauri/tauri.conf.json'),JSON.stringify({version,plugins:{updater:{pubkey:keys.pubkey}}}));
  await writeFile(join(root,'android/app/build.gradle.kts'),`defaultConfig { minSdk = 26; versionName = "${version}"; versionCode = 3 }`);
  await writeFile(join(root,'assets',name),exe);
  await writeFile(join(root,'assets',`${name}.sig`),keys.signature+'\n');
  await writeFile(join(root,'assets',`Wallta_${version}_android.apk`),'apk');
  return {root,dir:join(root,'assets'),...keys};
}
test('accepts stable tags and refuses prereleases, paths, shell text and leading zeroes', () => {
  assert.equal(typeof release.validateTag,'function','stable tag validation is implemented');
  assert.equal(release.validateTag('v0.1.2'),'0.1.2');
  for (const tag of ['0.1.2','v01.2.3','v1.2.3-beta','v1.2.3; echo x','../v1.2.3']) assert.throws(()=>release.validateTag(tag));
});
test('checks every application version and Android version code', async t => {
  const f = await fixture(t);
  assert.deepEqual(await release.validateVersions(f.root,'v0.1.2'),{version,versionCode:3});
  for (const [file,body] of [
    ['package.json',JSON.stringify({version:'0.1.3'})],
    ['package-lock.json',JSON.stringify({version,packages:{'':{version:'0.1.3'}}})],
    ['src-tauri/Cargo.toml','[package]\nname = "wallta"\nversion = "0.1.3"'],
    ['src-tauri/Cargo.lock','[[package]]\nname = "wallta"\nversion = "0.1.3"'],
    ['src-tauri/tauri.conf.json',JSON.stringify({version:'0.1.3'})],
    ['android/app/build.gradle.kts','versionName = "0.1.3"; versionCode = 3; minSdk = 26'],
  ]) {
    const path = join(f.root,file); const old = await readFile(path);
    await writeFile(path,body); await assert.rejects(release.validateVersions(f.root,'v0.1.2'),/version/i); await writeFile(path,old);
  }
  await writeFile(join(f.root,'android/app/build.gradle.kts'),'versionName = "0.1.2"; versionCode = 0; minSdk = 26');
  await assert.rejects(release.validateVersions(f.root,'v0.1.2'),/versionCode/i);
});
test('verifies installer bytes, key identity, authenticated version and signature', () => {
  const data = Buffer.from('installer'); const keys = signed(data);
  assert.equal(release.verifySignature(data,keys.signature,keys.pubkey,version,name),true);
  assert.throws(()=>release.verifySignature(Buffer.from('tampered'),keys.signature,keys.pubkey,version,name),/signature/i);
  assert.throws(()=>release.verifySignature(data,keys.signature,signed(data).pubkey,version,name),/key|signature/i);
  assert.throws(()=>release.verifySignature(data,keys.signature,keys.pubkey,'0.1.3',name),/version/i);
  assert.throws(()=>release.verifySignature(data,'invalid',keys.pubkey,version,name),/signature/i);
  const alteredComment = Buffer.from(Buffer.from(keys.signature,'base64').toString().replace('timestamp:1234','timestamp:9999')).toString('base64');
  assert.throws(()=>release.verifySignature(data,alteredComment,keys.pubkey,version,name),/comment signature/i);
});
test('writes complete stable manifest and correct SHA256 inventory including manifest', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2',date:'2026-10-27T00:00:00.000Z'});
  const manifest = JSON.parse(await readFile(join(f.dir,'latest.json'),'utf8'));
  assert.equal(manifest.version,version);
  assert.equal(manifest.platforms['windows-x86_64'].signature,f.signature);
  assert.equal(manifest.platforms['windows-x86_64'].url,`https://github.com/mawta/wallta/releases/download/v0.1.2/${name}`);
  assert.equal(manifest.android.versionCode,3);
  assert.equal(manifest.android.size,3);
  assert.equal(manifest.android.sha256,createHash('sha256').update('apk').digest('hex'));
  assert.equal(manifest.pub_date,'2026-10-27T00:00:00.000Z');
  assert.equal((await readFile(join(f.dir,'SHA256SUMS'),'utf8')).trim().split('\n').length,4);
  await release.verifyRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  await writeFile(join(f.dir,'latest.json'),JSON.stringify({...manifest,android:{...manifest.android,size:5}}));
  await assert.rejects(release.verifyRelease({root:f.root,dir:f.dir,tag:'v0.1.2'}),/manifest|checksum/i);
});
test('refuses unexpected, absent or duplicate artifacts before generating metadata', async t => {
  const f = await fixture(t);
  await writeFile(join(f.dir,'another.exe'),'extra');
  await assert.rejects(release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'}),/inventory/i);
  await rm(join(f.dir,'another.exe')); await rm(join(f.dir,`${name}.sig`));
  await assert.rejects(release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'}),/inventory/i);
});
test('blocks tracked internal notes and private credentials', () => {
  assert.equal(release.validateTrackedFiles(['README.md','docs/updating.md','.github/workflows/release.yml']),true);
  for (const name of ['docs/superpowers/plan.md','.local/notes.md','AGENTS.md','src/CLAUDE.md','keys/release.jks','.env.production','secrets/private.key']) assert.throws(()=>release.validateTrackedFiles([name]),/private|internal/i);
});
test('publishes only after exact draft inventory and downloaded contents pass verification', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  let published = false;
  const run = args => {
    if (args[0] === 'api' && args[1].includes('releases?')) return '[[{"tag_name":"v0.1.1","draft":false,"prerelease":false,"assets":[{"name":"latest.json","id":123}]}]]';
    if (args[0] === 'api' && args[1].endsWith('/assets/123')) return JSON.stringify({version:'0.1.1',android:{versionCode:2}});
    if (args[1] === 'view') return JSON.stringify({isDraft:true,isPrerelease:false,tagName:'v0.1.2',assets:readdirSync(f.dir).map(name=>({name,size:statSync(join(f.dir,name)).size}))});
    if (args[1] === 'download') for (const name of readdirSync(f.dir)) copyFileSync(join(f.dir,name),join(args[args.indexOf('--dir')+1],name));
    if (args[1] === 'edit') published = true;
    return '';
  };
  await release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run});
  assert.equal(published,true);
});
test('never overwrites existing releases or publishes partial uploaded inventory', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  const seen = [];
  await assert.rejects(release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run:args=>{seen.push(args);return '[[{"tag_name":"v0.1.2"}]]';}}),/already exists/i);
  assert.equal(seen.length,1);
  let published = false;
  await assert.rejects(release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run:args=>{
    if (args[0] === 'api') return '[[]]';
    if (args[1] === 'view') return JSON.stringify({isDraft:true,isPrerelease:false,tagName:'v0.1.2',assets:[]});
    if (args[1] === 'edit') published = true;
    return '';
  }}),/inventory/i);
  assert.equal(published,false);
});
test('refuses to make an older stable version latest', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  let mutated = false;
  await assert.rejects(release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run:args=>{
    if (args[0] === 'api') return '[[{"tag_name":"v0.2.0","draft":false,"prerelease":false}]]';
    mutated = true; throw new Error('unexpected mutation');
  }}),/newer stable/i);
  assert.equal(mutated,false);
});

test('refuses an Android versionCode that cannot upgrade the previous stable APK', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  for (const code of [3,4]) {
    let mutated = false;
    await assert.rejects(release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run:args=>{
      if (args[0] === 'api' && args[1].includes('releases?')) return '[[{"tag_name":"v0.1.1","draft":false,"prerelease":false,"assets":[{"name":"latest.json","id":123}]}]]';
      if (args[0] === 'api' && args[1].endsWith('/assets/123')) return JSON.stringify({version:'0.1.1',android:{versionCode:code}});
      mutated = true; throw new Error('unexpected mutation');
    }}),/versionCode/i);
    assert.equal(mutated,false);
  }
});
test('leaves a draft unpublished when downloaded bytes differ', async t => {
  const f = await fixture(t);
  await release.generateRelease({root:f.root,dir:f.dir,tag:'v0.1.2'});
  let published = false;
  await assert.rejects(release.publishRelease({root:f.root,dir:f.dir,tag:'v0.1.2',run:args=>{
    if (args[0] === 'api') return '[[]]';
    if (args[1] === 'view') return JSON.stringify({isDraft:true,isPrerelease:false,tagName:'v0.1.2',assets:readdirSync(f.dir).map(name=>({name,size:statSync(join(f.dir,name)).size}))});
    if (args[1] === 'download') {
      const target = args[args.indexOf('--dir')+1];
      for (const name of readdirSync(f.dir)) copyFileSync(join(f.dir,name),join(target,name));
      writeFileSync(join(target,`Wallta_${version}_android.apk`),'corrupt');
    }
    if (args[1] === 'edit') published = true;
    return '';
  }}),/manifest|checksum/i);
  assert.equal(published,false);
});
test('stages only the expected build identity and refuses replacing a staged artifact', async t => {
  const f = await fixture(t);
  const source = join(f.root,'apk-output'); const dir = join(f.root,'staged');
  await mkdir(source); await writeFile(join(source,'app-release.apk'),'signed-apk');
  await release.stageArtifact({root:f.root,dir,tag:'v0.1.2',platform:'android',source});
  assert.equal(await readFile(join(dir,`Wallta_${version}_android.apk`),'utf8'),'signed-apk');
  await assert.rejects(release.stageArtifact({root:f.root,dir,tag:'v0.1.2',platform:'android',source}),/EEXIST/);
  await writeFile(join(source,'app-release-unsigned.apk'),'unexpected');
  await assert.rejects(release.stageArtifact({root:f.root,dir,tag:'v0.1.2',platform:'android',source}),/inventory/);
});
