import { useTranslation } from 'react-i18next';
import { useRef, type ReactNode } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { useEditorStore } from '../../../store/useEditorStore';
import { useSettingsStore } from '../../../store/useSettingsStore';
import { useUIStore } from '../../../store/useUIStore';
import { useEditorActions } from '../../../hooks/useEditorActions';
import { Panel, type AppSettings } from '../../ui/AppProperties';
import { ADJUSTMENT_SECTION_TOOLS } from '../../../utils/adjustments';
import Basic from '../../adjustments/Basic';
import Curves from '../../adjustments/Curves';
import Color from '../../adjustments/Color';
import Details from '../../adjustments/Details';
import Effects from '../../adjustments/Effects';
import CropPanel from './CropPanel';
import CollapsibleSection from '../../ui/CollapsibleSection';
import Waveform from '../editor/Waveform';
import { DisplayMode } from '../../../utils/adjustments';
import Slider from '../../ui/Slider';

// Filter the existing widgets without changing the saved custom tool layout.
function tools(settings: AppSettings | null, allowed: string[]): AppSettings | null {
  if (!settings) return null;
  return {
    ...settings,
    adjustmentLayout: {
      ...settings.adjustmentLayout,
      hiddenTools: Object.values(ADJUSTMENT_SECTION_TOOLS)
        .flat()
        .map((tool) => tool.id)
        .filter((id) => !allowed.includes(id)),
    },
  };
}

