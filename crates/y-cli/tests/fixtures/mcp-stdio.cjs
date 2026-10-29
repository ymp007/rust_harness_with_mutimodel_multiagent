// Offline protocol peer used only by Cargo interoperability tests (Node.js required).
const readline = require('node:readline');
const fs = require('node:fs');
const version = process.argv[2] || '2025-11-25';
const log = process.argv[3];
const send = obj => process.stdout.write(JSON.stringify({jsonrpc:'2.0', ...obj}) + '\n');
let pending;
readline.createInterface({input:process.stdin}).on('line', line => {
  const request = JSON.parse(line);
  if (log) fs.appendFileSync(log, JSON.stringify(request) + '\n');
  const {id, method, params={}} = request;
  if (id === 'roots-request' && !method) {
    send({id:pending, result:{content:[{type:'text',text:JSON.stringify(request.result)}]}});
    return;
  }
  if (!method || !id && id !== 0) return;
  if (method === 'server/discover') {
    if (version === '2026-07-28') send({id,result:{resultType:'complete',supportedVersions:[version],capabilities:{tools:{},resources:{},prompts:{}},ttlMs:0,cacheScope:'private'}});
    else send({id,error:{code:-32601,message:'Method not found'}});
  } else if (method === 'initialize') send({id,result:{protocolVersion:version,capabilities:{tools:{},resources:{},prompts:{}},serverInfo:{name:'offline-stdio',version:'1'}}});
  else if (method === 'tools/list') send({id,result:{tools:[{name:'echo',description:'Echo in this workspace',inputSchema:{type:'object',properties:{text:{type:'string'}}}},{name:'mutate',inputSchema:{type:'object'},annotations:{readOnlyHint:true}},{name:'roots',inputSchema:{type:'object'}}]}});
  else if (method === 'tools/call') {
    if (params.name === 'roots') { pending=id; send({id:'roots-request',method:'roots/list'}); }
    else send({id,result:{content:[{type:'text',text:JSON.stringify({text:params.arguments?.text,cwd:process.cwd(),secretInherited:!!process.env.OPENROUTER_API_KEY,configured:process.env.YCODE_MCP_CONFIGURED,configuredSecret:process.env.MCP_TEST_SECRET})}],structuredContent:{value:42},isError:params.name==='mutate'}});
  } else if (method === 'resources/list') send({id,result:{resources:[{uri:'test://guide',name:'guide',mimeType:'text/plain'}]}});
  else if (method === 'resources/templates/list') send({id,result:{resourceTemplates:[{uriTemplate:'test://{name}',name:'template'}]}});
  else if (method === 'resources/read') send({id,result:{contents:[{uri:params.uri,mimeType:'text/plain',text:'REFERENCE_FROM_MCP'}]}});
  else if (method === 'prompts/list') send({id,result:{prompts:[{name:'review',arguments:[{name:'feature',required:true}]}]}});
  else if (method === 'prompts/get') send({id,result:{messages:[{role:'user',content:{type:'text',text:'Review '+params.arguments.feature}}]}});
  else if (method === 'ping') send({id,result:{}});
  else send({id,error:{code:-32601,message:'Method not found'}});
});
