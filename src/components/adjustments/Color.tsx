import PointColorPanel from './PointColor';
import { useEffect, useState, useMemo, CSSProperties, ReactNode } from 'react';
import { Pipette, Sliders } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { useTranslation } from 'react-i18next';
import Slider from '../ui/Slider';
import ColorWheel from '../ui/ColorWheel';
import {
  ColorAdjustment,
  ColorCalibration,
  HueSatLum,
  INITIAL_ADJUSTMENTS,
  SectionVisibility,
} from '../../utils/adjustments';
import { Adjustments, ColorGrading, getAdjustmentToolOrder, getHiddenAdjustmentTools } from '../../utils/adjustments';
import { AppSettings } from '../ui/AppProperties';
import Text from '../ui/Text';
import AdjustmentSubSection from './AdjustmentSubSection';
import { TextColors, TextWeights } from '../../types/typography';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { HSL_MIXER_BANDS, HslMixerBand, HslMixerProperty } from '../../utils/hslMixer';

interface ColorProps {
  color: string;
  name: string;
  label: string;
}

interface ColorPanelProps {
  adjustments: Adjustments;
  setAdjustments(adjustments: Partial<Adjustments>): any;
  appSettings: AppSettings | null;
  isForMask?: boolean;
  isWbPickerActive?: boolean;
  onToggleVisibility?: (key: string) => void;
  sectionVisibility?: SectionVisibility;
  toggleWbPicker?: () => void;
  onDragStateChange?: (isDragging: boolean) => void;
}

interface ToggleIconButtonProps {
  isActive: boolean;
  onClick: () => void;
  tooltip: string;
  children: ReactNode;
}

const MIXER_SWATCHES: Record<HslMixerBand, { color: string; hue: number }> = {
  reds: { color: '#f87171', hue: 0 },
  oranges: { color: '#fb923c', hue: 30 },
  yellows: { color: '#facc15', hue: 60 },
  greens: { color: '#4ade80', hue: 120 },
  aquas: { color: '#2dd4bf', hue: 180 },
  blues: { color: '#60a5fa', hue: 240 },
  purples: { color: '#a78bfa', hue: 300 },
  magentas: { color: '#f472b6', hue: 340 },
};

const MIXER_TRACK_PREFIX: Record<HslMixerProperty, string> = { hue: 'hue', saturation: 'sat', luminance: 'lum' };

const ToggleIconButton = ({ isActive, onClick, tooltip, children }: ToggleIconButtonProps) => (
  <button
    onClick={onClick}
    className={`p-1.5 rounded-md transition-colors ${
      isActive ? 'bg-accent text-button-text' : 'hover:bg-bg-secondary text-text-secondary'
    }`}
    data-tooltip={tooltip}
  >
    {children}
  </button>
);

interface ColorSwatchProps {
  color: string;
  isActive: boolean;
  name: string;
  ariaLabel: string;
  onClick: (name: string) => void;
}

