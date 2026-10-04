import {fileURLToPath} from 'node:url';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import {createServer} from 'vite';
const {chromium}=await import(process.env.PLAYWRIGHT_MODULE || 'playwright-core');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const out=process.env.COMPACT_SLIDER_RESULTS || fs.mkdtempSync('/tmp/compact-sliders-');
const files=[path.join(root,'.compact-validation.tsx'),path.join(root,'.compact-validation.html'),path.join(root,'src/components/ui/.Slider-before109.tsx')];
let server, browser;
const result={scenarios:[], interactions:[]};
try {
  fs.writeFileSync(files[0],fs.readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)),'harness.tsx')));
  fs.writeFileSync(files[1],'<html><head><meta charset="utf-8"></head><body style="margin:0"><div id="root"></div><script type="module" src="/.compact-validation.tsx"></script></body></html>');
  fs.writeFileSync(files[2],execFileSync('git',['show','a10ced9232bfa673c7169a46729fe2374b86c3d9:src/components/ui/Slider.tsx'],{cwd:root}));
  server=await createServer({root,server:{port:0,strictPort:false,host:'127.0.0.1',fs:{allow:[root,fs.realpathSync(root+'/node_modules')]}}});await server.listen();
  browser=await chromium.launch({executablePath:process.env.CHROMIUM_PATH || '/usr/bin/chromium'});
  const base=server.resolvedUrls.local[0];
  for (const dpi of [1,2]) {
    const context=await browser.newContext({viewport:{width:960,height:1500},deviceScaleFactor:dpi});
    const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto(base+'.compact-validation.html');await page.waitForFunction(()=>typeof window.__setHarness==='function');
    for(const theme of ['dark','light']) for(const language of ['en','de','ru']) for(const width of [240,260,320,420]) {
      await page.evaluate(config=>window.__setHarness(config),{theme,language,width,mask:false});await page.waitForTimeout(400);await page.evaluate(()=>document.fonts.ready);
      const measurement=await page.evaluate(()=> {
        const panels=[...document.querySelectorAll('[data-density]')].map(panel=> {
          const ranges=[...panel.querySelectorAll('[data-basic] input[type=range]')];
          const rects=ranges.map(el=>{const r=el.getBoundingClientRect();return {left:r.left,width:r.width,height:r.height}});
          const labels=[...panel.querySelectorAll('[data-tooltip]')].filter(el=>el.getAttribute('data-tooltip')&&!el.getAttribute('data-tooltip').includes('edit')).map(el=>el.getAttribute('data-tooltip'));
          return {density:panel.dataset.density,height:panel.getBoundingClientRect().height,ranges:rects,labels};
        });
        const domEqual=[0,1,2,3].map(i=>document.querySelector(`[data-before="${i}"]`).innerHTML===document.querySelector(`[data-after="${i}"]`).innerHTML);
        return {panels,domEqual};
      });
      if(errors.length)throw Error(JSON.stringify(errors));
      if(!measurement.domEqual.every(Boolean))throw Error('Default DOM changed: '+JSON.stringify(measurement.domEqual));
      const compact=measurement.panels.find(p=>p.density==='compact');
      // Exposure is inside the existing inset tone-mapper card; ordinary rows share a section.
      const ordinary=compact.ranges.slice(1);
      if(compact.ranges.some(r=>r.width<47||r.height<24))throw Error('Compact target too small');
      if(Math.max(...ordinary.map(r=>r.left))-Math.min(...ordinary.map(r=>r.left))>1) {fs.writeFileSync(path.join(out,'alignment-diagnostic.json'),JSON.stringify({dpi,theme,language,width,...measurement},null,2));await page.locator('main').screenshot({path:path.join(out,'alignment-diagnostic.png')});throw Error('Tracks not aligned');}
      result.scenarios.push({dpi,theme,language,width,...measurement});
      if((width===240||width===320||width===420)&&language!=='ru'&&dpi===1 || language==='ru'&&width===260&&theme==='light'&&dpi===2)
        await page.locator('main').screenshot({path:path.join(out,`sliders-${theme}-${language}-${width}-${dpi}x.png`)});
    }
    if(dpi===1) {
      await page.evaluate(()=>window.__setHarness({theme:'dark',language:'en',width:320,mask:false}));await page.waitForTimeout(400);
      for(const density of ['comfortable','compact']) {
        const range=page.locator(`[data-density=${density}] [data-basic] input[type=range]`).nth(1);
        const box=await range.boundingBox();await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.waitForTimeout(50);const dragStart=await page.evaluate(d=>window.__panelEdits[d].contrast,density);await page.mouse.move(box.x+box.width/2+40,box.y+box.height/2);await page.mouse.up();await page.waitForTimeout(400);
        const dragEnd=await page.evaluate(d=>window.__panelEdits[d].contrast,density);const drag=dragEnd-dragStart;
        await page.evaluate(d=>window.__resetPanels[d](),density);await page.waitForTimeout(400);
        // Arrows remain app-navigation shortcuts; typed values support numeric keys.
        await page.keyboard.press('Tab');await range.focus();
        const focus=await range.evaluate(el=>{const s=getComputedStyle(el);return {visible:el.matches(':focus-visible'),width:s.outlineWidth,style:s.outlineStyle}});
        if(!focus.visible||focus.width!=='2px'||focus.style!=='solid')throw Error('Keyboard focus failed: '+JSON.stringify(focus));
        await page.keyboard.press('ArrowRight');
        const navigationBlur=await range.evaluate(el=>document.activeElement!==el);if(!navigationBlur)throw Error('Navigation shortcut captured');
        const row=range.locator('xpath=ancestor::div[contains(concat(" ", normalize-space(@class), " "), " group ")][1]');
        await row.locator('span.cursor-text').first().click();
        const typed=row.locator('input[type=text]');await typed.fill('12');await typed.press('ArrowUp');await typed.press('Enter');await page.waitForTimeout(400);
        const keyboard=await page.evaluate(d=>window.__panelEdits[d].contrast,density);if(keyboard!==13)throw Error('Typed value/keyboard step failed: '+keyboard);
        await range.dblclick();await page.waitForTimeout(400);const reset=await page.evaluate(d=>window.__panelEdits[d].contrast,density);if(reset!==0)throw Error('Double reset failed');
        const fine={};
        for(const modifier of ['Alt','Shift']) {
          await page.evaluate(d=>window.__resetPanels[d](),density);await page.waitForTimeout(400);
          await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.mouse.down();await page.waitForTimeout(50);const fineStart=await page.evaluate(d=>window.__panelEdits[d].contrast,density);await page.keyboard.down(modifier);await page.mouse.move(box.x+box.width/2+40,box.y+box.height/2);await page.keyboard.up(modifier);await page.mouse.up();await page.waitForTimeout(400);
          fine[modifier]=(await page.evaluate(d=>window.__panelEdits[d].contrast,density))-fineStart;
          if(fine[modifier]<=0||fine[modifier]>=drag)throw Error('Fine modifier failed: '+JSON.stringify({modifier,drag,fine}));
        }
        await page.evaluate(d=>window.__resetPanels[d](),density);await page.waitForTimeout(400);
        await page.mouse.move(box.x+box.width/2,box.y+box.height/2);await page.keyboard.down('Shift');await page.mouse.wheel(0,-100);await page.keyboard.up('Shift');await page.waitForTimeout(400);
        const wheel=await page.evaluate(d=>window.__panelEdits[d].contrast,density);if(wheel!==1)throw Error('Shift wheel failed');
        result.interactions.push({density,dragStart,dragEnd,drag,keyboard,focus,navigationBlur,reset,fine,wheel});
      }
      if(result.interactions[0].drag<=0||Math.abs(result.interactions[0].drag-result.interactions[1].drag)>1)throw Error('Drag sensitivities differ: '+JSON.stringify(result.interactions));
      const touchClient=await context.newCDPSession(page);
      result.touch=[];
      for(const density of ['comfortable','compact']) {
        await page.evaluate(d=>window.__resetPanels[d](),density);await page.waitForTimeout(400);
        const r=page.locator(`[data-density=${density}] [data-basic] input[type=range]`).nth(1), box=await r.boundingBox();
        const x=box.x+box.width/2,y=box.y+box.height/2;
        await touchClient.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y}]});
        await page.waitForTimeout(50);await touchClient.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:x+40,y}]});await page.waitForTimeout(100);
        await touchClient.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await page.waitForTimeout(400);
        const value=await page.evaluate(d=>window.__panelEdits[d].contrast,density);if(value<=0)throw Error('Horizontal touch failed');
        await page.evaluate(d=>window.__resetPanels[d](),density);await page.waitForTimeout(400);
        await touchClient.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y}]});
        await touchClient.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:x+2,y:y+35}]});
        await touchClient.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await page.waitForTimeout(400);
        const vertical=await page.evaluate(d=>window.__panelEdits[d].contrast,density);if(vertical!==0)throw Error('Vertical scroll changed value');
        result.touch.push({density,value,vertical});
      }
      await touchClient.detach();
      await page.evaluate(()=>window.__setHarness({mask:true,width:240}));await page.waitForTimeout(400);
      if(await page.locator('[data-density=compact] [data-details] input[type=range]').count()<5)throw Error('Mask controls absent');
      await page.locator('main').screenshot({path:path.join(out,'sliders-mask-240.png')});result.maskControls=true;
    }
    await context.close();
  }
  fs.writeFileSync(path.join(out,'browser-results.json'),JSON.stringify(result,null,2));console.log('PASS '+result.scenarios.length+' scenarios; default DOM, alignment, targets, keyboard focus, wheel, drag and mask controls');
} finally {
  await browser?.close();await server?.close();for(const file of files)if(fs.existsSync(file))fs.unlinkSync(file);
}