export default function LightroomDevelopSections() {
  const { t } = useTranslation();
  const { adjustments, histogram, setEditor, isWbPickerActive } = useEditorStore(
    useShallow((s) => ({
      adjustments: s.adjustments,
      histogram: s.histogram,
      setEditor: s.setEditor,
      isWbPickerActive: s.isWbPickerActive,
    })),
  );
  const { appSettings, theme } = useSettingsStore(useShallow((s) => ({ appSettings: s.appSettings, theme: s.theme })));
  const { setUI, collapsibleSectionsState, setPanel, beforeAfterVisible } = useUIStore(
    useShallow((s) => ({
      setUI: s.setUI,
      collapsibleSectionsState: s.collapsibleSectionsState,
      setPanel: s.setPanel,
      beforeAfterVisible: s.beforeAfterVisible,
    })),
  );
  const { setAdjustments, handleLutSelect, setLutPreviewOverride } = useEditorActions();
  const onDragStateChange = (dragging: boolean) => setEditor({ isSliderDragging: dragging });
  const toggleWbPicker = () => setEditor((s) => ({ isWbPickerActive: !s.isWbPickerActive, mixerPickerProperty: null }));
  const shared = { adjustments, setAdjustments, onDragStateChange };
  const colorSaturation = useRef(adjustments.saturation === -100 ? 0 : adjustments.saturation);
  if (adjustments.saturation !== -100) colorSaturation.current = adjustments.saturation;
  const section = (id: string, title: string, content: ReactNode) => {
    const key = `lightroom_${id}`;
    const states = collapsibleSectionsState;
    return (
      <CollapsibleSection
        key={key}
        title={title}
        isOpen={states[key] ?? id === 'basic'}
        isContentVisible
        canToggleVisibility={false}
        compact
        onToggle={() =>
          setUI((s) => ({
            collapsibleSectionsState: {
              ...s.collapsibleSectionsState,
              ...(appSettings?.enableFocusMode
                ? Object.fromEntries(Object.keys(s.collapsibleSectionsState).map((k) => [k, false]))
                : s.collapsibleSectionsState),
              [key]: !(states[key] ?? id === 'basic'),
            },
          }))
        }
      >
        {content}
      </CollapsibleSection>
    );
  };
  return (
    <>
      <div className="h-36 shrink-0" aria-label={t('ui.waveform.tooltips.histogram')}>
        <Waveform
          histogramOnly
          waveformData={null}
          histogram={histogram}
          displayMode={DisplayMode.Histogram}
          setDisplayMode={() => {}}
          theme={theme}
          showClipping={adjustments.showClipping}
          onToggleClipping={() => setAdjustments((previous) => ({ ...previous, showClipping: !previous.showClipping }))}
        />
      </div>
      <div
        role="toolbar"
        aria-label={t('lightroomWorkspace.tools')}
        className="flex gap-2 border-b border-surface pb-2"
      >
        <button onClick={() => setPanel(Panel.Crop)} className="flex-1 rounded bg-surface py-2">
          {t('lightroomWorkspace.crop')}
        </button>
        <button onClick={() => setPanel(Panel.Ai)} className="flex-1 rounded bg-surface py-2">
          {t('lightroomWorkspace.healing')}
        </button>
        <button onClick={() => setPanel(Panel.Masks)} className="flex-1 rounded bg-surface py-2">
          {t('lightroomWorkspace.masks')}
        </button>
        <button
          aria-pressed={beforeAfterVisible}
          onClick={() => setUI({ beforeAfterVisible: !beforeAfterVisible })}
          className="flex-1 rounded bg-surface py-2"
        >
          {t('lightroomWorkspace.beforeAfter')}
        </button>
      </div>
      {section(
        'basic',
        t('lightroomWorkspace.basic'),
        <>
          <Color
            {...shared}
            appSettings={tools(appSettings, ['whiteBalance'])}
            isWbPickerActive={isWbPickerActive}
            toggleWbPicker={toggleWbPicker}
          />
          <div className="flex items-center gap-2 mb-3">
            <label className="flex-1">
              {t('lightroomWorkspace.profile')}
              <select
                aria-label={t('lightroomWorkspace.profileLabel')}
                className="w-full rounded bg-surface p-1"
                disabled={appSettings?.tonemapperOverrideEnabled}
                value={adjustments.toneMapper}
                onChange={(event) =>
                  setAdjustments((previous) => ({ ...previous, toneMapper: event.target.value as 'basic' | 'agx' }))
                }
              >
                <option value="basic">{t('lightroomWorkspace.basicProfile')}</option>
                <option value="agx">{t('lightroomWorkspace.agxProfile')}</option>
              </select>
            </label>
            <label className="flex-1">
              {t('lightroomWorkspace.treatment')}
              <select
                aria-label={t('lightroomWorkspace.treatment')}
                className="w-full rounded bg-surface p-1"
                value={adjustments.saturation === -100 ? 'bw' : 'color'}
                onChange={(event) =>
                  setAdjustments((previous) => ({
                    ...previous,
                    saturation: event.target.value === 'bw' ? -100 : colorSaturation.current,
                  }))
                }
              >
                <option value="color">{t('lightroomWorkspace.color')}</option>
                <option value="bw">{t('lightroomWorkspace.bw')}</option>
              </select>
            </label>
          </div>
          <Basic {...shared} appSettings={{ ...appSettings, tonemapperOverrideEnabled: true }} />
          <div className="mt-3" aria-label={t('lightroomWorkspace.presence')}>
            {(['structure', 'clarity', 'dehaze'] as const).map((key) => (
              <Slider
                key={key}
                label={
                  key === 'structure'
                    ? t('lightroomWorkspace.texture')
                    : key === 'clarity'
                      ? t('lightroomWorkspace.clarity')
                      : t('lightroomWorkspace.dehaze')
                }
                value={adjustments[key]}
                min={-100}
                max={100}
                step={1}
                onChange={(event: React.ChangeEvent<HTMLInputElement>) =>
                  setAdjustments((previous) => ({ ...previous, [key]: Number(event.target.value) }))
                }
                onDragStateChange={onDragStateChange}
              />
            ))}
          </div>
          <Color {...shared} appSettings={tools(appSettings, ['colorPresence'])} />
        </>,
      )}
      {section('curve', t('lightroomWorkspace.curve'), <Curves {...shared} histogram={histogram} theme={theme} />)}
      {section(
        'mixer',
        t('lightroomWorkspace.mixer'),
        <Color {...shared} appSettings={tools(appSettings, ['hue', 'colorMixer'])} />,
      )}
      {section(
        'grading',
        t('lightroomWorkspace.grading'),
        <Color {...shared} appSettings={tools(appSettings, ['colorGrading'])} />,
      )}
      {section(
        'detail',
        t('lightroomWorkspace.detail'),
        <Details {...shared} appSettings={tools(appSettings, ['sharpening', 'noiseReduction'])} />,
      )}
      {section(
        'lens',
        t('lightroomWorkspace.lens'),
        <>
          <CropPanel section="lens" />
          <Details {...shared} appSettings={tools(appSettings, ['chromaticAberration'])} />
        </>,
      )}
      {section('transform', t('lightroomWorkspace.transform'), <CropPanel section="transform" />)}
      {section(
        'effects',
        t('lightroomWorkspace.effects'),
        <Effects
          {...shared}
          appSettings={appSettings}
          handleLutSelect={handleLutSelect}
          onLutHover={setLutPreviewOverride}
        />,
      )}
      {section(
        'calibration',
        t('lightroomWorkspace.calibration'),
        <>
          <p className="text-xs text-text-secondary mb-2">{t('lightroomWorkspace.calibrationInfo')}</p>
          <Color {...shared} appSettings={tools(appSettings, ['colorCalibration'])} />
        </>,
      )}
    </>
  );
}