const ColorSwatch = ({ color, name, isActive, ariaLabel, onClick }: ColorSwatchProps) => {
  const [isPressed, setIsPressed] = useState(false);
  const [isHovered, setIsHovered] = useState(false);

  const handleMouseDown = () => {
    setIsPressed(true);
  };

  const handleMouseUp = () => {
    setIsPressed(false);
  };

  const handleMouseLeave = () => {
    setIsPressed(false);
    setIsHovered(false);
  };

  const handleMouseEnter = () => {
    setIsHovered(true);
  };

  const handleClick = () => {
    onClick(name);
  };

  const getTransform = () => {
    if (isPressed) return 'scale(0.95)';
    if (isActive) return 'scale(1.1)';
    if (isHovered) return 'scale(1.08)';
    return 'scale(1)';
  };

  return (
    <button
      aria-label={ariaLabel}
      className="relative w-6 h-6 focus:outline-hidden group"
      onClick={handleClick}
      onMouseDown={handleMouseDown}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseLeave}
      onMouseEnter={handleMouseEnter}
      onTouchStart={handleMouseDown}
      onTouchEnd={handleMouseUp}
    >
      <div
        className={`absolute inset-0 rounded-full border-2 transition-all duration-200 ease-out ${
          isActive ? 'border-white opacity-100' : 'scale-100 border-transparent opacity-0'
        }`}
        style={{
          transform: isActive ? (isPressed ? 'scale(1.1)' : 'scale(1.25)') : undefined,
          transition: isPressed
            ? 'transform 100ms cubic-bezier(0.4, 0, 0.2, 1), opacity 200ms ease-out'
            : 'transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1), opacity 200ms ease-out',
        }}
      />

      <div
        className={`absolute inset-0 rounded-full transition-all duration-150 ease-out ${
          isActive ? 'shadow-lg' : 'shadow-md'
        }`}
        style={{
          backgroundColor: color,
          transform: getTransform(),
          transition: isPressed
            ? 'transform 100ms cubic-bezier(0.4, 0, 0.2, 1)'
            : 'transform 200ms cubic-bezier(0.34, 1.56, 0.64, 1)',
        }}
      />
    </button>
  );
};

const ColorGradingPanel = ({ adjustments, setAdjustments, onDragStateChange }: ColorPanelProps) => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<'3way' | 'global'>('3way');
  const [isExpanded, setIsExpanded] = useState(false);
  const colorGrading = adjustments.colorGrading || INITIAL_ADJUSTMENTS.colorGrading;

  const handleChange = (grading: ColorGrading, newValue: HueSatLum) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorGrading: {
        ...(prev.colorGrading || INITIAL_ADJUSTMENTS.colorGrading),
        [grading]: newValue,
      },
    }));
  };

  const handleColorGradingSliderChange = (grading: ColorGrading, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorGrading: {
        ...(prev.colorGrading || INITIAL_ADJUSTMENTS.colorGrading),
        [grading]: parseFloat(value),
      },
    }));
  };

  const tabs = useMemo(
    () => [
      {
        id: '3way',
        icon: (
          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
            <circle cx="12" cy="6" r="4.5" />
            <circle cx="5" cy="18" r="4.5" />
            <circle cx="19" cy="18" r="4.5" />
          </svg>
        ),
      },
      {
        id: 'global',
        icon: (
          <div className="w-3.5 h-3.5 rounded-full" style={{ background: 'linear-gradient(to top, #666, #fff)' }} />
        ),
      },
    ],
    [],
  );

  return (
    <div>
      <div className="flex items-center justify-start gap-2 mb-4 mt-2 px-1">
        {tabs.map((tab) => {
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id as '3way' | 'global')}
              className={`w-7 h-7 rounded-full flex items-center justify-center transition-all focus:outline-none
                ${
                  isActive
                    ? 'ring-2 ring-offset-2 ring-offset-surface ring-accent text-text-primary'
                    : 'bg-bg-secondary text-text-secondary hover:text-text-primary hover:bg-bg-secondary/80'
                }`}
            >
              {tab.icon}
            </button>
          );
        })}

        <div className="w-px h-5 bg-text-secondary/20 mx-1" />

        <button
          onClick={() => setIsExpanded(!isExpanded)}
          className={`w-7 h-7 rounded-full flex items-center justify-center transition-all focus:outline-none
            ${
              isExpanded
                ? 'bg-accent text-button-text'
                : 'bg-bg-secondary text-text-secondary hover:text-text-primary hover:bg-bg-secondary/80'
            }`}
          data-tooltip={t('adjustments.color.toggleSliders')}
        >
          <Sliders size={14} />
        </button>
      </div>

      <div className="relative w-full mb-4">
        <AnimatePresence mode="wait">
          {activeTab === '3way' ? (
            <motion.div
              key="3way"
              initial={{ opacity: 0, x: -15 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -15 }}
              transition={{ duration: 0.2 }}
              className="w-full"
            >
              <div className="flex justify-center mb-4">
                <div className="w-[calc(50%-0.5rem)]">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.midtones}
                    label={t('adjustments.color.grading.midtones')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Midtones, val)}
                    value={colorGrading.midtones}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
              </div>
              <div className="flex justify-between mb-2 gap-4">
                <div className="w-full flex-1 min-w-0">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.shadows}
                    label={t('adjustments.color.grading.shadows')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Shadows, val)}
                    value={colorGrading.shadows}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
                <div className="w-full flex-1 min-w-0">
                  <ColorWheel
                    defaultValue={INITIAL_ADJUSTMENTS.colorGrading.highlights}
                    label={t('adjustments.color.grading.highlights')}
                    onChange={(val: HueSatLum) => handleChange(ColorGrading.Highlights, val)}
                    value={colorGrading.highlights}
                    onDragStateChange={onDragStateChange}
                    isExpanded={isExpanded}
                  />
                </div>
              </div>
            </motion.div>
          ) : (
            <motion.div
              key="global"
              initial={{ opacity: 0, x: 15 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 15 }}
              transition={{ duration: 0.2 }}
              className="w-full flex justify-center pb-2"
            >
              <div className="w-full max-w-70">
                <ColorWheel
                  defaultValue={INITIAL_ADJUSTMENTS.colorGrading.global}
                  label={t('adjustments.color.grading.global')}
                  onChange={(val: HueSatLum) => handleChange(ColorGrading.Global, val)}
                  value={colorGrading.global || INITIAL_ADJUSTMENTS.colorGrading.global}
                  onDragStateChange={onDragStateChange}
                  isExpanded={isExpanded}
                />
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div>
        <Slider
          defaultValue={50}
          label={t('adjustments.color.grading.blending')}
          max={100}
          min={0}
          onChange={(e: any) => handleColorGradingSliderChange(ColorGrading.Blending, e.target.value)}
          step={1}
          value={colorGrading.blending}
          onDragStateChange={onDragStateChange}
        />
        <Slider
          defaultValue={0}
          label={t('adjustments.color.grading.balance')}
          max={100}
          min={-100}
          onChange={(e: any) => handleColorGradingSliderChange(ColorGrading.Balance, e.target.value)}
          step={1}
          value={colorGrading.balance}
          onDragStateChange={onDragStateChange}
        />
      </div>
    </div>
  );
};

