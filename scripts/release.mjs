import { readFile, writeFile, readdir, mkdir, copyFile, mkdtemp, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createHash, createPublicKey, verify } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

export const repository = 'mawta/wallta';
export const releaseNotes = 'Lưu tài khoản Drive rồi chọn hoặc đổi thư mục ngay trên nguồn đã lưu. Ghi nhớ tài khoản và bộ ảnh Google Photos, hiển thị rõ nguồn đang dùng. Có thể lưu và áp dụng ngay, bật khởi động cùng Windows. Sửa lỗi theo dõi đăng nhập Google và cache khi đổi tài khoản.';
const readJson = async path => JSON.parse(await readFile(path,'utf8'));
const sha256 = data => createHash('sha256').update(data).digest('hex');
const requireValue = (value,message) => { if (!value) throw new Error(message); return value; };

export function validateTag(tag) {
  requireValue(/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag),'A stable vMAJOR.MINOR.PATCH tag is required');
  return tag.slice(1);
}

export async function validateVersions(root,tag) {
  const version = validateTag(tag);
  const pkg = await readJson(join(root,'package.json'));
  const lock = await readJson(join(root,'package-lock.json'));
  const tauri = await readJson(join(root,'src-tauri/tauri.conf.json'));
  const cargo = await readFile(join(root,'src-tauri/Cargo.toml'),'utf8');
  const cargoLock = await readFile(join(root,'src-tauri/Cargo.lock'),'utf8');
  const android = await readFile(join(root,'android/app/build.gradle.kts'),'utf8');
  const ownCargo = cargo.match(/\[package\]([\s\S]*?)(?=\n\[|$)/)?.[1];
  const ownLock = cargoLock.split('[[package]]').filter(block => /^name\s*=\s*"wallta"\s*$/m.test(block));
  const versions = {package:pkg.version,lock:lock.version,lockRoot:lock.packages?.['']?.version,tauri:tauri.version,
    cargo:ownCargo?.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    cargoLock:ownLock.length === 1 ? ownLock[0].match(/^version\s*=\s*"([^"]+)"/m)?.[1] : undefined,
    android:android.match(/\bversionName\s*=\s*"([^"]+)"/)?.[1]};
  for (const [file,value] of Object.entries(versions)) requireValue(value === version,`Application version mismatch in ${file}: expected ${version}`);
  const versionCode = Number(android.match(/\bversionCode\s*=\s*(\d+)\b/)?.[1]);
  requireValue(Number.isSafeInteger(versionCode) && versionCode > 0 && (version !== '0.1.2' || versionCode === 3),'Invalid Android versionCode');
  requireValue(/\bminSdk\s*=\s*26\b/.test(android),'Android minSdk must remain API 26');
  return {version,versionCode};
}

export function validateTrackedFiles(files) {
  const blocked = files.filter(file => /(^|\/)(?:\.local|\.agents|\.claude|\.codex)(\/|$)|^docs\/superpowers(?:\/|$)|(^|\/)(?:AGENTS|CLAUDE|GEMINI)\.md$|(^|\/)\.env(?:\..+)?$|\.(?:jks|keystore|key|p12|pfx)$/i.test(file.replaceAll('\\','/')) && !file.endsWith('.env.example'));
  requireValue(blocked.length === 0,`Private or internal files must not be tracked: ${blocked.join(', ')}`);
  return true;
}

function decodeBase64(value,label) {
  const text = value.trim();
  requireValue(text.length > 0 && text.length % 4 === 0 && /^[A-Za-z0-9+/]+={0,2}$/.test(text),`Invalid ${label}`);
  return Buffer.from(text,'base64');
}

export function verifySignature(data,signature,pubkey,version,filename) {
  const keyLines = decodeBase64(pubkey,'public key').toString('utf8').trim().split(/\r?\n/);
  const sigLines = decodeBase64(signature,'signature').toString('utf8').trim().split(/\r?\n/);
  requireValue(keyLines.length === 2 && sigLines.length === 4,'Invalid signature or public key structure');
  const key = decodeBase64(keyLines[1],'public key');
  const sig = decodeBase64(sigLines[1],'signature');
  requireValue(key.length === 42 && key.subarray(0,2).toString() === 'Ed','Invalid public key');
  requireValue(sig.length === 74 && sig.subarray(0,2).toString() === 'ED','Invalid prehashed signature');
  requireValue(key.subarray(2,10).equals(sig.subarray(2,10)),'Signature key identity mismatch');
  requireValue(sigLines[2].startsWith('trusted comment: '),'Invalid signature comment');
  const comment = sigLines[2].slice('trusted comment: '.length);
  const fields = comment.split('\t');
  requireValue(fields.filter(x=>x.startsWith('version:')).length === 1 && fields.includes(`version:${version}`),'Signature version mismatch');
  requireValue(fields.filter(x=>x.startsWith('file:')).length === 1 && fields.includes(`file:${filename}`),'Signature filename mismatch');
  const publicKey = createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),key.subarray(10)]),format:'der',type:'spki'});
  const payload = sig.subarray(10);
  requireValue(verify(null,createHash('blake2b512').update(data).digest(),publicKey,payload),'Installer signature verification failed');
  requireValue(verify(null,Buffer.concat([payload,Buffer.from(comment)]),publicKey,decodeBase64(sigLines[3],'global signature')),'Trusted comment signature verification failed');
  return true;
}

