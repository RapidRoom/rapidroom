import React,{useState} from 'react';
import {createRoot} from 'react-dom/client';
import ControlsPanel from './src/components/panel/right/ControlsPanel';
import MasksPanel from './src/components/panel/right/MasksPanel';
import {ContextMenuProvider} from './src/context/ContextMenuContext';
import {useEditorStore} from './src/store/useEditorStore';
import {useSettingsStore} from './src/store/useSettingsStore';
import {useUIStore} from './src/store/useUIStore';
import {INITIAL_ADJUSTMENTS,INITIAL_MASK_ADJUSTMENTS,INITIAL_MASK_CONTAINER} from './src/utils/adjustments';
import {THEMES} from './src/utils/themes';
import {createSubMask} from './src/utils/maskUtils';
import {Mask} from './src/components/panel/right/Masks';
import './src/i18n';import i18n from 'i18next';
import './src/styles.css';import '@fontsource/poppins/400.css';import '@fontsource/poppins/500.css';
const sub={...createSubMask(Mask.Radial,{width:1000,height:1000}),id:'radial-fixture'};
const mask={...INITIAL_MASK_CONTAINER,id:'mask-fixture',name:'Synthetic fixture mask',adjustments:{...INITIAL_MASK_ADJUSTMENTS},subMasks:[sub]};
const seed={...INITIAL_ADJUSTMENTS,masks:[mask]};
function App(){
 const[config,setConfig]=useState({width:320,density:'comfortable',theme:'dark',language:'en',subMask:false});
 window.__setFullPanel=async next=>{
  const merged={...config,...next};await i18n.changeLanguage(merged.language);
  const theme=THEMES.find(t=>t.id===merged.theme)??THEMES[0];Object.entries(theme.cssVariables).forEach(([k,v])=>document.documentElement.style.setProperty(k,v));
  useSettingsStore.setState({appSettings:{adjustmentDensity:merged.density,adjustmentLayout:{hiddenSections:['curves','color','effects'],hiddenTools:[]}},theme:merged.theme});
  useUIStore.setState({collapsibleSectionsState:{basic:true,details:true,curves:false,color:false,effects:false}});
  useEditorStore.setState({selectedImage:{path:'/synthetic/fixture.ARW',name:'Synthetic fixture',width:1000,height:1000,isReady:true},adjustments:seed,activeMaskContainerId:'mask-fixture',activeMaskId:merged.subMask?'radial-fixture':null});
  setConfig(merged);
 };
 return <ContextMenuProvider><main style={{display:'flex',gap:24,padding:16,background:'var(--app-bg-primary)',fontFamily:'Poppins, sans-serif'}}>
  <section data-full-panel="controls" className="bg-surface text-text-primary" style={{width:config.width,height:1300}}><ControlsPanel/></section>
  <section data-full-panel="masks" className="bg-surface text-text-primary" style={{width:config.width,height:1300}} key={JSON.stringify(config)}><MasksPanel/></section>
 </main></ContextMenuProvider>;
}
createRoot(document.getElementById('root')).render(<App/>);
