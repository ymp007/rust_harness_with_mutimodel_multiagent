// Reproducible real-server acceptance checks. No LLM credentials or accounts required.
// npm install --prefix target/mcp-interoperability --ignore-scripts --no-audit --no-fund \
//   @modelcontextprotocol/server-everything@2026.8.31 @modelcontextprotocol/server-filesystem@2026.8.31 \
//   @modelcontextprotocol/server-memory@2026.8.31 @modelcontextprotocol/sdk@1.32.1
// cargo build -p y-cli --locked
// node scripts/verify_mcp.mjs [path-to-ycode]
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import net from 'node:net';
import { pathToFileURL } from 'node:url';
const repo=path.resolve(import.meta.dirname,'..');
const executable=path.resolve(process.argv[2]||path.join(repo,'target/debug',process.platform==='win32'?'ycode.exe':'ycode'));
const modules=path.join(repo,'target/mcp-interoperability/node_modules');
const root=fs.mkdtempSync(path.join(os.tmpdir(),'ycode-mcp-'));
fs.writeFileSync(path.join(root,'sample.txt'),'OFFICIAL_FILESYSTEM_READ');
const entry=name=>path.join(modules,'@modelcontextprotocol',name,'dist/index.js');
const config={mcpServers:{
  everything:{command:process.execPath,args:[entry('server-everything'),'stdio']},
  filesystem:{command:process.execPath,args:[entry('server-filesystem'),'${YCODE_WORKSPACE}']},
  memory:{command:process.execPath,args:[entry('server-memory')],env:{MEMORY_FILE_PATH:'${YCODE_WORKSPACE}/memory.jsonl'}}
}};
const checks=[];
const run=(args)=>{
  const result=spawnSync(executable,['--no-global-config','--allow-mcp','--cwd',root,'mcp',...args],{encoding:'utf8',timeout:90000,windowsHide:true});
  assert.equal(result.status,0,`${args.join(' ')} failed: ${result.stderr} ${result.error||''}`);
  return JSON.parse(result.stdout);
};
const record=(name,details)=>{checks.push({name,status:'passed',...details});console.log(`PASS ${name}`);};
for(const name of ['everything','filesystem','memory']){
  fs.writeFileSync(path.join(root,'.mcp.json'),JSON.stringify(config));
  const result=run(['check',name]);assert.ok(result.catalogs.tools.length>0);
  record(`official-${name}-stdio`,{protocol:result.peer.protocolVersion,tools:result.catalogs.tools.length,resources:result.catalogs.resources?.length||0,prompts:result.catalogs.prompts?.length||0});
}
let result=run(['call','everything','echo','--arguments',JSON.stringify({message:'OFFICIAL_ECHO'})]);assert.match(JSON.stringify(result),/OFFICIAL_ECHO/);record('official-everything-tool');
result=run(['call','filesystem','read_text_file','--arguments',JSON.stringify({path:path.join(root,'sample.txt')})]);assert.match(JSON.stringify(result),/OFFICIAL_FILESYSTEM_READ/);record('official-filesystem-read');
result=run(['call','memory','create_entities','--arguments',JSON.stringify({entities:[{name:'MCP acceptance fixture',entityType:'test',observations:['isolated test data']}]})]);assert.equal(result.isError||false,false);record('official-memory-write');
result=run(['call','memory','read_graph']);assert.match(JSON.stringify(result),/MCP acceptance fixture/);record('official-memory-read');
const everything=run(['check','everything']);
if(everything.catalogs.resources?.length){result=run(['read','everything',everything.catalogs.resources[0].uri]);assert.ok(result.contents.length);record('official-everything-resource');}
if(everything.catalogs.prompts?.length){const prompt=everything.catalogs.prompts.find(p=>!(p.arguments||[]).some(a=>a.required));if(prompt){result=run(['prompt','everything',prompt.name]);assert.ok(result.messages.length);record('official-everything-prompt');}}
// Exercise the real reference server's Streamable HTTP and deprecated SSE endpoints.
const port=async()=>new Promise(resolve=>{const server=net.createServer();server.listen(0,'127.0.0.1',()=>{const n=server.address().port;server.close(()=>resolve(n));});});
const started=[];
try {
  for(const [mode,type,endpoint] of [['streamableHttp','http','/mcp'],['sse','sse','/sse']]){
    const p=await port();const child=spawn(process.execPath,['--import',pathToFileURL(path.join(repo,'scripts/mcp-loopback.mjs')).href,entry('server-everything'),mode],{cwd:root,env:{...process.env,PORT:String(p)},stdio:['ignore','pipe','pipe'],windowsHide:true});started.push(child);
    let logs='';child.stdout.on('data',b=>logs+=b);child.stderr.on('data',b=>logs+=b);
    let ready=false;
    for(let attempt=0;attempt<100;attempt++){
      if(child.exitCode!==null)throw Error(`Reference ${mode} exited: ${logs}`);
      ready=await new Promise(resolve=>{const socket=net.connect(p,'127.0.0.1');socket.on('connect',()=>{socket.destroy();resolve(true);});socket.on('error',()=>resolve(false));});
      if(ready)break;await new Promise(resolve=>setTimeout(resolve,100));
    }
    assert.ok(ready,logs);
    config.mcpServers[mode]={type,url:`http://127.0.0.1:${p}${endpoint}`};fs.writeFileSync(path.join(root,'.mcp.json'),JSON.stringify(config));
    const checked=run(['check',mode]);assert.ok(checked.catalogs.tools.length);
    result=run(['call',mode,'echo','--arguments',JSON.stringify({message:'REMOTE_REFERENCE_ECHO'})]);assert.match(JSON.stringify(result),/REMOTE_REFERENCE_ECHO/);
    record(`official-everything-${mode}`,{protocol:checked.peer.protocolVersion,tools:checked.catalogs.tools.length});
    child.kill();
  }
}finally{for(const child of started)child.kill();}
const versions=Object.fromEntries(['server-everything','server-filesystem','server-memory','sdk'].map(name=>[name,JSON.parse(fs.readFileSync(path.join(modules,'@modelcontextprotocol',name,'package.json'))).version]));
fs.writeFileSync(path.join(repo,'target/mcp-interoperability/results.json'),JSON.stringify({verifiedAt:new Date().toISOString(),platform:process.platform,versions,checks},null,2));
console.log(`Verified ${checks.length} real-server checks. Isolated fixtures retained at ${root}`);
