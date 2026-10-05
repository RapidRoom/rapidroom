import {fileURLToPath} from 'node:url';
import fs from 'node:fs';import path from 'node:path';
import {createServer} from 'vite';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || 'playwright-core');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..'),out=process.env.COMPACT_SLIDER_RESULTS || fs.mkdtempSync('/tmp/compact-fullpanels-'),files=[root+'/.fullpanel-validation.tsx',root+'/.fullpanel-validation.html'];let server,browser;const result={scenarios:[],limitations:['Headless Chromium, synthetic store state, native preset loading mocked. No photo rendering or native desktop test.']};
try{
 fs.writeFileSync(files[0],fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)),'fullpanel-harness.tsx')));fs.writeFileSync(files[1],'<html><head><meta charset="utf-8"></head><body style="margin:0"><div id="root"></div><script type="module" src="/.fullpanel-validation.tsx"></script></body></html>');
 server=await createServer({root,server:{port:0,host:'127.0.0.1',fs:{allow:[root,fs.realpathSync(root+'/node_modules')]}}});await server.listen();browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH || '/usr/bin/chromium'});
 for(const dpi of [1,2]){
  const context=await browser.newContext({viewport:{width:1000,height:1450},deviceScaleFactor:dpi});const page=await context.newPage(),errors=[];
  await page.addInitScript(()=>{window.__nativeCalls=[];window.__TAURI_INTERNALS__={invoke:async command=>{window.__nativeCalls.push(command);if(command==='load_presets')return [];throw Error('Unexpected native command: '+command);},transformCallback:()=>0,unregisterCallback:()=>{},convertFileSrc:()=>''};});
  page.on('pageerror',e=>errors.push(e.message));await page.goto(server.resolvedUrls.local[0]+'.fullpanel-validation.html');await page.waitForFunction(()=>typeof window.__setFullPanel==='function');
  for(const density of ['comfortable','compact'])for(const width of [240,320,420])for(const subMask of [false,true]){
   const config={density,width,subMask,theme:subMask?'light':'dark',language:width===240?'de':'en'};await page.evaluate(c=>window.__setFullPanel(c),config);await page.waitForTimeout(500);await page.evaluate(()=>document.fonts.ready);
   const measure=await page.evaluate(()=>[...document.querySelectorAll('[data-full-panel]')].map(p=>({panel:p.dataset.fullPanel,ranges:[...p.querySelectorAll('input[type=range]')].map(e=>{const b=e.getBoundingClientRect();return{width:b.width,height:b.height,aria:e.getAttribute('aria-label')??e.getAttribute('aria-labelledby')}})})));
   if(errors.length)throw Error('Full panels: '+JSON.stringify(errors));if(measure.some(p=>p.ranges.length<5))throw Error('Panel sliders missing: '+JSON.stringify(measure));if(measure.some(p=>p.ranges.some(r=>r.width<47||r.height<24)))throw Error('Full panel targets too small: '+JSON.stringify(measure));
   if(density==='compact'&&measure.some(p=>p.ranges.some(r=>!r.aria)))throw Error('Compact panel accessible labels missing');
   result.scenarios.push({dpi,...config,panels:measure});if(dpi===1&&width!==420)await page.locator('main').screenshot({path:out+`/fullpanels-${density}-${width}-${subMask?'radial':'container'}.png`});
  }
  const calls=await page.evaluate(()=>window.__nativeCalls);if(calls.some(c=>c!=='load_presets'))throw Error('Unexpected native call');await context.close();
 }
 result.complete=true;fs.writeFileSync(out+'/fullpanel-results.json',JSON.stringify(result,null,2));console.log('PASS '+result.scenarios.length+' full-panel scenarios, global/mask scopes, resize and radial controls.');
}finally{await browser?.close();await server?.close();for(const f of files)if(fs.existsSync(f))fs.unlinkSync(f);}
