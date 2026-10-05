import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import Slider, { SliderDensityScope } from './src/components/ui/Slider';
import BeforeSlider from './src/components/ui/.Slider-before109';
import Basic from './src/components/adjustments/Basic';
import Details from './src/components/adjustments/Details';
import { INITIAL_ADJUSTMENTS, INITIAL_MASK_ADJUSTMENTS } from './src/utils/adjustments';
import { THEMES } from './src/utils/themes';
import './src/i18n';
import i18n from 'i18next';
import './src/styles.css';
import '@fontsource/poppins/400.css';
import '@fontsource/poppins/500.css';

function Panel({ density, mask, width }) {
  const seed = mask ? { ...INITIAL_ADJUSTMENTS, ...INITIAL_MASK_ADJUSTMENTS } : { ...INITIAL_ADJUSTMENTS };
  const [adjustments, set] = useState(seed);
  window.__panelEdits ??= {};
  window.__panelEdits[density] = adjustments;
  window.__resetPanels ??= {};
  window.__resetPanels[density] = () => set(seed);
  return (
    <section
      data-density={density}
      style={{ width, padding: 8, flexShrink: 0 }}
      className="bg-surface text-text-primary"
    >
      <h2 style={{ fontSize: 16, marginBottom: 12 }}>
        {density} {mask ? 'mask' : 'global'}
      </h2>
      <SliderDensityScope density={density}>
        <div data-basic>
          <Basic
            adjustments={adjustments}
            setAdjustments={(part) => set((prev) => ({ ...prev, ...(typeof part === 'function' ? part(prev) : part) }))}
            isForMask={mask}
            appSettings={null}
          />
        </div>
        <div data-details style={{ marginTop: 12 }}>
          <Details
            adjustments={adjustments}
            setAdjustments={(part) => set((prev) => ({ ...prev, ...(typeof part === 'function' ? part(prev) : part) }))}
            appSettings={null}
            isForMask={mask}
          />
        </div>
      </SliderDensityScope>
    </section>
  );
}
const cases = [
  { label: 'Exposure', min: -5, max: 5, step: 0.01, value: -4.5 },
  {
    label: 'Threshold',
    min: 0,
    max: 80,
    step: 1,
    value: 15,
    defaultValue: 15,
    fillOrigin: 'min',
    disabled: true,
    suffix: '%',
  },
  {
    label: <span>Composite label</span>,
    min: -100,
    max: 100,
    step: 1,
    value: 30,
    markers: [{ value: 0, color: '#ff0000' }],
    trackClassName: 'bg-accent',
  },
  { label: 'Custom default', min: 0, max: 100, step: 1, value: 40, defaultValue: 40 },
];
function App() {
  const [config, setConfig] = useState({ width: 320, language: 'en', theme: 'dark', mask: false });
  window.__setHarness = async (next) => {
    const merged = { ...config, ...next };
    await i18n.changeLanguage(merged.language);
    const theme = THEMES.find((item) => item.id === merged.theme) ?? THEMES[0];
    Object.entries(theme.cssVariables).forEach(([key, val]) => document.documentElement.style.setProperty(key, val));
    document.documentElement.style.setProperty('--font-family', 'Poppins, sans-serif');
    setConfig(merged);
  };
  return (
    <>
      <main
        style={{
          display: 'flex',
          alignItems: 'start',
          gap: 24,
          padding: 16,
          background: 'var(--app-bg-primary)',
          fontFamily: 'Poppins, sans-serif',
        }}
      >
        <Panel
          key={'comfortable' + JSON.stringify(config)}
          density="comfortable"
          mask={config.mask}
          width={config.width}
        />
        <Panel key={'compact' + JSON.stringify(config)} density="compact" mask={config.mask} width={config.width} />
      </main>
      <aside style={{ display: 'none' }}>
        {cases.map((props, index) => (
          <React.Fragment key={index}>
            <div data-before={index}>
              <BeforeSlider {...props} onChange={() => {}} />
            </div>
            <div data-after={index}>
              <Slider {...props} onChange={() => {}} />
            </div>
          </React.Fragment>
        ))}
      </aside>
    </>
  );
}
createRoot(document.getElementById('root')).render(<App />);
