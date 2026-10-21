/** Offline assembly of the exact artifacts tested by the npm release workflow. */
import { cpSync, mkdirSync, readFileSync, readdirSync, writeFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

export const targets = [
  {suffix:'linux-x64-gnu', triple:'x86_64-unknown-linux-gnu', os:'linux', cpu:'x64', libc:'glibc'},
  {suffix:'linux-arm64-gnu', triple:'aarch64-unknown-linux-gnu', os:'linux', cpu:'arm64', libc:'glibc'},
  {suffix:'darwin-arm64', triple:'aarch64-apple-darwin', os:'darwin', cpu:'arm64', libc:undefined},
  {suffix:'win32-x64-msvc', triple:'x86_64-pc-windows-msvc', os:'win32', cpu:'x64', libc:undefined},
  {suffix:'darwin-x64', triple:'x86_64-apple-darwin', os:'darwin', cpu:'x64', libc:undefined},
  {suffix:'win32-arm64-msvc', triple:'aarch64-pc-windows-msvc', os:'win32', cpu:'arm64', libc:undefined},
  {suffix:'linux-x64-musl', triple:'x86_64-unknown-linux-musl', os:'linux', cpu:'x64', libc:'musl'},
  {suffix:'linux-arm64-musl', triple:'aarch64-unknown-linux-musl', os:'linux', cpu:'arm64', libc:'musl'},
] as const;
export function selectTarget(platform: string, cpu: string, libc?: string): typeof targets[number] {
  const target = targets.find(item => item.os === platform && item.cpu === cpu && item.libc === libc);
  if (!target) throw new Error('Unsupported package host');
  return target;
}

export function hostTarget(): typeof targets[number] {
  const libc = process.platform === 'linux'
    ? (typeof object(object(process.report.getReport()).header).glibcVersionRuntime === 'string' ? 'glibc' : 'musl')
    : undefined;
  return selectTarget(process.platform,process.arch,libc);
}

const commonFiles = ['index.js','index.d.ts','units.d.ts','execution.cjs','spars.mjs','README.md','LICENSE','THIRD_PARTY_NOTICES.md'] as const;
const repository = {type:'git',url:'git+https://github.com/tareksanger/SpaRs.git',directory:'bindings/node'};
interface ReleaseIdentity { name: string; version: string; sha: string; target: string }
interface CargoPackage {
  id: string;
  name: string;
  version: string;
  source: string | null;
  manifestPath: string;
  license: string | null;
  repository: string | null;
  licenseFile: string | null;
}

function object(value: unknown): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) throw new Error('Expected JSON object');
  return Object.fromEntries(Object.entries(value));
}
function json(path: string): Record<string, unknown> { return object(JSON.parse(readFileSync(path, 'utf8'))); }
function cargoPackage(value: unknown): CargoPackage {
  const pkg = object(value);
  if (typeof pkg.id !== 'string' || typeof pkg.name !== 'string' || typeof pkg.version !== 'string' ||
      typeof pkg.manifest_path !== 'string' || (pkg.source !== null && typeof pkg.source !== 'string')) throw new Error('Invalid Cargo package');
  function optionalString(value: unknown): string | null {
    if (value === null || value === undefined) return null;
    if (typeof value !== 'string') throw new Error('Invalid Cargo metadata string');
    return value;
  }
  return {id:pkg.id,name:pkg.name,version:pkg.version,source:pkg.source,manifestPath:pkg.manifest_path,
    license:optionalString(pkg.license),repository:optionalString(pkg.repository),licenseFile:optionalString(pkg.license_file)};
}
function identity(path: string): ReleaseIdentity {
  const value = json(path);
  if (typeof value.name !== 'string' || !/^@[a-z0-9-]+\/[a-z0-9-]+$/.test(value.name) ||
      typeof value.version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value.version) ||
      typeof value.sha !== 'string' || !/^[a-f0-9]{40}$/.test(value.sha) || typeof value.target !== 'string') {
    throw new Error('Invalid release identity');
  }
  return {name:value.name,version:value.version,sha:value.sha,target:value.target};
}
function writeJson(path: string, value: object): void { writeFileSync(path, JSON.stringify(value,null,2)+'\n'); }
function create(path: string): void {
  if (existsSync(path)) throw new Error('Output already exists; use a fresh output directory');
  mkdirSync(path,{recursive:true});
}