export function artifactNames(version) {
  return [`Wallta_${version}_android.apk`,`Wallta_${version}_x64-setup.exe`,`Wallta_${version}_x64-setup.exe.sig`];
}

async function inventory(dir,expected) {
  const entries = await readdir(dir,{withFileTypes:true});
  requireValue(entries.every(item=>item.isFile()) && JSON.stringify(entries.map(x=>x.name).sort()) === JSON.stringify([...expected].sort()),'Release artifact inventory mismatch');
}

async function manifestFor({root,dir,tag,date}) {
  const {version,versionCode} = await validateVersions(root,tag);
  const [apkName,exeName,sigName] = artifactNames(version);
  const apk = await readFile(join(dir,apkName));
  const exe = await readFile(join(dir,exeName));
  const signature = (await readFile(join(dir,sigName),'utf8')).trim();
  requireValue(apk.length > 0 && exe.length > 0,'Empty release artifact');
  const pubkey = (await readJson(join(root,'src-tauri/tauri.conf.json'))).plugins?.updater?.pubkey;
  verifySignature(exe,signature,pubkey ?? '',version,exeName);
  requireValue(typeof date === 'string' && new Date(date).toISOString() === date,'Invalid release publication date');
  const base = `https://github.com/${repository}/releases/download/${tag}/`;
  return {version,notes:releaseNotes,pub_date:date,platforms:{'windows-x86_64':{url:base+exeName,signature}},android:{url:base+apkName,sha256:sha256(apk),size:apk.length,versionCode}};
}

async function checksums(dir,names) {
  const lines = [];
  for (const name of [...names].sort()) lines.push(`${sha256(await readFile(join(dir,name)))}  ${name}`);
  return lines.join('\n')+'\n';
}

export async function generateRelease({root,dir,tag,date = new Date().toISOString()}) {
  const version = validateTag(tag);
  await inventory(dir,artifactNames(version));
  const manifest = await manifestFor({root,dir,tag,date});
  await writeFile(join(dir,'latest.json'),JSON.stringify(manifest,null,2)+'\n',{flag:'wx'});
  await writeFile(join(dir,'SHA256SUMS'),await checksums(dir,[...artifactNames(version),'latest.json']),{flag:'wx'});
  await verifyRelease({root,dir,tag});
}

export async function verifyRelease({root,dir,tag}) {
  const version = validateTag(tag);
  await inventory(dir,[...artifactNames(version),'latest.json','SHA256SUMS']);
  const manifest = await readJson(join(dir,'latest.json'));
  const expected = await manifestFor({root,dir,tag,date:manifest.pub_date});
  requireValue(JSON.stringify(manifest) === JSON.stringify(expected),'Release manifest does not match artifacts');
  requireValue(await readFile(join(dir,'SHA256SUMS'),'utf8') === await checksums(dir,[...artifactNames(version),'latest.json']),'Release checksum mismatch');
}

export async function stageArtifact({root,dir,tag,platform,source}) {
  const {version} = await validateVersions(root,tag);
  const names = artifactNames(version);
  const expected = platform === 'windows' ? names.slice(1) : platform === 'android' ? ['app-release.apk'] : [];
  requireValue(expected.length,'Invalid build platform');
  // The bundle/output directory must contain exactly one matching application identity.
  const entries = await readdir(source,{withFileTypes:true});
  const binaries = entries.filter(x=>x.isFile() && /\.(exe|sig|apk)$/.test(x.name)).map(x=>x.name).sort();
  requireValue(JSON.stringify(binaries) === JSON.stringify([...expected].sort()),'Build artifact inventory mismatch');
  await mkdir(dir,{recursive:true});
  for (const name of expected) await copyFile(join(source,name),join(dir,platform === 'android' ? names[0] : name),1);
}