const ColorCalibrationPanel = ({ adjustments, setAdjustments, onDragStateChange }: ColorPanelProps) => {
  const { t } = useTranslation();
  const [activePrimary, setActivePrimary] = useState('red');
  const colorCalibration = adjustments.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration;

  const PRIMARY_COLORS = useMemo(
    () => [
      { name: 'red', color: '#f87171', label: t('adjustments.color.calibration.colors.red') },
      { name: 'green', color: '#4ade80', label: t('adjustments.color.calibration.colors.green') },
      { name: 'blue', color: '#60a5fa', label: t('adjustments.color.calibration.colors.blue') },
    ],
    [t],
  );

  const handleShadowsChange = (value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorCalibration: {
        ...(prev.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration),
        shadowsTint: parseFloat(value),
      },
    }));
  };

  const handlePrimaryChange = (key: 'Hue' | 'Saturation', value: string) => {
    const fullKey = `${activePrimary}${key}` as keyof ColorCalibration;
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      colorCalibration: {
        ...(prev.colorCalibration || INITIAL_ADJUSTMENTS.colorCalibration),
        [fullKey]: parseFloat(value),
      },
    }));
  };

  const currentValues = {
    hue: colorCalibration[`${activePrimary}Hue` as keyof ColorCalibration] || 0,
    saturation: colorCalibration[`${activePrimary}Saturation` as keyof ColorCalibration] || 0,
  };

  const trackSuffix = `${activePrimary}s`;

  return (
    <div>
      <div>
        <Text color={TextColors.primary} weight={TextWeights.medium} className="mb-1">
          {t('adjustments.color.calibration.shadows')}
        </Text>
        <Slider
          data-adjustment-key="colorCalibration.shadowsTint"
          label={t('adjustments.color.calibration.tint')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={colorCalibration.shadowsTint}
          onChange={(e: any) => handleShadowsChange(e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName="tint-gradient-track"
        />
      </div>
      <div className="mt-3">
        <Text color={TextColors.primary} weight={TextWeights.medium} className="mb-3">
          {t('adjustments.color.calibration.primaries')}
        </Text>
        <div className="flex justify-center gap-6 mb-4 px-1">
          {PRIMARY_COLORS.map(({ name, color, label }) => (
            <ColorSwatch
              color={color}
              isActive={activePrimary === name}
              key={name}
              name={name}
              onClick={setActivePrimary}
              ariaLabel={t('adjustments.color.ariaSelectColor', { name: label })}
            />
          ))}
        </div>
        <Slider
          data-adjustment-key="colorCalibration.*Hue"
          label={t('adjustments.color.calibration.hue')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={currentValues.hue}
          onChange={(e: any) => handlePrimaryChange('Hue', e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName={`hue-slider-${trackSuffix}`}
        />
        <Slider
          data-adjustment-key="colorCalibration.*Saturation"
          label={t('adjustments.color.calibration.saturation')}
          min={-100}
          max={100}
          step={1}
          defaultValue={0}
          value={currentValues.saturation}
          onChange={(e: any) => handlePrimaryChange('Saturation', e.target.value)}
          onDragStateChange={onDragStateChange}
          trackClassName={`sat-slider-${trackSuffix}`}
        />
      </div>
    </div>
  );
};

export default function ColorPanel({
  adjustments,
  setAdjustments,
  appSettings,
  isForMask = false,
  isWbPickerActive = false,
  toggleWbPicker,
  onDragStateChange,
  onToggleVisibility,
  sectionVisibility,
}: ColorPanelProps) {
  const { t } = useTranslation();
  const [activeColor, setActiveColor] = useState('reds');
  const hiddenTools = getHiddenAdjustmentTools(appSettings?.adjustmentLayout);
  const toolOrder = getAdjustmentToolOrder('color', appSettings?.adjustmentLayout?.toolOrder);

  const HSL_COLORS = useMemo<Array<ColorProps>>(
    () =>
      HSL_MIXER_BANDS.map((name) => ({
        name,
        color: MIXER_SWATCHES[name].color,
        label: t(`adjustments.color.mixerColors.${name}`),
      })),
    [t],
  );

  const hsl = adjustments?.hsl || INITIAL_ADJUSTMENTS.hsl;

  const mixerTrackStyle = useMemo(
    () =>
      Object.fromEntries(
        HSL_MIXER_BANDS.flatMap((name) => [
          [`--hsl-mixer-hue-${name}`, `${(((MIXER_SWATCHES[name].hue + hsl[name].hue) % 360) + 360) % 360}`],
          [`--hsl-mixer-sat-${name}`, `${(hsl[name].saturation + 100) / 2}%`],
        ]),
      ) as CSSProperties,
    [hsl],
  );

  const isMixerExpanded = useUIStore((state) => state.isColorMixerExpanded);
  const setUI = useUIStore((state) => state.setUI);
  const [mixerMode, setMixerMode] = useState<'hsl' | 'pointColor'>('hsl');
  const [mixerTab, setMixerTab] = useState<HslMixerProperty>('hue');
  const mixerPickerProperty = useEditorStore((state) => (isForMask ? null : state.mixerPickerProperty));
  const isMixerPickerDragging = useEditorStore((state) => mixerPickerProperty !== null && state.isSliderDragging);
  const setEditor = useEditorStore((state) => state.setEditor);

  const mixerTabs = useMemo<Array<{ id: HslMixerProperty; label: string }>>(
    () => [
      { id: 'hue', label: t('adjustments.color.hue') },
      { id: 'saturation', label: t('adjustments.color.saturation') },
      { id: 'luminance', label: t('adjustments.color.luminance') },
    ],
    [t],
  );

  useEffect(() => {
    if (!isForMask) return () => setEditor({ mixerPickerProperty: null });
  }, [isForMask, setEditor]);

  const toggleMixerExpanded = () => {
    if (isMixerExpanded && mixerPickerProperty) setEditor({ mixerPickerProperty: null });
    setUI({ isColorMixerExpanded: !isMixerExpanded });
  };

  const toggleMixerPicker = () => {
    setEditor({ mixerPickerProperty: mixerPickerProperty ? null : mixerTab, isWbPickerActive: false });
  };

  const selectMixerTab = (tab: HslMixerProperty) => {
    setMixerTab(tab);
    if (mixerPickerProperty) setEditor({ mixerPickerProperty: tab });
  };

  const handleAdjustmentChange = (key: ColorAdjustment, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({ ...prev, [key]: parseFloat(value) }));
  };

  const handleHslChange = (color: string, key: HslMixerProperty, value: string) => {
    setAdjustments((prev: Partial<Adjustments>) => ({
      ...prev,
      hsl: {
        ...(prev.hsl || {}),
        [color]: {
          ...(prev.hsl?.[color] || {}),
          [key]: parseFloat(value),
        },
      },
    }));
  };

  const renderMixerSlider = (color: string, property: HslMixerProperty, label: string) => (
    <Slider
      animateValueChanges={!isMixerPickerDragging}
      key={`${color}-${property}`}
      label={label}
      max={100}
      min={-100}
      onChange={(e: any) => handleHslChange(color, property, e.target.value)}
      step={1}
      value={hsl[color][property]}
      trackClassName={`${MIXER_TRACK_PREFIX[property]}-slider-${color}`}
      onDragStateChange={onDragStateChange}
    />
  );

  return (
    <div className="flex flex-col gap-4">
      {!hiddenTools.includes('whiteBalance') && (
        <AdjustmentSubSection
          actions={
            !isForMask &&
            toggleWbPicker && (
              <ToggleIconButton
                isActive={isWbPickerActive}
                onClick={toggleWbPicker}
                tooltip={t('adjustments.color.wbPickerTooltip')}
              >
                <Pipette size={16} />
              </ToggleIconButton>
            )
          }
          id="whiteBalance"
          order={toolOrder.indexOf('whiteBalance')}
          title={t('adjustments.color.whiteBalance')}
        >
          <Slider
            label={t('adjustments.color.temperature')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Temperature, e.target.value)}
            step={1}
            value={adjustments.temperature || 0}
            trackClassName="temperature-gradient-track"
            onDragStateChange={onDragStateChange}
          />
          <Slider
            label={t('adjustments.color.tint')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Tint, e.target.value)}
            step={1}
            value={adjustments.tint || 0}
            trackClassName="tint-gradient-track"
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('colorPresence') && (
        <AdjustmentSubSection
          id="colorPresence"
          order={toolOrder.indexOf('colorPresence')}
          title={t('adjustments.color.presence')}
        >
          <Slider
            label={t('adjustments.color.vibrance')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Vibrance, e.target.value)}
            step={1}
            value={adjustments.vibrance || 0}
            onDragStateChange={onDragStateChange}
          />
          <Slider
            label={t('adjustments.color.saturation')}
            max={100}
            min={-100}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Saturation, e.target.value)}
            step={1}
            value={adjustments.saturation || 0}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('hue') && (
        <AdjustmentSubSection
          id="hue"
          order={toolOrder.indexOf('hue')}
          title={isForMask ? t('adjustments.color.localHue') : t('adjustments.color.hue')}
        >
          <Slider
            label={t('adjustments.color.hue')}
            max={180}
            min={-180}
            onChange={(e: any) => handleAdjustmentChange(ColorAdjustment.Hue, e.target.value)}
            step={1}
            value={adjustments.hue || 0}
            trackClassName="hue-range-track"
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('colorGrading') && (
        <AdjustmentSubSection
          id="colorGrading"
          isContentVisible={sectionVisibility?.colorGrading}
          onToggleVisibility={onToggleVisibility && (() => onToggleVisibility('colorGrading'))}
          order={toolOrder.indexOf('colorGrading')}
          title={t('adjustments.color.colorGrading')}
        >
          <ColorGradingPanel
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            appSettings={appSettings}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}

      {!hiddenTools.includes('colorMixer') && (
        <AdjustmentSubSection
          actions={
            <div className="flex items-center gap-1">
              {isMixerExpanded && !isForMask && mixerMode === 'hsl' && (
                <ToggleIconButton
                  isActive={mixerPickerProperty !== null}
                  onClick={toggleMixerPicker}
                  tooltip={t('adjustments.color.mixerPickerTooltip')}
                >
                  <Pipette size={16} />
                </ToggleIconButton>
              )}
              <ToggleIconButton
                isActive={isMixerExpanded}
                onClick={toggleMixerExpanded}
                tooltip={t('adjustments.color.toggleMixerExpanded')}
              >
                <Sliders size={16} />
              </ToggleIconButton>
            </div>
          }
          id="colorMixer"
          isContentVisible={sectionVisibility?.colorMixer}
          onToggleVisibility={onToggleVisibility && (() => onToggleVisibility('colorMixer'))}
          order={toolOrder.indexOf('colorMixer')}
          title={t('adjustments.color.colorMixer')}
        >
          <div className="flex gap-1 mb-3 p-1 rounded-lg bg-surface-secondary">
            {(['hsl', 'pointColor'] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={mixerMode === mode}
                className={`flex-1 h-7 rounded-md text-xs ${mixerMode === mode ? 'bg-surface text-text-primary' : 'text-text-secondary'}`}
                onClick={() => {
                  setMixerMode(mode);
                  setEditor({ isPointColorPickerActive: false, mixerPickerProperty: null });
                }}
              >
                {t(mode === 'hsl' ? 'pointColor.hsl' : 'pointColor.title')}
              </button>
            ))}
          </div>
          {mixerMode === 'pointColor' ? (
            <PointColorPanel
              points={adjustments.pointColor ?? []}
              isForMask={isForMask}
              onChange={(pointColor) => setAdjustments({ pointColor })}
              onDragStateChange={onDragStateChange}
            />
          ) : (
            <div style={mixerTrackStyle}>
              {isMixerExpanded ? (
                <>
                  <div className="flex items-center gap-1 p-1 mb-3 rounded-lg bg-surface-secondary">
                    {mixerTabs.map(({ id, label }) => (
                      <button
                        key={id}
                        className={`flex-1 h-7 rounded-md text-xs transition-all ${
                          mixerTab === id
                            ? 'bg-surface text-text-primary'
                            : 'text-text-secondary hover:text-text-primary'
                        }`}
                        onClick={() => selectMixerTab(id)}
                        type="button"
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                  {HSL_COLORS.map(({ name, label }) => renderMixerSlider(name, mixerTab, label))}
                </>
              ) : (
                <>
                  <div className="flex justify-between mb-4 px-1">
                    {HSL_COLORS.map(({ name, color, label }) => (
                      <ColorSwatch
                        color={color}
                        isActive={activeColor === name}
                        key={name}
                        name={name}
                        onClick={setActiveColor}
                        ariaLabel={t('adjustments.color.ariaSelectColor', { name: label })}
                      />
                    ))}
                  </div>
                  {mixerTabs.map(({ id, label }) => renderMixerSlider(activeColor, id, label))}
                </>
              )}
            </div>
          )}
        </AdjustmentSubSection>
      )}

      {!isForMask && !hiddenTools.includes('colorCalibration') && (
        <AdjustmentSubSection
          id="colorCalibration"
          order={toolOrder.indexOf('colorCalibration')}
          title={t('adjustments.color.calibration.title')}
        >
          <ColorCalibrationPanel
            adjustments={adjustments}
            setAdjustments={setAdjustments}
            appSettings={appSettings}
            onDragStateChange={onDragStateChange}
          />
        </AdjustmentSubSection>
      )}
    </div>
  );
}