/** Include upstream texts from the locked Cargo dependency closure, including build dependencies. */
export function dependencyLicenses(root: string, triple: string): string {
  return licenseTexts(JSON.parse(execFileSync('cargo', ['metadata','--format-version','1','--locked','--offline','--filter-platform',triple], {cwd:root,encoding:'utf8'})), root);
}

export function licenseTexts(value: unknown, root: string): string {
  const metadata = object(value);
  if (!Array.isArray(metadata.packages)) throw new Error('Missing Cargo packages');
  const resolution = object(metadata.resolve);
  if (!Array.isArray(resolution.nodes)) throw new Error('Missing Cargo dependency graph');
  const packages = metadata.packages.map((entry: unknown) => cargoPackage(entry));
  const binding = packages.find(pkg => pkg.name === 'spars-node');
  if (!binding) throw new Error('Missing Node crate');
  const graph = new Map<string, string[]>();
  for (const entry of resolution.nodes) {
    const node = object(entry);
    if (typeof node.id !== 'string' || !Array.isArray(node.dependencies) || !node.dependencies.every((id: unknown) => typeof id === 'string')) throw new Error('Invalid dependency graph');
    graph.set(node.id,node.dependencies);
  }
  const included = new Set<string>();
  function visit(id: string): void {
    if (included.has(id)) return;
    included.add(id);
    const dependencies = graph.get(id);
    if (!dependencies) throw new Error('Incomplete dependency graph');
    for (const child of dependencies) visit(child);
  }
  visit(binding.id);
  const sections: string[] = [];
  for (const pkg of packages) {
    if (!included.has(pkg.id) || pkg.source === null) continue;
    const dir = resolve(pkg.manifestPath,'..');
    const files: string[] = [];
    function findNotices(relative: string): void {
      for (const entry of readdirSync(join(dir,relative),{withFileTypes:true})) {
        const path = join(relative,entry.name);
        if (entry.isDirectory()) findNotices(path);
        else if (entry.isFile() && /^(license|licence|copying|notice|copyright)([._-]|$)/i.test(entry.name)) files.push(path);
      }
    }
    findNotices('');
    if (pkg.licenseFile && !files.includes(pkg.licenseFile)) files.push(pkg.licenseFile);
    if (!files.length && ['napi','napi-build','napi-derive','napi-derive-backend','napi-sys'].includes(pkg.name)) {
      const vcs = object(json(join(dir,'.cargo_vcs_info.json')).git);
      if (pkg.license !== 'MIT' || pkg.repository !== 'https://github.com/napi-rs/napi-rs' ||
          !['2763e12efc855748485129952a6ccb97ac991c06','38162bb0eb324ae24b402982bad3d4ef3f24c90a'].includes(String(vcs.sha1))) throw new Error('Review changed NAPI-RS license provenance');
      sections.push(`${pkg.name} ${pkg.version}\n${readFileSync(join(root,'licenses/napi-rs-MIT.txt'),'utf8')}`);
      continue;
    }
    if (!files.length) throw new Error(`Missing license texts: ${pkg.name} ${pkg.version}`);
    sections.push(`${pkg.name} ${pkg.version}\n${files.sort().map(file => readFileSync(join(dir,file),'utf8')).join('\n')}`);
  }
  return sections.sort().join('\n\n----------------------------------------\n\n')+'\n';
}

export function collect(root: string, suffix: string, output: string, sha: string): void {
  const target = targets.find(item => item.suffix === suffix);
  if (!target) throw new Error('Unsupported package target');
  if (hostTarget().suffix !== target.suffix) throw new Error('Build on the matching native runner');
  const binding = join(root,'bindings/node');
  const manifest = json(join(binding,'package.json'));
  const notices = dependencyLicenses(root,target.triple);
  create(output);
  writeJson(join(output,'release.json'),{name:manifest.name,version:manifest.version,sha,target:suffix});
  identity(join(output,'release.json'));
  for (const file of ['index.js','index.d.ts','units.d.ts','execution.cjs',`spars-node.${suffix}.node`]) cpSync(join(binding,file),join(output,file));
  cpSync(join(binding,'bin/spars.mjs'),join(output,'spars.mjs'));
  cpSync(join(binding,'README.md'),join(output,'README.md'));
  for (const file of ['LICENSE','THIRD_PARTY_NOTICES.md','licenses']) cpSync(join(root,file),join(output,file),{recursive:true});
  writeFileSync(join(output,'DEPENDENCY_LICENSES.txt'),notices);
}