function gh(args,options = {}) {
  return execFileSync('gh',args[0] === 'api' ? args : [...args,'--repo',repository],{encoding:'utf8',stdio:['ignore','pipe','pipe'],...options});
}

export async function publishRelease({root,dir,tag,run = gh}) {
  await verifyRelease({root,dir,tag});
  // Any existing tag release (including a failed draft) needs deliberate operator review.
  const releases = JSON.parse(run(['api',`repos/${repository}/releases?per_page=100`,'--paginate','--slurp'])).flat();
  requireValue(!releases.some(release=>release.tag_name === tag),'Release already exists; assets will never be overwritten');
  const version = validateTag(tag).split('.').map(BigInt);
  const candidate = await readJson(join(dir,'latest.json'));
  for (const prior of releases.filter(item=>!item.draft && !item.prerelease && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(item.tag_name))) {
    const previous = validateTag(prior.tag_name).split('.').map(BigInt);
    const firstDifference = version.findIndex((part,index)=>part !== previous[index]);
    requireValue(firstDifference >= 0 && version[firstDifference] > previous[firstDifference],'A newer stable release already exists');
    const manifests = (prior.assets ?? []).filter(asset=>asset.name === 'latest.json');
    requireValue(manifests.length === 1 && Number.isSafeInteger(manifests[0].id) && manifests[0].id > 0,'Cannot verify previous Android versionCode: release manifest missing');
    const previousManifest = JSON.parse(run(['api',`repos/${repository}/releases/assets/${manifests[0].id}`,'-H','Accept: application/octet-stream']));
    const previousCode = previousManifest.android?.versionCode;
    requireValue(previousManifest.version === prior.tag_name.slice(1) && Number.isSafeInteger(previousCode) && previousCode > 0,'Cannot verify previous Android versionCode: invalid release manifest');
    requireValue(candidate.android.versionCode > previousCode,'Android versionCode must increase for every stable release');
  }
  const notesPath = join(dir,'..',`wallta-release-notes-${validateTag(tag)}.txt`);
  await writeFile(notesPath,releaseNotes+'\n');
  const assets = [...artifactNames(validateTag(tag)),'latest.json','SHA256SUMS'];
  run(['release','create',tag,'--draft','--verify-tag','--title',`Wallta ${tag}`,'--notes-file',notesPath]);
  run(['release','upload',tag,...assets.map(name=>join(dir,name))]);
  const state = JSON.parse(run(['release','view',tag,'--json','isDraft,isPrerelease,assets,tagName']));
  requireValue(state.isDraft && !state.isPrerelease && state.tagName === tag,'Unexpected release state');
  requireValue(JSON.stringify(state.assets.map(asset=>asset.name).sort()) === JSON.stringify(assets.sort()),'Uploaded release inventory mismatch');
  for (const asset of state.assets) requireValue(asset.size === (await readFile(join(dir,asset.name))).length,'Uploaded release size mismatch');
  const downloaded = await mkdtemp(join(tmpdir(),'wallta-verify-'));
  try {
    run(['release','download',tag,'--dir',downloaded]);
    await verifyRelease({root,dir:downloaded,tag});
    for (const name of assets) requireValue(sha256(await readFile(join(downloaded,name))) === sha256(await readFile(join(dir,name))),'Uploaded release content mismatch');
    run(['release','edit',tag,'--draft=false','--latest']);
  } finally { await rm(downloaded,{recursive:true,force:true}); }
}

async function main() {
  const [command,tag,dirArg,platform,sourceArg] = process.argv.slice(2);
  const root = process.cwd(); const dir = resolve(dirArg ?? 'release-assets');
  if (command === 'privacy') {
    validateTrackedFiles(execFileSync('git',['ls-files','-z'],{encoding:'utf8'}).split('\0').filter(Boolean));
  } else if (command === 'validate') {
    const result = await validateVersions(root,tag);
    if (process.env.GITHUB_OUTPUT) await writeFile(process.env.GITHUB_OUTPUT,`tag=${tag}\nversion=${result.version}\nversionCode=${result.versionCode}\n`,{flag:'a'});
  } else if (command === 'stage') await stageArtifact({root,dir,tag,platform,source:resolve(sourceArg)});
  else if (command === 'generate') await generateRelease({root,dir,tag});
  else if (command === 'verify') await verifyRelease({root,dir,tag});
  else if (command === 'publish') await publishRelease({root,dir,tag});
  else throw new Error('Expected privacy, validate, stage, generate, verify or publish command');
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main().catch(error=>{console.error(error.message);process.exitCode=1;});