export function stageRelease(input: string, output: string, sha: string, version: string): void {
  let first: ReleaseIdentity | undefined;
  let previous: string | undefined;
  for (const target of targets) {
    const dir = join(input,target.suffix);
    const current = identity(join(dir,'release.json'));
    if (current.sha !== sha || current.version !== version || current.target !== target.suffix || (first && current.name !== first.name)) throw new Error('Release identity mismatch');
    first ??= current;
    const binaries = readdirSync(dir).filter(file => file.endsWith('.node'));
    if (binaries.length !== 1 || binaries[0] !== `spars-node.${target.suffix}.node`) throw new Error('Missing or unexpected native binary');
    for (const file of [...commonFiles,'DEPENDENCY_LICENSES.txt']) {
      const content = readFileSync(join(dir,file));
      if (!content.length) throw new Error(`Empty release file: ${file}`);
      if (previous && commonFiles.some(common => common === file) && !content.equals(readFileSync(join(previous,file)))) throw new Error(`Wrapper mismatch: ${file}`);
    }
    if (!readdirSync(join(dir,'licenses')).length) throw new Error('Missing upstream licenses');
    previous = dir;
  }
  if (!first || !previous) throw new Error('No artifacts');
  create(output);
  const base = {version:first.version,license:'MIT',repository,engines:{node:'>=22'},publishConfig:{access:'public',registry:'https://registry.npmjs.org/'}};
  const main = join(output,'main');
  mkdirSync(join(main,'bin'),{recursive:true});
  for (const file of commonFiles) cpSync(join(previous,file),join(main,file === 'spars.mjs' ? 'bin/spars.mjs' : file));
  cpSync(join(previous,'licenses'),join(main,'licenses'),{recursive:true});
  writeJson(join(main,'package.json'),{...base,name:first.name,description:'Native SpaRs inference for Node.js',main:'index.js',types:'index.d.ts',bin:{spars:'bin/spars.mjs'},
    optionalDependencies:Object.fromEntries(targets.map(target => [`${first.name}-${target.suffix}`,first.version]))});
  for (const target of targets) {
    const dir = join(output,target.suffix);
    mkdirSync(dir);
    const binary = `spars-node.${target.suffix}.node`;
    for (const file of [binary,'LICENSE','THIRD_PARTY_NOTICES.md','DEPENDENCY_LICENSES.txt','licenses']) cpSync(join(input,target.suffix,file),join(dir,file),{recursive:true});
    writeFileSync(join(dir,'README.md'),`Native ${target.suffix} binary for ${first.name}. Install ${first.name} to use SpaRs.\n`);
    writeJson(join(dir,'package.json'),{...base,name:`${first.name}-${target.suffix}`,main:binary,os:[target.os],cpu:[target.cpu],...(target.libc ? {libc:[target.libc]} : {})});
  }
}

export function pack(output: string): void {
  const tarballs = join(output,'tarballs');
  create(tarballs);
  for (const name of [...targets.map(target => target.suffix),'main']) {
    execFileSync('npm',['pack','--cache',resolve(output,'cache'),'--offline','--ignore-scripts','--pack-destination',resolve(tarballs)],{cwd:join(output,name),stdio:'inherit'});
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command,input,output,sha,version] = process.argv.slice(2);
  if (command === 'collect' && input && output && sha) collect(process.cwd(),input,output,sha);
  else if (command === 'stage' && input && output && sha && version) { stageRelease(input,output,sha,version); pack(output); }
  else throw new Error('Usage: release.mts collect TARGET OUTPUT SHA | stage INPUT OUTPUT SHA VERSION');
}
